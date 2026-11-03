import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { refreshStatus } from "../lib/connection";
import { isDesktop, signIn } from "../lib/openrouter";
import {
  clearProviderKey,
  listProviders,
  openDocs,
  saveProviderKey,
  setActiveProvider,
  setCustomEndpoint,
  setProviderModel,
  type ProviderInfo,
} from "../lib/providers";

/**
 * The provider list: every free tier QuillNot can talk to, with the key for each one
 * stored separately in the OS keychain.
 *
 * The idea is lifted from FreeLLMAPI — every free tier is an OpenAI-compatible
 * endpoint, so the only things that differ are the base URL, the key and a default
 * model. What is *not* lifted is the middleman: there is no local router holding a
 * unified token, requests go from this app to the provider you picked.
 */
export function ProviderList() {
  const [rows, setRows] = useState<ProviderInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isDesktop()) return;
    let cancelled = false;
    void (async () => {
      try {
        const providers = await listProviders();
        if (!cancelled) setRows(providers);
      } catch (caught) {
        if (!cancelled) setError(caught instanceof Error ? caught.message : String(caught));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /** Applies a mutation's result, and keeps the header badge honest. */
  function apply(next: ProviderInfo[]) {
    setRows(next);
    setError(null);
    void refreshStatus();
  }

  // Counted from the stored keys, not from "can be used": a local endpoint needs no key,
  // so it is usable on a fresh install, and counting that as connected would report an
  // account nobody added.
  const connected = rows?.filter((row) => row.hint !== null).length ?? 0;
  const active = rows?.find((row) => row.active);

  return (
    <section className="card">
      <header className="card__head">
        <span className="card__icon">
          <Icon name="key" size={17} />
        </span>
        <h2>Providers</h2>
        <span className={`statusdot${active?.connected ? " is-on" : ""}`} />
        <span className="card__state">
          {rows === null
            ? "Reading…"
            : `${connected} connected · ${active?.label ?? "none"} answers rewrites`}
        </span>
      </header>

      <p className="prov__intro">
        Every provider here is OpenAI-compatible and has a free tier, so QuillNot only
        needs a base URL, a key and a model. Keys live in the OS keychain, one entry
        each; requests go straight from this app to the provider, and the active one
        takes over the others if it is rate limited.
      </p>

      {error ? (
        <p className="errline">
          <Icon name="warning" size={14} />
          <span>{error}</span>
        </p>
      ) : null}

      {!isDesktop() ? (
        <p className="muted-note">
          Run the app (npm run tauri dev) to manage providers. A browser tab cannot hold a
          keychain entry.
        </p>
      ) : rows === null ? null : (
        <div className="prov__list">
          {rows.map((row) => (
            <ProviderRow key={row.id} row={row} onChange={apply} onError={setError} />
          ))}
        </div>
      )}
    </section>
  );
}

function ProviderRow({
  row,
  onChange,
  onError,
}: {
  row: ProviderInfo;
  onChange: (rows: ProviderInfo[]) => void;
  onError: (message: string | null) => void;
}) {
  const [keyDraft, setKeyDraft] = useState("");
  const [modelDraft, setModelDraft] = useState(row.model);
  const [baseDraft, setBaseDraft] = useState(row.baseUrl);
  const [busy, setBusy] = useState<string | null>(null);
  const custom = row.id === "custom";

  // A model saved elsewhere (or a provider switch) must land in the field.
  useEffect(() => {
    setModelDraft(row.model);
    setBaseDraft(row.baseUrl);
  }, [row.model, row.baseUrl]);

  async function run(which: string, action: () => Promise<ProviderInfo[]>) {
    setBusy(which);
    onError(null);
    try {
      onChange(await action());
    } catch (caught) {
      onError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(null);
    }
  }

  const modelChanged = modelDraft.trim() !== row.model;
  const baseChanged = custom && baseDraft.trim() !== row.baseUrl;
  // A local endpoint is "connected" without a key, so the key fields key off the hint
  // rather than the status dot.
  const hasKey = row.hint !== null;

  return (
    <div className={`prov${row.active ? " is-active" : ""}`}>
      <div className="prov__top">
        <span className={`statusdot${row.connected ? " is-on" : ""}`} />
        <span className="prov__name">{row.label}</span>
        {row.active ? <span className="chip chip--live">Active</span> : null}
        <button
          type="button"
          className="btn btn--sm btn--quiet prov__use"
          disabled={busy !== null || row.active || !row.connected}
          title={
            row.connected
              ? "Send rewrites to this provider"
              : "Add a key first"
          }
          onClick={() => void run("use", () => setActiveProvider(row.id))}
        >
          {busy === "use" ? "Switching…" : row.active ? "In use" : "Use"}
        </button>
      </div>

      <p className="prov__free">
        {row.freeTier}{" "}
        <button
          type="button"
          className="linkbtn"
          onClick={() => void openDocs(row.docs)}
        >
          Docs
        </button>
      </p>

      <div className="prov__fields">
        {custom ? (
          <label className="prov__field">
            <span>Base URL</span>
            <input
              className="input input--flex"
              value={baseDraft}
              placeholder="http://127.0.0.1:11434/v1"
              spellCheck={false}
              onChange={(event) => setBaseDraft(event.target.value)}
            />
          </label>
        ) : null}

        <label className="prov__field">
          <span>Model</span>
          <input
            className="input input--flex"
            value={modelDraft}
            placeholder={row.defaultModel || "model id"}
            spellCheck={false}
            onChange={(event) => setModelDraft(event.target.value)}
          />
        </label>

        <button
          type="button"
          className="btn btn--sm btn--quiet"
          disabled={busy !== null || (!modelChanged && !baseChanged)}
          onClick={() =>
            void run("model", () =>
              custom
                ? setCustomEndpoint(baseDraft, modelDraft)
                : setProviderModel(row.id, modelDraft),
            )
          }
        >
          {busy === "model" ? "Saving…" : "Save"}
        </button>
      </div>

      <div className="prov__fields">
        {hasKey ? (
          <>
            <span className="prov__key">{row.hint ?? "key stored"}</span>
            <button
              type="button"
              className="btn btn--sm btn--quiet"
              disabled={busy !== null}
              onClick={() => void run("remove", () => clearProviderKey(row.id))}
            >
              {busy === "remove" ? "Removing…" : "Remove key"}
            </button>
          </>
        ) : (
          <>
            <input
              className="input input--flex"
              type="password"
              value={keyDraft}
              placeholder={row.keyPrefix ? `${row.keyPrefix}…` : "Paste the API key"}
              spellCheck={false}
              aria-label={`${row.label} API key`}
              onChange={(event) => setKeyDraft(event.target.value)}
            />
            <button
              type="button"
              className="btn btn--sm"
              disabled={busy !== null || !keyDraft.trim()}
              onClick={() =>
                void run("key", async () => {
                  const next = await saveProviderKey(row.id, keyDraft.trim());
                  setKeyDraft("");
                  return next;
                })
              }
            >
              {busy === "key" ? "Saving…" : "Save key"}
            </button>
          </>
        )}

        {row.supportsOAuth && !row.connected ? (
          <button
            type="button"
            className="btn btn--sm btn--quiet"
            disabled={busy !== null}
            onClick={() =>
              void run("signin", async () => {
                await signIn();
                return listProviders();
              })
            }
          >
            {busy === "signin" ? "Waiting for the browser…" : "Sign in instead"}
          </button>
        ) : null}
      </div>

      {!row.needsKey ? (
        <p className="prov__note">
          A local server needs no key. Leave the key empty and point the base URL at it.
        </p>
      ) : null}
    </div>
  );
}
