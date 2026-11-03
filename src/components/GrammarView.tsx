import { useMemo, useState } from "react";
import { Icon } from "../icons";
import { applyIssue, mockCheck, type Issue } from "../lib/grammar";
import { countWords } from "../lib/text";

const SAMPLE =
  "This is a sentence  with a double space, and it also has alot of small problems. " +
  "i dont think the writer really read it before sending it out. The the release notes " +
  "mention a feature that is very useful ,but nobody tested the install on macOS.";

export function GrammarView() {
  const [text, setText] = useState("");
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [busy, setBusy] = useState(false);

  const words = countWords(text);
  const counts = useMemo(() => {
    const tally: Record<string, number> = {};
    for (const issue of issues ?? []) tally[issue.kind] = (tally[issue.kind] ?? 0) + 1;
    return tally;
  }, [issues]);

  async function check() {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      setIssues(await mockCheck(text));
    } finally {
      setBusy(false);
    }
  }

  function accept(issue: Issue) {
    const next = applyIssue(text, issue);
    setText(next);
    // Re-run so the remaining cards point at the text that now exists, not at
    // offsets from the version the model saw.
    setIssues(issues ? issues.filter((item) => item.id !== issue.id).map((item) => shift(item, issue)) : null);
  }

  return (
    <div className="workspace">
      <div className="workspace__body">
        <div className="modebar">
          <span className="modebar__static">Checker</span>
          <span className="modebar__note">Every fix is a suggestion until you accept it.</span>
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
                  title="Clear"
                  aria-label="Clear text"
                  onClick={() => {
                    setText("");
                    setIssues(null);
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
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) void check();
              }}
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
                {issues && issues.length > 0 ? (
                  <button
                    type="button"
                    className="linkbtn"
                    onClick={() => {
                      let next = text;
                      for (const issue of [...issues].sort((a, b) => b.start - a.start)) next = applyIssue(next, issue);
                      setText(next);
                      setIssues([]);
                    }}
                  >
                    Accept all
                  </button>
                ) : null}
              </div>
            </header>

            <div className="pane__body pane__body--list">
              {issues === null ? (
                <p className="empty">Run the checker and each problem shows up here as a card you can accept or ignore.</p>
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
                          <button type="button" className="btn btn--sm" onClick={() => accept(issue)}>
                            Accept
                          </button>
                        </div>
                      </div>
                      <p className="issue__quote">
                        <span className="issue__old">{issue.quote}</span>
                        <Icon name="arrow" size={13} />
                        <span className="issue__new">{issue.suggestion}</span>
                      </p>
                      <p className="issue__message">{issue.message}</p>
                    </article>
                  ))}
                </>
              )}
            </div>

            <footer className="pane__foot">
              <span className="count count--soft">{issues ? `${issues.length} open` : "not run"}</span>
              <span className="count count--soft">⌘↩ to check</span>
            </footer>
          </section>
        </div>
      </div>

      <aside className="rail" aria-label="Views">
        <button type="button" className="rail__item" title="History — a later milestone">
          <Icon name="history" size={19} />
          <span>History</span>
        </button>
        <button type="button" className="rail__item" title="Statistics — a later milestone">
          <Icon name="stats" size={19} />
          <span>Stats</span>
        </button>
      </aside>
    </div>
  );
}

/** Keeps a remaining card pointing at the same words after an earlier fix changed the length. */
function shift(issue: Issue, applied: Issue): Issue {
  if (issue.start < applied.start) return issue;
  const delta = applied.suggestion.length - (applied.end - applied.start);
  return { ...issue, start: issue.start + delta, end: issue.end + delta };
}
