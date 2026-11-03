import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { recordOutcome } from "../lib/samples";
import { countWords } from "../lib/text";
import { FORMALITY, LANGUAGES, translate, type Formality } from "../lib/translate";

const SAMPLE =
  "We shipped the update on a Friday, which everyone says not to do. It broke the one " +
  "thing nobody tested, and the release notes still claimed it worked.";

/**
 * Translate: the same request as a rewrite, with a language where the mode would be.
 *
 * The text goes to the provider the person connected and nowhere else, because the model
 * answers from its own weights rather than from a translation service. That is why this
 * tool has no key of its own and no account: it is the rewrite pipeline pointed at a
 * different brief.
 */
export function TranslateView() {
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [target, setTarget] = useState<string>("Spanish");
  const [formality, setFormality] = useState<Formality>("Neutral");
  const [model, setModel] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const inWords = countWords(input);
  const outWords = countWords(output);

  async function run() {
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    setCopied(false);
    if (output && model) void recordOutcome(model, "paraphrase", "rerun");
    setOutput("");
    setModel(null);
    try {
      const result = await translate(input, target, formality);
      setOutput(result.text);
      setModel(result.model);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    if (!output) return;
    await navigator.clipboard.writeText(output);
    void recordOutcome(model, "paraphrase", "kept");
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  // The same two shortcuts as the paraphraser: the command key is the command key in every
  // tool, and a translation is run often enough that reaching for the button gets old.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      if (event.key === "Enter") {
        event.preventDefault();
        void run();
        return;
      }
      if (event.key.toLowerCase() === "c") {
        if (window.getSelection()?.toString() || !output) return;
        event.preventDefault();
        void copy();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <div className="workspace">
      <div className="workspace__body">
        <div className="modebar">
          <span className="modebar__static">Translate</span>
          <span className="modebar__note">
            Your provider's model does the translating. The text goes there and nowhere else,
            and the register is a sentence in the brief rather than a service tier.
          </span>
        </div>

        <div className="panes">
          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Text</span>
              <div className="pane__actions">
                {input ? null : (
                  <button type="button" className="linkbtn" onClick={() => setInput(SAMPLE)}>
                    Use a sample
                  </button>
                )}
                <select
                  className="input input--select"
                  value={target}
                  aria-label="Target language"
                  disabled={busy}
                  onChange={(event) => setTarget(event.target.value)}
                >
                  {LANGUAGES.map((language) => (
                    <option key={language} value={language}>
                      {language}
                    </option>
                  ))}
                </select>
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
              placeholder="Paste the text to translate…"
              spellCheck
              aria-label="Text to translate"
              readOnly={busy}
            />

            <footer className="pane__foot">
              <span className="count">
                {inWords} {inWords === 1 ? "word" : "words"}
              </span>
              <div className="typer__actions">
                <select
                  className="input input--select"
                  value={formality}
                  aria-label="Register"
                  disabled={busy}
                  onChange={(event) => setFormality(event.target.value as Formality)}
                >
                  {FORMALITY.map((entry) => (
                    <option key={entry} value={entry}>
                      {entry}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="btn"
                  disabled={!input.trim() || busy}
                  onClick={() => void run()}
                >
                  {busy ? "Translating…" : "Translate"}
                </button>
              </div>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">{target}</span>
              <div className="pane__actions">
                <span className="count count--soft">{model ?? " "}</span>
                <button
                  type="button"
                  className="iconbtn"
                  onClick={() => void copy()}
                  title="Copy translation"
                  aria-label="Copy translation"
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
                  The {target} text appears here. Names, numbers and paragraph breaks are
                  kept; the register is {formality.toLowerCase()}.
                </p>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count">
                {outWords} {outWords === 1 ? "word" : "words"}
              </span>
              <span className="count count--soft">
                {busy
                  ? "translating…"
                  : output
                    ? "the language was the whole brief"
                    : "⌘↩ to run"}
              </span>
            </footer>
          </section>
        </div>
      </div>
    </div>
  );
}
