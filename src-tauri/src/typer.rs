//! Typer: typing that looks like a person.
//!
//! Three parts, and only the last one touches the system:
//!
//! - [`plan`] turns text into timed keystrokes: delays from a narrow distribution,
//!   shorter inside a word and longer at the start of one, a dwell after punctuation
//!   and a beat at a line break, and an occasional typo that is noticed and corrected;
//! - the driver in [`typer_start`] sleeps out each step, and can be paused, resumed or
//!   stopped between any two keystrokes;
//! - [`inject`] posts the events, which a webview cannot do. On macOS that is
//!   `CGEventPost` with the character attached as a Unicode string, and it only works
//!   once the app has been granted Accessibility.
//!
//! The rails live here rather than in the UI, because they are the promise: nothing is
//! typed while a secure field has focus, the text comes from our own editor and never
//! from the clipboard, nothing typed is stored anywhere, and a stop is checked before
//! every keystroke rather than after a paragraph.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{openrouter, prompt, providers, samples};

/// `src/lib/typer.ts` listens for exactly this string.
pub const TYPER_EVENT: &str = "typer:state";

/// Time between "start" and the first keystroke, so the person can put the focus where
/// they actually want the text. Without it the first characters land in this app, which
/// is the one window that is never the target.
const COUNTDOWN_MS: u64 = 3_000;

/// Below this nothing looks human, and above it the output is a paste with extra steps.
const MIN_CPS: f64 = 2.0;
const MAX_CPS: f64 = 18.0;
/// What a comfortable typist does, before the speed multiplier.
const BASE_CPS: f64 = 7.0;

/* ------------------------------------------------------------------- planning */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Char(char),
    Backspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// How long to wait before this keystroke, relative to the one before it.
    pub wait_ms: u32,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub total_ms: u64,
    pub cps: f64,
}

/// What the UI sends. `deadline_secs` wins over `speed`: "finish by 0:45" is the whole
/// point of the control, so obeying it is more use than multiplying it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Share of characters that get a wrong key first. Clamped to 0..=0.25.
    pub typo_rate: f64,
    /// Multiplier on the resting pace, 0.25..=3.
    pub speed: f64,
    pub deadline_secs: Option<f64>,
    /// Stop the run after this many seconds in which no keystroke was accepted. A pause is
    /// deliberate and does not count; a run nobody is watching does.
    pub idle_secs: Option<f64>,
}

impl Default for Config {
    fn default() -> Self {
        Self { typo_rate: 0.04, speed: 1.0, deadline_secs: None, idle_secs: None }
    }
}

impl Config {
    fn sanitised(&self) -> Self {
        Self {
            typo_rate: self.typo_rate.clamp(0.0, 0.25),
            speed: self.speed.clamp(0.25, 3.0),
            deadline_secs: self.deadline_secs.filter(|secs| *secs >= 1.0).map(|secs| secs.min(3600.0)),
            // Under five seconds is not an idle limit, it is a race with the plan's own
            // punctuation dwells, so it is refused rather than obeyed.
            idle_secs: self.idle_secs.filter(|secs| *secs >= 5.0).map(|secs| secs.min(600.0)),
        }
    }
}

/* ------------------------------------------------------------ injector vocabulary */

/// A keystroke as an injector sees it: a printable character, or one of the two actions
/// that are keys rather than text. Separate from [`Action`] so the mapping can be tested
/// on a machine with no display and no injector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Enter,
}

/// The key an action turns into. A newline is Return, not a character: most editors
/// ignore a literal newline in a key event.
pub fn key_for(action: Action) -> Key {
    match action {
        Action::Char('\n') => Key::Enter,
        Action::Char(ch) => Key::Char(ch),
        Action::Backspace => Key::Backspace,
    }
}

/// The X11 keysym for a key.
///
/// Latin-1 keysyms are their own code point; everything else uses the Unicode keysym
/// range, which is how X11 names a character no standard layout has.
pub fn keysym(key: Key) -> u32 {
    const BACKSPACE: u32 = 0xff08;
    const RETURN: u32 = 0xff0d;
    match key {
        Key::Backspace => BACKSPACE,
        Key::Enter => RETURN,
        Key::Char(ch) => {
            let code = ch as u32;
            if code < 0x100 {
                code
            } else {
                0x0100_0000 + code
            }
        }
    }
}

/// Where an injector gave up. Naming the stage is the difference between "typing failed"
/// and a sentence that says which thing to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Display,
    XTest,
    Keymap,
    Post,
}

impl Stage {
    pub fn message(self) -> &'static str {
        match self {
            Stage::Display => {
                "X11 could not open the display. Is DISPLAY set, and is an X server running?"
            }
            Stage::XTest => "This X server does not offer XTest, so nothing can be injected into it.",
            Stage::Keymap => "X11 had no free keycode to type that character with.",
            Stage::Post => "X11 refused the keystroke.",
        }
    }
}

/* ------------------------------------------------------------------ idle stopping */

/// Stops a run that has accepted nothing for the configured time.
///
/// Time is passed in rather than read here, so the arithmetic is tested without waiting
/// ten seconds per case.
#[derive(Debug, Clone)]
pub struct IdleWatch {
    limit: Option<Duration>,
    last: Instant,
}

impl IdleWatch {
    pub fn new(limit: Option<Duration>, now: Instant) -> Self {
        Self { limit, last: now }
    }

    /// A keystroke landed, which is the only thing that resets the clock.
    pub fn note(&mut self, now: Instant) {
        self.last = now;
    }

    pub fn exceeded(&self, now: Instant) -> bool {
        match self.limit {
            Some(limit) => now.saturating_duration_since(self.last) >= limit,
            None => false,
        }
    }
}

/* ------------------------------------------------------------------ chord arming */

/// The text and settings the chord would type if it started a run. The text lives in the
/// webview, so the view hands it here whenever it changes and no run is live.
#[derive(Debug, Clone)]
pub struct Armed {
    pub text: String,
    pub config: Config,
}

#[derive(Default)]
pub struct ArmedText(Mutex<Option<Armed>>);

/// What the chord does, which depends on whether a run is live and whether anything is
/// armed. Pure, so the whole truth table is tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordAction {
    Stop,
    Start,
    Nothing,
}

