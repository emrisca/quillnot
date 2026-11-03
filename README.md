# QuillNot

A desktop paraphrasing and grammar-checking workspace, in the shape people already know,
running on [OpenRouter](https://openrouter.ai)'s zero-priced models. No subscription, no
quota you have to buy, no text leaving your machine except to the model you picked.

Built with Tauri v2, React 19 and Vite.

## Status

**M7 — the ranking.** Rewrites stream from the free model you pick, the grammar checker only
offers a fix for a quote it found in your text, every run is kept locally in a JSON log that
History and Statistics read, and the app checks its own release feed for updates. The
workspace answers ⌘↩ and ⌘C anywhere, reads the tone of the text in the right rail, compares a
rewrite sentence by sentence, exports a run as Markdown, and offers Summarize as a ninth
mode. The AI Models view scores every free model for the task you are about to run — from
OpenRouter's endpoint stats plus this app's own timing of each call — sorts on any column, and
lets you leave the choice to **Auto** or pin one model. **Typer** is the last milestone; the
rail marks it dimmed.

The header's `Mock` badge means no account is connected and results come from the local mock
in `src/lib/paraphrase.ts`, so the app is usable before anyone signs in. Once a key is in the
keychain it reads `Live`.

[PLAN.md](PLAN.md) has the milestones and what each one is waiting on.

## Running it

```bash
npm install
npm run tauri dev      # the app
npm run dev            # the frontend alone, on http://localhost:1421
npm run typecheck
npm run tauri build    # a bundle in src-tauri/target/release/bundle
```

No API key is needed to look around: without one the tools answer from the mock. A key is
entered in Settings, stored in the OS keychain and used only from Rust.

## Releases

Tagging `v0.1.1` runs `.github/workflows/release.yml`, which builds macOS (Apple silicon and
Intel) and Windows, signs the updater archives and publishes the release with the
`latest.json` the app reads.

The signing key is a minisign pair made once with:

```bash
npx tauri signer generate -w .secrets/quillnot-updater.key
```

Its public half belongs in `tauri.conf.json` (`plugins.updater.pubkey`); its private half
belongs in the repository secret `TAURI_SIGNING_PRIVATE_KEY`, and must also be present
locally for `npm run tauri build` to emit updater artifacts. Losing it means existing
installs can no longer be updated — there is no way to re-sign an update for them.

The bundles are not notarised or code-signed, so the first launch on macOS needs
right-click → Open. Updates installed from inside the app are verified against the release
key instead.

## Layout

```
src/
  App.tsx                 the shell: rail, header, active tool
  components/
    Sidebar.tsx           the tool rail
    TopBar.tsx            brand, tool title, free-models and mock badges
    ModeTabs.tsx          the rewrite modes and the synonym slider
    ParaphraseView.tsx    editor · result · diff highlighting
    GrammarView.tsx       editor · issue cards · accept a fix
    SettingsView.tsx      key, model, updates, about (fields arrive per milestone)
  lib/
    diff.ts               word-level LCS used to highlight what changed
    paraphrase.ts         the eight modes, and the mock transport M2 replaces
    grammar.ts            local rule checks, the shape the model's issues take
    text.ts               word counting
    tools.ts              what the rail contains
  styles/index.css        the whole look, tokens first
src-tauri/                the Tauri v2 shell
```

## Licence

MIT.
