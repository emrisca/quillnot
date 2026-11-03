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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

/// `src/lib/typer.ts` listens for exactly this string.
pub const TYPER_EVENT: &str = "typer:state";

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
}

impl Default for Config {
    fn default() -> Self {
        Self { typo_rate: 0.04, speed: 1.0, deadline_secs: None }
    }
}

impl Config {
    fn sanitised(&self) -> Self {
        Self {
            typo_rate: self.typo_rate.clamp(0.0, 0.25),
            speed: self.speed.clamp(0.25, 3.0),
            deadline_secs: self.deadline_secs.filter(|secs| *secs >= 1.0).map(|secs| secs.min(3600.0)),
        }
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
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Status::Typing,
            2 => Status::Paused,
            3 => Status::Done,
            4 => Status::Stopped,
            5 => Status::Blocked,
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

async fn drive(app: AppHandle, control: Arc<Control>, steps: Vec<Step>) {
    control.total.store(steps.len(), Ordering::Relaxed);
    control.set(Status::Typing);
    emit(&app, &control);

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
        if inject::apply(step.action).is_err() {
            finish(&app, &control, Status::Blocked, typed);
            return;
        }
        if matches!(step.action, Action::Char(_)) {
            typed += 1;
            control.typed.store(typed, Ordering::Relaxed);
        }
        if typed % 6 == 0 {
            emit(&app, &control);
        }
    }

    finish(&app, &control, Status::Done, typed);
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
#[tauri::command]
pub fn typer_start(
    app: AppHandle,
    state: State<'_, Typer>,
    text: String,
    config: Config,
) -> Result<PlanInfo, String> {
    if text.trim().is_empty() {
        return Err("There is nothing to type yet.".into());
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

    let plan = plan(&text, &config);
    if plan.steps.is_empty() {
        return Err("There is nothing to type yet.".into());
    }

    let control = Arc::new(Control::default());
    let summary = PlanInfo::from(&plan);

    // Replacing the stored run is also how a second start cancels the first: the old
    // driver sees its own cancel flag and stops mid-word.
    let previous = state.0.lock().unwrap().replace(control.clone());
    if let Some(previous) = previous {
        previous.cancel.store(true, Ordering::Relaxed);
    }

    let handle = app.clone();
    let driver = control.clone();
    tauri::async_runtime::spawn(async move {
        drive(handle, driver, plan.steps).await;
    });

    Ok(summary)
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

/// Whether this build can type at all, which is a different question from whether the
/// person granted permission: a Windows build reports `false` here.
#[tauri::command]
pub fn typer_accessibility() -> bool {
    inject::trusted()
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

#[cfg(not(target_os = "macos"))]
mod inject {
    //! Everything else, for now. The plan's Windows and X11 injectors are separate
    //! work; reporting that plainly beats a menu entry that does nothing.

    use super::Action;

    pub fn trusted() -> bool {
        false
    }

    pub fn secure_input() -> bool {
        false
    }

    pub fn request() -> Result<(), String> {
        Err("Typer types through macOS Accessibility, which this build does not have.".into())
    }

    pub fn apply(_action: Action) -> Result<(), String> {
        Err("Typer is macOS-only in this build.".into())
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
}
