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
use tauri::AppHandle;

use crate::{openrouter, prompt, providers};

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
    let route = providers::resolve(app, model)?;
    let started = std::time::Instant::now();
    let result = run(&route.attempts, text).await?;

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
    run(&route.attempts, text).await
}

async fn run(attempts: &[providers::Attempt], text: &str) -> Result<GrammarResult, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("There is nothing to check.".to_string());
    }

    let (system, user) = prompt::grammar_prompt(text);
    let (model, raw) = openrouter::complete(attempts, &system, &user).await?;
    let parsed = parse_issues(&raw)?;
    let (issues, dropped, unlocated) = verify(text, parsed);

    Ok(GrammarResult {
        issues,
        model,
        dropped,
        unlocated,
    })
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

    fn raw(quote: &str, suggestion: &str, category: &str) -> RawIssue {
        RawIssue {
            original: Some(quote.to_string()),
            replacement: Some(suggestion.to_string()),
            message: Some("test".to_string()),
            category: Some(category.to_string()),
        }
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
}
