import { useCallback, useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import {
  isDesktop,
  modelCatalogue,
  setAutoModel,
  setPinnedModel,
  type Catalogue,
  type ModelTask,
  type RankedModel,
} from "../lib/openrouter";

const percent = (value: number | null): string =>
  value === null ? "—" : `${(value * 100).toFixed(value >= 0.995 ? 0 : 1)}%`;

const millis = (value: number | null): string =>
  value === null ? "—" : `${Math.round(value)} ms`;

const rate = (value: number | null): string =>
  value === null ? "—" : `${Math.round(value)} tok/s`;

const scoreText = (value: number | null): string => (value === null ? "—" : value.toFixed(0));

const TASKS: { id: ModelTask; label: string }[] = [
  { id: "paraphrase", label: "Paraphrase" },
  { id: "grammar", label: "Grammar" },
];

type SortKey = "rank" | "name" | "uptime" | "latency" | "ours" | "runs" | "score";

const SORTS: { key: SortKey; label: string; className?: string }[] = [
  { key: "rank", label: "#", className: "mrank" },
  { key: "name", label: "Model" },
  { key: "uptime", label: "Uptime", className: "mnum" },
  { key: "latency", label: "Reported", className: "mnum" },
  { key: "ours", label: "Ours", className: "mnum" },
  { key: "runs", label: "Calls", className: "mnum" },
  { key: "score", label: "Score", className: "mnum" },
];

/**
 * What the numbers actually say about the endpoint right now. Most free models
 * report a day-long uptime and nothing else, so "up" here means "reported up in the
 * last window OpenRouter filled in" — not "we measured this lately". The `Ours`
 * column is the measured half; a dash there means this app has never called it.
 */
const availability = (row: RankedModel): { label: string; tone: "up" | "down" | "unknown" } => {
  if (row.uptime === null) return { label: "No stats reported in any window", tone: "unknown" };
  if (row.uptime <= 0) return { label: "Reported down", tone: "down" };
  return { label: "Reported up", tone: "up" };
};

/** A missing number sorts last whichever way the column is sorted. */
function compare(a: RankedModel, b: RankedModel, key: SortKey): number {
  if (key === "name") return (a.name ?? a.id).localeCompare(b.name ?? b.id);
  if (key === "rank") return 0;

  const pick = (row: RankedModel): number | null => {
    if (key === "uptime") return row.uptime;
    if (key === "latency") return row.latencyMs;
    if (key === "ours") return row.ourLatencyMs;
    if (key === "runs") return row.runs;
    return row.score;
  };

  const left = pick(a);
  const right = pick(b);
  if (left === null && right === null) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return left - right;
}

/**
 * The AI Models view: every zero-priced model, scored for the task you are about to
 * run, with what Auto picks and a pin for when you would rather decide.
 *
 * Two numbers sit side by side on purpose. The platform's are what OpenRouter reports
 * and are usually a day-long uptime; ours are the calls this app made itself. The
 * dashes matter: no evidence is not zero, and a table that rendered a missing sample
 * as "0 ms" would rank a model it has never tried above one it has.
 */
export function ModelsView() {
  const [task, setTask] = useState<ModelTask>("paraphrase");
  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; dir: 1 | -1 }>({ key: "score", dir: -1 });

  const load = useCallback(async (which: ModelTask) => {
    if (!isDesktop()) {
      setError(
        "Run the app (npm run tauri dev) to read the live catalogue; this browser preview has no Rust side to ask.",
      );
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setCatalogue(await modelCatalogue(which));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load(task);
  }, [load, task]);

  async function choose(action: () => Promise<void>) {
    try {
      await action();
      await load(task);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  function toggleSort(key: SortKey) {
    setSort((current) =>
      current.key === key ? { key, dir: current.dir === 1 ? -1 : 1 } : { key, dir: -1 },
    );
  }

  const rows = useMemo(() => {
    if (!catalogue) return [];
    if (sort.key === "rank") return catalogue.rows;
    return [...catalogue.rows].sort((a, b) => {
      const order = compare(a, b, sort.key);
      // Ties fall back to the measured ranking, so a sort never shuffles for no reason.
      return order === 0 ? 0 : order * sort.dir;
    });
  }, [catalogue, sort]);

  const scored = catalogue?.rows.filter((row) => row.score !== null).length ?? 0;
  const measured = catalogue?.rows.filter((row) => row.runs > 0).length ?? 0;

  return (
    <div className="mpage">
      <div className="mpage__head">
        <div className="mpage__titles">
          <h2 className="mpage__title">
            {catalogue ? `${catalogue.rows.length} free models` : "Free models"}
            {catalogue ? (
              <span className="mpage__sub">
                {" "}
                · {scored} with numbers · {measured} we have called
              </span>
            ) : null}
          </h2>
          <p className="mpage__note">
            Scored for the task you pick, from two sources. One is what OpenRouter reports:
            uptime from the most recent window that says anything, plus latency and
            throughput over 30 minutes. The other is every call this app has made, timed per
            model and per task. Our numbers outweigh the platform's as they accumulate, and
            three failures in a row demote a model until it answers again. A dash means no
            evidence, never zero. Until this app has called a model for a task, Auto has
            nothing to go on and leaves the run to the gateway router rather than picking
            whichever id sorts first.
          </p>
        </div>
        <button
          type="button"
          className="btn btn--sm btn--quiet"
          onClick={() => void load(task)}
          disabled={busy}
        >
          {busy ? "Reading…" : "Refresh"}
        </button>
      </div>

      <div className="mtoolbar">
        <div className="mtasks">
          {TASKS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={`chip-btn${task === entry.id ? " is-on" : ""}`}
              aria-pressed={task === entry.id}
              onClick={() => setTask(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>

        <div className="mauto">
          <button
            type="button"
            className={`chip-btn${catalogue?.auto ? " is-on" : ""}`}
            aria-pressed={Boolean(catalogue?.auto)}
            disabled={!isDesktop() || busy}
            title="Let the ranking choose the model for each run"
            onClick={() => void choose(() => setAutoModel(!catalogue?.auto))}
          >
            Auto
          </button>
          <span className="mauto__note">
            {catalogue === null
              ? "Reading the ranking…"
              : catalogue.pinned
                ? `pinned to ${catalogue.pinned}, so Auto is set aside`
                : catalogue.auto
                  ? catalogue.autoChoice
                    ? `would send this task to ${catalogue.autoChoice}`
                    : "nothing measured yet, so your provider's configured model answers"
                  : "off, so your provider's configured model answers"}
          </span>
        </div>
      </div>

      {error ? (
        <p className="errline">
          <Icon name="warning" size={14} />
          <span>{error}</span>
        </p>
      ) : null}

      <div className="mcard">
        {busy && !catalogue ? (
          <p className="empty" style={{ padding: "18px" }}>
            Asking OpenRouter about every free endpoint…
          </p>
        ) : rows.length > 0 ? (
          <table className="mtable">
            <thead>
              <tr>
                {SORTS.map((column) => (
                  <th
                    key={column.key}
                    className={`${column.className ?? ""} msort`}
                    onClick={() => toggleSort(column.key)}
                    title={`Sort by ${column.label.replace("#", "rank")}`}
                  >
                    {column.label}
                    <span className="msort__mark">
                      {sort.key === column.key ? (sort.dir === 1 ? "▲" : "▼") : ""}
                    </span>
                  </th>
                ))}
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
                    {row.chosen ? <span className="chip chip--live">Auto</span> : null}
                    {row.streak > 0 ? (
                      <span
                        className="mfail"
                        title={`${row.streak} failure${row.streak === 1 ? "" : "s"} in a row`}
                      >
                        {row.streak} fail{row.streak === 1 ? "" : "s"}
                      </span>
                    ) : null}
                    <span className="mpills">
                      <span className="mpill">
                        {row.providers} provider{row.providers === 1 ? "" : "s"}
                      </span>
                      {row.contextLength ? (
                        <span className="mpill">{Math.round(row.contextLength / 1000)}k ctx</span>
                      ) : null}
                    </span>
                    <button
                      type="button"
                      className={`mpin${row.pinned ? " is-pinned" : ""}`}
                      disabled={!isDesktop() || busy}
                      title={row.pinned ? "Clear the pin" : "Always send this task to this model"}
                      onClick={() =>
                        void choose(() => setPinnedModel(row.pinned ? null : row.id))
                      }
                    >
                      {row.pinned ? "pinned" : "pin"}
                    </button>
                  </td>
                  <td className={`mnum${row.uptime === null ? " mdash" : ""}`}>
                    {percent(row.uptime)}
                    {row.uptimeWindow ? <span className="mwindow">{row.uptimeWindow}</span> : null}
                  </td>
                  <td className={`mnum${row.latencyMs === null ? " mdash" : ""}`}>
                    {millis(row.latencyMs)}
                  </td>
                  <td className={`mnum${row.ourLatencyMs === null ? " mdash" : ""}`}>
                    {millis(row.ourLatencyMs)}
                    {row.runs > 0 ? (
                      <span className="mwindow">
                        {percent(row.successRate)}
                      </span>
                    ) : null}
                  </td>
                  <td className={`mnum${row.runs === 0 ? " mdash" : ""}`}>{row.runs || "—"}</td>
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
        ) : catalogue ? (
          <p className="empty" style={{ padding: "18px" }}>
            No zero-priced text models came back. That is unusual. Refresh in a moment.
          </p>
        ) : null}
      </div>
    </div>
  );
}
