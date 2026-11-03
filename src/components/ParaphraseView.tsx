import { useMemo, useState } from "react";
import { Icon } from "../icons";
import { diffWords } from "../lib/diff";
import { mockParaphrase, modeLabel, type Mode } from "../lib/paraphrase";
import { countWords } from "../lib/text";
import { ModeTabs } from "./ModeTabs";

const SAMPLE =
  "This update brings faster Mermaid diagrams that match your theme, reorderable filters in " +
  "Bases, and new URI and CLI commands for adding content under a heading. Diagrams are now " +
  "rendered using our new library Pele, which is compatible with the latest Mermaid 12.1.0 " +
  "syntax. They are also responsive to the width of a note, and they can be zoomed and panned " +
  "when they are scaled down.";

const RAIL = [
  { key: "history", label: "History", icon: "history" },
  { key: "compare", label: "Compare", icon: "compare" },
  { key: "stats", label: "Statistics", icon: "stats" },
  { key: "tone", label: "Tone", icon: "tone" },
] as const;

export function ParaphraseView() {
  const [mode, setMode] = useState<Mode>("standard");
  const [strength, setStrength] = useState(2);
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [highlight, setHighlight] = useState(true);
  const [copied, setCopied] = useState(false);
  const [tipOpen, setTipOpen] = useState(true);

  const pieces = useMemo(() => (output ? diffWords(input, output) : null), [input, output]);
  const inWords = countWords(input);
  const outWords = countWords(output);

  async function run() {
    if (!input.trim() || busy) return;
    setBusy(true);
    setCopied(false);
    try {
      setOutput(await mockParaphrase(input, mode, strength));
    } finally {
      setBusy(false);
    }
  }

  function clearAll() {
    setInput("");
    setOutput("");
  }

  async function copy() {
    if (!output) return;
    await navigator.clipboard.writeText(output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

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
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) void run();
              }}
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
                Paraphrased · {modeLabel(mode)}
              </span>
              <div className="pane__actions">
                <button
                  type="button"
                  className={`chip-btn${highlight ? " is-on" : ""}`}
                  onClick={() => setHighlight((value) => !value)}
                  title="Highlight what changed"
                  aria-pressed={highlight}
                >
                  Changes
                </button>
                <button type="button" className="iconbtn" onClick={() => void copy()} title="Copy result" aria-label="Copy result" disabled={!output}>
                  <Icon name={copied ? "check" : "copy"} size={17} />
                </button>
              </div>
            </header>

            <div className="pane__body">
              {output && pieces ? (
                pieces.after.map((piece, index) =>
                  piece.changed && highlight ? (
                    <mark key={index} className="hl">
                      {piece.text}
                    </mark>
                  ) : (
                    <span key={index}>{piece.text}</span>
                  ),
                )
              ) : (
                <p className="empty">
                  Your rewrite appears here. Nothing leaves this machine until you add an OpenRouter key in Settings.
                </p>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count">
                {outWords} {outWords === 1 ? "word" : "words"}
              </span>
              <span className="count count--soft">{busy ? "working…" : output ? "tap a highlighted phrase · copy to keep" : "⌘↩ to run"}</span>
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

      <aside className="rail" aria-label="Views">
        {RAIL.map((item) => (
          <button key={item.key} type="button" className="rail__item" title={`${item.label} — a later milestone`}>
            <Icon name={item.icon} size={19} />
            <span>{item.label}</span>
          </button>
        ))}
      </aside>
    </div>
  );
}
