//! The grammar checker: ask for issue triples, then verify every one of them.
//!
//! The hard rule from the plan: a model's character offsets are worthless. What a
//! model does reliably is *quote*. So the request asks for `{original, replacement,
//! message}` and this module searches the document for `original` itself:
//!
//! - found exactly once → the issue carries a verified span, in UTF-16 units, which is
//!   what a JavaScript string indexes by;
//! - found more than once → the issue is shown but not applied, because splicing it
//!   would as likely fix the wrong sentence;
//! - not found at all → dropped. The model invented or paraphrased the quote, and an
//!   invented quote is not evidence of a mistake.
//!
//! Parsing is deliberately forgiving of the shape a free model actually returns: a
//! fenced block, a bare array, or a reply that runs out of tokens mid-object.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use std::future::Future;
use std::pin::Pin;

use futures_util::stream::{FuturesUnordered, StreamExt};

use crate::{openrouter, prompt, providers, samples};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub kind: String,
    pub message: String,
    pub quote: String,
    pub suggestion: String,
    /// UTF-16 offsets, 0 when `unique` is false.
    pub start: usize,
    pub end: usize,
    /// False when the quote appears more than once: show it, do not apply it.
    pub unique: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarResult {
    pub issues: Vec<Issue>,
    pub model: String,
    /// Triples the model returned that could not be trusted, and why the list is
    /// shorter than it looked.
    pub dropped: usize,
    /// How many issues were found but cannot be placed.
    pub unlocated: usize,
}

/// Issues found so far, sent once per block so the pane fills as the check runs rather
/// than after the last one. A five-block check used to show nothing for half a minute.
pub const PROGRESS_EVENT: &str = "grammar:progress";

/// A block big enough to hold a paragraph or three, small enough that a free model
/// answers it in a couple of seconds.
const BLOCK_CHARS: usize = 1200;

/// The first block is deliberately smaller than the rest. Nobody sees anything until the
/// first block comes back, so a 500-character opener puts a card on screen in the time a
/// 1,200-character one takes to type out its JSON, and the rest is read while that card
/// is already there.
const FIRST_BLOCK_CHARS: usize = 500;

/// How many blocks may be in the air at once. Three keeps a check responsive without
/// stampeding a free tier's per-minute limit.
const IN_FLIGHT: usize = 3;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: usize,
    total: usize,
    issues: Vec<Issue>,
    model: String,
}

impl Issue {
    /// Block-relative offsets become document offsets once the block it came from is
    /// known. Only placed issues move: an unplaced one has no position to shift.
    fn shifted(mut self, base: usize) -> Self {
        if self.unique {
            self.start += base;
            self.end += base;
        }
        self
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawIssue {
    #[serde(default)]
    original: Option<String>,
    #[serde(default)]
    replacement: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawPayload {
    Wrapped { issues: Vec<RawIssue> },
    Bare(Vec<RawIssue>),
}

impl RawPayload {
    fn into_vec(self) -> Vec<RawIssue> {
        match self {
            RawPayload::Wrapped { issues } => issues,
            RawPayload::Bare(issues) => issues,
        }
    }
}

/// The command behind the Grammar Checker view.
pub async fn check(app: &AppHandle, text: &str, model: Option<String>) -> Result<GrammarResult, String> {
    let route = providers::resolve(app, model, samples::GRAMMAR)?;
    let started = std::time::Instant::now();
    let result = run(Some(app), &route.attempts, text).await?;

    // A check is a run like any other, so it belongs in the history and in the count
    // of words processed. Its `result` stays empty: the checker suggested, and the
    // person has not accepted anything yet.
    crate::history::record_grammar(
        app,
        text,
        &result.model,
        result.issues.len(),
        result.unlocated,
        result.dropped,
        started.elapsed().as_millis() as u64,
    );

    Ok(result)
}

/// The same without a window, for `--grammar` in the self-test.
pub async fn check_headless(text: &str) -> Result<GrammarResult, String> {
    let route = providers::resolve_headless(None)?;
    run(None, &route.attempts, text).await
}

async fn run(
    app: Option<&AppHandle>,
    attempts: &[providers::Attempt],
    text: &str,
) -> Result<GrammarResult, String> {
    let network = Network { app, attempts };
    run_with(text, &network, |progress| {
        if let Some(handle) = app {
            let _ = handle.emit(PROGRESS_EVENT, progress);
        }
    })
    .await
}

/// Answers one block of text. The app's implementation is [`openrouter::complete`]; the
/// tests substitute a stub that sleeps, so the scheduler can be timed with no network, no
/// key and no model.
trait Answerer: Sync {
    fn answer<'a>(
        &'a self,
        system: String,
        user: String,
    ) -> Pin<Box<dyn Future<Output = Result<(String, String), String>> + Send + 'a>>;
}

struct Network<'a> {
    app: Option<&'a AppHandle>,
    attempts: &'a [providers::Attempt],
}

impl Answerer for Network<'_> {
    fn answer<'a>(
        &'a self,
        system: String,
        user: String,
    ) -> Pin<Box<dyn Future<Output = Result<(String, String), String>> + Send + 'a>> {
        Box::pin(async move {
            openrouter::complete(self.app, samples::GRAMMAR, self.attempts, &system, &user).await
        })
    }
}