pub fn chord_action(running: bool, armed: bool) -> ChordAction {
    if running {
        ChordAction::Stop
    } else if armed {
        ChordAction::Start
    } else {
        ChordAction::Nothing
    }
}

/// What the UI shows before a run: how fast it will be and how long it will take.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInfo {
    pub cps: f64,
    pub total_ms: u64,
    pub steps: u32,
    pub chars: u32,
}

impl From<&Plan> for PlanInfo {
    fn from(plan: &Plan) -> Self {
        Self {
            cps: plan.cps,
            total_ms: plan.total_ms,
            steps: plan.steps.len() as u32,
            chars: plan.steps.iter().filter(|step| matches!(step.action, Action::Char(_))).count() as u32,
        }
    }
}

fn seed_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x5eed)
}

pub fn plan(text: &str, config: &Config) -> Plan {
    plan_with_seed(text, config, seed_now())
}

/// The pace the plan will run at, exposed so the UI can warn before a run rather than
/// after it: a deadline nobody can meet is clamped, not silently overrun.
pub fn pace(chars: usize, config: &Config) -> f64 {
    match config.deadline_secs {
        Some(secs) if chars > 0 => (chars as f64 / secs).clamp(MIN_CPS, MAX_CPS),
        _ => (BASE_CPS * config.speed).clamp(MIN_CPS, MAX_CPS),
    }
}

pub fn plan_with_seed(text: &str, config: &Config, seed: u64) -> Plan {
    let config = config.sanitised();
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Plan { steps: Vec::new(), total_ms: 0, cps: 0.0 };
    }

    let mut cps = pace(chars.len(), &config);
    let mut plan = build(&chars, cps, &config, seed);

    // A deadline is met by pacing the whole run, not by racing at the end. Punctuation
    // dwells and typos take time the character count does not account for, so the pace
    // is re-derived from what the previous pass actually took. Each pass is seeded the
    // same way, which keeps the result deterministic.
    if let Some(secs) = config.deadline_secs {
        let target = (secs * 1000.0).max(1.0);
        for _ in 0..5 {
            let total = plan.total_ms as f64;
            if total <= 0.0 {
                break;
            }
            let next = (cps * total / target).clamp(MIN_CPS, MAX_CPS);
            if (next - cps).abs() < 0.01 {
                break;
            }
            cps = next;
            plan = build(&chars, cps, &config, seed);
        }
    }

    plan.cps = cps;
    plan
}

fn build(chars: &[char], cps: f64, config: &Config, seed: u64) -> Plan {
    let base = 1000.0 / cps;
    let mut rng = StdRng::seed_from_u64(seed);

    let mut steps: Vec<Step> = Vec::with_capacity(chars.len());
    let mut pending = 0.0f64;
    let mut previous: Option<char> = None;

    for ch in chars.iter().copied() {
        let mut delay = base * rng.gen_range(0.75..=1.25);

        // A word starts on a slower key and then runs in a burst: the shape of the
        // rhythm matters more than the average speed.
        let at_word_start = previous.map_or(true, |last| last.is_whitespace() || is_pause(last));
        if at_word_start && !ch.is_whitespace() {
            delay *= 1.35;
        } else if !ch.is_whitespace() {
            delay *= 0.85;
        }

        if let Some(last) = previous {
            delay += dwell(last, &mut rng);
        }
        pending += delay;

        // A typo is corrected on the spot instead of three characters later, because a
        // late correction would mean backspacing through text that is already right.
        // The keystrokes still look human: a wrong letter, a pause, a backspace, the
        // right one.
        if rng.gen::<f64>() < config.typo_rate {
            match substitute(ch, &mut rng) {
                Some(wrong) => {
                    steps.push(Step { wait_ms: pending.round() as u32, action: Action::Char(wrong) });
                    pending = base * rng.gen_range(2.2..=4.0);
                    steps.push(Step { wait_ms: pending.round() as u32, action: Action::Backspace });
                    pending = base * rng.gen_range(0.9..=1.4);
                }
                None if !ch.is_whitespace() => {
                    // Nothing sensible to mistype, so hold the key instead.
                    steps.push(Step { wait_ms: pending.round() as u32, action: Action::Char(ch) });
                    pending = base * rng.gen_range(0.6..=1.0);
                    steps.push(Step { wait_ms: pending.round() as u32, action: Action::Backspace });
                    pending = base * rng.gen_range(0.9..=1.4);
                }
                None => {}
            }
        }

        steps.push(Step { wait_ms: pending.round() as u32, action: Action::Char(ch) });
        pending = 0.0;
        previous = Some(ch);
    }

    let total_ms = steps.iter().map(|step| u64::from(step.wait_ms)).sum();
    Plan { steps, total_ms, cps }
}

fn is_pause(ch: char) -> bool {
    matches!(ch, ',' | ';' | ':' | '.' | '!' | '?')
}

fn dwell(previous: char, rng: &mut StdRng) -> f64 {
    match previous {
        ',' | ';' | ':' => rng.gen_range(90.0..=160.0),
        '.' | '!' | '?' => rng.gen_range(140.0..=240.0),
        '\n' => rng.gen_range(350.0..=500.0),
        _ => 0.0,
    }
}

/// A neighbouring key on a QWERTY board, which is what a real slip looks like.
fn substitute(ch: char, rng: &mut StdRng) -> Option<char> {
    const ROWS: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
    if let Some(row) = ROWS.iter().find(|row| row.contains(ch.to_ascii_lowercase())) {
        let letters: Vec<char> = row.chars().collect();
        let index = letters.iter().position(|letter| *letter == ch.to_ascii_lowercase())?;
        let offset: isize = if rng.gen::<bool>() { 1 } else { -1 };
        let neighbour = index as isize + offset;
        if neighbour < 0 || neighbour as usize >= letters.len() {
            return None;
        }
        let picked = letters[neighbour as usize];
        return Some(if ch.is_uppercase() { picked.to_ascii_uppercase() } else { picked });
    }
    None
}

