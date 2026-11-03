# QuillNot — plan of attack

A Tauri desktop app that does the two things people actually pay QuillBot for — **rewrite
this paragraph** and **fix what is wrong with it** — using OpenRouter's free models. No
subscription and no server of our own; the account is the OpenRouter one each person already
has, and their calls are billed to their own credits.

Status: **M8 shipped, with the gaps each milestone names.** The shell, both tools against
live free models, the verified grammar checker, local history and statistics, the provider
list, the updater, the polish pass, the catalogue's per-call ranking and Typer all exist.
What is missing is in one place. Typer's four remaining pieces now exist: an X11 injector
over XTest, the generated "finish by" mode, the chord's start half, and the idle-second stop.
The logic of all four is unit tested headless; the X calls themselves need a live X server,
and no keystroke has yet landed in another application on any platform. The score
now carries a quality term built from kept, undone and rerun verdicts, its weights and its
penalty table are per-task settings, the remaining-quota header a provider returns is shown
in the header, and the catalogue asks whether it changed before downloading itself, keeps
rows that vanish upstream and refreshes at launch. What is unproven is proof rather than
code: Typer has never posted a keystroke on a desktop with Accessibility granted, and no
update has installed over a previous version.

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

- one request at a time per block, then a **fallback chain** — if the chosen model refuses,
  the next free model in the picker is tried before the user sees an error (built);
- exponential backoff on 429/5xx, and the remaining-requests header surfaced in the UI —
  **built**: the wait before the next provider doubles from 700 ms to a cap of five
  seconds, and the remaining-request count a provider returns appears in the header after
  every call. `Retry-After` is still quoted back in the error message rather than waited
  on, because moving to the next model is usually better than stalling on this one;
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

**Providers, not just OpenRouter.** Nearly every free tier worth using is OpenAI-compatible, so
the client takes a base URL, a key and a model id instead of assuming one vendor. Google is the
exception now: AI Studio's `AQ.` Auth keys are rejected on Gemini's OpenAI-compatible route, so
Google is called on its native endpoint — the key in `x-goog-api-key`, `contents` instead of
`messages`, and a stream of candidates instead of choices. Settings lists
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
zero-priced model with at least one endpoint that streams text, cached in the data directory
and refreshed whenever the AI Models view opens.

**The cache validates itself.** A refresh sends `If-None-Match` with the validator stored
beside the last table, so an unchanged catalogue is a 304 and the per-model endpoint lookups
are skipped entirely. The cache is read and conditionally refreshed at launch, on a
background task, so AI Models is warm before anyone opens it. An id that vanished upstream is
not dropped: the next table keeps the row with `available: false`, so a pin or a fallback
chain holding it can say what happened to it, and the table marks it `gone`. The
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

**All four terms exist now.** A sample records the provider that answered, time to first
token, prompt and completion tokens, the HTTP status, and — once the person has decided —
whether the reply was kept, undone or rerun. Quality is the mean of those verdicts, with a
keep worth one, a rerun worth 0.4 and an undo worth nothing, and undecided calls left out
rather than counted as good. The weights are `TaskWeights` per task in `settings.json`,
written through `set_task_weights`, and the penalty table subtracts for a failure streak,
for replies people undid, and for an endpoint that has been answering 429, each capped so a
model can always climb back. Tests prove a kept reply outranks an undone one, that changing
the weights changes the order, and that the rejection penalty moves a repeatedly-rejected
model down. Settings → Ranking edits them on screen: a task picker, the three weights and
the three penalties as number fields, saved through those same commands, with the Rust side
clamping what a hand-edited settings file could otherwise turn into nonsense. Until that
card existed this paragraph's controls were the one part of the plan still thinner than
the plan imagined.

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
   X11 `XTEST` through `x11-dl`, which borrows the highest keycode and remaps it for one
   keystroke when the current layout cannot type the character. The character-to-keysym
   mapping, the newline-to-Return mapping and the failure stages are unit tested; the X
   calls themselves need a running X server, which this machine does not have.

**"Finish by time"** is two modes. *Paced* takes the remaining characters and the time budget
and derives characters per second from them, clamped to human bounds, so a two-minute schedule
for 600 words lands the last character at 2:00 rather than racing or stalling. *Generated*
sends what is already typed to the model and types on from there — the same client as the
paraphraser with a different prompt. **Built:** the model is asked for roughly as many
characters as the time left can hold at a resting pace (60 to 1200), a fence or a
"Continuation:" label is stripped before the join, the continuation is joined with one space,
and the run starts on the whole piece so the injected text is the finished document rather
than a fragment.