/// The scheduling half of a check, with the model call supplied from outside.
///
/// Two things are measurable here and both used to be wrong. The first block is small, so
/// the first `on_progress` call is one short answer away instead of the whole document's.
/// The remaining blocks go into the air together, so a five-block document costs two
/// rounds of latency rather than five. Progress always carries every issue found so far,
/// in document order, however the blocks happened to finish.
async fn run_with<P>(
    text: &str,
    answerer: &dyn Answerer,
    mut on_progress: P,
) -> Result<GrammarResult, String>
where
    P: FnMut(Progress),
{
    let text = text.trim();
    if text.is_empty() {
        return Err("There is nothing to check.".to_string());
    }

    let planned = blocks(text);
    let total = planned.len();
    let mut answers: Vec<Option<Result<(String, String), String>>> =
        (0..total).map(|_| None).collect();
    let mut in_flight = FuturesUnordered::new();
    let mut next = 0usize;
    let mut done = 0usize;
    let mut all: Vec<Issue> = Vec::new();
    let mut dropped = 0usize;
    let mut unreadable = 0usize;
    let mut model = String::new();
    let mut last_error = String::new();

    while next < total || !in_flight.is_empty() {
        while next < total && in_flight.len() < IN_FLIGHT {
            let index = next;
            let block = planned[index].1.clone();
            let (system, user) = prompt::grammar_prompt(&block);
            let answer = answerer.answer(system, user);
            in_flight.push(async move { (index, answer.await) });
            next += 1;
        }

        let Some((index, outcome)) = in_flight.next().await else {
            break;
        };
        answers[index] = Some(outcome);
        done += 1;

        // Rebuilt from every answered block rather than appended to, because blocks no
        // longer finish in document order: a later block can land first, and its issues
        // still have to be merged where they belong in the text.
        all.clear();
        dropped = 0;
        unreadable = 0;
        for (position, slot) in answers.iter().enumerate() {
            let Some(outcome) = slot else { continue };
            let (offset, block) = &planned[position];
            match outcome {
                Ok((used, raw)) => {
                    model = used.clone();
                    match parse_issues(raw) {
                        Ok(parsed) => {
                            let (found, lost, _) = verify(block, parsed);
                            let base = utf16_len(&text[..*offset]);
                            all.extend(found.into_iter().map(|issue| issue.shifted(base)));
                            dropped += lost;
                        }
                        Err(err) => {
                            // One block answered in prose must not throw away the blocks
                            // that worked.
                            unreadable += 1;
                            last_error = err.clone();
                        }
                    }
                }
                Err(err) => {
                    unreadable += 1;
                    last_error = err.clone();
                }
            }
        }

        // Ids are React keys, so they are re-numbered after every merge rather than per
        // block, where two blocks would both start at `m0`.
        for (position, issue) in all.iter_mut().enumerate() {
            issue.id = format!("m{position}");
        }
        all.sort_by_key(|issue| if issue.unique { issue.start } else { usize::MAX });

        on_progress(Progress {
            done,
            total,
            issues: all.clone(),
            model: model.clone(),
        });
    }

    // Nothing parsed at all is a failed check, not a clean document. When every block
    // failed at the transport rather than the parser, that error is the useful one.
    if all.is_empty() && unreadable == total {
        return Err(last_error);
    }

    let unlocated = all.iter().filter(|issue| !issue.unique).count();
    Ok(GrammarResult {
        issues: all,
        model,
        dropped,
        unlocated,
    })
}

