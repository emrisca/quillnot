import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { useConnection } from "../lib/connection";
import {
  DEFAULT_PRESET,
  SKILL_PRESETS,
  humanize,
  loadHumanizer,
  saveHumanizer,
} from "../lib/humanizer";
import { isDesktop, onDelta } from "../lib/openrouter";
import { mockParaphrase } from "../lib/paraphrase";
import { recordOutcome } from "../lib/samples";
import { countWords } from "../lib/text";

const SAMPLE =
  "In today's fast-moving landscape, it is important to note that the team leveraged a robust " +
  "set of tools in order to ensure that the release landed smoothly, showcasing a commitment to " +
  "excellence that will continue to drive value going forward.";

/**
 * The humanizer: a rewrite whose instructions are visible.
 *
 * Every other tool in this app hides its prompt behind a mode name. Here the prompt is the
 * third pane, editable, and sent verbatim with each block — so "make it plainer" is
 * something the person can actually say, and can check.
 */
export function HumanizerView() {
  const connection = useConnection();

  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [model, setModel] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const [preset, setPreset] = useState(DEFAULT_PRESET.id);
  const [skill, setSkill] = useState(DEFAULT_PRESET.skill);
  // Saving waits until the stored value has been read, so a fresh mount cannot write the
  // preset over the text the person left last time.
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState(false);

  const live = isDesktop() && connection.connected;
  const inWords = countWords(input);
  const outWords = countWords(output);

  useEffect(() => {
    let mounted = true;
    void loadHumanizer()
      .then((stored) => {
        if (!mounted) return;
        const known = SKILL_PRESETS.find((entry) => entry.id === stored.preset);
        setPreset(stored.preset ?? DEFAULT_PRESET.id);
        setSkill(stored.skill ?? known?.skill ?? DEFAULT_PRESET.skill);
      })
      .catch(() => {
        // An unreadable settings file costs the saved text, not the tool: the preset
        // still loads and everything typed from here is saved again.
      })
      .finally(() => {
        if (mounted) setLoaded(true);
      });
    return () => {
      mounted = false;
    };
  }, []);

  // Autosave rather than a Save button: the requirement is that edits are remembered, and
  // a button is exactly what people forget to press.
  useEffect(() => {
    if (!loaded) return;
    const timer = window.setTimeout(() => {
      void saveHumanizer(skill, preset)
        .then(() => {
          setSaved(true);
          window.setTimeout(() => setSaved(false), 1600);
        })
        .catch(() => {
          // A browser preview has no settings file; nothing to report there.
        });
    }, 500);
    return () => window.clearTimeout(timer);
  }, [skill, preset, loaded]);

  function choosePreset(id: string) {
    const next = SKILL_PRESETS.find((entry) => entry.id === id);
    if (!next) return;
    setPreset(next.id);
    setSkill(next.skill);
  }

  async function run() {
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    setCopied(false);
    // Running again over a reply nobody copied is the person saying "not quite", which
    // the ranking counts for less than a keep and more than an undo.
    if (output && model) void recordOutcome(model, "paraphrase", "rerun");

    try {
      if (!live) {
        setOutput(await mockParaphrase(input, "standard", 2));
        setModel(null);
        return;
      }

      setOutput("");
      setModel(null);
      let buffer = "";
      const stop = await onDelta((delta) => {
        buffer += delta.text;
        setOutput(buffer);
      });
      try {
        const result = await humanize(input, skill);
        setOutput(result.text);
        setModel(result.model);
      } finally {
        stop();
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    if (!output) return;
    await navigator.clipboard.writeText(output);
    // Copying is the closest thing to "I am using this" that a desktop app can see.
    void recordOutcome(model, "paraphrase", "kept");
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  return (
    <div className="workspace">
      <div className="workspace__body">
        <div className="modebar">
          <span className="modebar__static">Humanizer</span>
          <span className="modebar__note">
            The skill on the right is sent with every block, word for word. Edit it, and what
            the model is told changes with it.
          </span>
        </div>

        <div className="panes panes--skill">
          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Text to edit</span>
              <div className="pane__actions">
                {input ? null : (
                  <button type="button" className="linkbtn" onClick={() => setInput(SAMPLE)}>
                    Use a sample
                  </button>
                )}
                <button
                  type="button"
                  className="iconbtn"
                  title="Clear"
                  aria-label="Clear text"
                  disabled={busy}
                  onClick={() => {
                    if (output && model) void recordOutcome(model, "paraphrase", "undone");
                    setInput("");
                    setOutput("");
                    setError(null);
                    setModel(null);
                  }}
                >
                  <Icon name="trash" size={17} />
                </button>
              </div>
            </header>

            <textarea
              className="editor"
              value={input}
              onChange={(event) => setInput(event.target.value)}
              placeholder="Paste the text that sounds like a machine wrote it…"
              spellCheck
              aria-label="Text to edit"
              readOnly={busy}
            />

            <footer className="pane__foot">
              <span className="count">
                {inWords} {inWords === 1 ? "word" : "words"}
              </span>
              <button
                type="button"
                className="btn"
                disabled={!input.trim() || busy || !skill.trim()}
                onClick={() => void run()}
              >
                {busy ? "Working…" : "Humanize"}
              </button>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Result</span>
              <div className="pane__actions">
                <span className="count count--soft">{model ?? (live ? "" : "mock")}</span>
                <button
                  type="button"
                  className="iconbtn"
                  onClick={() => void copy()}
                  title="Copy result"
                  aria-label="Copy result"
                  disabled={!output}
                >
                  <Icon name={copied ? "check" : "copy"} size={17} />
                </button>
              </div>
            </header>

            <div className="pane__body">
              {error ? (
                <p className="errline">
                  <Icon name="warning" size={14} />
                  <span>{error}</span>
                </p>
              ) : null}

              {output ? (
                <span>{output}</span>
              ) : (
                <p className="empty">
                  {live
                    ? "The edited text appears here, streamed from the model as it is written."
                    : "Connect an account in Settings and this pane streams from a free model. Until then it answers from the local mock, so the shape of the tool is still here to judge."}
                </p>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count">
                {outWords} {outWords === 1 ? "word" : "words"}
                {outWords > 0 && inWords > 0 ? (
                  <span className="count count--delta">
                    {outWords === inWords
                      ? "same length"
                      : `${outWords > inWords ? "+" : ""}${Math.round(((outWords - inWords) / inWords) * 100)}%`}
                  </span>
                ) : null}
              </span>
              <span className="count count--soft">
                {busy ? "streaming…" : output ? "the skill did that, not a mode name" : " "}
              </span>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Skill</span>
              <div className="pane__actions">
                <select
                  className="input input--select"
                  value={preset}
                  aria-label="Skill preset"
                  onChange={(event) => choosePreset(event.target.value)}
                >
                  {SKILL_PRESETS.map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.label}
                    </option>
                  ))}
                </select>
              </div>
            </header>

            <textarea
              className="editor skill__text"
              value={skill}
              onChange={(event) => setSkill(event.target.value)}
              spellCheck={false}
              aria-label="The skill bundled with the request"
              placeholder="Say how the text should be edited. This is sent to the model as written."
            />

            <footer className="pane__foot">
              <span className="count">
                {skill.trim().length.toLocaleString()} characters
                {saved ? " · saved" : ""}
              </span>
              <button
                type="button"
                className="btn btn--sm btn--quiet"
                disabled={preset === DEFAULT_PRESET.id || busy}
                title="Put this preset's text back, discarding your edits"
                onClick={() => choosePreset(preset)}
              >
                Reset
              </button>
            </footer>
          </section>
        </div>
      </div>
    </div>
  );
}
