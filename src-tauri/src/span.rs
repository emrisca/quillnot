//! Reworking part of an answer.
//!
//! QuillBot's most useful gesture is not the whole-document rewrite, it is selecting one
//! clause and asking again. This module is the small, cheap version of that: one sentence
//! is rewritten so it drops back into the paragraph, and one word gets a short list of
//! alternatives that fit the sentence it sits in.
//!
//! Both calls are the same provider route and the same sample log as a full rewrite, so
//! a model that fails here is measured here too, and Auto learns from it. Nothing is
//! stored: a span rewrite is an edit to text the app already has, not a run of its own.

use serde::Serialize;
use tauri::AppHandle;

use crate::{openrouter, prompt, providers, samples};

/// The most context worth sending: enough for the model to keep voice and tense, short
/// enough that a sentence edit costs a fraction of a full rewrite.
const CONTEXT_LIMIT: usize = 4_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanResult {
    pub text: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynonymResult {
    pub words: Vec<String>,
    pub model: String,
}

fn context_of(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= CONTEXT_LIMIT {
        return trimmed.to_string();
    }
    // Keep the beginning, where the paragraph's subject usually is.
    let cut: String = trimmed.chars().take(CONTEXT_LIMIT).collect();
    format!("{cut}…")
}

/// Rewrites one sentence. `mode` reuses the workspace's own modes, so "make this
/// sentence formal" is the same instruction as the formal paraphrase, applied to less
/// text.
#[tauri::command]
pub async fn rephrase_sentence(
    app: AppHandle,
    sentence: String,
    context: String,
    mode: Option<String>,
) -> Result<SpanResult, String> {
    let sentence = sentence.trim().to_string();
    if sentence.is_empty() {
        return Err("Select a sentence first.".into());
    }
    if sentence.chars().count() > CONTEXT_LIMIT {
        return Err("That selection is too long to rework as one sentence.".into());
    }

    let mode = mode.unwrap_or_else(|| "standard".to_string());
    let route = providers::resolve(&app, None, samples::PARAPHRASE)?;
    let (system, user) = prompt::sentence_prompt(&sentence, &context_of(&context), &mode);

    let (model, body) =
        openrouter::complete(Some(&app), samples::PARAPHRASE, &route.attempts, &system, &user).await?;

    let text = tidy(&body);
    if text.is_empty() {
        return Err("The model returned nothing for that sentence. Try again.".into());
    }

    Ok(SpanResult { text, model })
}

/// Alternatives for one word, in the sentence it was selected from, so "breadth" in
/// "the breadth of a note" comes back as width and scope rather than as loaves.
#[tauri::command]
pub async fn word_synonyms(
    app: AppHandle,
    word: String,
    sentence: String,
) -> Result<SynonymResult, String> {
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err("Select a word first.".into());
    }
    if word.split_whitespace().count() > 1 {
        return Err("Synonyms are for a single word. Select the sentence to rework it.".into());
    }

    let route = providers::resolve(&app, None, samples::PARAPHRASE)?;
    let (system, user) = prompt::synonym_prompt(&word, &context_of(&sentence));

    let (model, body) =
        openrouter::complete(Some(&app), samples::PARAPHRASE, &route.attempts, &system, &user).await?;

    let words = parse_words(&body, &word);
    if words.is_empty() {
        return Err("The model offered no alternatives for that word.".into());
    }

    Ok(SynonymResult { words, model })
}

/// Strips what a model adds around an answer even when told not to: fences, wrapping
/// quotes, a leading label.
fn tidy(body: &str) -> String {
    let mut text = body.trim().to_string();

    if let Some(rest) = text.strip_prefix("```") {
        let without_language = rest.split_once('\n').map(|(_, body)| body).unwrap_or("");
        text = without_language.trim_end_matches("```").trim().to_string();
    }

    for prefix in ["Rewritten sentence:", "Sentence:", "Rewrite:"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim().to_string();
        }
    }

    let pairs = [('"', '"'), ('“', '”'), ('\'', '\'')];
    for (open, close) in pairs {
        if text.starts_with(open) && text.ends_with(close) && text.chars().count() > 2 {
            text = text[open.len_utf8()..text.len() - close.len_utf8()].trim().to_string();
        }
    }

    text
}

