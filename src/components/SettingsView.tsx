import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { defaultModel, setDefaultModel } from "../lib/defaults";
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

  // The provider list is long enough to swamp this page, so it opens in its own window
  // over it. The page keeps one compact row that says what the window is for.
  const [providersOpen, setProvidersOpen] = useState(false);
  const [model, setModel] = useState("");

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
    void defaultModel().then((saved) => {
      if (live && saved) setModel(saved);
    });
    return () => {
      live = false;
    };
  }, []);

  // Escape closes the providers window, the way every other transient surface here does.
  useEffect(() => {
    if (!providersOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setProvidersOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [providersOpen]);

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
        <section className="card">
          <header className="card__head">
            <span className="card__icon">
              <Icon name="key" size={17} />
            </span>
            <h2>Providers</h2>
          </header>
          <div className="row">
            <div className="row__text">
              <span className="row__label">Free tiers, keys and accounts</span>
              <span className="row__detail">
                Every provider QuillNot can talk to, each key kept in the system keychain.
                The list is longer than this page, so it opens in its own window.
              </span>
            </div>
            <div className="row__control">
              <button
                type="button"
                className="btn btn--sm"
                onClick={() => setProvidersOpen(true)}
              >
                Manage providers
              </button>
            </div>
          </div>
        </section>

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

          <div className="row">
            <div className="row__text">
              <span className="row__label">Default model</span>
              <span className="row__detail">
                The model every run starts with when nothing is pinned. Leave it empty and
                each provider answers with the model it prefers.
              </span>
            </div>
            <div className="row__control">
              <input
                className="input"
                value={model}
                placeholder="the provider's own default"
                aria-label="Default model"
                disabled={!isDesktop()}
                onChange={(event) => setModel(event.target.value)}
                onBlur={() => {
                  void setDefaultModel(model).then(() =>
                    setNote(
                      model.trim()
                        ? `Runs now start with ${model.trim()} where that provider offers it.`
                        : "Default model cleared.",
                    ),
                  );
                }}
              />
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

      {providersOpen ? (
        <div
          className="sheet"
          role="dialog"
          aria-modal="true"
          aria-label="Providers"
          onClick={(event) => {
            if (event.target === event.currentTarget) setProvidersOpen(false);
          }}
        >
          <div className="card sheet__panel">
            <header className="sheet__head">
              <h2 className="sheet__title">Providers</h2>
              <span className="sheet__note">
                Keys live in the system keychain. Nothing here is sent to QuillNot.
              </span>
              <button
                type="button"
                className="btn btn--sm btn--quiet"
                onClick={() => setProvidersOpen(false)}
              >
                Close
              </button>
            </header>
            <div className="sheet__body">
              <ProviderList />
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
