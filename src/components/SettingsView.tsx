import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { setStatus, useConnection } from "../lib/connection";
import { clearKey, freeModels, isDesktop, saveKey, signIn } from "../lib/openrouter";

const VERSION = "0.1.0";

/**
 * The account, for real: sign in through OpenRouter's PKCE flow, or paste a key.
 * The key never comes back to this file — only whether one is stored, and a hint
 * showing its two ends.
 */
function OpenRouterCard() {
  const connection = useConnection();
  const [busy, setBusy] = useState<null | "signin" | "save" | "disconnect">(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [modelCount, setModelCount] = useState<number | null>(null);

  useEffect(() => {
    if (!connection.connected || !isDesktop()) return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await freeModels();
        if (!cancelled) setModelCount(list.length);
      } catch {
        // The count is a nicety; a failed catalogue fetch must not look like a failure
        // to connect.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [connection.connected]);

  async function attempt(which: "signin" | "save" | "disconnect", action: () => Promise<void>) {
    setBusy(which);
    setError(null);
    try {
      await action();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="card">
      <header className="card__head">
        <span className="card__icon">
          <Icon name="key" size={17} />
        </span>
        <h2>OpenRouter account</h2>
        <span className={`statusdot${connection.connected ? " is-on" : ""}`} />
        <span className="card__state">
          {connection.connected ? `Connected · ${connection.hint ?? ""}` : "Not connected"}
        </span>
      </header>

      {error ? (
        <p className="errline">
          <Icon name="warning" size={14} />
          <span>{error}</span>
        </p>
      ) : null}

      {connection.connected ? (
        <div className="setrow">
          <button
            type="button"
            className="btn btn--sm"
            disabled={busy !== null}
            onClick={() =>
              void attempt("disconnect", async () => {
                setStatus(await clearKey());
                setModelCount(null);
              })
            }
          >
            {busy === "disconnect" ? "Disconnecting…" : "Disconnect"}
          </button>
          <span className="muted-note">
            {modelCount === null
              ? "Reading the free catalogue…"
              : `${modelCount} zero-priced models available. The ranked table arrives in M7.`}
          </span>
        </div>
      ) : (
        <>
          <div className="setrow">
            <button
              type="button"
              className="btn btn--sm"
              disabled={busy !== null || !isDesktop()}
              onClick={() => void attempt("signin", async () => setStatus(await signIn()))}
            >
              {busy === "signin" ? "Waiting for the browser…" : "Sign in with OpenRouter"}
            </button>
            <span className="muted-note">
              {isDesktop()
                ? "Authorises QuillNot against your own account. Requests are billed to your credits and the key is stored in the OS keychain."
                : "Run the app (npm run tauri dev) to sign in; this browser preview cannot hold a keychain entry."}
            </span>
          </div>

          <div className="setrow">
            <input
              className="input"
              type="password"
              value={draft}
              placeholder="…or paste an sk-or- key"
              onChange={(event) => setDraft(event.target.value)}
              spellCheck={false}
              aria-label="OpenRouter API key"
            />
            <button
              type="button"
              className="btn btn--sm btn--quiet"
              disabled={busy !== null || !draft.trim()}
              onClick={() =>
                void attempt("save", async () => {
                  setStatus(await saveKey(draft.trim()));
                  setDraft("");
                })
              }
            >
              {busy === "save" ? "Saving…" : "Save key"}
            </button>
          </div>
        </>
      )}
    </section>
  );
}

const SECTIONS = [
  {
    title: "Updates",
    icon: "download",
    rows: [
      {
        label: "Version",
        detail: `QuillNot ${VERSION}`,
        action: "Check for updates",
        milestone: "M5",
      },
      {
        label: "Channel",
        detail: "Reads latest.json from this repository's GitHub releases, signed with the release key.",
        action: "Stable",
        milestone: "M5",
      },
    ],
  },
  {
    title: "Data",
    icon: "stats",
    rows: [
      {
        label: "History",
        detail: "Every run is kept locally so you can reopen it. Nothing is uploaded and there is no account to make.",
        action: "—",
        milestone: "M4",
      },
      {
        label: "Statistics",
        detail: "Words processed, runs per mode, models used — from the local history, not from a server.",
        action: "—",
        milestone: "M4",
      },
    ],
  },
] as const;

export function SettingsView() {
  return (
    <div className="settings">
      <div className="settings__inner">
        <OpenRouterCard />

        {SECTIONS.map((section) => (
          <section key={section.title} className="card">
            <header className="card__head">
              <span className="card__icon">
                <Icon name={section.icon} size={17} />
              </span>
              <h2>{section.title}</h2>
            </header>

            {section.rows.map((row) => (
              <div key={row.label} className="row">
                <div className="row__text">
                  <span className="row__label">{row.label}</span>
                  <span className="row__detail">{row.detail}</span>
                </div>
                <div className="row__control">
                  <span className="chip chip--milestone">{row.milestone}</span>
                  <span className="row__action">{row.action}</span>
                </div>
              </div>
            ))}
          </section>
        ))}

        <p className="settings__note">
          QuillNot reads and writes your text and calls the model you pick, nothing else.
        </p>
      </div>
    </div>
  );
}
