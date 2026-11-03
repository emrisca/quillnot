import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { clearHistory, defaultMode, listRuns, openDataDir, setDefaultMode } from "../lib/history";
import { isDesktop } from "../lib/openrouter";
import { MODES, type Mode } from "../lib/paraphrase";
import {
  appVersion,
  checkUpdate,
  installUpdate,
  transfer,
  type UpdateInfo,
  type UpdateProgress,
} from "../lib/updates";
import { ProviderList } from "./ProviderList";
import type { SideView } from "./Rail";

export function SettingsView({ onOpen }: { onOpen: (view: SideView) => void }) {
  const [mode, setMode] = useState<Mode>("standard");
  const [runs, setRuns] = useState<number | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const [version, setVersion] = useState<string | null>(null);
  const [found, setFound] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState<"checking" | "installing" | null>(null);
  const [step, setStep] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    let live = true;
    void defaultMode().then((saved) => {
      if (live && saved) setMode(saved as Mode);
    });
    void listRuns().then((loaded) => {
      if (live) setRuns(loaded.length);
    });
    void appVersion().then((current) => {
      if (live) setVersion(current);
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

  async function check() {
    setBusy("checking");
    setError(null);
    setFound(null);
    try {
      const update = await checkUpdate();
      setFound(update);
      setChecked(true);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    setBusy("installing");
    setError(null);
    setStep(null);
    try {
      await installUpdate(setStep);
      // Only reached if the restart did not happen, which is the honest thing to say
      // rather than leaving the card spinning forever.
      setBusy(null);
      setNote("Update installed. Reopen QuillNot to use it.");
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      setBusy(null);
    }
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
                Words processed, runs per mode and which models answered, all counted from
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
                QuillNot {version ?? "…"}. Releases come from this repository's GitHub
                releases, and every download is checked against the release key, so a build
                signed by anyone else is refused before it replaces this one.
              </span>
            </div>
            <div className="row__control">
              <button
                type="button"
                className="btn btn--sm btn--quiet"
                onClick={() => void check()}
                disabled={!isDesktop() || busy !== null}
              >
                {busy === "checking" ? "Checking…" : "Check for updates"}
              </button>
            </div>
          </div>

          {error ? (
            <p className="update__error" role="status">
              {error}
            </p>
          ) : null}

          {checked && !found && !error ? (
            <p className="update__quiet" role="status">
              <Icon name="check" size={14} /> You are on the newest release.
            </p>
          ) : null}

          {found ? (
            <div className="update">
              <p className="update__head">
                Version {found.version} is available. You have {found.currentVersion}.
              </p>
              {found.notes ? <pre className="update__notes">{found.notes}</pre> : null}
              <div className="update__actions">
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => void install()}
                  disabled={busy !== null}
                >
                  {busy === "installing" ? "Installing…" : "Install and restart"}
                </button>
                {step ? <span className="update__step">{transfer(step)}</span> : null}
              </div>
            </div>
          ) : null}

          <p className="update__foot">
            An update replaces the app, nothing else: your keys stay in the keychain and
            your history stays in the data folder.
          </p>
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
