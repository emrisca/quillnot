# QuillNot — plan of attack

A Tauri desktop app that does the two things people actually pay QuillBot for — **rewrite
this paragraph** and **fix what is wrong with it** — using OpenRouter's free models. No
subscription and no server of our own; the account is the OpenRouter one each person already
has, and their calls are billed to their own credits.

Status: **M8 shipped.** Every milestone in this plan now exists: the shell, both tools
against live free models, the verified grammar checker, local history and statistics, the
provider list, the updater, the polish pass, the catalogue's per-call ranking, and Typer.
What is left is not a milestone but proof — Typer has never posted a keystroke on a desktop
with Accessibility granted, and no update has installed over a previous version.

Two features carry it past being a QuillBot skin. A **scored catalogue of every free
endpoint** OpenRouter exposes (§3), so the app calls the model that is fast and up *now*
instead of one somebody hardcoded. And **Typer** (§4), a field that types the way a person
does, hickups and all.

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

`GET /api/v1/models/{author}/{slug}/endpoints` goes further, per **provider** serving a model:
`uptime_last_5m`, `uptime_last_30m`, `uptime_last_1d`, `latency_last_30m`,
`throughput_last_30m`, `context_length`, `max_completion_tokens`, `quantization`, `status`. That
is the raw material for §3 — and it comes back `null` on an endpoint nobody called recently,
which decides how the ranking has to treat missing evidence.

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

**The account is the user's, not the build's.** QuillNot ships no key. Settings offers *Sign in
with OpenRouter* — OpenRouter's OAuth PKCE flow: the app opens
`https://openrouter.ai/auth?callback_url=…&code_challenge=…&code_challenge_method=S256`, the
person authorises it, and the code that comes back on the loopback redirect is exchanged at
`POST /api/v1/auth/keys` for a key billed to *their* credits. Localhost callbacks are supported
on any port, so the app binds an ephemeral one for the redirect and closes it immediately; the
key goes straight into the keychain and never into the webview. Pasting a key stays as the
fallback for anyone who prefers it.

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

**Providers, not just OpenRouter.** Every free tier worth using is OpenAI-compatible, so the
client takes a base URL, a key and a model id instead of assuming one vendor. Settings lists
them, each key in its own keychain entry, one of them active: that provider answers every
rewrite, and when it refuses, the block falls through to OpenRouter's free router. The idea is
FreeLLMAPI's (`github.com/tashfeenahmed/freellmapi`) without its middleman — no local router
process, no unified bearer token, no encrypted key database. Requests go from this app to the
provider, and model ids are editable per provider because they change faster than releases.

**Local history, stored in the app's own data directory** — never in the repo, never uploaded.
Each run stores the original, the result, the mode, the model and the token counts, so the
statistics view is real numbers rather than decoration. Those same rows are what the ranking in
§3 is built from: a run is a measurement.

## 3. The catalogue: every free endpoint, scored

Two GETs, no key needed, carry everything a ranking needs:

- `GET /api/v1/models` — one row per model: `id`, `context_length`, `pricing` (`prompt`,
  `completion`, `input_cache_read`), `architecture` (modalities), `supported_parameters`.
- `GET /api/v1/models/{author}/{slug}/endpoints` — one row per **provider** serving it:
  `provider_name`, `quantization`, `context_length`, `max_completion_tokens`, `status`,
  `supported_parameters`, and the measurements — `uptime_last_5m`, `uptime_last_30m`,
  `uptime_last_1d`, `latency_last_30m`, `throughput_last_30m`.

Checked on 3 Nov 2026: 17 zero-priced prompt models, every stat field present in the payload —
and `null` on a free endpoint with no recent traffic. That null is the design's most important
input. Read as zero it would bury every model that is merely new or lightly used, so **null
means no evidence, never a bad score**.

"Collect as many useful free APIs as possible" is therefore a catalogue, not a crawler: every
zero-priced model with at least one endpoint that streams text, refreshed on launch and on
demand, cached in the data directory with its validator so a second launch costs one
conditional request. Ids that vanished upstream are marked rather than silently dropped. The
catalogue is also what makes the fallback chain in §1 mechanical — if the preferred model is
down, the next one is a row in a table we already have.

**Every call is a sample.** Each run records model, provider, task, first-token latency, total
duration, tokens per second, HTTP status, and whether the result was kept, undone or rerun.
After a few dozen runs those numbers beat the platform's 30-minute window, because they are
ours, per task, on this connection.

**Scoring, per task** (`paraphrase`, `grammar`, `typer-finish`):

