import { useEffect, useState } from "react";
import { Icon } from "../icons";
import {
  ago,
  clearHistory,
  deleteRun,
  listRuns,
  openDataDir,
  preview,
  restoreRun,
  type Run,
} from "../lib/history";
import { isDesktop } from "../lib/openrouter";

/**
 * Every run, newest first, as a list that reopens one.
 *
 * The rows come from `history.jsonl` in the app's data directory, so "delete" here
 * deletes the only copy — which is why deleting all of them asks first.
 */
export function HistoryView({ onClose }: { onClose: () => void }) {
  const [runs, setRuns] = useState<Run[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    void listRuns()
      .then((loaded) => {
        if (live) setRuns(loaded);
      })
      .catch((caught: unknown) => {
        if (live) setError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      live = false;
    };
  }, []);

  async function remove(id: string) {
    setBusy(true);
    try {
      setRuns(await deleteRun(id));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function wipe() {
    const count = runs?.length ?? 0;
    if (!window.confirm(`Delete all ${count} runs? This cannot be undone.`)) return;
    setBusy(true);
    try {
      setRuns(await clearHistory());
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="side">
      <header className="side__head">
        <button type="button" className="iconbtn" onClick={onClose} title="Back to the workspace" aria-label="Back">
          <Icon name="arrow" size={17} />
        </button>
        <h2 className="side__title">History</h2>
        <span className="count count--soft">
          {runs ? `${runs.length} ${runs.length === 1 ? "run" : "runs"}` : "…"}
        </span>
        <div className="side__actions">
          <button type="button" className="linkbtn" onClick={() => void openDataDir()}>
            Open folder
          </button>
          {runs && runs.length > 0 ? (
            <button type="button" className="linkbtn linkbtn--danger" onClick={() => void wipe()} disabled={busy}>
              Delete all
            </button>
          ) : null}
        </div>
      </header>

      <div className="side__body">
        {error ? (
          <p className="errline">
            <Icon name="warning" size={14} />
            <span>{error}</span>
          </p>
        ) : null}

        {!isDesktop() ? (
          <p className="empty">
            History is kept by the desktop app, in its own data folder. Run{" "}
            <code>npm run tauri dev</code> to see it fill up.
          </p>
        ) : runs === null ? (
          <p className="empty">Reading the log…</p>
        ) : runs.length === 0 ? (
          <p className="empty">
            Nothing yet. Every rewrite and every check is saved here as it happens, and
            stays on this machine.
          </p>
        ) : (
          <ul className="hist">
            {runs.map((run) => (
              <li key={run.id} className="hist__row">
                <button
                  type="button"
                  className="hist__open"
                  title="Open this run in the workspace"
                  onClick={() => restoreRun(run)}
                >
                  <span className="hist__meta">
                    <span className={`chip chip--${run.tool}`}>{run.tool}</span>
                    {run.mode ? <span className="hist__mode">{run.mode}</span> : null}
                    <span className="hist__when">{ago(run.at)}</span>
                  </span>
                  <span className="hist__text">{preview(run.original)}</span>
                  <span className="hist__stats">
                    {run.wordsIn} → {run.wordsOut} words
                    {run.note ? ` · ${run.note}` : ""}
                    {run.model ? ` · ${run.model}` : ""}
                    {run.ms > 0 ? ` · ${(run.ms / 1000).toFixed(1)}s` : ""}
                  </span>
                </button>
                <button
                  type="button"
                  className="iconbtn iconbtn--tight"
                  title="Delete this run"
                  aria-label="Delete this run"
                  disabled={busy}
                  onClick={() => void remove(run.id)}
                >
                  <Icon name="trash" size={15} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
