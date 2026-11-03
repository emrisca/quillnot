# QuillNot — plan of attack

A Tauri desktop app that does the two things people actually pay QuillBot for — **rewrite
this paragraph** and **fix what is wrong with it** — using OpenRouter's free models. No
subscription, no server of our own, no account beyond an OpenRouter key.

Status: **M1 in progress.** The shell, the workspace and both tool views exist against mock
data. Nothing talks to OpenRouter yet, and that is deliberate: the layout is the expensive
part to get wrong, and it is cheap to iterate on before an API sits underneath it.

---

## 1. What "free" actually buys, and where it bites

OpenRouter is an OpenAI-compatible gateway: `POST https://openrouter.ai/api/v1/chat/completions`,
`Authorization: Bearer <key>`, one body shape for every model. A model whose `pricing.prompt`
is `"0"` costs nothing to call, and `GET /api/v1/models` is the authority on which those are
today — 17 of them at the time of writing, all text-in/text-out except two music models. The
ones worth trying for rewriting:

| Model id | Context | Why it is on the list |
| --- | --- | --- |
| `thinkingmachines/inkling:free` | 1M | largest free general model; handles whole documents |
| `nvidia/nemotron-3.5-lightning:free` | 1M | fast, long context, good at instruction-following |
| `google/gemma-4-31b-it:free` | 262k | strong prose quality for its size |
| `nvidia/nemotron-3-ultra-550b-a55b:free` | 1M | heaviest reasoning, slower first token |
| `openrouter/free` | 200k | the gateway's own router across free models |

The list is **not** a constant in the app. It is fetched from `/models`, filtered to
`pricing.prompt == 0`, and shown in a picker; a hardcoded free model silently stops being free
the moment a provider changes a price. The picker keeps the last good list cached so the app is
usable offline, and marks entries that have disappeared from upstream.

**Rate limits are the real constraint, not price.** Free models are capped per account (a small
number of requests per minute and per day, plus a lifetime credit ceiling before a key may use
them at all), and a busy provider returns 429 with a `Retry-After`-ish hint. So the client is
built around limits rather than around a spinner:

- one request at a time per window, with the remaining-requests header surfaced in the UI;
- exponential backoff on 429/5xx, and a **fallback chain** — if the chosen model refuses, the
  next free model in the picker is tried before the user sees an error;
- long text is chunked on paragraph boundaries and reassembled, because a 12k-word document in
  one call will both time out and burn the daily quota;
- streaming, so a slow free model still shows progress instead of an apparently hung window.

## 2. Decisions that shape the code

**The key never reaches the webview.** It is written once, kept in the OS keychain (macOS
Keychain, Windows Credential Manager) through Rust, and every request is made from Rust. The
frontend calls an `invoke("paraphrase", …)` command and receives text events; it cannot read the
key, and a compromised page in the webview cannot exfiltrate it. Settings shows only a masked
fingerprint of the stored key.

**Never trust a model's character offsets.** A grammar checker needs to know *where* each
problem is, so the model is asked for `{original, replacement, message}` triples and the app
finds `original` in the text itself, verifying it is unique before offering a fix. Models
recount offsets wrong almost every time; they quote text correctly. Where a quote appears more
than once, the issue is shown without a position rather than applied to the wrong sentence.

**Structure by schema, not by parsing prose.** Requests use structured outputs (JSON schema)
where the model supports it, and fall back to a fenced-JSON instruction plus a tolerant parser
that repairs a truncated final object. A grammar run that returns unparseable JSON shows the raw
reply behind a disclosure rather than a generic failure.

**Whole-document paraphrase keeps the document's shape.** Paragraph breaks, lists and heading
lines are preserved: the splitter emits blocks, each block is rewritten on its own, and the
blocks are rejoined. Otherwise a paraphrase of an essay arrives as one wall of text, which is
the single most common complaint about every one of these tools.

**Local history, stored in the app's own data directory** — never in the repo, never uploaded.
Each run stores the original, the result, the mode, the model and the token counts, so the
statistics view is real numbers rather than decoration.

## 3. Architecture

```
┌──────────────────────────── QuillNot (Tauri v2) ───────────────────────────┐
│  React 19 + Vite                              Rust core                   │
│  ┌─────────────────────────────┐   invoke    ┌──────────────────────────┐  │
│  │ Sidebar · mode tabs         │ ──────────▶ │ openrouter.rs            │  │
│  │ two-pane workspace          │ ◀────────── │  keychain · reqwest      │  │
│  │ issue cards · diff output   │  events     │  streaming · backoff     │  │
│  └─────────────────────────────┘             │  chunker · fallbacks     │  │
│  lib/diff.ts (word LCS)                      └───────────┬──────────────┘  │
│  lib/prompt.ts (modes)                                   │ https           │
└──────────────────────────────────────────────────────────┼─────────────────┘
                                                           ▼
                                          openrouter.ai/api/v1  (free models)
```

Rust owns everything that touches the network or the OS: `reqwest` with streaming,
`keyring` for the key, `dirs` for the data directory, `tauri-plugin-updater` for releases.
The frontend owns presentation and the diff, and is replaceable without touching the network
layer — which is why M1 can be built before any of it exists.