```
score = w₁·uptime + w₂·speed + w₃·quality − penalties
```

- *uptime* — platform `uptime_last_30m`, falling back to our own success ratio, then to a 0.5
  prior when neither exists;
- *speed* — median first-token latency and throughput, log-scaled, blended from platform stats
  and our own samples;
- *quality* — the one signal that cannot be bought: did the reply parse, was it length-sane,
  did the person keep it or undo and rerun? A model that emits unusable JSON six times in ten
  loses to one that is slower and always right;
- *penalties* — 429 rate, a context too small for this document, a price that is no longer zero.

The weights are per task, stored beside the samples and adjustable in Settings, because nobody —
least of all a plan — knows in advance that one model is the best paraphraser and another the
best JSON emitter. **Settings → AI Models** shows the table (model, provider, uptime, latency,
throughput, score for the selected task) with an **Auto** mode that takes the top row and a pin
for anyone who disagrees with it.

## 4. Typer: typing that looks like a person

A field that types the text it holds — into itself, or into whatever window has focus — with a
human cadence rather than a pasted block: bursts and stalls, longer pauses at commas and
paragraph breaks, **hiccups** (a held key, a doubled character, a backspace and a retype), and a
shortcut that starts and stops it mid-word.

Three parts, and only the last is hard:

1. **The plan.** Text becomes timed keystrokes: per-character delay drawn from a distribution
   with a per-word burst, extra dwell at `,` `.` `;` `?` and a full beat at line breaks, a
   configurable typo rate with the correction landing a few characters later.
2. **The driver.** A cancellable task emits each keystroke on schedule — pause, resume, and a
   speed multiplier that makes a 90-second script land at 60.
3. **The injector.** OS-level events, which a webview cannot produce: macOS `CGEventPost` with
   `CGEventKeyboardSetUnicodeString` for arbitrary characters (needs Accessibility permission,
   asked for once, with a sentence saying why); Windows `SendInput` with `KEYEVENTF_UNICODE`;
   X11 `XTEST`.

**"Finish by time"** is two modes. *Paced* takes the remaining characters and the time budget
and derives characters per second from them, clamped to human bounds, so a two-minute schedule
for 600 words lands the last character at 2:00 rather than racing or stalling. *Generated*
sends what is already typed to the model and types on from there — the same client as the
paraphraser with a different prompt.

**Rails, because this is the one feature that reaches into other applications.**

- one shortcut to start and stop, plus Escape; stop when the target window loses focus; stop
  after N seconds with no keystroke accepted;
- a visible state — the app is never typing invisibly — and never on by default;
- refuse a secure field: on macOS `IsSecureEventInputEnabled()` is a hard no, so password
  prompts are untouchable;
- the text comes from our own field, never from the clipboard, and is never re-read from
  another app.

M8 builds it. The rail carries the entry from now on, dimmed, so the shape of the app is honest
about where it is going.

## 5. Architecture

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

## 6. Milestones

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

**Mostly shipped.** `grammar.rs` does the verification half: the model returns triples, the
app searches the document for each quote, and a span exists only for a quote found exactly
once. A quote that is not in the text is dropped — an invented quote is not evidence of a
mistake — and one that appears twice is shown without a position. Parsing survives a fenced
block, a bare array, a preamble sentence and a reply cut off mid-object. The view is wired to
it: real issues, accept, ignore, accept-all (which skips anything unplaceable), the model that
answered and how many triples were refused. What remains is the underline rendering of the
flagged spans inside the input pane.

**Done when:** a paragraph with planted spelling, punctuation and agreement errors yields
issues whose quotes are found in the text, and accepting all of them produces clean prose.

### M4 · History, statistics, settings

SQLite (or a JSON log if the volume stays small) of every run; a history list that restores a
run into the workspace; statistics from those rows — words processed, runs per mode, models
used, and words saved. Settings: key, default model, default mode, theme (light/dark),
data-directory controls, and a confirm-gated "delete history".

**Shipped.** A JSON log rather than SQLite, which the plan allows: one line per run in
`history.jsonl` beside the settings, capped at 500 rows, replaced atomically so a failed write
leaves the old history rather than half of a new one, and one unreadable line costs only that
line. A rewrite records both sides, the mode and the model, which is what makes reopening it
work; a check records the text and what was found, with no result, because the checker only
suggested. The rail's History and Statistics entries are real views over those rows, Settings
gained a default mode and a Data card that opens the folder and deletes with a confirmation,
and the statistics are counted from the log rather than fetched from anywhere.

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

