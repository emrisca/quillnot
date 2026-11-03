# QuillNot

A desktop paraphrasing and grammar-checking workspace, in the shape people already know,
running on [OpenRouter](https://openrouter.ai)'s zero-priced models. No subscription, no
quota you have to buy, no text leaving your machine except to the model you picked.

Built with Tauri v2, React 19 and Vite.

## Status

**M1 — the workspace exists, on mock output.** Both tools are laid out end to end: mode
tabs, the two-pane editor, per-pane word counts, the action rail and the suggestion card.
The paraphrase and grammar results are produced locally by `src/lib/paraphrase.ts` and
`src/lib/grammar.ts` so the layout can be judged before an API sits under it. The `Mock`
badge in the header marks that honestly.

[PLAN.md](PLAN.md) has the milestones: the OpenRouter client next, then the real grammar
checker, history and statistics, then the updater and releases.

## Running it

```bash
npm install
npm run tauri dev      # the app
npm run dev            # the frontend alone, on http://localhost:1421
npm run typecheck
npm run tauri build    # a bundle in src-tauri/target/release/bundle
```

No API key is needed for this milestone. From M2 the key is entered in Settings, stored in
the OS keychain, and used only from Rust.

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
