import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { runStats, type Stats } from "../lib/history";
import { modeLabel, type Mode } from "../lib/paraphrase";
import { isDesktop } from "../lib/openrouter";

/**
 * Counted from the local history, not from a server: every number here is a run this
 * app made and is willing to show the row for.
 */
export function StatsView({ onClose }: { onClose: () => void }) {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void runStats()
      .then((loaded) => {
        if (live) setStats(loaded);
      })
      .catch((caught: unknown) => {
        if (live) setError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      live = false;
    };
  }, []);

  const delta = stats?.wordsDelta ?? 0;

  return (
    <div className="side">
      <header className="side__head">
        <button type="button" className="iconbtn" onClick={onClose} title="Back to the workspace" aria-label="Back">
          <Icon name="arrow" size={17} />
        </button>
        <h2 className="side__title">Statistics</h2>
        <span className="count count--soft">local only</span>
      </header>

      <div className="side__body">
        {error ? (
          <p className="errline">
            <Icon name="warning" size={14} />
            <span>{error}</span>
          </p>
        ) : null}

        {!isDesktop() ? (
          <p className="empty">Statistics are counted from the desktop app's own history.</p>
        ) : stats === null ? (
          <p className="empty">Counting…</p>
        ) : stats.runs === 0 ? (
          <p className="empty">No runs yet — the numbers start with the first rewrite.</p>
        ) : (
          <>
            <div className="tiles">
              <div className="tile">
                <span className="tile__value">{stats.runs}</span>
                <span className="tile__label">runs</span>
              </div>
              <div className="tile">
                <span className="tile__value">{stats.wordsIn.toLocaleString()}</span>
                <span className="tile__label">words in</span>
              </div>
              <div className="tile">
                <span className="tile__value">{stats.wordsOut.toLocaleString()}</span>
                <span className="tile__label">words out</span>
              </div>
              <div className="tile">
                <span className={`tile__value${delta < 0 ? " is-down" : ""}`}>
                  {delta > 0 ? "+" : ""}
                  {delta.toLocaleString()}
                </span>
                <span className="tile__label">words net</span>
              </div>
            </div>

            <section className="card">
              <header className="card__head">
                <span className="card__icon">
                  <Icon name="grammar" size={15} />
                </span>
                <h2>What was run</h2>
              </header>
              {Object.entries(stats.perTool).map(([tool, count]) => (
                <div key={tool} className="brow">
                  <span className="brow__label">{tool === "grammar" ? "Grammar checks" : "Rewrites"}</span>
                  <span className="brow__bar">
                    <span
                      className="brow__fill"
                      style={{ width: `${Math.round((count / Math.max(stats.runs, 1)) * 100)}%` }}
                    />
                  </span>
                  <span className="brow__value">{count}</span>
                </div>
              ))}
              {Object.entries(stats.perMode).map(([mode, count]) => (
                <div key={mode} className="brow">
                  <span className="brow__label">{modeLabel(mode as Mode)}</span>
                  <span className="brow__bar">
                    <span
                      className="brow__fill"
                      style={{ width: `${Math.round((count / Math.max(stats.runs, 1)) * 100)}%` }}
                    />
                  </span>
                  <span className="brow__value">{count}</span>
                </div>
              ))}
            </section>

            <section className="card">
              <header className="card__head">
                <span className="card__icon">
                  <Icon name="models" size={15} />
                </span>
                <h2>Models that answered</h2>
              </header>
              {stats.models.map((entry) => (
                <div key={entry.model} className="brow">
                  <span className="brow__label brow__label--mono">{entry.model}</span>
                  <span className="brow__bar">
                    <span
                      className="brow__fill"
                      style={{ width: `${Math.round((entry.runs / Math.max(stats.models[0]?.runs ?? 1, 1)) * 100)}%` }}
                    />
                  </span>
                  <span className="brow__value">{entry.runs}</span>
                </div>
              ))}
            </section>

            <p className="settings__note">
              These are measured here, on your connection, from the rows in History.
              Nothing in this view was fetched from a server.
            </p>
          </>
        )}
      </div>
    </div>
  );
}