**Shipped, not yet exercised end to end.** `tauri-plugin-updater` with the minisign pair
generated by `tauri signer generate` — public half in `tauri.conf.json`, private half in
`.secrets/` and in the repository secret the workflow reads. Checking and installing both
happen in Rust (`updates.rs`), because the endpoint, the key and the downloaded archive
should not be reachable from the webview; the frontend gets a version string and a byte
count. Settings → Updates shows the version, checks on demand, lists the release notes and
installs with a progress line, and a check at launch shows one dismissible bar only when
something is actually there. `.github/workflows/release.yml` builds macOS (both
architectures) and Windows on a `v*` tag through `tauri-action` with `includeUpdaterJson`.
What is still unproven is the last step: no release has been published, so no update has
actually installed over a previous version. Two things worth knowing — the release key has
to be in `.secrets/quillnot-updater.key` for `tauri build` to produce updater artifacts, and
an unsigned macOS bundle installing an update is untested territory.

### M6 · Polish

Keyboard shortcuts (⌘↩ to run, ⌘C to copy the result), tone detection in the right rail,
sentence-level diff comparison view, export to Markdown, and the one premium-ish extra worth
having — summarize — as a mode on the same client, since it is the same request with a
different prompt.

**Shipped.** ⌘↩ runs and ⌘C copies from anywhere in the workspace, not only with the caret in a
box, and both leave a selection alone. The right rail reads tone from the words itself —
contractions, hedges, boosters, formal connectives, warmth — with a certainty meter, because a
model call per edit would be slow and would change its mind. The result pane compares at
sentence level alongside the word diff: sentences that survived are paired, a reworded run is
paired in order, and the footer says how many changed. Export writes a Markdown file with the
rewrite and the original to Downloads, never overwriting a name. Summarize is a ninth mode on
the same client, and the prompt asks for at most a third of the length with nothing invented.

The tone reading is a heuristic and the rail calls it a reading, not a verdict; nothing here
knows what a text is *for*, which is the one thing a person still has to judge.

### M7 · The catalogue and the ranking

`catalog.rs`: the two GETs in §3, filtered to zero-priced models with a text-out endpoint;
`samples.rs`, the log every call writes to; scoring per task that folds our own numbers into
the platform's; AI Models with the sortable table, Auto and pinning. The fallback chain reads
the table instead of a hardcoded order.

**Shipped.** The table lists every zero-priced model with its best uptime and the window it
came from, the platform's latency where there is any, our median latency and success rate for
the chosen task, the call count, and one score; every column sorts. Every call — rewrite or
grammar check, success or failure — is timed into `samples.jsonl` per model and per task, and
the score treats the platform's uptime as a prior worth three of our calls, so our own history
takes over as it accumulates. Three failures in a row halve a model's score until it answers
again. Auto picks the best model we have actually measured for that task; with nothing
measured it does *not* fall back to the alphabet, it leaves the run to `openrouter/free`, the
gateway router. A pin overrides Auto, and the failover chain is the chosen model, two measured
alternatives, then the router.

The live run that set the expectations still holds: OpenRouter reports no latency or
throughput at all for these free endpoints, and thirteen of fourteen report a 100% day-long
uptime, so the platform's half of the score is mostly a reachability check. That is exactly
why the samples exist.

**Done when:** the table lists every zero-priced endpoint with its uptime and our measured
latency; Auto picks a model that is not simply the alphabetically first; a model made to fail
three times running drops down the order and climbs back once it recovers.

**Verified:** `--rank` against the live API — 14 free text models, 13 with numbers, one
reporting 0%, and Auto falling to the router rather than the alphabet — plus unit tests for
the prior, the demotion, the recovery, the per-task separation, the pin and the fallback
filter. Not verified with a connected account: no real run has written a sample on this
machine yet, so the measured half of the table is untested against live traffic.

### M8 · Typer

The keystroke planner, the cancellable driver and the per-platform injector (§4), plus the
Accessibility request, the secure-field refusal and the shortcut. Paced completion first,
generated completion after.

**Shipped, macOS only.** `typer.rs` plans text into timed keystrokes: a delay per character
from a narrow distribution, shorter inside a word and longer at its start, a dwell after
`,;:` and a longer one after `.!?`, a beat at a line break, and a typo rate that types a
neighbouring key, pauses, backspaces and retypes. A deadline is met by re-deriving the pace
from what the last pass actually took, so punctuation dwells cannot push the finish past the
target, and a pace faster than a person is clamped at 18 keys a second instead of pretending.
The driver sleeps out each step and checks the stop flag on both sides of every sleep, so
Escape lands mid-word. The injector posts `CGEvent` keyboard events with the character
attached as a Unicode string, so no keyboard layout has to be guessed; Return and Delete go
as real key codes. Nothing is typed while `IsSecureEventInputEnabled` is set, and a run does
not start without `AXIsProcessTrusted`, which is asked for with a sentence saying why.