/// Reads the synonym list whether the model answered with JSON, a numbered list or a
/// comma-separated line, because the free models do all three on the same prompt.
fn parse_words(body: &str, word: &str) -> Vec<String> {
    let cleaned = body.trim();
    let cleaned = cleaned
        .strip_prefix("```json")
        .or_else(|| cleaned.strip_prefix("```"))
        .unwrap_or(cleaned)
        .trim_end_matches("```")
        .trim();

    let mut candidates: Vec<String> = Vec::new();

    let json = serde_json::from_str::<serde_json::Value>(cleaned).ok().or_else(|| {
        // A model that wrapped the array in prose still usually has one in there.
        let start = cleaned.find('[')?;
        let end = cleaned.rfind(']')?;
        serde_json::from_str::<serde_json::Value>(&cleaned[start..=end]).ok()
    });

    match json {
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    candidates.push(text.to_string());
                }
            }
        }
        Some(serde_json::Value::Object(map)) => {
            for key in ["words", "synonyms", "alternatives"] {
                if let Some(serde_json::Value::Array(items)) = map.get(key) {
                    for item in items {
                        if let Some(text) = item.as_str() {
                            candidates.push(text.to_string());
                        }
                    }
                }
            }
        }
        _ => {
            for line in cleaned.lines() {
                for piece in line.split(',') {
                    candidates.push(piece.to_string());
                }
            }
        }
    }

    let wanted = word.to_lowercase();
    let mut seen: Vec<String> = Vec::new();
    for candidate in candidates {
        // Numbering, bullets and quotes are noise; so is the original word coming back.
        let text = candidate
            .trim()
            .trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.' || ch == ')' || ch == '-' || ch == '*')
            .trim()
            .trim_matches(|ch| ch == '"' || ch == '“' || ch == '”' || ch == '\'' || ch == '`')
            .trim()
            .to_string();

        if text.is_empty() || text.split_whitespace().count() != 1 {
            continue;
        }
        let key = text.to_lowercase();
        if key == wanted || seen.iter().any(|kept| kept.to_lowercase() == key) {
            continue;
        }
        seen.push(text);
        if seen.len() == 5 {
            break;
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fenced_answer_loses_its_fence_and_quotes() {
        assert_eq!(tidy("```\nsimply this\n```"), "simply this");
        assert_eq!(tidy("“simply this”"), "simply this");
        assert_eq!(tidy("Rewritten sentence: simply this"), "simply this");
        assert_eq!(tidy("  simply this  "), "simply this");
    }

    #[test]
    fn a_json_array_is_read_in_order() {
        let words = parse_words("[\"width\", \"scope\", \"reach\"]", "breadth");
        assert_eq!(words, vec!["width", "scope", "reach"]);
    }

    #[test]
    fn a_prose_wrapped_array_still_parses() {
        let body = "Here are a few: [\"width\", \"scope\"]";
        assert_eq!(parse_words(body, "breadth"), vec!["width", "scope"]);
    }

    #[test]
    fn a_numbered_list_is_accepted_when_the_model_ignores_the_json_request() {
        let words = parse_words("1. width\n2. scope\n3. reach", "breadth");
        assert_eq!(words, vec!["width", "scope", "reach"]);
    }

    #[test]
    fn the_original_word_and_duplicates_are_dropped() {
        let words = parse_words("[\"breadth\", \"width\", \"Width\", \"scope\"]", "Breadth");
        assert_eq!(words, vec!["width", "scope"]);
    }

    #[test]
    fn multiword_answers_are_not_synonyms() {
        assert!(parse_words("[\"the width of\", \"scope\"]", "breadth") == vec!["scope"]);
    }

    #[test]
    fn a_long_paragraph_is_cut_for_the_context_only() {
        let long = "a".repeat(CONTEXT_LIMIT + 50);
        let context = context_of(&long);
        assert_eq!(context.chars().count(), CONTEXT_LIMIT + 1);
        assert!(context.ends_with('…'));
    }
}