## 4. Milestones

### M1 · Shell and workspace — this milestone

Tauri v2 + React 19 + Vite, pinned dev port, the QuillBot-shaped layout: a tool rail on the
left, mode tabs across the top, an editor pane beside a result pane, and an action rail on the
right. Both tools are laid out end to end — paraphraser with mode tabs and a synonym slider,
grammar checker with an issue list that can accept a fix — against mock output, so the shape is
reviewable without an API key.

**Done when:** `npm run typecheck`, `npm run build` and `npm run tauri dev` are clean and the
window renders at its minimum size without the panes collapsing.

### M2 · OpenRouter client

`openrouter.rs`: keychain read/write, a masked-key settings row, `/models` fetch filtered to
free prompt pricing, model picker with cached list, streaming completion with backoff and
fallback chain, 429/remaining-quota surfacing. `lib/prompt.ts` grows one prompt per mode, each
asking for a rewrite plus a one-line explanation of what it changed. Chunking on paragraph
boundaries.

**Done when:** a 1,000-word document paraphrases in Standard mode on a free model, streaming,
and the app survives an induced 429 without losing the text.

### M3 · Grammar checker

Structured-output request returning issue triples; local verification of each quote; issue
cards with accept / ignore / accept-all; offsets re-applied to the editor text so a second run
sees the corrected document. Underline rendering of the flagged spans in the input pane.

**Done when:** a paragraph with planted spelling, punctuation and agreement errors yields
issues whose quotes are found in the text, and accepting all of them produces clean prose.

### M4 · History, statistics, settings

SQLite (or a JSON log if the volume stays small) of every run; a history list that restores a
run into the workspace; statistics from those rows — words processed, runs per mode, models
used, and words saved. Settings: key, default model, default mode, theme (light/dark),
data-directory controls, and a confirm-gated "delete history".

### M5 · Updates and releases

`tauri-plugin-updater` with a minisign keypair, endpoints pointed at
`https://github.com/emrisca/quillnot/releases/latest/download/latest.json`, and a Settings →
Updates panel with the current version, a **Check for updates** button, release notes and an
**Install and restart** action — plus a silent check on launch that only surfaces when
something is actually available. A GitHub Actions release workflow on `v*` tags builds macOS and
Windows through `tauri-action` with `includeUpdaterJson: true`, signing the updater artifact with
the repository secret. No notarisation and no code-signing certificate: macOS launch needs
right-click → Open, and the release notes say so.

**Done when:** 0.1.0 is published, 0.1.1 installs over it from inside the app, and the
`latest.json` the app reads is the one the workflow wrote.

### M6 · Polish

Keyboard shortcuts (⌘↩ to run, ⌘C to copy the result), tone detection in the right rail,
sentence-level diff comparison view, export to Markdown, and the "premium-ish" extras that
QuillBot gates — summarize and humanize — as modes on the same client, since they are the same
request with a different prompt.

## 5. What the UI mirrors, and what it deliberately does not

Mirrored: the left tool rail with icon-over-label items, mode tabs with an underline on the
active one and a synonym slider on the right, the two-pane editor with per-pane word counts and
a green action pill, the right-hand action rail, and the pale-yellow suggestion card that offers
a mode change.

Not mirrored: the "Upgrade to Premium" banner, the AI-detector and plagiarism claims (they need
a scoring model we do not have and would be dishonest to fake), and the QuillBot name and
mark. Nothing here is scraped from QuillBot; the layout is a familiar shape, the code is ours.

## 6. Risks

| Risk | Severity | Mitigation |
| --- | --- | --- |
| A free model stops being free or disappears | High, it is the whole premise | Model list is fetched and filtered by live pricing; picker never hardcodes ids; fallback chain across free models |
| Daily/minute rate limits on free tiers | High, it is the daily experience | Queue with backoff, remaining-quota readout, chunking, per-document request budget shown before a long run |
| Free-model prose quality is below QuillBot's | Medium | Mode prompts tuned per model family; "Custom" instructions; keep the original one keystroke away |
| Model returns unusable JSON for grammar | Medium | Schema when supported, tolerant parser, raw reply behind a disclosure, quote-verification before any fix is offered |
| Key leakage | High | Key only in the OS keychain, read only by Rust; never in the webview, never in a log, never in the repo |
| Long text times out | Medium | Paragraph chunking, streaming, cancel button |
| Unsigned builds warn on launch | Low | Expected and documented in the release notes; no certificate budget for a free tool |

## 7. Open questions

1. **Whose key?** A build that expects each user to paste their own OpenRouter key is free to
   run and honest about limits. Bundling one would mean shipping a secret to every download and
   eating everyone's quota. The plan assumes **user-supplied key**, with the app usable in mock
   mode before one is added.
2. **Grammar checker vs paraphraser default.** QuillBot opens on the paraphraser; this does the
   same, but the grammar tool is the one people keep installed, so it may deserve the default
   slot once it is real.
3. **Windows build.** The release workflow will cover it, but the primary target here is macOS
   on Apple silicon, like the other two desktop apps in this workspace.