The view shows the plan before the run starts — keys per second, keystrokes and estimated
seconds — and it comes from the same planner the driver follows, so the estimate is not a
guess. The rail entry stopped being dimmed.

**Done when:** on macOS with Accessibility granted, Typer types a paragraph into TextEdit at a
human cadence, stops mid-word on the shortcut and resumes, and a "finish by 0:45" run lands
within two seconds of the target. Nothing is typed while a secure field has focus.

**Verified:** ten unit tests, including the invariant that the keystrokes reproduce the text
exactly at a 25% typo rate across forty seeds, a 600-character run landing within two seconds
of a 45-second deadline, the clamp when a deadline is impossible, and determinism under a
fixed seed. **Not verified:** the injector has never posted an event, because that needs a
desktop session with Accessibility granted and someone watching TextEdit. The Windows and X11
injectors do not exist, and those builds report Typer as unavailable rather than offering a
button that does nothing.

## 7. What the UI mirrors, and what it deliberately does not

Mirrored: the left tool rail with icon-over-label items, mode tabs with an underline on the
active one and a synonym slider on the right, the two-pane editor with per-pane word counts and
a green action pill, the right-hand action rail, and the pale-yellow suggestion card that offers
a mode change.

Not mirrored: the "Upgrade to Premium" banner, and the QuillBot name and mark. Nothing here is
scraped from QuillBot; the layout is a familiar shape, the code is ours.

**Removed outright:** the AI detector, the plagiarism checker and the AI humanizer. The first
two need a trained classifier over text and a corpus of sources to compare against; faking
either with a chat model produces confident nonsense, and both invite claims the app cannot
support. The humanizer is a paraphrase aimed at a detector that does not exist here, so it was
decoration. The rail advertises only what a text model can actually do, plus what is planned.

## 8. Risks

| Risk | Severity | Mitigation |
| --- | --- | --- |
| A free model stops being free or disappears | High, it is the whole premise | Model list is fetched and filtered by live pricing; picker never hardcodes ids; fallback chain across free models |
| Daily/minute rate limits on free tiers | High, it is the daily experience | Queue with backoff, remaining-quota readout, chunking, per-document request budget shown before a long run |
| Free-model prose quality is below QuillBot's | Medium | Mode prompts tuned per model family; "Custom" instructions; keep the original one keystroke away |
| Model returns unusable JSON for grammar | Medium | Schema when supported, tolerant parser, raw reply behind a disclosure, quote-verification before any fix is offered |
| Key leakage | High | Key only in the OS keychain, read only by Rust; never in the webview, never in a log, never in the repo |
| Long text times out | Medium | Paragraph chunking, streaming, cancel button |
| Unsigned builds warn on launch | Low | Expected and documented in the release notes; no certificate budget for a free tool |
| Ranking on missing platform stats | Medium | `null` uptime is "no evidence", never zero; our own samples fill the gap, with a 0.5 prior until they exist |
| Typer read as a keylogger by AV or the OS | High — it injects keystrokes | Opt-in only, visible state, no clipboard, nothing typed is stored, source in the open, Accessibility asked for by name |
| Typer types into the wrong window | High | Focus check before the first keystroke, a countdown with Escape, stop the moment focus moves |

## 9. Open questions

1. ~~**Whose key?**~~ **Answered: each person's own, obtained by signing in.** Settings offers
   *Sign in with OpenRouter* — the OAuth PKCE flow in §2 — so someone authorises once and the
   key that comes back is billed to their own credits and revocable from their account page.
   Pasting a key stays as a fallback, and the app stays usable in mock mode before either.
2. **Grammar checker vs paraphraser default.** QuillBot opens on the paraphraser; this does the
   same, but the grammar tool is the one people keep installed, so it may deserve the default
   slot once it is real.
3. **Windows build.** The release workflow will cover it, but the primary target here is macOS
   on Apple silicon, like the other two desktop apps in this workspace.
4. **Typer's blast radius.** Typing into other applications is the one feature that can get a
   person banned by a site whose terms forbid posing as a human, or flagged by security
   software. It ships opt-in with a visible state; whether its default target is our own field
   or the focused window anywhere is worth deciding before M8 rather than during it.
