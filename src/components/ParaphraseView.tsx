import { useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import { isDesktop, onDelta, paraphrase } from "../lib/openrouter";
import { useConnection } from "../lib/connection";
import { compareSentences, diffWords } from "../lib/diff";
import { exportMarkdown, fileName, fileStem, markdown } from "../lib/export";
import { defaultMode, takeRestore } from "../lib/history";
import { MODES, mockParaphrase, modeLabel, type Mode } from "../lib/paraphrase";
import { countWords } from "../lib/text";
import { ModeTabs } from "./ModeTabs";
import { Rail, type SideView } from "./Rail";

const SAMPLE =
  "This update brings faster Mermaid diagrams that match your theme, reorderable filters in " +
  "Bases, and new URI and CLI commands for adding content under a heading. Diagrams are now " +
  "rendered using our new library Pele, which is compatible with the latest Mermaid 12.1.0 " +
  "syntax. They are also responsive to the width of a note, and they can be zoomed and panned " +
  "when they are scaled down.";

export function ParaphraseView({ onOpen }: { onOpen: (view: SideView) => void }) {
  const connection = useConnection();
  // A run opened from History brings its own text, result and mode; the Settings
  // default only applies to a workspace that opened empty.
  const [restored] = useState(() => takeRestore("paraphrase"));
  const [mode, setMode] = useState<Mode>((restored?.mode as Mode) ?? "standard");
  const [strength, setStrength] = useState(2);
  const [input, setInput] = useState(restored?.original ?? "");
  const [output, setOutput] = useState(restored?.result ?? "");
  const [model, setModel] = useState<string | null>(restored?.model || null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [highlight, setHighlight] = useState(true);
  const [comparing, setComparing] = useState(false);
  const [copied, setCopied] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [tipOpen, setTipOpen] = useState(true);

  const live = isDesktop() && connection.connected;

  useEffect(() => {
    if (restored) return;
    void defaultMode().then((stored) => {
      if (stored && MODES.some((entry) => entry.id === stored)) setMode(stored as Mode);
    });
  }, [restored]);

  const pieces = useMemo(() => (output ? diffWords(input, output) : null), [input, output]);
  const pairs = useMemo(() => (output ? compareSentences(input, output) : null), [input, output]);
  const inWords = countWords(input);
  const outWords = countWords(output);

  async function run() {
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    setCopied(false);
    setSaved(null);

    try {
      if (!live) {
        setOutput(await mockParaphrase(input, mode, strength));
        setModel(null);
        return;
      }

      // Stream first, settle on the command's result: the deltas make the pane feel
      // alive, the reply is the authority on what the text finally is.
      setOutput("");
      setModel(null);
      let buffer = "";
      const unlisten = await onDelta((delta) => {
        buffer += delta.text;
        setOutput(buffer);
      });
      try {
        const result = await paraphrase({ text: input, mode, strength });
        setOutput(result.text);
        setModel(result.model);
      } finally {
        unlisten();
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  function clearAll() {
    setInput("");
    setOutput("");
    setError(null);
    setModel(null);
    setSaved(null);
  }

  async function copy() {
    if (!output) return;
    await navigator.clipboard.writeText(output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  async function exportFile() {
    if (!output) return;
    try {
      const path = await exportMarkdown(
        fileStem(mode),
        markdown({ original: input, result: output, mode, model }),
      );
      setSaved(path ? `Saved ${fileName(path)} to Downloads` : "Markdown downloaded");
      window.setTimeout(() => setSaved(null), 6000);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  // Shortcuts work from anywhere in the workspace, not just with the caret in a
  // particular box, so the listener is on the window and re-registered every render:
  // it closes over the current text, which is cheaper than keeping it in sync.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;

      if (event.key === "Enter") {
        event.preventDefault();
        void run();
        return;
      }

      if (event.key.toLowerCase() === "c") {
        // Leave ⌘C alone when the writer has selected something of their own.
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
        <ModeTabs mode={mode} onMode={setMode} strength={strength} onStrength={setStrength} />

        <div className="panes">
          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Your text</span>
              <div className="pane__actions">
                {input ? null : (
                  <button type="button" className="linkbtn" onClick={() => setInput(SAMPLE)}>
                    Use a sample
                  </button>
                )}
                <button type="button" className="iconbtn" onClick={clearAll} title="Clear both panes" aria-label="Clear both panes">
                  <Icon name="trash" size={17} />
                </button>
              </div>
            </header>

            <textarea
              className="editor"
              value={input}
              onChange={(event) => setInput(event.target.value)}
              placeholder="Paste the paragraph you want rewritten…"
              spellCheck
              aria-label="Text to paraphrase"
            />

            <footer className="pane__foot">
              <span className="count">
                {inWords} {inWords === 1 ? "word" : "words"}
              </span>
              <button type="button" className="btn" onClick={() => void run()} disabled={!input.trim() || busy}>
                {busy ? "Paraphrasing…" : "Paraphrase"}
              </button>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">
                {modeLabel(mode)} · Paraphrased
                {model ? <span className="modelchip">{model}</span> : null}
              </span>
              <div className="pane__actions">
                <button
                  type="button"
                  className={`chip-btn${highlight ? " is-on" : ""}`}
                  onClick={() => {
                    setComparing(false);
                    setHighlight(true);
                  }}
                  title="Highlight the words that changed"
                  aria-pressed={highlight}
                >
                  Changes
                </button>
                <button
                  type="button"
                  className={`chip-btn${comparing ? " is-on" : ""}`}
                  onClick={() => setComparing((value) => !value)}
                  disabled={!output}
                  title="Compare the original and the rewrite sentence by sentence"
                  aria-pressed={comparing}
                >
                  Sentences
                </button>
                <button type="button" className="iconbtn" onClick={() => void copy()} title="Copy result" aria-label="Copy result" disabled={!output}>
                  <Icon name={copied ? "check" : "copy"} size={17} />
                </button>
                <button type="button" className="iconbtn" onClick={() => void exportFile()} title="Save as Markdown" aria-label="Save as Markdown" disabled={!output}>
                  <Icon name="download" size={17} />
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

              {comparing && pairs ? (
                <div className="cmp" aria-label="Sentence comparison">
                  <div className="cmp__head">
                    <span>Original</span>
                    <span>Rewrite</span>
                  </div>
                  {pairs.map((pair, index) => (
                    <div key={index} className={`cmp__pair${pair.changed ? " is-changed" : ""}`}>
                      <p className="cmp__cell cmp__before">
                        {pair.before ?? <span className="cmp__gone">— dropped —</span>}
                      </p>
                      <p className="cmp__cell cmp__after">
                        {pair.after ?? <span className="cmp__gone">— removed —</span>}
                      </p>
                    </div>
                  ))}
                  <p className="cmp__foot">
                    {pairs.filter((pair) => pair.changed).length} of {pairs.length} sentences changed.
                  </p>
                </div>
              ) : output && pieces ? (
                pieces.after.map((piece, index) =>
                  piece.changed && highlight ? (
                    <mark key={index} className="hl">
                      {piece.text}
                    </mark>
                  ) : (
                    <span key={index}>{piece.text}</span>
                  ),
                )
              ) : output ? (
                <span>{output}</span>
              ) : (
                <p className="empty">
                  {live
                    ? "Your rewrite appears here, streamed from the model as it is written."
                    : "Your rewrite appears here. Connect an OpenRouter account in Settings and this pane streams from a free model instead."}
                </p>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count">
                {outWords} {outWords === 1 ? "word" : "words"}
                {outWords > 0 && inWords > 0 ? (
                  <span className="count count--delta">
                    {outWords === inWords ? "same length" : `${outWords > inWords ? "+" : ""}${Math.round(((outWords - inWords) / inWords) * 100)}%`}
                  </span>
                ) : null}
              </span>
              <span className="count count--soft">
                {busy
                  ? "streaming…"
                  : saved
                    ? saved
                    : output
                      ? "⌘C copies · ⌘↩ runs again"
                      : "⌘↩ to run"}
              </span>
            </footer>
          </section>

          {tipOpen ? (
            <aside className="tip">
              <div className="tip__head">
                <span className="tip__spark">
                  <Icon name="sparkle" size={15} />
                </span>
                <button type="button" className="iconbtn iconbtn--tight" onClick={() => setTipOpen(false)} aria-label="Dismiss suggestion">
                  <Icon name="close" size={14} />
                </button>
              </div>
              <p className="tip__title">Need this text to be easier to understand?</p>
              <p className="tip__body">
                <span className="tip__mode">Simple mode</span>
                Short sentences and plain words, for a reader in a hurry.
              </p>
              <button
                type="button"
                className="btn btn--sm"
                onClick={() => {
                  setMode("simple");
                  setTipOpen(false);
                }}
              >
                Try it
              </button>
            </aside>
          ) : null}
        </div>
      </div>

      <Rail onOpen={onOpen} text={output || input} comparing={comparing} onCompare={output ? () => setComparing((value) => !value) : undefined} />
    </div>
  );
}
