import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { clearHistory, defaultMode, listRuns, openDataDir, setDefaultMode } from "../lib/history";
import { isDesktop } from "../lib/openrouter";
import { MODES, type Mode } from "../lib/paraphrase";
import { ProviderList } from "./ProviderList";
import type { SideView } from "./Rail";

const VERSION = "0.1.0";

export function SettingsView({ onOpen }: { onOpen: (view: SideView) => void }) {
  const [mode, setMode] = useState<Mode>("standard");
  const [runs, setRuns] = useState<number | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void defaultMode().then((saved) => {
      if (live && saved) setMode(saved as Mode);
    });
    void listRuns().then((loaded) => {
      if (live) setRuns(loaded.length);
    });
    return () => {
      live = false;
    };
  }, []);

  async function choose(next: Mode) {
    setMode(next);
    await setDefaultMode(next);
  }

  async function wipe() {
    if (!window.confirm("Delete every saved run? This cannot be undone.")) return;
    await clearHistory();
    setRuns(0);
    setNote("History deleted.");
  }

  return (
    <div className="settings">
      <div className="settings__inner">
        <ProviderList />

        <section className="card">
          <header className="card__head">
            <span className="card__icon">
              <Icon name="paraphrase" size={17} />
            </span>
            <h2>Defaults</h2>
          </header>
          <div className="row">
            <div className="row__text">
              <span className="row__label">Default mode</span>
              <span className="row__detail">
                The mode a new rewrite starts in. Opening a run from history keeps that
                run's own mode.
              </span>
            </div>
            <div className="row__control">
              <select
                className="input input--select"
                value={mode}
                aria-label="Default rewrite mode"
                disabled={!isDesktop()}
                onChange={(event) => void choose(event.target.value as Mode)}
              >
                {MODES.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.label}
                  </option>
                ))}
              </select>
            </div>
          </div>
        </section>

        <section className="card">
          <header className="card__head">
            <span className="card__icon">
              <Icon name="stats" size={17} />
            </span>
            <h2>Data</h2>
          </header>

          <div className="row">
            <div className="row__text">
              <span className="row__label">History</span>
              <span className="row__detail">
                {runs === null
                  ? "Every run is kept locally so you can reopen it."
                  : `${runs} ${runs === 1 ? "run" : "runs"} saved in this app's data folder. Nothing is uploaded.`}
              </span>
            </div>
            <div className="row__control">
              <button type="button" className="btn btn--sm btn--quiet" onClick={() => onOpen("history")}>
                View
              </button>
              <button type="button" className="btn btn--sm btn--quiet" onClick={() => void openDataDir()} disabled={!isDesktop()}>
                Open folder
              </button>
            </div>
          </div>

          <div className="row">
            <div className="row__text">
              <span className="row__label">Statistics</span>
              <span className="row__detail">
                Words processed, runs per mode and which models answered — counted from
                the history above.
              </span>
            </div>
            <div className="row__control">
              <button type="button" className="btn btn--sm btn--quiet" onClick={() => onOpen("stats")}>
                Open
              </button>
            </div>
          </div>

          <div className="row">
            <div className="row__text">
              <span className="row__label">Delete history</span>
              <span className="row__detail">
                Removes every saved run. Your settings, keys and the model cache stay.
              </span>
            </div>
            <div className="row__control">
              <button
                type="button"
                className="btn btn--sm btn--quiet btn--danger"
                onClick={() => void wipe()}
                disabled={!isDesktop() || runs === 0}
              >
                Delete all
              </button>
            </div>
          </div>
        </section>

        <section className="card">
          <header className="card__head">
            <span className="card__icon">
              <Icon name="download" size={17} />
            </span>
            <h2>Updates</h2>
          </header>
          <div className="row">
            <div className="row__text">
              <span className="row__label">Version</span>
              <span className="row__detail">
                QuillNot {VERSION}. Releases read latest.json from this repository's
                GitHub releases, signed with the release key.
              </span>
            </div>
            <div className="row__control">
              <span className="chip chip--milestone">M5</span>
            </div>
          </div>
        </section>

        <p className="settings__note">
          {note ? `${note} ` : ""}
          QuillNot reads and writes your text and calls the provider you pick, nothing
          else. There is no QuillNot account and no server in the middle.
        </p>
      </div>
    </div>
  );
}
