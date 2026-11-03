import { useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import { applyIssue, checkGrammar, type Issue } from "../lib/grammar";
import { takeRestore } from "../lib/history";
import { useConnection } from "../lib/connection";
import { isDesktop } from "../lib/openrouter";
import { countWords } from "../lib/text";
import { Rail, type SideView } from "./Rail";

const SAMPLE =
  "This is a sentence  with a double space, and it also has alot of small problems. " +
  "i dont think the writer really read it before sending it out. The the release notes " +
  "mention a feature that is very useful ,but nobody tested the install on macOS.";

export function GrammarView({ onOpen }: { onOpen: (view: SideView) => void }) {
  const connection = useConnection();
  // A check opened from History arrives with the text it checked.
  const [restored] = useState(() => takeRestore("grammar"));
  const [text, setText] = useState(restored?.original ?? "");
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [model, setModel] = useState<string | null>(null);
  const [dropped, setDropped] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const live = isDesktop() && connection.connected;
  const words = countWords(text);
  const counts = useMemo(() => {
    const tally: Record<string, number> = {};
    for (const issue of issues ?? []) tally[issue.kind] = (tally[issue.kind] ?? 0) + 1;
    return tally;
  }, [issues]);

  async function check() {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await checkGrammar(text);
      setIssues(result.issues);
      setModel(result.model);
      setDropped(result.dropped + result.unlocated);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setIssues(null);
    } finally {
      setBusy(false);
    }
  }

  async function copyText() {
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  // The same window-wide shortcuts as the paraphraser: ⌘↩ checks, ⌘C copies the text
  // being checked, unless the writer has a selection of their own.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      if (event.key === "Enter") {
        event.preventDefault();
        void check();
        return;
      }
      if (event.key.toLowerCase() === "c") {
        if (window.getSelection()?.toString() || !text) return;
        event.preventDefault();
        void copyText();
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function accept(issue: Issue) {
    // A quote that appears more than once has no position, so there is nothing safe
    // to splice. The card stays until the writer edits it by hand.
    if (issue.unique === false) return;
    const next = applyIssue(text, issue);
    setText(next);
    // Re-run so the remaining cards point at the text that now exists, not at
    // offsets from the version the model saw.
    setIssues(issues ? issues.filter((item) => item.id !== issue.id).map((item) => shift(item, issue)) : null);
  }

  const applyable = (issues ?? []).filter((issue) => issue.unique !== false);

  return (
    <div className="workspace">
      <div className="workspace__body">
        <div className="modebar">
          <span className="modebar__static">Checker</span>
          <span className="modebar__note">
            Every fix is a suggestion until you accept it, and each one was found in your
            text before it was offered.
          </span>
        </div>

        <div className="panes">
          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Your text</span>
              <div className="pane__actions">
                {text ? null : (
                  <button type="button" className="linkbtn" onClick={() => setText(SAMPLE)}>
                    Use a sample
                  </button>
                )}
                <button
                  type="button"
                  className="iconbtn"
                  title="Copy this text"
                  aria-label="Copy this text"
                  disabled={!text}
                  onClick={() => void copyText()}
                >
                  <Icon name={copied ? "check" : "copy"} size={17} />
                </button>
                <button
                  type="button"
                  className="iconbtn"
                  title="Clear"
                  aria-label="Clear text"
                  onClick={() => {
                    setText("");
                    setIssues(null);
                    setModel(null);
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
              placeholder="Paste the text you want checked…"
              spellCheck
              aria-label="Text to check"
            />

            <footer className="pane__foot">
              <span className="count">
                {words} {words === 1 ? "word" : "words"}
              </span>
              <button type="button" className="btn" onClick={() => void check()} disabled={!text.trim() || busy}>
                {busy ? "Checking…" : "Check grammar"}
              </button>
            </footer>
          </section>

          <section className="pane">
            <header className="pane__head">
              <span className="pane__label">Suggestions</span>
              <div className="pane__actions">
                {applyable.length > 0 ? (
                  <button
                    type="button"
                    className="linkbtn"
                    title="Accept every fix that could be placed in the text"
                    onClick={() => {
                      let next = text;
                      for (const issue of [...applyable].sort((a, b) => b.start - a.start)) next = applyIssue(next, issue);
                      setText(next);
                      setIssues((issues ?? []).filter((issue) => issue.unique === false));
                    }}
                  >
                    Accept all
                  </button>
                ) : null}
              </div>
            </header>

            <div className="pane__body pane__body--list">
              {error ? (
                <p className="errline">
                  <Icon name="warning" size={14} />
                  <span>{error}</span>
                </p>
              ) : issues === null ? (
                <p className="empty">
                  Run the checker and each problem shows up here as a card you can accept
                  or ignore.
                </p>
              ) : issues.length === 0 ? (
                <div className="clean">
                  <span className="clean__mark">
                    <Icon name="check" size={18} />
                  </span>
                  <p>Nothing to fix in this text.</p>
                </div>
              ) : (
                <>
                  <div className="issuebar">
                    <span className="issuebar__count">{issues.length} to fix</span>
                    {Object.entries(counts).map(([kind, count]) => (
                      <span key={kind} className={`chip chip--${kind}`}>
                        {count} {kind}
                      </span>
                    ))}
                  </div>
                  {issues.map((issue) => (
                    <article key={issue.id} className={`issue is-${issue.kind}`}>
                      <div className="issue__head">
                        <span className={`chip chip--${issue.kind}`}>{issue.kind}</span>
                        <div className="issue__actions">
                          <button
                            type="button"
                            className="iconbtn iconbtn--tight"
                            title="Ignore"
                            aria-label="Ignore suggestion"
                            onClick={() => setIssues((current) => (current ? current.filter((item) => item.id !== issue.id) : null))}
                          >
                            <Icon name="close" size={15} />
                          </button>
                          <button
                            type="button"
                            className="btn btn--sm"
                            disabled={issue.unique === false}
                            title={
                              issue.unique === false
                                ? "This wording appears more than once, so there is no safe place to apply it"
                                : `Replace with “${issue.suggestion}”`
                            }
                            onClick={() => accept(issue)}
                          >
                            Accept
                          </button>
                        </div>
                      </div>
                      <p className="issue__quote">
                        {/* A doubled space has nothing to show for itself; the open-box
                            marks make the fix legible instead of rendering as an empty
                            span either side of the arrow. */}
                        <span className="issue__old">{issue.quote.trim() ? issue.quote : "␣␣"}</span>
                        <Icon name="arrow" size={13} />
                        <span className="issue__new">{issue.suggestion || "␣"}</span>
                      </p>
                      <p className="issue__message">
                        {issue.message}
                        {issue.unique === false ? " Found more than once, so it has no position." : ""}
                      </p>
                    </article>
                  ))}
                </>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count count--soft">
                {issues ? `${issues.length} open` : "not run"}
                {dropped > 0 ? ` · ${dropped} not trusted` : ""}
              </span>
              <span className="count count--soft">
                {model ? <span className="modelchip">{model}</span> : null}
                {live ? "⌘C copies · ⌘↩ to check" : "mock · ⌘C copies · ⌘↩ to check"}
              </span>
            </footer>
          </section>
        </div>
      </div>

      <Rail onOpen={onOpen} text={text} />
    </div>
  );
}

/** Keeps a remaining card pointing at the same words after an earlier fix changed the length. */
function shift(issue: Issue, applied: Issue): Issue {
  if (!issue.unique || issue.start < applied.start) return issue;
  const delta = applied.suggestion.length - (applied.end - applied.start);
  return { ...issue, start: issue.start + delta, end: issue.end + delta };
}