/// Paragraph blocks with the byte offset each one starts at.
///
/// A blank line is where a document can be checked on its own: a quoted mistake never
/// spans one, so a block's issues belong to that block. A paragraph longer than a block is
/// cut at a sentence end, which is the change that matters for a document pasted as one
/// wall of prose: the old splitter only looked for line breaks, so that document went to
/// the model as a single request and the first card waited for all of it.
fn blocks(text: &str) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'\n' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
            push_paragraph(text, start, index, &mut out);
            let mut next = index;
            while next < bytes.len() && bytes[next] == b'\n' {
                next += 1;
            }
            start = next;
            index = next;
            continue;
        }
        index += 1;
    }
    push_paragraph(text, start, bytes.len(), &mut out);

    // Whitespace-only blocks are not worth a request.
    out.retain(|(_, block)| !block.trim().is_empty());
    if out.is_empty() {
        out.push((0, text.to_string()));
    }
    out
}

/// Cuts one paragraph into blocks. The first is [`FIRST_BLOCK_CHARS`] because the first
/// card is what the person is waiting for; the rest are [`BLOCK_CHARS`]. A cut prefers the
/// last sentence end that fits, then a line break, then a space: a model checks whole
/// sentences better than fragments of them.
fn push_paragraph(text: &str, start: usize, end: usize, out: &mut Vec<(usize, String)>) {
    let mut cursor = start;
    while cursor < end {
        let limit = if out.is_empty() { FIRST_BLOCK_CHARS } else { BLOCK_CHARS };
        if end - cursor <= limit {
            out.push((cursor, text[cursor..end].to_string()));
            return;
        }
        let boundary = cut_point(text, cursor, end, limit);
        out.push((cursor, text[cursor..boundary].to_string()));
        cursor = boundary;
        // The space or line break we split on belongs to neither block.
        while cursor < end && matches!(text.as_bytes()[cursor], b' ' | b'\n') {
            cursor += 1;
        }
    }
}

/// Where to cut a run longer than `limit`, in bytes. Every boundary this returns is a char
/// boundary, so slicing at it is safe whatever the text contains.
fn cut_point(text: &str, cursor: usize, end: usize, limit: usize) -> usize {
    let mut window_end = (cursor + limit).min(end);
    while window_end > cursor && !text.is_char_boundary(window_end) {
        window_end -= 1;
    }
    let window = &text[cursor..window_end];

    // A sentence end, but not so early that the block becomes a fragment.
    if let Some(index) = window.rfind(['.', '!', '?']) {
        if index + 1 >= limit / 3 {
            return cursor + index + 1;
        }
    }
    if let Some(index) = window.rfind('\n') {
        return cursor + index;
    }
    if let Some(index) = window.rfind(' ') {
        return cursor + index;
    }
    // No punctuation, no break and no space: a hard cut still beats one request for the
    // whole document.
    window_end
}