/* -------------------------------------------------------------------- running */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Idle = 0,
    Typing = 1,
    Paused = 2,
    Done = 3,
    Stopped = 4,
    Blocked = 5,
    /// QuillNot's own window had focus when a keystroke was due, so it would have landed
    /// here rather than in the document the run was aimed at.
    Unfocused = 6,
    /// Nothing was accepted for the configured idle time, so the run stopped itself.
    Stalled = 7,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Typing => "typing",
            Status::Paused => "paused",
            Status::Done => "done",
            Status::Stopped => "stopped",
            Status::Blocked => "blocked",
            Status::Unfocused => "unfocused",
            Status::Stalled => "stalled",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Status::Typing,
            2 => Status::Paused,
            3 => Status::Done,
            4 => Status::Stopped,
            5 => Status::Blocked,
            6 => Status::Unfocused,
            7 => Status::Stalled,
            _ => Status::Idle,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub status: &'static str,
    pub typed: usize,
    pub total: usize,
}

/// Shared with the driver task. Atomics rather than a mutex so a stop request is seen
/// between keystrokes and never waits on a lock the driver is holding.
#[derive(Default)]
struct Control {
    cancel: AtomicBool,
    paused: AtomicBool,
    status: AtomicU8,
    typed: AtomicUsize,
    total: AtomicUsize,
}

impl Control {
    fn payload(&self) -> StatusPayload {
        StatusPayload {
            status: Status::from_u8(self.status.load(Ordering::Relaxed)).as_str(),
            typed: self.typed.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }

    fn set(&self, status: Status) {
        self.status.store(status as u8, Ordering::Relaxed);
    }
}

/// The run in progress, if any. Starting a second one cancels the first.
#[derive(Default)]
pub struct Typer(Mutex<Option<Arc<Control>>>);

fn emit(app: &AppHandle, control: &Control) {
    let _ = app.emit(TYPER_EVENT, control.payload());
}

fn finish(app: &AppHandle, control: &Control, status: Status, typed: usize) {
    control.typed.store(typed, Ordering::Relaxed);
    control.set(status);
    emit(app, control);
}

/// Whether QuillNot's own window has focus.
///
/// A keystroke posted now would land in this app, which is the one window that is never
/// the target: the countdown exists so the person can click elsewhere, and this is what
/// happens when they did not. Cheap enough to ask before every keystroke, and correct
/// on every platform without a second OS API.
fn our_window_is_focused(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
}

async fn drive(
    app: AppHandle,
    control: Arc<Control>,
    steps: Vec<Step>,
    idle_limit: Option<Duration>,
) {
    control.total.store(steps.len(), Ordering::Relaxed);
    control.set(Status::Typing);
    emit(&app, &control);

    let mut idle = IdleWatch::new(idle_limit, Instant::now());

    // The countdown, in slices so a stop lands during it rather than after it.
    let mut waited = 0u64;
    while waited < COUNTDOWN_MS {
        if control.cancel.load(Ordering::Relaxed) {
            finish(&app, &control, Status::Stopped, 0);
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        waited += 100;
    }

    let mut typed = 0usize;
    for step in steps {
        // A pause parks the run where it is. The check on both sides of the sleep is
        // what makes a stop land mid-word rather than at the end of a line.
        while control.paused.load(Ordering::Relaxed) {
            if control.cancel.load(Ordering::Relaxed) {
                finish(&app, &control, Status::Stopped, typed);
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        if control.cancel.load(Ordering::Relaxed) {
            finish(&app, &control, Status::Stopped, typed);
            return;
        }
        // A run that has accepted nothing for the configured time is a run nobody is
        // watching. Checked after the pause loop, so a deliberate pause is not mistaken
        // for a stall.
        if idle.exceeded(Instant::now()) {
            finish(&app, &control, Status::Stalled, typed);
            return;
        }
        if inject::secure_input() {
            finish(&app, &control, Status::Blocked, typed);
            return;
        }
        if step.wait_ms > 0 {
            tokio::time::sleep(Duration::from_millis(u64::from(step.wait_ms))).await;
        }
        if control.cancel.load(Ordering::Relaxed) {
            finish(&app, &control, Status::Stopped, typed);
            return;
        }
        if idle.exceeded(Instant::now()) {
            finish(&app, &control, Status::Stalled, typed);
            return;
        }
        // Checked at the last moment before the key is posted, not at the start: the
        // point of the countdown is that focus changes during it.
        if our_window_is_focused(&app) {
            finish(&app, &control, Status::Unfocused, typed);
            return;
        }
        if inject::apply(step.action).is_err() {
            finish(&app, &control, Status::Blocked, typed);
            return;
        }
        if matches!(step.action, Action::Char(_)) {
            typed += 1;
            control.typed.store(typed, Ordering::Relaxed);
            idle.note(Instant::now());
        }
        if typed % 6 == 0 {
            emit(&app, &control);
        }
    }

    finish(&app, &control, Status::Done, typed);
}

/// Stops whatever is running, for a caller with no `State` to hand: the global shortcut
/// fires from the plugin's own thread, and by the time it is pressed the person is
/// looking at the window the text was going into, not at this app.
pub fn stop_all(app: &AppHandle) {
    let state = app.state::<Typer>();
    let running = state.0.lock().unwrap().as_ref().cloned();
    if let Some(control) = running {
        control.cancel.store(true, Ordering::Relaxed);
        control.paused.store(false, Ordering::Relaxed);
        emit(app, &control);
    }
}

/// The chord used until someone sets their own: whatever the platform calls its command
/// key, plus Alt and Shift, and S.
#[cfg(target_os = "macos")]
pub const DEFAULT_STOP_SHORTCUT: &str = "Super+Alt+Shift+KeyS";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_STOP_SHORTCUT: &str = "Control+Alt+Shift+KeyS";

/// The chord currently registered with the OS, so a change can release the old one
/// instead of leaving two live.
#[derive(Default)]
pub struct StopShortcut(Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>);

fn parse_shortcut(accelerator: &str) -> Result<tauri_plugin_global_shortcut::Shortcut, String> {
    use tauri_plugin_global_shortcut::Shortcut;
    Shortcut::try_from(accelerator)
        .map_err(|err| format!("That is not a shortcut this system accepts: {err}"))
}

/// Registers one chord and remembers it.
fn apply_stop_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let shortcut = parse_shortcut(accelerator)?;
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                chord_fired(&handle);
            }
        })
        .map_err(|err| format!("the shortcut could not be registered: {err}"))?;

    *app.state::<StopShortcut>().0.lock().unwrap() = Some(shortcut);
    Ok(())
}

