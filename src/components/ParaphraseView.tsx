import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../icons";
import { isDesktop, onDelta, paraphrase } from "../lib/openrouter";
import { useConnection } from "../lib/connection";
import { compareSentences, diffWords } from "../lib/diff";
import { exportMarkdown, fileName, fileStem, markdown } from "../lib/export";
import { defaultMode, takeRestore } from "../lib/history";
import { MODES, mockParaphrase, modeLabel, type Mode } from "../lib/paraphrase";
import { countWords } from "../lib/text";
import { isSingleWord, rephraseSentence, sentenceAt, splitSentences, wordSynonyms } from "../lib/span";
import { ModeTabs } from "./ModeTabs";
import { Rail, type SideView } from "./Rail";
import { SelectionMenu } from "./SelectionMenu";

const SAMPLE =
  "This update brings faster Mermaid diagrams that match your theme, reorderable filters in " +
  "Bases, and new URI and CLI commands for adding content under a heading. Diagrams are now " +
  "rendered using our new library Pele, which is compatible with the latest Mermaid 12.1.0 " +
  "syntax. They are also responsive to the width of a note, and they can be zoomed and panned " +
  "when they are scaled down.";

/** A selection in the result pane, already widened to the sentence it sits in. */
type SpanSelection = {
  start: number;
  end: number;
  sentence: string;
  sentenceStart: number;
  sentenceEnd: number;
  index: number;
  word: string | null;
  x: number;
  y: number;
};

/** Character offset of a range boundary inside the rendered result. */
function offsetIn(root: HTMLElement, node: Node, offset: number): number | null {
  if (node.nodeType !== Node.TEXT_NODE) return null;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let total = 0;
  let current = walker.nextNode();
  while (current) {
    if (current === node) return total + offset;
    total += current.textContent?.length ?? 0;
    current = walker.nextNode();
  }
  return null;
}

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

  // The sentence menu: a selection in the result pane, the synonyms for it, and whether
  // the last span edit is still in flight.
  const resultRef = useRef<HTMLDivElement | null>(null);
  const [sel, setSel] = useState<SpanSelection | null>(null);
  const [synonyms, setSynonyms] = useState<string[] | null>(null);
  const [spanBusy, setSpanBusy] = useState(false);
  const [spanError, setSpanError] = useState<string | null>(null);

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
    setSel(null);

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
    setSel(null);
  }

  async function copy() {
    if (!output) return;
    await navigator.clipboard.writeText(output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  /* ------------------------------------------------ one sentence at a time */

  function readSelection() {
    const container = resultRef.current;
    if (!container || !output) return;
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
      setSel(null);
      return;
    }

    const range = selection.getRangeAt(0);
    if (!container.contains(range.commonAncestorContainer)) {
      setSel(null);
      return;
    }
    // Offsets are only meaningful if what is on screen is exactly the rewrite. A diff
    // rendering that dropped or reordered a character would splice in the wrong place,
    // so the menu stays shut rather than guessing.
    if (container.textContent !== output) {
      setSel(null);
      return;
    }

    const start = offsetIn(container, range.startContainer, range.startOffset);
    const end = offsetIn(container, range.endContainer, range.endOffset);
    if (start === null || end === null || end <= start) {
      setSel(null);
      return;
    }

    const span = sentenceAt(output, start);
    if (!span) {
      setSel(null);
      return;
    }

    const picked = output.slice(start, end).trim();
    const index = splitSentences(output).findIndex((entry) => entry.start === span.start);
    const rect = range.getBoundingClientRect();

    setSpanError(null);
    setSynonyms(null);
    setSel({
      start,
      end,
      sentence: span.text,
      sentenceStart: span.start,
      sentenceEnd: span.end,
      index: index < 0 ? 0 : index,
      word: isSingleWord(picked) ? picked : null,
      x: Math.max(12, Math.min(rect.left, window.innerWidth - 280)),
      y: Math.max(12, Math.min(rect.bottom + 8, window.innerHeight - 340)),
    });
  }

  async function rephraseSpan() {
    if (!sel || spanBusy) return;
    setSpanBusy(true);
    setSpanError(null);
    try {
      const result = await rephraseSentence(sel.sentence, output, mode);
      setOutput(output.slice(0, sel.sentenceStart) + result.text + output.slice(sel.sentenceEnd));
      setModel(result.model);
      setSel(null);
      window.getSelection()?.removeAllRanges();
    } catch (caught) {
      setSpanError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSpanBusy(false);
    }
  }

  function revertSpan() {
    if (!sel) return;
    const source = splitSentences(input)[sel.index];
    if (!source) {
      setSpanError("This sentence has no counterpart in the original text.");
      return;
    }
    setOutput(output.slice(0, sel.sentenceStart) + source.text + output.slice(sel.sentenceEnd));
    setSel(null);
    window.getSelection()?.removeAllRanges();
  }

  async function copySpan() {
    if (!sel) return;
    await navigator.clipboard.writeText(sel.sentence);
    setSel(null);
  }

  function swapWord(word: string) {
    if (!sel) return;
    setOutput(output.slice(0, sel.start) + word + output.slice(sel.end));
    setSel(null);
    window.getSelection()?.removeAllRanges();
  }

  // Synonyms arrive only for a single selected word, and only once per selection.
  useEffect(() => {
    if (!sel?.word) {
      setSynonyms(null);
      return;
    }
    let live = true;
    setSynonyms(null);
    void wordSynonyms(sel.word, sel.sentence)
      .then((result) => {
        if (live) setSynonyms(result.words);
      })
      .catch((caught) => {
        if (!live) return;
        setSynonyms([]);
        setSpanError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      live = false;
    };
  }, [sel?.word, sel?.sentence]);

  // A click anywhere else, or Escape, dismisses the menu without touching the text.
  useEffect(() => {
    if (!sel) return;
    const onDown = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest(".selmenu") || target?.closest(".resultbody")) return;
      setSel(null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSel(null);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [sel]);

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

              <div
                className="resultbody"
                ref={resultRef}
                onMouseUp={readSelection}
                onKeyUp={readSelection}
              >
              {comparing && pairs ? (
                <div className="cmp" aria-label="Sentence comparison">
                  <div className="cmp__head">
                    <span>Original</span>
                    <span>Rewrite</span>
                  </div>
                  {pairs.map((pair, index) => (
                    <div key={index} className={`cmp__pair${pair.changed ? " is-changed" : ""}`}>
                      <p className="cmp__cell cmp__before">
                        {pair.before ?? <span className="cmp__gone">dropped</span>}
                      </p>
                      <p className="cmp__cell cmp__after">
                        {pair.after ?? <span className="cmp__gone">removed</span>}
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

      {sel ? (
        <SelectionMenu
          x={sel.x}
          y={sel.y}
          sentence={sel.sentence}
          word={sel.word}
          synonyms={synonyms}
          canRevert={splitSentences(input).length > sel.index}
          busy={spanBusy}
          error={spanError}
          onRephrase={() => void rephraseSpan()}
          onRevert={revertSpan}
          onCopy={() => void copySpan()}
          onSynonym={swapWord}
          onClose={() => setSel(null)}
        />
      ) : null}

      <Rail onOpen={onOpen} text={output || input} comparing={comparing} onCompare={output ? () => setComparing((value) => !value) : undefined} />
    </div>
  );
}