**Rails, because this is the one feature that reaches into other applications.**

- a stop chord that reaches the app while another window has focus — `⌘⌥⇧S` on macOS,
  `Ctrl+Alt+Shift+S` elsewhere — plus Escape while this window has focus. A run also refuses
  to post a keystroke while QuillNot's own window has focus, because that is the one window
  that is never the target. **Built:** the chord is a toggle. The view arms the editor's text
  whenever the text or settings change and no run is live, and the chord starts that text when
  nothing is running and stops a run when one is; arming goes through the same `start_run` as
  the button, so the chord is not a weaker path. A run also stops itself after N idle seconds,
  counted from the last accepted keystroke, with a deliberate pause excluded;
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
│  src-tauri/src/prompt.rs (modes)                         │ https           │
└──────────────────────────────────────────────────────────┼─────────────────┘
                                                           ▼
                                          openrouter.ai/api/v1  (free models)
```

Rust owns everything that touches the network or the OS: `reqwest` with streaming,
`keyring` for the key, Tauri's own `app_data_dir` for the data directory,
`tauri-plugin-updater` for releases and `tauri-plugin-global-shortcut` for Typer's stop chord.
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
answered and how many triples were refused. The flagged spans are underlined in the input
pane by a mirror behind the textarea, and the check runs paragraph by paragraph, so cards
arrive while the rest of the document is still being read instead of after the last token.

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
and the statistics are counted from the log rather than fetched from anywhere. Settings also
writes the default model now, which the router has read since M2: the field was in the settings
file and in the fallback chain, but nothing on screen could set it.

Providers left this page for their own window. The list is longer than the settings column and
it grows with every provider added, so it opens in a panel over the page from one button.

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
something is actually there. `.github/workflows/release.yml` builds Apple silicon and
Windows on a `v*` tag through `tauri-action` with `includeUpdaterJson`. Intel Macs are not
targeted: the job that built them was removed once nobody was going to install the result.

What is still unproven is the last step: 0.1.1 and 0.2.0 are published and 0.2.1 sits as a
draft, but no update has been installed from inside the app over an installed copy. Two
things worth knowing — the release key has to be in `.secrets/quillnot-updater.key` for
`tauri build` to produce updater artifacts, and an unsigned macOS bundle installing an
update is untested territory. The release notes say *Open Anyway*, not right-click → Open:
macOS 15 removed that shortcut.

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

**Written for macOS, Windows and X11.** `typer.rs` plans text into timed keystrokes: a delay per character
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

**Verified:** the planner's ten unit tests, including the invariant that the keystrokes
reproduce the text exactly at a 25% typo rate across forty seeds, a 600-character run landing
within two seconds of a 45-second deadline, the clamp when a deadline is impossible, and
determinism under a fixed seed. Nine more cover the chord toggle, the idle watch, the
newline and keysym mapping, the four distinct injector failure messages, and continuation
sizing and joining; a tenth, in `prompt.rs`, pins the continuation prompt. **Not verified:**
no injector has ever posted an event on any platform, because that needs a desktop session
and someone watching another window.

The Windows injector exists: `SendInput` with `KEYEVENTF_UNICODE`, one code unit at a time so
the active keyboard layout cannot reinterpret the character, a surrogate pair for anything
outside the BMP, and Return/Backspace as real virtual keys. It compiles and its encoding is
unit-tested, but it has not typed a character into Notepad. There is no Windows equivalent of
`IsSecureEventInputEnabled`, so the secure-field refusal is macOS-only until one is found.
The X11 injector exists: `XTEST` through `x11-dl`, with each keystroke mapped to a keysym
(Latin-1 as itself, anything else in the Unicode keysym range, newline as Return) and a spare
keycode borrowed through `XChangeKeyboardMapping` for the one keystroke when the layout cannot
type the character, then restored. The key mapping, the keysym mapping and the four failure
stages — display, XTest, keymap, post — are unit tested. The module type-checks for
`x86_64-unknown-linux-gnu`: it was extracted with its helpers into a scratch crate carrying the
same `x11-dl` dependency and `cargo check --target x86_64-unknown-linux-gnu` passes, which is
how two real API mistakes were found and fixed (`x11-dl` names the XTest loader struct
`Xf86vmode`, and `XChangeKeyboardMapping` takes its keysym array mutably). Checking the whole
app for that target is not possible on this machine: `libdbus-sys` and the GTK build scripts
need Linux pkg-config and development headers, which a Windows host cannot provide. **Not
verified:** the X calls need a running X server and this machine has none, so the Linux
injector has never posted an event either, and no Linux bundle is built by the release
workflow. X11 has no Accessibility permission and no secure-input query, so `supported` and
`trusted` are both true there and the secure-field refusal remains macOS-only. The menu asks
the Rust side for `supported`, `granted` and `platform`, so each build shows its own
directions.

**Since the first pass:** the stop chord in §4 is registered through
`tauri-plugin-global-shortcut`, so it reaches the app while another window has focus, and the
driver now stops as `unfocused` rather than posting a keystroke into QuillNot's own window.

The chord is now a toggle rather than a stop button. The view arms the editor's text
(`typer_arm`) whenever the text or settings change and no run is live, and the chord starts
that text through the same `start_run` as the button when nothing is running, or stops a live
run when one is. The text stays armed after a run, so a run stopped in its first sentence can
be restarted with the same chord, and emptying the editor disarms it. `chord_action(running,
armed)` is unit tested over all four cases. A run also stops itself as `stalled` after the
configured idle seconds, counted from the last accepted keystroke; a pause does not count,
because it is deliberate. The idle arithmetic is tested with synthetic instants, and the
clamp that refuses a limit under five seconds is tested too.

**Generated "finish by"** is built as §4 describes: `typer_generate` asks the model for the
ending sized to the time the existing text leaves (60 to 1200 characters), strips a fence or
a "Continuation:" label, joins it with one space, and starts a run on the whole piece.
Sizing, stripping, joining and the prompt are unit tested; the model call itself is untested
here because no provider key is configured, so no continuation has been generated for real.

## 7. What the UI mirrors, and what it deliberately does not

Mirrored: the left tool rail with icon-over-label items, mode tabs with an underline on the
active one and a synonym slider on the right, the two-pane editor with per-pane word counts and
a green action pill, the right-hand action rail, and the pale-yellow suggestion card that offers
a mode change.

Not mirrored: the "Upgrade to Premium" banner, and the QuillBot name and mark. Nothing here is
scraped from QuillBot; the layout is a familiar shape, the code is ours.

**Removed outright:** the AI detector and the plagiarism checker. Both need a trained
classifier over text and a corpus of sources to compare against; faking either with a chat
model produces confident nonsense, and both invite claims the app cannot support. The rail
advertises only what a text model can actually do, plus what is planned.

**The humanizer came back, on different terms.** As a paraphrase aimed at a detector it was
decoration, so it went. It returned as *AI Humanizer*: a rewrite whose prompt is a pane — the
**skill** — which is editable, remembered in `settings.json`, chosen from presets (one of them
this workspace's `unslop`), and sent verbatim with every block. Nothing claims to defeat a
detector; the claim is that you can read what the model was told.

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

## Since 0.2.5: one catalogue, one ranking, one chord

The AI Models tab stopped being an OpenRouter table. Every connected provider is asked
for its model list (`/models` on Groq and Mistral, Gemini's native `models` on Google,
the configured URL for a local endpoint) and the rows join OpenRouter's in a single
ranking. Rows from the other providers carry no reported uptime or latency, because those
providers publish none: their score rests on what this app measures, which is the same
evidence OpenRouter's rows are blended with.

Auto therefore picks the best row in the whole table, not the best OpenRouter row, and
falls through the ranking on refusal: best, second best, third, whichever provider they
live on, then the active provider's configured model and finally the gateway router. A
model is tried once per block; the duplicate that used to appear when Auto's choice was
also the first ranked row is gone. Cerebras left the registry: its free access now asks
for a payment method first, which puts it outside a list that is meant to need no card.

Typer's global chord was already a toggle, but both halves went through the same start
path, which waited three seconds for focus to move. The chord is pressed in the window
the text belongs in, so it now starts at once; only the Start button keeps the countdown,
because only the button leaves focus in this app.