/// Lets go of the chord before another one takes its place.
fn release_stop_shortcut(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let current = app.state::<StopShortcut>().0.lock().unwrap().take();
    if let Some(shortcut) = current {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

/// The chord this build will use: the person's, or the default.
pub fn configured_shortcut(app: &AppHandle) -> String {
    crate::store::read_settings(app)
        .typer_stop_shortcut
        .unwrap_or_else(|| DEFAULT_STOP_SHORTCUT.to_string())
}

/// Whether a run is live, which is what the chord's stop half needs to know. A finished
/// run leaves its control in the slot, so the status is read rather than the slot.
fn run_is_live(app: &AppHandle) -> bool {
    let state = app.state::<Typer>();
    let guard = state.0.lock().unwrap();
    match guard.as_ref() {
        Some(control) => matches!(control.payload().status, "typing" | "paused"),
        None => false,
    }
}

/// The chord is one button with two halves: it stops a live run, and it starts the armed
/// text when nothing is running. Arming happens in the view, because the text lives there.
///
/// The armed text is kept after a start, so a run that was stopped in its first sentence
/// can be started again with the same chord. It is replaced or dropped when the text
/// changes, so the chord can never type an old draft.
fn chord_fired(app: &AppHandle) {
    let armed = app.state::<ArmedText>().0.lock().unwrap().clone();
    match chord_action(run_is_live(app), armed.is_some()) {
        ChordAction::Stop => stop_all(app),
        ChordAction::Start => {
            if let Some(armed) = armed {
                if let Err(err) = start_run(app, armed.text, armed.config) {
                    eprintln!("typer: the chord could not start a run: {err}");
                }
            }
        }
        ChordAction::Nothing => eprintln!("typer: the chord was pressed with nothing armed"),
    }
}

/// Hands the chord the text it should type. The view calls this whenever the text or its
/// settings change and no run is live; empty text disarms it.
#[tauri::command]
pub fn typer_arm(state: State<'_, ArmedText>, text: String, config: Config) {
    let mut armed = state.0.lock().unwrap();
    *armed = if text.trim().is_empty() {
        None
    } else {
        Some(Armed { text, config })
    };
}

/// Forgets the armed text, so the chord cannot start a run from it.
#[tauri::command]
pub fn typer_disarm(state: State<'_, ArmedText>) {
    *state.0.lock().unwrap() = None;
}

/// Registers the stop chord at startup, best-effort.
///
/// Escape still stops a run while this window has focus; this chord is the one that works
/// when it does not, which is the normal case during a run. Another application may
/// already own the chord, so a refusal falls back to the default rather than leaving the
/// app with no way to stop a run.
pub fn register_stop_shortcut(app: &AppHandle) {
    let wanted = configured_shortcut(app);
    if apply_stop_shortcut(app, &wanted).is_ok() {
        return;
    }

    if wanted != DEFAULT_STOP_SHORTCUT {
        if let Err(err) = apply_stop_shortcut(app, DEFAULT_STOP_SHORTCUT) {
            eprintln!("typer: no stop shortcut could be registered: {err}");
        }
    } else {
        eprintln!("typer: the stop shortcut is already owned by another application");
    }
}

/// The chord the person can see in the menu.
#[tauri::command]
pub fn typer_shortcut(app: AppHandle) -> String {
    configured_shortcut(&app)
}

/// Takes a chord the person just pressed, makes it live, and remembers it.
///
/// The old chord is released first and restored on failure, so a combination the OS
/// refuses (or another app already owns) cannot leave the panic button missing.
#[tauri::command]
pub fn set_typer_shortcut(app: AppHandle, shortcut: String) -> Result<String, String> {
    let shortcut = shortcut.trim().to_string();
    if shortcut.is_empty() {
        return Err("Press a key combination first.".to_string());
    }
    // Refuse nonsense before the live chord is touched.
    parse_shortcut(&shortcut)?;

    let previous = configured_shortcut(&app);
    release_stop_shortcut(&app);

    if let Err(err) = apply_stop_shortcut(&app, &shortcut) {
        let _ = apply_stop_shortcut(&app, &previous);
        return Err(err);
    }

    let mut settings = crate::store::read_settings(&app);
    settings.typer_stop_shortcut = Some(shortcut.clone());
    crate::store::write_settings(&app, &settings)?;
    Ok(shortcut)
}

/* ------------------------------------------------------------------- commands */

#[tauri::command]
pub fn typer_status(state: State<'_, Typer>) -> StatusPayload {
    match state.0.lock().unwrap().as_ref() {
        Some(control) => control.payload(),
        None => StatusPayload { status: "idle", typed: 0, total: 0 },
    }
}

/// The plan without typing anything: the UI calls this as the text changes, so the
/// estimate on screen is the schedule the driver will actually follow.
#[tauri::command]
pub fn typer_plan(text: String, config: Config) -> PlanInfo {
    PlanInfo::from(&plan(&text, &config))
}

/// Starts typing into whatever has focus. Every reason to refuse is checked here, so
/// the UI shows one message instead of a run that silently types nothing.
///
/// The button and the chord's start half both come through here, so they cannot drift
/// apart: arming the chord is not a second, weaker way to run.
pub fn start_run(app: &AppHandle, text: String, config: Config) -> Result<PlanInfo, String> {
    if text.trim().is_empty() {
        return Err("There is nothing to type yet.".into());
    }
    if !inject::supported() {
        return Err(
            "Typer types into other apps through their input system, and this build has \
             no injector for this platform yet. The macOS and Windows builds have one."
                .into(),
        );
    }
    if !inject::trusted() {
        return Err(
            "Typer needs Accessibility permission before it can type into another app. \
             Grant it in System Settings, then try again."
                .into(),
        );
    }
    if inject::secure_input() {
        return Err(
            "A secure input field has focus right now. Typer never types into a password \
             prompt; click back into the document you want to type into."
                .into(),
        );
    }

    let config = config.sanitised();
    let idle_limit = config.idle_secs.map(Duration::from_secs_f64);
    let plan = plan(&text, &config);
    if plan.steps.is_empty() {
        return Err("There is nothing to type yet.".into());
    }

    let control = Arc::new(Control::default());
    let summary = PlanInfo::from(&plan);

    // Replacing the stored run is also how a second start cancels the first: the old
    // driver sees its own cancel flag and stops mid-word.
    let previous = app.state::<Typer>().0.lock().unwrap().replace(control.clone());
    if let Some(previous) = previous {
        previous.cancel.store(true, Ordering::Relaxed);
    }

    let handle = app.clone();
    let driver = control.clone();
    tauri::async_runtime::spawn(async move {
        drive(handle, driver, plan.steps, idle_limit).await;
    });

    Ok(summary)
}

#[tauri::command]
pub fn typer_start(app: AppHandle, text: String, config: Config) -> Result<PlanInfo, String> {
    start_run(&app, text, config)
}

/// What a generated run turned into: the text it types, how much the model added, and the
/// schedule it will follow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    pub text: String,
    pub added: usize,
    pub plan: PlanInfo,
}

/// How many characters a continuation should add so the run meets its deadline.
///
/// Sized at the resting pace, clamped at both ends: a single word wastes a model call, and
/// more than a minute of prose cannot be typed into a short deadline at any human speed.
pub fn continuation_target(remaining_secs: f64) -> usize {
    const LEAST: f64 = 60.0;
    const MOST: f64 = 1200.0;
    (remaining_secs.max(0.0) * BASE_CPS).clamp(LEAST, MOST).round() as usize
}

/// The text with a continuation appended: one space at the join, no fence, no preamble.
/// An empty continuation leaves the text exactly as it was.
pub fn join_continuation(original: &str, continuation: &str) -> String {
    let cleaned = strip_continuation(continuation);
    if cleaned.is_empty() {
        return original.to_string();
    }
    let mut joined = original.trim_end().to_string();
    if !joined.is_empty() {
        joined.push(' ');
    }
    joined.push_str(&cleaned);
    joined
}

/// Models like a fence or a "Continuation:" label even when told not to. Both are removed
/// here rather than asked for more firmly, because one of them will still arrive one day.
fn strip_continuation(raw: &str) -> String {
    let mut body = raw.trim();
    if let Some(rest) = body.strip_prefix("```") {
        body = rest.trim_start_matches(|ch: char| ch.is_ascii_alphabetic()).trim_start();
        body = body.strip_suffix("```").unwrap_or(body);
    } else if let Some(rest) = body.strip_suffix("```") {
        body = rest;
    }
    let body = body.trim();
    let body = body.strip_prefix("Continuation:").unwrap_or(body);
    body.trim().to_string()
}

/// The generated half of "finish by": the model writes the rest of the text to the length
/// the remaining time allows, and the run starts on the joined result.
///
/// The run starts from the joined text rather than only the continuation, because the
/// injected typing has to produce a whole document: a continuation sent on its own would
/// land after whatever the target already contains.
#[tauri::command]
pub async fn typer_generate(
    app: AppHandle,
    text: String,
    config: Config,
) -> Result<Generated, String> {
    if text.trim().is_empty() {
        return Err("There is nothing to continue yet.".into());
    }

    let config = config.sanitised();
    let Some(deadline) = config.deadline_secs else {
        return Err(
            "Generated typing works towards a \"finish by\" deadline; set one first.".into(),
        );
    };

    // What the existing text already costs at a resting pace, so the model is asked for
    // the time that is actually left rather than the whole deadline.
    let resting = Config { deadline_secs: None, ..config.clone() };
    let already = plan(&text, &resting).total_ms as f64 / 1000.0;
    let target = continuation_target((deadline - already).max(1.0));

    let route = providers::resolve(&app, None, samples::PARAPHRASE)?;
    let (system, user) = prompt::continuation_prompt(&text, target);
    let (reply, _model) = openrouter::complete(
        Some(&app),
        samples::PARAPHRASE,
        &route.attempts,
        &system,
        &user,
    )
    .await?;

    let joined = join_continuation(&text, &reply);
    let added = joined.chars().count().saturating_sub(text.chars().count());
    if added == 0 {
        return Err(
            "The model did not add anything to type. Try again, or write the ending yourself."
                .into(),
        );
    }

    let plan = start_run(&app, joined.clone(), config)?;
    Ok(Generated { text: joined, added, plan })
}

#[tauri::command]
pub fn typer_pause(app: AppHandle, state: State<'_, Typer>) -> StatusPayload {
    if let Some(control) = state.0.lock().unwrap().as_ref() {
        control.paused.store(true, Ordering::Relaxed);
        control.set(Status::Paused);
        emit(&app, control);
        return control.payload();
    }
    StatusPayload { status: "idle", typed: 0, total: 0 }
}

#[tauri::command]
pub fn typer_resume(app: AppHandle, state: State<'_, Typer>) -> StatusPayload {
    if let Some(control) = state.0.lock().unwrap().as_ref() {
        control.paused.store(false, Ordering::Relaxed);
        control.set(Status::Typing);
        emit(&app, control);
        return control.payload();
    }
    StatusPayload { status: "idle", typed: 0, total: 0 }
}

#[tauri::command]
pub fn typer_stop(app: AppHandle, state: State<'_, Typer>) -> StatusPayload {
    if let Some(control) = state.0.lock().unwrap().as_ref() {
        control.cancel.store(true, Ordering::Relaxed);
        control.paused.store(false, Ordering::Relaxed);
        emit(&app, control);
        return control.payload();
    }
    StatusPayload { status: "idle", typed: 0, total: 0 }
}

/// What the Typer menu has to know, as three separate facts rather than one yes/no.
///
/// `supported` is whether this platform has an injector at all; `granted` is whether the
/// person has given that injector the permission it needs; `platform` is the OS itself,
/// so the menu can show the macOS Accessibility steps on macOS and a plain sentence
/// somewhere else instead of directions that do not exist there.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    pub supported: bool,
    pub granted: bool,
    pub platform: &'static str,
}

