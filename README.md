# QuillNot

A desktop paraphrasing and grammar-checking workspace on
[OpenRouter](https://openrouter.ai)'s zero-priced models. No subscription, no quota to buy,
and no text leaves your machine except to the model you picked.

Built with Tauri v2, React 19 and Vite.

## Status

Typer is the newest milestone, and it types on macOS and Windows. Rewrites stream from the free
model you pick, the grammar checker offers a fix only for a quote it found in your text, every
run is kept in a local JSON log that History and Statistics read, and the app checks its own
release feed for updates.

The workspace answers the run and copy shortcuts anywhere, reads tone in the right rail,
compares a rewrite sentence by sentence, exports a run as Markdown, and offers Summarize
alongside the other rewrite modes. AI Models scores every free model for the task you are about
to run, using OpenRouter's endpoint stats and this app's own timing of each call, sorts on any
column, and lets you leave the choice to Auto or pin one model.

AI Humanizer is a rewrite whose prompt is a third pane. The skill it shows is sent with every
block exactly as written, it can be edited, it is remembered between runs, and one of its
presets is this workspace's unslop style.

Typer types the text in its editor into whichever window has focus, one keystroke at a time, at
a human pace, with typos it notices and corrects and a deadline it paces itself to meet. The
macOS build posts Core Graphics events and needs Accessibility. The Windows build uses
`SendInput` with `KEYEVENTF_UNICODE`. A build with no injector, X11 so far, says so instead of
offering a button that does nothing. The stop chord is `⌘⌥⇧S` on macOS and
`Ctrl+Alt+Shift+S` on Windows, and it works while another window has focus. A run also refuses
to type into QuillNot itself.

Windows has no equivalent of `IsSecureEventInputEnabled`, which is how the macOS build knows a
password prompt holds the keyboard. There the system's secure desktop is what protects a
credential.

The `Mock` badge in the header means no account is connected, so results come from the local
mock in `src/lib/paraphrase.ts`. That makes the app usable before anyone signs in. Once a key
is in the keychain the badge reads `Live`.

[PLAN.md](PLAN.md) has the milestones and what each one is waiting on.

## Running it

```bash
npm install
npm run tauri dev      # the app
npm run dev            # the frontend alone, on http://localhost:1421
npm run typecheck
npm run tauri build    # a bundle in src-tauri/target/release/bundle
```

No API key is needed to look around: without one the tools answer from the mock. Add a key in
Settings; it is stored in the OS keychain and used only from Rust.

## Releases

Tagging a version runs `.github/workflows/release.yml`, which builds Apple silicon and Windows,
signs the updater archives and publishes the release with the `latest.json` the app reads.
Intel Macs are not built: the app targets Apple silicon.

The signing key is a minisign pair, made once with:

```bash
npx tauri signer generate -w .secrets/quillnot-updater.key
```

Its public half belongs in `tauri.conf.json` (`plugins.updater.pubkey`). The private half
belongs in the repository secret `TAURI_SIGNING_PRIVATE_KEY`, and must also be present locally
for `npm run tauri build` to emit updater artifacts. Losing it means existing installs can no
longer be updated, because there is no way to sign a new update for them.

The macOS bundles are ad-hoc signed, not notarised, so the first launch is blocked until you
allow it: open the app, then System Settings → Privacy & Security → Security → Open Anyway,
and authenticate. macOS 15 removed the Control-click shortcut, and it never worked for an app
macOS reports as damaged. Updates installed from inside the app are verified against the
release key instead. To check an `.app` against the usual causes, run
`skills/references/macos-bundle-check.sh /Applications/QuillNot.app` from the EmrysSkills
workspace.

## Layout

```
src/
  App.tsx                 the shell: rail, header, active tool
  components/
    Sidebar.tsx           the tool rail
    TopBar.tsx            brand, title, the free-models and mock badges
    ModeTabs.tsx          the rewrite modes and the synonym slider
    ParaphraseView.tsx    editor, result, diff highlighting
    GrammarView.tsx       editor, issue cards, accept a fix
    HumanizerView.tsx     text, result, and the skill sent with it
    TyperView.tsx         the plan, the controls, the run's state
    ModelsView.tsx        the scored table of free models
    SettingsView.tsx      providers, defaults, data, updates
    HistoryView.tsx       every kept run, reopenable
    StatsView.tsx         counts read from those runs
  lib/
    openrouter.ts         the commands and the streamed deltas
    paraphrase.ts         the nine modes and the browser mock
    humanizer.ts          the skill presets and the bundled request
    typer.ts              the commands behind the Typer view
    grammar.ts            local rule checks, the shape the model's issues take
    diff.ts               word-level LCS used to highlight what changed
    tools.ts              what the rail contains
  styles/index.css        the whole look, tokens first
src-tauri/                the Tauri v2 shell: keychain, providers, catalogue, Typer, updates
```

## Licence

MIT.
