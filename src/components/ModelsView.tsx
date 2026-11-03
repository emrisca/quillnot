import { useCallback, useEffect, useState } from "react";
import { Icon } from "../icons";
import { isDesktop, modelCatalogue, type RankedModel } from "../lib/openrouter";

const percent = (value: number | null): string =>
  value === null ? "—" : `${(value * 100).toFixed(value >= 0.995 ? 0 : 1)}%`;

const millis = (value: number | null): string =>
  value === null ? "—" : `${Math.round(value)} ms`;

const rate = (value: number | null): string =>
  value === null ? "—" : `${Math.round(value)} tok/s`;

const scoreText = (value: number | null): string => (value === null ? "—" : value.toFixed(0));

/**
 * What the numbers actually say about the endpoint right now. Most free models
 * report a day-long uptime and nothing else, so "up" here means "reported up in the
 * last window OpenRouter filled in" — not "we measured this lately".
 */
const availability = (row: RankedModel): { label: string; tone: "up" | "down" | "unknown" } => {
  if (row.uptime === null) return { label: "No stats reported in any window", tone: "unknown" };
  if (row.uptime <= 0) return { label: "Reported down", tone: "down" };
  return { label: "Reported up", tone: "up" };
};

/**
 * The AI Models view: every zero-priced model, the best availability and speed any
 * provider reported for it, and the score that falls out of those numbers.
 *
 * The dashes matter. A free endpoint with no traffic in the window reports null, and
 * a table that rendered that as "0%" would rank a brand-new model last for the
 * crime of being new. A dash means no evidence; the evidence dots say how much the
 * score actually rests on.
 */
export function ModelsView() {
  const [rows, setRows] = useState<RankedModel[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!isDesktop()) {
      setError("Run the app (npm run tauri dev) to read the live catalogue; this browser preview has no Rust side to ask.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setRows(await modelCatalogue());
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const scored = rows?.filter((row) => row.score !== null).length ?? 0;

  return (
    <div className="mpage">
      <div className="mpage__head">
        <div className="mpage__titles">
          <h2 className="mpage__title">
            {rows ? `${rows.length} free models` : "Free models"}
            {rows ? <span className="mpage__sub"> · {scored} with live numbers</span> : null}
          </h2>
          <p className="mpage__note">
            From OpenRouter's endpoint stats: uptime from the most recent window that reports
            anything, latency and throughput over 30 minutes. A dash means no traffic in that
            window, not zero — and a score resting on a single signal sits below its face value,
            so an unmeasured model cannot outrank a measured one. The dots say how much stands
            behind each number. Our own per-task samples and model pinning arrive in M7.
          </p>
        </div>
        <button type="button" className="btn btn--sm btn--quiet" onClick={() => void load()} disabled={busy}>
          {busy ? "Reading…" : "Refresh"}
        </button>
      </div>

      {error ? (
        <p className="errline">
          <Icon name="warning" size={14} />
          <span>{error}</span>
        </p>
      ) : null}

      <div className="mcard">
        {busy && !rows ? (
          <p className="empty" style={{ padding: "18px" }}>
            Asking OpenRouter about every free endpoint…
          </p>
        ) : rows && rows.length > 0 ? (
          <table className="mtable">
            <thead>
              <tr>
                <th className="mrank">#</th>
                <th>Model</th>
                <th className="mnum">Uptime</th>
                <th className="mnum">Latency</th>
                <th className="mnum">Throughput</th>
                <th className="mnum">Score</th>
                <th>Evidence</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row, index) => (
                <tr key={row.id}>
                  <td className="mrank">{index + 1}</td>
                  <td>
                    <span
                      className={`mstate is-${availability(row).tone}`}
                      title={availability(row).label}
                    />
                    <span className="mname">{row.name ?? row.id}</span>
                    <span className="mid"> {row.id}</span>
                    <span className="mpills">
                      <span className="mpill">{row.providers} provider{row.providers === 1 ? "" : "s"}</span>
                      {row.contextLength ? (
                        <span className="mpill">{Math.round(row.contextLength / 1000)}k ctx</span>
                      ) : null}
                    </span>
                  </td>
                  <td className={`mnum${row.uptime === null ? " mdash" : ""}`}>
                    {percent(row.uptime)}
                    {row.uptimeWindow ? <span className="mwindow">{row.uptimeWindow}</span> : null}
                  </td>
                  <td className={`mnum${row.latencyMs === null ? " mdash" : ""}`}>{millis(row.latencyMs)}</td>
                  <td className={`mnum${row.throughput === null ? " mdash" : ""}`}>{rate(row.throughput)}</td>
                  <td className="mnum">
                    <span className="mscore">
                      <span className="mscore__bar">
                        <span
                          className="mscore__fill"
                          style={{ width: `${Math.max(2, Math.min(100, row.score ?? 0))}%` }}
                        />
                      </span>
                      <span className={row.score === null ? "mdash" : ""}>{scoreText(row.score)}</span>
                    </span>
                  </td>
                  <td>
                    <span className="mevidence" title={`${row.evidence} of 2 signals reported`}>
                      <i className={row.evidence >= 1 ? "on" : ""} />
                      <i className={row.evidence >= 2 ? "on" : ""} />
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : rows ? (
          <p className="empty" style={{ padding: "18px" }}>
            No zero-priced text models came back. That is unusual — refresh in a moment.
          </p>
        ) : null}
      </div>
    </div>
  );
}