/// Pulls the JSON out of whatever wrapper the reply came in, and survives a reply
/// that was cut off before its last closing brace.
fn parse_issues(raw: &str) -> Result<Vec<RawIssue>, String> {
    let json = extract_json(raw);
    if let Ok(payload) = serde_json::from_str::<RawPayload>(&json) {
        return Ok(payload.into_vec());
    }

    // Truncated: keep the complete objects and drop the tail. A model that ran out of
    // tokens still gave us the issues it managed to finish.
    let array_start = json.find('[').ok_or_else(|| {
        "The model did not answer with the JSON the checker asked for.".to_string()
    })?;
    let inner = &json[array_start + 1..];

    let mut cut = inner.len();
    while let Some(end) = inner[..cut].rfind('}') {
        let candidate = format!("[{}]", &inner[..=end]);
        if let Ok(payload) = serde_json::from_str::<RawPayload>(&candidate) {
            return Ok(payload.into_vec());
        }
        cut = end;
    }

    Err("Could not read the model's JSON — it was malformed rather than cut short.".to_string())
}

/// Everything from the first `{` or `[` to the last matching bracket, so code fences
/// and a sentence of preamble do not matter.
fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
    let start = trimmed
        .find(['{', '['])
        .unwrap_or(0);
    let body = &trimmed[start..];
    let end = body
        .rfind(['}', ']'])
        .map(|index| index + 1)
        .unwrap_or(body.len());
    body[..end].to_string()
}

fn normalize_kind(raw: Option<String>) -> String {
    let lower = raw.unwrap_or_default().to_lowercase();
    let kind = if lower.contains("spell") {
        "spelling"
    } else if lower.contains("punct") {
        "punctuation"
    } else if lower.contains("style") || lower.contains("clarity") || lower.contains("word") {
        "style"
    } else {
        "grammar"
    };
    kind.to_string()
}

/// UTF-16 length of a slice, which is the unit a JavaScript string indexes by.
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Turns raw triples into issues with spans this app found for itself.
///
/// Returns the issues, how many triples were dropped as untrustworthy, and how many
/// survived without a position.
fn verify(text: &str, raw: Vec<RawIssue>) -> (Vec<Issue>, usize, usize) {
    const LIMIT: usize = 40;

    let mut issues: Vec<Issue> = Vec::new();
    let mut dropped = 0usize;
    let mut seen: Vec<(String, String)> = Vec::new();

    for item in raw {
        let quote = item.original.unwrap_or_default();
        let suggestion = item.replacement.unwrap_or_default();
        // A quote of nothing but spaces is legitimate — it is how a doubled space gets
        // reported — so this checks emptiness, not trimmed emptiness.
        if quote.is_empty() || quote == suggestion {
            dropped += 1;
            continue;
        }
        if seen.contains(&(quote.clone(), suggestion.clone())) {
            dropped += 1;
            continue;
        }
        seen.push((quote.clone(), suggestion.clone()));

        let hits: Vec<usize> = text.match_indices(&quote).map(|(index, _)| index).collect();
        if hits.is_empty() {
            // Not in the document: the quote was invented or paraphrased.
            dropped += 1;
            continue;
        }

        let message = item
            .message
            .unwrap_or_else(|| "This could read better.".to_string())
            .trim()
            .to_string();
        let kind = normalize_kind(item.category);

        let unique = hits.len() == 1;
        let (start, end) = if unique {
            let byte_start = hits[0];
            let start = utf16_len(&text[..byte_start]);
            (start, start + utf16_len(&quote))
        } else {
            (0, 0)
        };

        // Two issues over the same words would fight when both are accepted; the
        // first one wins, which is also the one nearest the start of the text.
        if unique
            && issues
                .iter()
                .any(|existing| existing.unique && start < existing.end && end > existing.start)
        {
            dropped += 1;
            continue;
        }

        issues.push(Issue {
            id: format!("m{}", issues.len()),
            kind,
            message,
            quote,
            suggestion,
            start,
            end,
            unique,
        });
        if issues.len() >= LIMIT {
            break;
        }
    }

    issues.sort_by_key(|issue| if issue.unique { issue.start } else { usize::MAX });
    let unlocated = issues.iter().filter(|issue| !issue.unique).count();
    (issues, dropped, unlocated)
}

