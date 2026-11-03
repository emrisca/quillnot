import { MODES, type Mode } from "../lib/paraphrase";

type Props = {
  mode: Mode;
  onMode: (mode: Mode) => void;
  strength: number;
  onStrength: (value: number) => void;
};

/**
 * The mode row. The slider is QuillBot's synonym dial: in M2 it becomes part of
 * the prompt ("rewrite as loosely as this allows"), and it already gates how many
 * substitutions the mock applies, so it is not decoration.
 */
export function ModeTabs({ mode, onMode, strength, onStrength }: Props) {
  return (
    <div className="modebar">
      <div className="modebar__tabs" role="tablist" aria-label="Rewrite mode">
        {MODES.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={mode === item.id}
            title={item.hint}
            className={`mode${mode === item.id ? " is-active" : ""}`}
            onClick={() => onMode(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      <label className="synonyms">
        <span>Synonyms</span>
        <input
          type="range"
          min={1}
          max={4}
          step={1}
          value={strength}
          onChange={(event) => onStrength(Number(event.target.value))}
          aria-label="Synonym strength"
        />
      </label>

      <span className="modebar__lang">English (US)</span>
    </div>
  );
}
