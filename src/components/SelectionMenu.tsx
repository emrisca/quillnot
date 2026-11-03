import { Icon } from "../icons";

type Props = {
  x: number;
  y: number;
  sentence: string;
  /** The selected word, when the selection is a single one: synonyms only make sense then. */
  word: string | null;
  synonyms: string[] | null;
  canRevert: boolean;
  busy: boolean;
  error: string | null;
  onRephrase: () => void;
  onRevert: () => void;
  onCopy: () => void;
  onSynonym: (word: string) => void;
  onClose: () => void;
};

/**
 * The menu that opens on a selection in the result pane. It is the same gesture the
 * paraphraser exists for, at the scale of one sentence: rephrase this, put the original
 * back, or swap one word.
 */
export function SelectionMenu({
  x,
  y,
  sentence,
  word,
  synonyms,
  canRevert,
  busy,
  error,
  onRephrase,
  onRevert,
  onCopy,
  onSynonym,
  onClose,
}: Props) {
  return (
    <div
      className="selmenu"
      role="menu"
      aria-label="Sentence actions"
      style={{ left: x, top: y }}
      // Keeping the focus where it is keeps the selection visible while the menu is used.
      onMouseDown={(event) => event.preventDefault()}
    >
      <button type="button" className="selmenu__item" disabled={busy} onClick={onRephrase}>
        <Icon name="paraphrase" size={15} />
        <span>{busy ? "Rephrasing…" : "Rephrase sentence"}</span>
      </button>
      <button
        type="button"
        className="selmenu__item"
        disabled={!canRevert || busy}
        title={canRevert ? "Put the original wording back" : "This sentence has no match in the original text"}
        onClick={onRevert}
      >
        <Icon name="arrow" size={15} />
        <span>Revert to original sentence</span>
      </button>
      <button type="button" className="selmenu__item" disabled={busy} onClick={onCopy}>
        <Icon name="copy" size={15} />
        <span>Copy sentence</span>
      </button>

      {error ? <p className="selmenu__error">{error}</p> : null}

      {word ? (
        <>
          <div className="selmenu__rule" />
          <p className="selmenu__heading">Synonyms</p>
          {synonyms === null ? (
            <p className="selmenu__note">Looking for alternatives…</p>
          ) : synonyms.length === 0 ? (
            <p className="selmenu__note">No alternatives for "{word}".</p>
          ) : (
            <ul className="selmenu__words">
              <li>
                <button type="button" className="selmenu__word is-current" onClick={onClose}>
                  <span>{word}</span>
                  <Icon name="check" size={14} />
                </button>
              </li>
              {synonyms.map((candidate) => (
                <li key={candidate}>
                  <button type="button" className="selmenu__word" onClick={() => onSynonym(candidate)}>
                    <span>{candidate}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : null}

      <button type="button" className="selmenu__close" onClick={onClose} aria-label="Close">
        <Icon name="close" size={13} />
      </button>
      <span className="selmenu__sentence" hidden>
        {sentence}
      </span>
    </div>
  );
}