/// Whether this build can type at all, and separately whether it has been allowed to.
#[tauri::command]
pub fn typer_accessibility() -> Access {
    Access {
        supported: inject::supported(),
        granted: inject::trusted(),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
pub fn typer_open_settings() -> Result<(), String> {
    inject::request()
}

/* -------------------------------------------------------------------- injector */

#[cfg(target_os = "macos")]
mod inject {
    //! macOS: Core Graphics events, with the character attached as a Unicode string so
    //! no keyboard layout has to be guessed. Requires Accessibility permission, which
    //! is the gatekeeper for posting events at all.

    use std::ffi::c_void;
    use std::ptr;

    use super::Action;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(source: *mut c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, text: *const u16);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: *const c_void);
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn IsSecureEventInputEnabled() -> u8;
    }

    /// kVK_Delete, the backspace key, and kVK_Return.
    const KEY_DELETE: u16 = 51;
    const KEY_RETURN: u16 = 36;
    const HID_EVENT_TAP: u32 = 0;

    /// Whether this platform has an injector at all. macOS does.
    pub fn supported() -> bool {
        true
    }

    pub fn trusted() -> bool {
        unsafe { AXIsProcessTrusted() == 1 }
    }

    pub fn secure_input() -> bool {
        unsafe { IsSecureEventInputEnabled() == 1 }
    }

    /// Opens the pane where the app is granted Accessibility. The system's own prompt
    /// cannot add an app to that list, so the pane is the honest destination: it is
    /// the same place the prompt sends people.
    pub fn request() -> Result<(), String> {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status()
            .map_err(|error| format!("System Settings could not be opened: {error}"))?;
        Ok(())
    }

    pub fn apply(action: Action) -> Result<(), String> {
        match action {
            // A newline is Return, not a character: most editors ignore a literal \n
            // in a key event.
            Action::Char('\n') => press(KEY_RETURN),
            Action::Char(ch) => character(ch),
            Action::Backspace => press(KEY_DELETE),
        }
    }

    fn character(ch: char) -> Result<(), String> {
        let mut buffer = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buffer);
        unsafe {
            let event = CGEventCreateKeyboardEvent(ptr::null_mut(), 0, true);
            if event.is_null() {
                return Err("macOS refused to create a keyboard event.".into());
            }
            CGEventKeyboardSetUnicodeString(event, encoded.len(), encoded.as_ptr());
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event as *const c_void);
        }
        Ok(())
    }

    fn press(key: u16) -> Result<(), String> {
        unsafe {
            for down in [true, false] {
                let event = CGEventCreateKeyboardEvent(ptr::null_mut(), key, down);
                if event.is_null() {
                    return Err("macOS refused to create a keyboard event.".into());
                }
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event as *const c_void);
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod inject {
    //! Windows: `SendInput` with `KEYEVENTF_UNICODE`.
    //!
    //! The character travels as its UTF-16 code unit, not as a virtual key, so the active
    //! keyboard layout cannot turn an `é` into something else — the same reason the macOS
    //! injector attaches a string to its event instead of guessing a key code. A character
    //! outside the BMP is two units, sent in order, which is what a real keyboard does.
    //!
    //! There is no permission to grant. A desktop process may synthesise input at any
    //! time; the one gate is the secure desktop, where UAC collects a password and the
    //! system ignores everything posted to it.

    use std::mem::size_of;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VIRTUAL_KEY, VK_BACK, VK_RETURN,
    };

    use super::Action;

    pub fn supported() -> bool {
        true
    }

    pub fn trusted() -> bool {
        true
    }

    /// Windows has no equivalent of `IsSecureEventInputEnabled`.
    ///
    /// The macOS build can ask the OS whether a password prompt holds the keyboard; there
    /// is no such query here, so a password field is only protected by the system ignoring
    /// input on the secure desktop. That gap is named here rather than papered over.
    pub fn secure_input() -> bool {
        false
    }

    pub fn request() -> Result<(), String> {
        // Nothing to open: there is no permission for this platform.
        Ok(())
    }

    pub fn apply(action: Action) -> Result<(), String> {
        match action {
            // A newline is Return, not a character: most editors ignore a literal \n typed
            // as text.
            Action::Char('\n') => key(VK_RETURN, 0, 0),
            Action::Char(ch) => character(ch),
            Action::Backspace => key(VK_BACK, 0, 0),
        }
    }

    fn character(ch: char) -> Result<(), String> {
        for unit in units(ch) {
            key(0, unit, KEYEVENTF_UNICODE)?;
        }
        Ok(())
    }

    /// The code units one character types, in order.
    fn units(ch: char) -> Vec<u16> {
        let mut buffer = [0u16; 2];
        ch.encode_utf16(&mut buffer).iter().copied().collect()
    }

    /// One press and release. `scan` carries the UTF-16 unit when `flags` asks for a
    /// Unicode keystroke, and is ignored otherwise.
    fn key(vk: VIRTUAL_KEY, scan: u16, flags: u32) -> Result<(), String> {
        for release in [0, KEYEVENTF_KEYUP] {
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: scan,
                        dwFlags: flags | release,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };

            let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
            if sent != 1 {
                return Err("Windows refused the keystroke.".to_string());
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn this_build_can_type_and_needs_no_permission() {
            assert!(supported());
            assert!(trusted());
        }

        #[test]
        fn a_character_is_its_utf16_code_units() {
            assert_eq!(units('a'), vec![0x0061]);
            assert_eq!(units('é'), vec![0x00e9]);
            // Outside the BMP: a surrogate pair, in the order the target expects.
            assert_eq!(units('𝄞'), vec![0xd834, 0xdd1e]);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod inject {
    //! X11: XTest, with a spare keycode remapped to the wanted keysym for anything the
    //! current layout cannot already type.
    //!
    //! The untestable edge is deliberately thin and named: [`Server`] is the only thing
    //! that talks to the X server. Everything above it — which keysym a character becomes,
    //! which key a newline becomes, which stage failed and what to say about it — is pure
    //! and covered by tests that run with no display.
    //!
    //! X11 has no Accessibility permission to ask for. The desktop's own lock screen is
    //! what stops injected input, and this build has no way to query it, which is said
    //! plainly rather than implied by a "granted" that is always true.

    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use x11_dl::xlib;
    use x11_dl::xtest;

    use super::{key_for, keysym, Action, Key, Stage};

    /// The keysym that means "nothing here", which is what a keycode is restored to.
    const NO_SYMBOL: xlib::KeySym = 0;

    pub fn supported() -> bool {
        true
    }

    pub fn trusted() -> bool {
        true
    }

    /// X11 has no equivalent of `IsSecureEventInputEnabled`. Said so rather than answered
    /// with a `false` that reads like a check.
    pub fn secure_input() -> bool {
        false
    }

    pub fn request() -> Result<(), String> {
        Err("X11 has no Accessibility permission to grant. Typer needs an X server with \
             XTest, which every normal desktop has."
            .into())
    }

    pub fn apply(action: Action) -> Result<(), String> {
        let key = key_for(action);
        with_server(|server| server.press(key))
    }

    /// Runs one keystroke against the single shared display connection, opening it on
    /// first use. One mutex means two keystrokes cannot interleave their keymap changes.
    fn with_server<T>(body: impl FnOnce(&Server) -> Result<T, Stage>) -> Result<T, String> {
        static SERVER: OnceLock<Mutex<Option<Server>>> = OnceLock::new();
        let slot = SERVER.get_or_init(|| Mutex::new(None));
        let mut guard = slot
            .lock()
            .map_err(|_| "the X11 connection was lost".to_string())?;
        if guard.is_none() {
            *guard = Some(Server::open()?);
        }
        let server = guard.as_ref().expect("just opened");
        body(server).map_err(|stage| stage.message().to_string())
    }

    /// The whole untestable surface: a display pointer and the three X calls that use it.
    struct Server {
        xlib: xlib::Xlib,
        xtest: xtest::XTest,
        display: *mut xlib::Display,
    }

    // The pointer is only ever used while `with_server` holds its mutex, so moving it
    // between threads is safe even though the raw pointer says otherwise.
    unsafe impl Send for Server {}

    impl Server {
        fn open() -> Result<Self, String> {
            let xlib = xlib::Xlib::open().map_err(|_| Stage::Display.message().to_string())?;
            let xtest = xtest::XTest::open().map_err(|_| Stage::XTest.message().to_string())?;
            let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
            if display.is_null() {
                return Err(Stage::Display.message().to_string());
            }
            Ok(Self { xlib, xtest, display })
        }

        /// One press and release of `key`.
        fn press(&self, key: Key) -> Result<(), Stage> {
            let wanted = xlib::KeySym::from(keysym(key));

            // A key the layout already has needs no remapping, which is the common case
            // for the Latin text these documents are written in.
            let mapped = unsafe { (self.xlib.XKeysymToKeycode)(self.display, wanted) };
            if mapped != 0 {
                return self.tap(mapped);
            }

            // Otherwise borrow the highest keycode for one keystroke and put it back, so a
            // character no layout has can still be typed without changing the keyboard.
            let spare = self.spare_keycode().ok_or(Stage::Keymap)?;
            self.remap(spare, wanted)?;
            let tapped = self.tap(spare);
            self.remap(spare, NO_SYMBOL)?;
            tapped
        }

        /// Press, release, flush. A zero return from either event means the server refused
        /// it, which is the only failure XTest reports.
        fn tap(&self, keycode: u8) -> Result<(), Stage> {
            let code = u32::from(keycode);
            let pressed = unsafe { (self.xtest.XTestFakeKeyEvent)(self.display, code, 1, 0) };
            let released = unsafe { (self.xtest.XTestFakeKeyEvent)(self.display, code, 0, 0) };
            unsafe {
                (self.xlib.XFlush)(self.display);
            }
            if pressed == 0 || released == 0 {
                return Err(Stage::Post);
            }
            Ok(())
        }

        /// The highest keycode the server reports is the conventional spare: layouts use
        /// the low ones, and the top is left for exactly this kind of remapping.
        fn spare_keycode(&self) -> Option<u8> {
            let mut min = 0i32;
            let mut max = 0i32;
            unsafe {
                (self.xlib.XDisplayKeycodes)(self.display, &mut min, &mut max);
            }
            if max <= min || max <= 0 || max > 255 {
                return None;
            }
            Some(max as u8)
        }

        fn remap(&self, keycode: u8, symbol: xlib::KeySym) -> Result<(), Stage> {
            let symbols = [symbol];
            unsafe {
                (self.xlib.XChangeKeyboardMapping)(
                    self.display,
                    i32::from(keycode),
                    1,
                    symbols.as_ptr(),
                    1,
                );
                (self.xlib.XSync)(self.display, 0);
            }
            // The server applies a mapping asynchronously; a keystroke sent in the same
            // round trip would use the old symbol.
            std::thread::sleep(Duration::from_millis(5));
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn this_build_has_an_injector_and_no_permission_to_ask_for() {
            assert!(supported());
            assert!(trusted());
        }

        #[test]
        fn a_newline_is_the_return_keysym() {
            assert_eq!(keysym(key_for(Action::Char('\n'))), 0xff0d);
            assert_eq!(keysym(key_for(Action::Backspace)), 0xff08);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the target app would show, keystroke by keystroke.
    fn rendered(plan: &Plan) -> String {
        let mut out = String::new();
        for step in &plan.steps {
            match step.action {
                Action::Char(ch) => out.push(ch),
                Action::Backspace => {
                    out.pop();
                }
            }
        }
        out
    }

    const SAMPLE: &str = "The release notes claim the install works, but nobody ran it on a \
                          clean Mac. That is the sort of thing a second pair of eyes catches.";

    #[test]
    fn what_lands_on_screen_is_the_text_even_with_typos() {
        for seed in 0..40 {
            let config = Config { typo_rate: 0.25, ..Config::default() };
            let plan = plan_with_seed(SAMPLE, &config, seed);
            assert_eq!(rendered(&plan), SAMPLE, "seed {seed}");
        }
    }

    #[test]
    fn no_typos_means_one_keystroke_per_character() {
        let config = Config { typo_rate: 0.0, ..Config::default() };
        let plan = plan_with_seed(SAMPLE, &config, 7);
        assert_eq!(plan.steps.len(), SAMPLE.chars().count());
        assert!(plan.steps.iter().all(|step| step.action != Action::Backspace));
    }

    #[test]
    fn a_deadline_is_met_within_two_seconds() {
        let text = "word ".repeat(120); // 600 characters
        let config = Config { deadline_secs: Some(45.0), ..Config::default() };
        let plan = plan_with_seed(&text, &config, 3);
        let target = 45_000i64;
        let drift = (plan.total_ms as i64 - target).abs();
        assert!(drift <= 2_000, "landed at {} ms, target {target} ms", plan.total_ms);
    }

    #[test]
    fn an_impossible_deadline_is_clamped_instead_of_racing() {
        let text = "a".repeat(2000);
        let config = Config { deadline_secs: Some(5.0), ..Config::default() };
        let plan = plan_with_seed(&text, &config, 11);
        assert!((plan.cps - MAX_CPS).abs() < f64::EPSILON);
        assert!(plan.total_ms > 5_000, "2000 characters cannot be typed in 5 s at any human speed");
    }

    #[test]
    fn a_faster_setting_types_faster() {
        let slow = plan_with_seed(SAMPLE, &Config { speed: 0.5, ..Config::default() }, 5);
        let quick = plan_with_seed(SAMPLE, &Config { speed: 2.0, ..Config::default() }, 5);
        assert!(quick.total_ms * 2 < slow.total_ms, "{} vs {}", quick.total_ms, slow.total_ms);
    }

    #[test]
    fn punctuation_earns_a_pause() {
        let config = Config { typo_rate: 0.0, ..Config::default() };
        let plan = plan_with_seed("a.\nb", &config, 1);
        // The step after a full stop and the step after a line break both carry a dwell,
        // so at least one wait is far longer than a resting keystroke.
        let base = 1000.0 / plan.cps;
        assert!(plan.steps.iter().any(|step| f64::from(step.wait_ms) > base * 2.5));
    }

    #[test]
    fn the_same_seed_repeats_the_same_rhythm() {
        let config = Config { typo_rate: 0.1, ..Config::default() };
        let first = plan_with_seed(SAMPLE, &config, 99);
        let second = plan_with_seed(SAMPLE, &config, 99);
        assert_eq!(first.steps, second.steps);
    }

    #[test]
    fn every_step_waits_and_nothing_waits_forever() {
        let plan = plan_with_seed(SAMPLE, &Config::default(), 2);
        assert!(plan.steps.iter().all(|step| (1..=2000).contains(&step.wait_ms)));
    }

    #[test]
    fn the_plan_survives_emptiness_and_newlines() {
        assert!(plan_with_seed("", &Config::default(), 1).steps.is_empty());
        let plan = plan_with_seed("\n\n", &Config::default(), 1);
        assert_eq!(rendered(&plan), "\n\n");
    }

    #[test]
    fn a_deadline_wins_over_the_speed_multiplier() {
        let paced = Config { deadline_secs: Some(30.0), speed: 3.0, ..Config::default() };
        let chars = SAMPLE.chars().count();
        assert!((pace(chars, &paced) - chars as f64 / 30.0).abs() < 0.001);
    }

    #[test]
    fn the_chord_starts_when_nothing_runs_and_stops_when_something_does() {
        assert_eq!(chord_action(true, true), ChordAction::Stop);
        assert_eq!(chord_action(true, false), ChordAction::Stop);
        assert_eq!(chord_action(false, true), ChordAction::Start);
        assert_eq!(chord_action(false, false), ChordAction::Nothing);
    }

    #[test]
    fn idle_time_runs_from_the_last_accepted_keystroke() {
        let start = Instant::now();
        let mut watch = IdleWatch::new(Some(Duration::from_secs(10)), start);
        assert!(!watch.exceeded(start + Duration::from_secs(9)));
        assert!(watch.exceeded(start + Duration::from_secs(10)));
        watch.note(start + Duration::from_secs(9));
        assert!(!watch.exceeded(start + Duration::from_secs(15)), "a keystroke resets it");
        assert!(watch.exceeded(start + Duration::from_secs(19)));
    }

    #[test]
    fn no_idle_limit_never_stops_a_run() {
        let watch = IdleWatch::new(None, Instant::now());
        assert!(!watch.exceeded(Instant::now() + Duration::from_secs(3_600)));
    }

    #[test]
    fn an_idle_limit_too_short_to_beat_the_dwells_is_refused() {
        let rushed = Config { idle_secs: Some(1.0), ..Config::default() }.sanitised();
        assert_eq!(rushed.idle_secs, None);
        let long = Config { idle_secs: Some(9_999.0), ..Config::default() }.sanitised();
        assert_eq!(long.idle_secs, Some(600.0));
    }

    #[test]
    fn a_newline_is_the_return_key_not_a_character() {
        assert_eq!(key_for(Action::Char('\n')), Key::Enter);
        assert_eq!(key_for(Action::Char('a')), Key::Char('a'));
        assert_eq!(key_for(Action::Backspace), Key::Backspace);
    }

    #[test]
    fn keysyms_are_themselves_in_latin_1_and_unicode_beyond_it() {
        assert_eq!(keysym(Key::Char('A')), 0x41);
        assert_eq!(keysym(Key::Char('é')), 0xe9);
        assert_eq!(keysym(Key::Char('𝄞')), 0x0100_0000 + 0x1d11e);
        // A newline only becomes Return in the action mapping; the raw character keeps
        // its own keysym, which is what makes the mapping the place to look for it.
        assert_eq!(keysym(key_for(Action::Char('\n'))), keysym(Key::Enter));
        assert_eq!(keysym(Key::Backspace), 0xff08);
        assert_eq!(keysym(Key::Enter), 0xff0d);
        assert_ne!(keysym(Key::Char('a')), keysym(Key::Char('b')));
        assert_ne!(keysym(Key::Char(' ')), keysym(Key::Enter));
    }

    #[test]
    fn every_injector_stage_names_a_different_failure() {
        let stages = [Stage::Display, Stage::XTest, Stage::Keymap, Stage::Post];
        for (index, stage) in stages.iter().enumerate() {
            assert!(!stage.message().is_empty());
            for earlier in &stages[..index] {
                assert_ne!(stage.message(), earlier.message(), "two stages share a message");
            }
        }
    }

    #[test]
    fn a_continuation_is_sized_from_the_time_left() {
        assert_eq!(continuation_target(0.0), 60, "a shorter answer is not worth a call");
        assert_eq!(continuation_target(100.0), 700, "100 s at the resting pace");
        assert_eq!(continuation_target(600.0), 1200, "capped so the deadline stays reachable");
    }

    #[test]
    fn a_continuation_is_joined_without_repeating_the_text() {
        assert_eq!(join_continuation("The release", "notes were late."), "The release notes were late.");
        assert_eq!(
            join_continuation("Ends with a space ", "```\nmore text\n```"),
            "Ends with a space more text"
        );
        assert_eq!(join_continuation("Same", "   "), "Same");
        assert_eq!(join_continuation("Same", "Continuation: more"), "Same more");
        assert_eq!(join_continuation("", "Fresh start"), "Fresh start");
    }
}
