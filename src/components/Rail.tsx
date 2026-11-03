import { Icon } from "../icons";
import { readTone } from "../lib/tone";

/** The views the right rail can open. */
export type SideView = "history" | "stats";

type Props = {
  onOpen: (view: SideView) => void;
  /** The text whose tone to read — the rewrite, once there is one. */
  text?: string;
  /** Whether the result pane is currently showing the sentence comparison. */
  comparing?: boolean;
  /** Given only by a view that has two texts to compare. */
  onCompare?: () => void;
};

/**
 * The right rail: the two views that replace the workspace, a comparison toggle, and a
 * reading of the tone.
 *
 * Tone sits here rather than in the result pane because it describes the text as a
 * whole, and it updates as the text does without anyone asking it to — a rewrite that
 * becomes stiff should not need a click to notice.
 */
export function Rail({ onOpen, text, comparing = false, onCompare }: Props) {
  const tone = text ? readTone(text) : null;

  return (
    <aside className="rail" aria-label="Views">
      <button type="button" className="rail__item" onClick={() => onOpen("history")} title="Every run you have kept">
        <Icon name="history" size={19} />
        <span>History</span>
      </button>

      <button type="button" className="rail__item" onClick={() => onOpen("stats")} title="What you have run, and on which models">
        <Icon name="stats" size={19} />
        <span>Statistics</span>
      </button>

      <button
        type="button"
        className={`rail__item${comparing ? " is-on" : ""}`}
        disabled={!onCompare}
        aria-pressed={comparing}
        title={onCompare ? "Read the rewrite sentence by sentence" : "Available once there is a rewrite"}
        onClick={() => onCompare?.()}
      >
        <Icon name="compare" size={19} />
        <span>Compare</span>
      </button>

      <div className="rail__panel" title="Counted from the words in the rewrite: contractions, hedges, boosters, connectives and warmth.">
        <span className="rail__panelhead">
          <Icon name="tone" size={13} /> Tone
        </span>
        {tone ? (
          <>
            <span className="rail__tone">{tone.label}</span>
            <span className="rail__meter" aria-hidden="true">
              <i style={{ width: `${tone.certainty}%` }} />
            </span>
            <span className="rail__tonenote">{tone.note}</span>
            {tone.signals.length > 0 ? <span className="rail__signals">{tone.signals.join(" · ")}</span> : null}
          </>
        ) : (
          <span className="rail__tonenote">A reading appears once there is a paragraph to read.</span>
        )}
      </div>
    </aside>
  );
}
