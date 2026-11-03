import { useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import { isDesktop } from "../lib/openrouter";
import { countWords } from "../lib/text";
import {
  onTyperState,
  typerAccessibility,
  typerArm,
  typerDisarm,
  typerGenerate,
  typerOpenSettings,
  typerPause,
  typerPlan,
  typerResume,
  typerStart,
  typerStatus,
  typerStop,
  typerShortcut,
  setTyperShortcut,
  type TyperAccess,
  type TyperConfig,
  type TyperPlan,
  type TyperState,
} from "../lib/typer";

const SAMPLE =
  "We shipped the update on a Friday, which everyone says not to do. It broke the one " +
  "thing nobody tested, and the release notes still claimed it worked. Next time the " +
  "checklist runs before the tag, not after it.";

const IDLE: TyperState = { status: "idle", typed: 0, total: 0 };

/** The OS names a person recognizes, for the copy that has to change with the platform. */
const PLATFORM_NAMES: Record<string, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

const platformName = (id: string) => PLATFORM_NAMES[id] ?? "this system";

/**
 * The chord spelled the way the platform's keyboard spells it. The accelerator arrives in
 * the plugin's spelling ("Control+Alt+Shift+KeyS"), which is not something to put on screen.
 */
function prettyChord(accelerator: string, platform: string | undefined): string {
  const mac = platform === "macos";
  const parts = accelerator.split("+").filter(Boolean);
  if (parts.length === 0) return mac ? "\u2318\u2325\u21e7S" : "Ctrl+Alt+Shift+S";
  return parts
    .map((part) => {
      if (part === "Super") return mac ? "\u2318" : "Win";
      if (part === "Control") return mac ? "\u2303" : "Ctrl";
      if (part === "Alt") return mac ? "\u2325" : "Alt";
      if (part === "Shift") return mac ? "\u21e7" : "Shift";
      if (part.startsWith("Key")) return part.slice(3);
      if (part.startsWith("Digit")) return part.slice(5);
      if (part.startsWith("Arrow")) return part.slice(5);
      return part;
    })
    .join(mac ? "" : "+");
}

/** What the shortcut defaults to before anyone has changed it, per platform. */
const defaultAccelerator = (platform: string | undefined) =>
  platform === "macos" ? "Super+Alt+Shift+KeyS" : "Control+Alt+Shift+KeyS";

const LABELS: Record<TyperState["status"], string> = {
  idle: "Ready",
  typing: "Typing",
  paused: "Paused",
  done: "Finished",
  stopped: "Stopped",
  blocked: "Stopped: a secure field has focus",
  unfocused: "Stopped: QuillNot had focus",
  stalled: "Stopped: nothing was accepted",
};

export function TyperView() {
  const [text, setText] = useState("");
  // Held in the view rather than in settings: these are per-run decisions, and a typo
  // rate that survived a restart would be a surprise on the next run.
  const [typoRate, setTypoRate] = useState(0.04);
  const [speed, setSpeed] = useState(1);
  const [deadlineOn, setDeadlineOn] = useState(false);
  const [deadline, setDeadline] = useState(45);
  const [idleOn, setIdleOn] = useState(false);
  const [idleSeconds, setIdleSeconds] = useState(30);
  const [state, setState] = useState<TyperState>(IDLE);
  const [plan, setPlan] = useState<TyperPlan | null>(null);
  // The stop chord as the OS has it, and whether the next keypress is being captured as a
  // new one. Capturing listens on the window in the capture phase, so the chord is read
  // before anything on the page can use the key for itself.
  const [chord, setChord] = useState("");
  const [capturing, setCapturing] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  // When the first keystroke landed, for the words-per-minute readout. Start is not the
  // moment to count from: the three-second countdown would be measured as silence.
  const [firstKeyAt, setFirstKeyAt] = useState<number | null>(null);
  const [, setTick] = useState(0);
  // Whether this platform can type at all, and whether it has been allowed to: two
  // answers that lead to two different menus, so they are kept apart.
  const [access, setAccess] = useState<TyperAccess | null>(null);
  const [error, setError] = useState<string | null>(null);

  const desktop = isDesktop();
  const supported = access?.supported ?? false;
  const needsPermission = desktop && access?.supported === true && !access.granted;
  const running = state.status === "typing" || state.status === "paused";
  const words = countWords(text);
  const config: TyperConfig = useMemo(
    () => ({
      typoRate,
      speed,
      deadlineSecs: deadlineOn ? deadline : null,
      idleSecs: idleOn ? idleSeconds : null,
    }),
    [typoRate, speed, deadlineOn, deadline, idleOn, idleSeconds],
  );

  useEffect(() => {
    if (!desktop) return;
    void typerAccessibility()
      .then(setAccess)
      .catch(() => setAccess({ supported: false, granted: false, platform: "" }));
    void typerStatus()
      .then(setState)
      .catch(() => setState(IDLE));
    void typerShortcut()
      .then(setChord)
      .catch(() => setChord(""));

    let unlisten: (() => void) | undefined;
    void onTyperState((next) => setState(next)).then((off) => {
      unlisten = off;
    });
    return () => unlisten?.();
  }, [desktop]);

  // The chord's start half needs the text, which lives here. Armed on every change while
  // nothing is running, so the chord can never type an old draft; disarmed when the
  // editor is emptied.
  useEffect(() => {
    if (!desktop || running) return;
    if (!text.trim()) {
      void typerDisarm();
      return;
    }
    const id = window.setTimeout(() => void typerArm(text, config), 400);
    return () => window.clearTimeout(id);
  }, [desktop, running, text, config]);

  // The estimate is the plan itself, so the numbers on screen before a run are the ones
  // the driver will follow. It is cheap: no network, no injection, just arithmetic.
  useEffect(() => {
    if (!desktop || !text.trim()) {
      setPlan(null);
      return;
    }
    let live = true;
    void typerPlan(text, config)
      .then((next) => {
        if (live) setPlan(next);
      })
      .catch(() => {
        if (live) setPlan(null);
      });
    return () => {
      live = false;
    };
  }, [text, config, desktop]);

  async function start() {
    setError(null);
    setNote(null);
    setFirstKeyAt(null);
    try {
      const summary = await typerStart(text, config);
      setPlan(summary);
      setAccess((current) => (current ? { ...current, granted: true } : current));
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      if (message.toLowerCase().includes("accessibility")) {
        setAccess((current) => (current ? { ...current, granted: false } : current));
      }
    }
  }

  // "Finish by" with the ending written for you: the model continues the text to the
  // length the deadline leaves room for, and the run starts on the joined result.
  async function generate() {
    setError(null);
    setNote(null);
    setFirstKeyAt(null);
    try {
      const result = await typerGenerate(text, config);
      setText(result.text);
      setPlan(result.plan);
      setNote(`The model wrote ${result.added} more characters and the run has started.`);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  // WPM counts five characters to a word, from the first keystroke that landed. The clock
  // is the wall clock on purpose: pausing a run should lower the average, because that is
  // what the person watching another window experienced.
  useEffect(() => {
    if (state.typed > 0 && firstKeyAt === null) setFirstKeyAt(Date.now());
    if (firstKeyAt !== null && (state.status === "idle" || (state.status === "typing" && state.typed === 0))) {
      setFirstKeyAt(null);
    }
  }, [state.typed, state.status, firstKeyAt]);

  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => setTick((count) => count + 1), 500);
    return () => window.clearInterval(id);
  }, [running]);

  const wpm =
    firstKeyAt !== null && state.typed > 0
      ? Math.round(state.typed / 5 / ((Date.now() - firstKeyAt) / 60000))
      : 0;

  // Capturing the new chord. A modifier on its own is not a chord, and a chord with no
  // modifier would swallow that key in every application, so both are refused with a
  // sentence instead of silently doing nothing.
  useEffect(() => {
    if (!capturing) return;
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setCapturing(false);
        setNote(null);
        return;
      }
      const code = event.code;
      if (!code || /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(code)) return;
      const parts: string[] = [];
      if (event.metaKey) parts.push("Super");
      if (event.ctrlKey) parts.push("Control");
      if (event.altKey) parts.push("Alt");
      if (event.shiftKey) parts.push("Shift");
      if (parts.length === 0) {
        setError("A global shortcut needs at least one modifier, or it would eat that key everywhere.");
        return;
      }
      setCapturing(false);
      setError(null);
      void setTyperShortcut([...parts, code].join("+"))
        .then((saved) => {
          setChord(saved);
          setNote(`Stop chord is now ${prettyChord(saved, access?.platform)}.`);
        })
        .catch((caught) => {
          setError(caught instanceof Error ? caught.message : String(caught));
        });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, access?.platform]);

  const chordText = prettyChord(chord || defaultAccelerator(access?.platform), access?.platform);

  // Escape is the panic button and always works, because the person reaching for it is
  // usually watching the wrong window.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !running || capturing) return;
      event.preventDefault();
      void typerStop().then(setState);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [running, capturing]);

  const percent = state.total > 0 ? Math.min(100, Math.round((state.typed / state.total) * 100)) : 0;
  const seconds = plan ? plan.totalMs / 1000 : 0;

  return (
    <div className="workspace">
      <div className="workspace__body">
        <div className="modebar">
          <span className="modebar__static">Typer</span>
          <span className="modebar__note">
            Types into whichever window has focus, one keystroke at a time, at a pace a
            person could keep up with.
          </span>
        </div>

        <div className="panes">
          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Text to type</span>
              <div className="pane__actions">
                {text ? null : (
                  <button type="button" className="linkbtn" onClick={() => setText(SAMPLE)}>
                    Use a sample
                  </button>
                )}
                <button
                  type="button"
                  className="iconbtn"
                  title="Clear"
                  aria-label="Clear text"
                  disabled={running}
                  onClick={() => {
                    setText("");
                    setError(null);
                  }}
                >
                  <Icon name="trash" size={17} />
                </button>
              </div>
            </header>

            <textarea
              className="editor"
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder="Paste what you want typed…"
              spellCheck
              aria-label="Text to type"
              readOnly={running}
            />

            <footer className="pane__foot">
              <span className="count">
                {words} {words === 1 ? "word" : "words"}
                {plan ? ` · ${plan.chars} keystrokes` : ""}
              </span>
              <div className="typer__actions">
                {state.status === "typing" && state.typed === 0 ? (
                  <span className="count count--soft">click into the window to type into</span>
                ) : null}
                {running ? (
                  <>
                    <button
                      type="button"
                      className="btn btn--quiet"
                      onClick={() =>
                        void (state.status === "paused" ? typerResume() : typerPause()).then(setState)
                      }
                    >
                      {state.status === "paused" ? "Resume" : "Pause"}
                    </button>
                    <button type="button" className="btn btn--danger" onClick={() => void typerStop().then(setState)}>
                      Stop
                    </button>
                  </>
                ) : (
                  <>
                    {deadlineOn ? (
                      <button
                        type="button"
                        className="btn btn--quiet"
                        disabled={!desktop || !supported || !text.trim()}
                        title="Write the ending to fit the deadline, then type the whole piece"
                        onClick={() => void generate()}
                      >
                        Write the rest
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="btn"
                      disabled={!desktop || !supported || !text.trim()}
                      onClick={() => void start()}
                    >
                      Start typing
                    </button>
                  </>
                )}
              </div>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">The run</span>
              <div className="pane__actions">
                <span className="count count--soft">
                  {plan ? `${plan.cps.toFixed(1)} keys/s · ${Math.round(seconds)}s` : " "}
                  {wpm > 0 ? ` · ${wpm} wpm` : ""}
                </span>
              </div>
            </header>

            <div className="pane__body">
              {!desktop ? (
                <p className="errline">
                  <Icon name="warning" size={14} />
                  <span>
                    Typer posts keystrokes to the system, so it only works in the desktop app.
                  </span>
                </p>
              ) : access && !access.supported ? (
                <div className="typer__grant">
                  <p>
                    Typer types into other apps through the system's own input API. The macOS
                    and Windows builds have one; this build has none for{" "}
                    {platformName(access.platform)} yet, so nothing here will type until it
                    ships.
                  </p>
                </div>
              ) : needsPermission ? (
                <div className="typer__grant">
                  <p>
                    macOS blocks keystrokes from apps it does not trust. Add QuillNot under
                    <strong> Privacy &amp; Security → Accessibility</strong>, then start again.
                    Nothing is typed until you do.
                  </p>
                  <button
                    type="button"
                    className="btn btn--sm"
                    onClick={() => void typerOpenSettings().catch(() => {})}
                  >
                    Open Accessibility settings
                  </button>
                </div>
              ) : null}

              {error ? (
                <p className="errline">
                  <Icon name="warning" size={14} />
                  <span>{error}</span>
                </p>
              ) : null}

              <div className="typer__status">
                <span className={`typer__pill is-${state.status}`}>{LABELS[state.status]}</span>
                <span className="typer__count">
                  {state.typed} of {state.total || plan?.chars || 0} keystrokes
                  {wpm > 0 ? ` · ${wpm} wpm` : ""}
                </span>
                {note ? <span className="count count--soft">{note}</span> : null}
              </div>
              <div className="typer__meter" aria-hidden="true">
                <i style={{ width: `${percent}%` }} />
              </div>

              <div className="typer__controls">
                <label className="typer__row">
                  <span className="typer__label">Typo rate</span>
                  <input
                    type="range"
                    min={0}
                    max={15}
                    step={1}
                    value={Math.round(typoRate * 100)}
                    disabled={running}
                    onChange={(event) => setTypoRate(Number(event.target.value) / 100)}
                  />
                  <span className="typer__value">{Math.round(typoRate * 100)}%</span>
                </label>

                <label className="typer__row">
                  <span className="typer__label">Pace</span>
                  <input
                    type="range"
                    min={50}
                    max={200}
                    step={10}
                    value={Math.round(speed * 100)}
                    disabled={running || deadlineOn}
                    onChange={(event) => setSpeed(Number(event.target.value) / 100)}
                  />
                  <span className="typer__value">{speed.toFixed(1)}×</span>
                </label>

                <label className="typer__row">
                  <span className="typer__label">Finish by</span>
                  <span className="typer__inline">
                    <input
                      type="checkbox"
                      checked={deadlineOn}
                      disabled={running}
                      onChange={(event) => setDeadlineOn(event.target.checked)}
                      aria-label="Finish by a deadline"
                    />
                    <input
                      className="typer__num"
                      type="number"
                      min={5}
                      max={3600}
                      step={5}
                      value={deadline}
                      disabled={running || !deadlineOn}
                      onChange={(event) => setDeadline(Number(event.target.value) || 45)}
                      aria-label="Seconds to finish in"
                    />
                    <span className="typer__value">seconds</span>
                  </span>
                </label>
                <label className="typer__row">
                  <span className="typer__label">Stop when idle</span>
                  <span className="typer__inline">
                    <input
                      type="checkbox"
                      checked={idleOn}
                      disabled={running}
                      onChange={(event) => setIdleOn(event.target.checked)}
                      aria-label="Stop after a stretch with no keystroke accepted"
                    />
                    <input
                      className="typer__num"
                      type="number"
                      min={5}
                      max={600}
                      step={5}
                      value={idleSeconds}
                      disabled={running || !idleOn}
                      onChange={(event) => setIdleSeconds(Number(event.target.value) || 30)}
                      aria-label="Seconds of silence before stopping"
                    />
                    <span className="typer__value">seconds</span>
                  </span>
                </label>

                <div className="typer__row">
                  <span className="typer__label">Stop chord</span>
                  <span className="typer__inline">
                    <span className="kbd">{capturing ? "press the keys…" : chordText}</span>
                    <button
                      type="button"
                      className="btn btn--sm btn--quiet"
                      disabled={!desktop}
                      onClick={() => {
                        setNote(null);
                        setError(null);
                        setCapturing((on) => !on);
                      }}
                    >
                      {capturing ? "Cancel" : "Change"}
                    </button>
                  </span>
                </div>
              </div>

              <ul className="typer__rails">
                <li>Nothing is typed while a password field has focus.</li>
                <li>The text comes from this editor, never from the clipboard.</li>
                <li>
                  {chordText} stops a run from anywhere, even while another app has focus,
                  and starts the text in this editor when nothing is running.
                </li>
                {idleOn ? (
                  <li>
                    A run that accepts no keystroke for {idleSeconds} seconds stops itself.
                  </li>
                ) : null}
                <li>Escape stops it too, while QuillNot has focus.</li>
                <li>
                  Start waits three seconds so you can click into the target window. If
                  QuillNot is still focused when a keystroke is due, the run stops instead of
                  typing into this app.
                </li>
                <li>Nothing that is typed is stored or sent anywhere.</li>
              </ul>
            </div>

            <footer className="pane__foot">
              <span className="count count--soft">
                {deadlineOn && plan && plan.cps >= 18
                  ? "That deadline is faster than anyone types, so the pace is clamped."
                  : "A deadline is met by pacing the whole run, not by racing at the end."}
              </span>
            </footer>
          </section>
        </div>
      </div>
    </div>
  );
}