/// The span at those UTF-16 offsets, or None if they do not describe one. Used by the
/// self-test to prove the offsets it hands to the frontend point at the quoted words.
pub fn slice_utf16(text: &str, start: usize, end: usize) -> Option<String> {
    if end < start {
        return None;
    }
    let mut out = String::new();
    let mut position = 0usize;
    for character in text.chars() {
        let width = character.len_utf16();
        if position >= end {
            break;
        }
        if position >= start && position + width <= end {
            out.push(character);
        }
        position += width;
    }
    if position < end {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::time::{Duration, Instant};

    fn raw(quote: &str, suggestion: &str, category: &str) -> RawIssue {
        RawIssue {
            original: Some(quote.to_string()),
            replacement: Some(suggestion.to_string()),
            message: Some("test".to_string()),
            category: Some(category.to_string()),
        }
    }

    #[test]
    fn blocks_are_paragraphs_and_their_offsets_point_at_them() {
        let text = "First para.\n\nSecond para.\n\n\nThird para.";
        let planned = blocks(text);
        assert_eq!(planned.len(), 3);
        for (offset, block) in &planned {
            assert_eq!(&text[*offset..*offset + block.len()], block);
        }
        assert_eq!(planned[1].1, "Second para.");
    }

    #[test]
    fn a_long_paragraph_is_cut_at_a_line_break() {
        let line = "a sentence that is long enough to matter.\n";
        let text = line.repeat(60);
        let planned = blocks(&text);
        assert!(planned.len() > 1, "a long body is split");
        for (offset, block) in &planned {
            assert!(block.chars().count() <= BLOCK_CHARS + 64, "blocks stay small");
            assert_eq!(&text[*offset..*offset + block.len()], block, "the offset is real");
        }
        let (offset, block) = planned.last().expect("at least one block");
        assert_eq!(offset + block.len(), text.len(), "the last block runs to the end");
    }

    #[test]
    fn shifting_a_block_issue_lands_on_the_same_words_in_the_document() {
        let text = "Their is one.\n\nTheir is another.";
        let planned = blocks(text);
        let (offset, block) = &planned[1];
        let (found, _, _) = verify(block, vec![raw("Their", "There", "grammar")]);
        let base = utf16_len(&text[..*offset]);
        let placed = found[0].clone().shifted(base);
        assert_eq!(
            slice_utf16(text, placed.start, placed.end),
            Some("Their".to_string())
        );
        assert_eq!(placed.start, base, "the quote starts the second block");
    }

    #[test]
    fn a_quote_found_once_gets_a_span_that_slices_back_to_itself() {
        let text = "Their is a problem with the installer.";
        let (issues, dropped, unlocated) = verify(text, vec![raw("Their", "There", "grammar")]);
        assert_eq!(dropped, 0);
        assert_eq!(unlocated, 0);
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert!(issue.unique);
        assert_eq!(issue.start, 0);
        assert_eq!(issue.end, 5);
        assert_eq!(
            slice_utf16(text, issue.start, issue.end),
            Some("Their".to_string())
        );
        assert_eq!(issue.kind, "grammar");
    }

    #[test]
    fn a_quote_that_is_not_in_the_text_is_dropped_not_placed() {
        let text = "The installer fails on older machines.";
        let (issues, dropped, _) = verify(text, vec![raw("The installer crashes", "fixed", "grammar")]);
        assert!(issues.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn a_quote_that_appears_twice_is_shown_but_not_placed() {
        let text = "the cat saw the dog.";
        let (issues, _, unlocated) = verify(text, vec![raw("the", "The", "spelling")]);
        assert_eq!(issues.len(), 1, "a repeated quote still deserves a card");
        assert!(!issues[0].unique);
        assert_eq!(issues[0].start, 0);
        assert_eq!(unlocated, 1);

        // A replacement identical to the quote is not a fix at all.
        let (issues, dropped, _) = verify(text, vec![raw("the", "the", "punctuation")]);
        assert!(issues.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn a_doubled_space_can_be_quoted_and_replaced() {
        let text = "This is  spaced.";
        let (issues, _, _) = verify(text, vec![raw("  ", " ", "punctuation")]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].suggestion, " ");
        assert_eq!(slice_utf16(text, issues[0].start, issues[0].end), Some("  ".to_string()));
    }

    #[test]
    fn issues_over_the_same_words_do_not_stack() {
        let text = "alot of small problems";
        let (issues, dropped, _) = verify(
            text,
            vec![raw("alot", "a lot", "spelling"), raw("alot of", "a lot of", "spelling")],
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn offsets_are_utf16_so_the_webview_can_slice_the_string() {
        let text = "Ok 🙂 teh cat";
        let (issues, _, _) = verify(text, vec![raw("teh", "the", "spelling")]);
        let issue = &issues[0];
        assert_eq!(
            slice_utf16(text, issue.start, issue.end),
            Some("teh".to_string())
        );
        // The emoji is two UTF-16 units but one char, so a char index would be off.
        assert_eq!(text.char_indices().count(), 12);
        assert!(issue.start > text[..text.find("teh").unwrap()].chars().count());
    }

    #[test]
    fn a_fenced_reply_parses() {
        let raw = "```json\n{\"issues\":[{\"original\":\"teh\",\"replacement\":\"the\",\"message\":\"typo\",\"category\":\"spelling\"}]}\n```";
        let parsed = parse_issues(raw).expect("a fenced object");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].original.as_deref(), Some("teh"));
    }

    #[test]
    fn a_reply_that_ran_out_of_tokens_keeps_its_finished_issues() {
        let raw = "{\"issues\":[{\"original\":\"teh\",\"replacement\":\"the\",\"message\":\"typo\",\"category\":\"spelling\"},{\"original\":\"alot\",\"repl";
        let parsed = parse_issues(raw).expect("the first issue survives");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn a_bare_array_and_a_preamble_both_parse() {
        let raw = "Here you go: [{\"original\":\"teh\",\"replacement\":\"the\"}]";
        let parsed = parse_issues(raw).expect("a bare array");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn nonsense_is_an_error_rather_than_an_empty_result() {
        assert!(parse_issues("I could not find any problems.").is_err());
    }

    #[test]
    fn categories_are_folded_into_the_four_the_ui_styles() {
        assert_eq!(normalize_kind(Some("Spelling".into())), "spelling");
        assert_eq!(normalize_kind(Some("punctuation".into())), "punctuation");
        assert_eq!(normalize_kind(Some("clarity".into())), "style");
        assert_eq!(normalize_kind(None), "grammar");
    }

    #[test]
    fn an_empty_suggestion_deletes_the_quoted_span() {
        let text = "This is, uh, fine.";
        let (issues, _, _) = verify(text, vec![raw(", uh,", "", "style")]);
        assert_eq!(issues[0].suggestion, "");
        assert_eq!(slice_utf16(text, issues[0].start, issues[0].end), Some(", uh,".to_string()));
    }

    /// A model with a known price per character and no network. The delay is proportional
    /// to the prompt, which is what makes the block plan measurable: a request carrying the
    /// whole document takes as long as the document is, and a request carrying 500
    /// characters takes a fifth of that.
    struct Stub {
        per_char_ms: f64,
    }

    impl Stub {
        fn new(per_char_ms: f64) -> Self {
            Self { per_char_ms }
        }

        fn delay(&self, chars: usize) -> Duration {
            Duration::from_millis((chars as f64 * self.per_char_ms) as u64)
        }
    }

    impl Answerer for Stub {
        fn answer<'a>(
            &'a self,
            _system: String,
            user: String,
        ) -> Pin<Box<dyn Future<Output = Result<(String, String), String>> + Send + 'a>> {
            let delay = self.delay(user.len());
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Ok(("stub".to_string(), "{\"issues\":[]}".to_string()))
            })
        }
    }

    #[test]
    fn a_paragraph_with_no_line_breaks_is_still_cut_into_blocks() {
        // The old splitter only looked for newlines, so this document went out as one
        // request and the first card waited for every word of it.
        let text = "the writer did a thing that reads badly. ".repeat(120);
        assert!(text.len() > 4_800);
        assert!(!text.contains('\n'));

        let planned = blocks(&text);
        assert!(planned.len() >= 4, "blocks: {}", planned.len());
        for (offset, block) in &planned {
            assert_eq!(&text[*offset..*offset + block.len()], block, "the offset is real");
            assert!(block.chars().count() <= BLOCK_CHARS + 8, "no oversized block");
        }
        assert!(
            planned[0].1.chars().count() <= FIRST_BLOCK_CHARS + 8,
            "the first block is the short one: {}",
            planned[0].1.chars().count()
        );
    }

    #[tokio::test]
    async fn the_first_card_costs_one_short_block_not_the_whole_document() {
        let text = "the writer did a thing that reads badly. ".repeat(120);
        let planned = blocks(&text);

        // 0.25 ms per character is a slow free model emitting JSON: 250 ms per 1,000
        // characters. The model time is simulated; the scheduling it measures is the real
        // code path.
        let stub = Stub::new(0.25);
        let whole_document = stub.delay(prompt::grammar_prompt(&text).1.len());

        let started = Instant::now();
        let mut first: Option<(Duration, usize)> = None;
        let result = run_with(&text, &stub, |progress| {
            if first.is_none() {
                first = Some((started.elapsed(), progress.done));
            }
        })
        .await
        .expect("the stub always answers");
        let (elapsed, done) = first.expect("one progress event per answered block");

        eprintln!(
            "first card: {:?} after {} of {} blocks; one whole-document request would have taken {:?}",
            elapsed,
            done,
            planned.len(),
            whole_document
        );

        assert_eq!(done, 1, "the first event is the first block, not the document");
        assert!(
            elapsed < whole_document / 2,
            "first card {elapsed:?} against {whole_document:?} for one whole-document request"
        );
        assert!(result.issues.is_empty());
    }

    #[tokio::test]
    async fn blocks_after_the_first_are_in_the_air_together() {
        let text = "a clause long enough to be split. ".repeat(160);
        let planned = blocks(&text);
        assert!(planned.len() >= 5, "blocks: {}", planned.len());

        let stub = Stub::new(0.05);
        let started = Instant::now();
        let mut last = 0usize;
        run_with(&text, &stub, |progress| last = progress.done)
            .await
            .expect("the stub always answers");
        let elapsed = started.elapsed();
        let sequential: Duration = planned
            .iter()
            .map(|(_, block)| stub.delay(prompt::grammar_prompt(block).1.len()))
            .sum();

        eprintln!(
            "{} blocks: {:?} with three in the air, {:?} one after another",
            planned.len(),
            elapsed,
            sequential
        );

        assert_eq!(last, planned.len(), "every block reported");
        assert!(
            elapsed * 3 < sequential * 2,
            "overlap: {elapsed:?} against {sequential:?} one at a time"
        );
    }

    /// Answers every block with the same quote, but the later blocks answer first. The
    /// merged list still has to read in document order.
    struct Echo;

    impl Answerer for Echo {
        fn answer<'a>(
            &'a self,
            _system: String,
            user: String,
        ) -> Pin<Box<dyn Future<Output = Result<(String, String), String>> + Send + 'a>> {
            let delay = if user.contains("first") {
                60
            } else if user.contains("second") {
                30
            } else {
                0
            };
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Ok((
                    "echo".to_string(),
                    "{\"issues\":[{\"original\":\"teh\",\"replacement\":\"the\",\"message\":\"typo\",\"category\":\"spelling\"}]}"
                        .to_string(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn issues_from_overlapping_blocks_arrive_in_document_order() {
        let text = "teh first thing is here.\n\nteh second thing is here.\n\nteh third thing is here.";
        let result = run_with(text, &Echo, |_| {})
            .await
            .expect("the stub always answers");

        assert_eq!(result.issues.len(), 3);
        let starts: Vec<usize> = result.issues.iter().map(|issue| issue.start).collect();
        assert!(
            starts.windows(2).all(|pair| pair[0] < pair[1]),
            "document order: {starts:?}"
        );
        for issue in &result.issues {
            assert_eq!(
                slice_utf16(text, issue.start, issue.end),
                Some("teh".to_string()),
                "every span still points at its quote"
            );
        }
    }
}
