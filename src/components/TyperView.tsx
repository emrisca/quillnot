import { useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import { isDesktop } from "../lib/openrouter";
import { countWords } from "../lib/text";
import {
  onTyperState,
  typerAccessibility,
  typerOpenSettings,
  typerPause,
  typerPlan,
  typerResume,
  typerStart,
  typerStatus,
  typerStop,
  type TyperConfig,
  type TyperPlan,
  type TyperState,
} from "../lib/typer";

const SAMPLE =
  "We shipped the update on a Friday, which everyone says not to do. It broke the one " +
  "thing nobody tested, and the release notes still claimed it worked. Next time the " +
  "checklist runs before the tag, not after it.";

const IDLE: TyperState = { status: "idle", typed: 0, total: 0 };

const LABELS: Record<TyperState["status"], string> = {
  idle: "Ready",
  typing: "Typing",
  paused: "Paused",
  done: "Finished",
  stopped: "Stopped",
  blocked: "Stopped: a secure field has focus",
};

export function TyperView() {
  const [text, setText] = useState("");
  // Held in the view rather than in settings: these are per-run decisions, and a typo
  // rate that survived a restart would be a surprise on the next run.
  const [typoRate, setTypoRate] = useState(0.04);
  const [speed, setSpeed] = useState(1);
  const [deadlineOn, setDeadlineOn] = useState(false);
  const [deadline, setDeadline] = useState(45);
  const [state, setState] = useState<TyperState>(IDLE);
  const [plan, setPlan] = useState<TyperPlan | null>(null);
  const [trusted, setTrusted] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  const desktop = isDesktop();
  const running = state.status === "typing" || state.status === "paused";
  const words = countWords(text);
  const config: TyperConfig = useMemo(
    () => ({ typoRate, speed, deadlineSecs: deadlineOn ? deadline : null }),
    [typoRate, speed, deadlineOn, deadline],
  );

  useEffect(() => {
    if (!desktop) return;
    void typerAccessibility()
      .then(setTrusted)
      .catch(() => setTrusted(false));
    void typerStatus()
      .then(setState)
      .catch(() => setState(IDLE));

    let unlisten: (() => void) | undefined;
    void onTyperState((next) => setState(next)).then((off) => {
      unlisten = off;
    });
    return () => unlisten?.();
  }, [desktop]);

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
    try {
      const summary = await typerStart(text, config);
      setPlan(summary);
      setTrusted(true);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      if (message.toLowerCase().includes("accessibility")) setTrusted(false);
    }
  }

  // Escape is the panic button and always works, because the person reaching for it is
  // usually watching the wrong window.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !running) return;
      event.preventDefault();
      void typerStop().then(setState);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [running]);

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
                  <button
                    type="button"
                    className="btn"
                    disabled={!desktop || !text.trim()}
                    onClick={() => void start()}
                  >
                    Start typing
                  </button>
                )}
              </div>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">The run</span>
              <div className="pane__actions">
                <span className="count count--soft">
                  {plan ? `${plan.cps.toFixed(1)} keys/s · ${Math.round(seconds)}s` : " "}
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
              ) : trusted === false ? (
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
                </span>
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
                  <input
                    type="checkbox"
                    checked={deadlineOn}
                    disabled={running}
                    onChange={(event) => setDeadlineOn(event.target.checked)}
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
                </label>
              </div>

              <ul className="typer__rails">
                <li>Nothing is typed while a password field has focus.</li>
                <li>The text comes from this editor, never from the clipboard.</li>
                <li>Escape stops immediately, mid-word, wherever you are.</li>
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
