//! Mode instructions and the block splitter.
//!
//! The prompts are the product: the mode is a paragraph of instructions and a
//! sampling strength, not a different algorithm. Kept here rather than in the
//! frontend so the model call and its instructions ship together, and so M7 can
//! benchmark one task per mode without a second source of truth.

const SYSTEM: &str = "You rewrite text on request. You return only the rewritten text: \
no preamble, no explanation, no quotation marks and no markdown fences. You preserve the \
meaning, every fact, every name, number and date, and the paragraph breaks you were given. \
You never add information that is not in the text.";

fn mode_instruction(mode: &str) -> &'static str {
    match mode {
        "fluency" => "Smooth the phrasing. Join sentences that should be one, split run-ons, \
                      and fix anything that reads awkwardly. Keep the register and the length.",
        "formal" => "Rewrite in a formal register: no contractions, no slang, heavier \
                     vocabulary. Keep every fact and roughly the length.",
        "simple" => "Rewrite for a general reader: short sentences, plain words, one idea per \
                     sentence. Keep the meaning exactly.",
        "creative" => "Rewrite with livelier verbs and fresher comparisons. Same argument, \
                       more colour. Do not invent facts.",
        "expand" => "Add a little detail and connective tissue so the text reads fuller. \
                     Elaborate only on what is already there.",
        "shorten" => "Cut filler. Say the same thing in fewer words, keeping every fact.",
        "summarize" => "Summarise: keep the point and every fact it rests on, drop repetition \
                        and examples, and come back in at most a third of the length. Add \
                        nothing that is not already in the text.",
        "custom" => "Rewrite for clarity while keeping the meaning and the length.",
        _ => "Rewrite for clarity and natural phrasing. Keep the meaning, the facts and \
              roughly the length.",
    }
}

fn strength_instruction(strength: u8) -> &'static str {
    match strength {
        1 => "Change words only where it clearly helps; keep the author's phrasing.",
        2 => "Change the wording where it helps while keeping the author's voice.",
        3 => "Rewrite more freely: change wording and sentence structure.",
        _ => "Rewrite as freely as you can while keeping the meaning intact.",
    }
}

pub fn paraphrase_prompt(block: &str, mode: &str, strength: u8) -> (String, String) {
    let user = format!(
        "{}\n{}\n\nText to rewrite:\n\n{block}",
        mode_instruction(mode),
        strength_instruction(strength)
    );
    (SYSTEM.to_string(), user)
}

const GRAMMAR_SYSTEM: &str = "You are a careful copy editor. You report only real mistakes: \
spelling, grammar, punctuation and clear style problems. You never rewrite for taste, you never \
comment on content, and you never invent a problem to seem useful. You answer with JSON only.";

/// The grammar request.
///
/// Every issue must quote the text exactly: the app searches for the quote itself and
/// ignores any character offsets the model offers, because those are wrong nearly
/// every time while quotes are reliable. That contract is spelled out here rather than
/// assumed, including what to do about a doubled space, which has no visible original.
pub fn grammar_prompt(text: &str) -> (String, String) {
    let user = format!(
        "Find the mistakes in the text below.\n\n\
         Answer with a single JSON object and nothing else:\n\
         {{\"issues\":[{{\"original\":\"...\",\"replacement\":\"...\",\"message\":\"...\",\"category\":\"spelling|grammar|punctuation|style\"}}]}}\n\n\
         Rules:\n\
         - \"original\" must be copied character for character from the text, including its \
           spaces and punctuation. Do not summarise it and do not fix it there.\n\
         - Keep \"original\" as short as it can be and still be unique in the text: a word, a \
           phrase, at most a sentence.\n\
         - \"replacement\" is what that span becomes. It may be empty to delete something.\n\
         - \"message\" is one short sentence a writer would understand.\n\
         - For a doubled space or a space before punctuation, quote the run of spaces itself.\n\
         - No issue if the text is already correct: answer {{\"issues\":[]}}. Never pad the list.\n\n\
         Text:\n\n{text}"
    );
    (GRAMMAR_SYSTEM.to_string(), user)
}

/// Splits on blank lines and regroups paragraphs until `max_chars`.
///
/// Two deliberate properties. A paragraph longer than the budget is left whole
/// rather than cut mid-sentence, because half a sentence is worse input than a long
/// one. And runs of blank lines collapse to one when the text is reassembled — the
/// only change this makes to a document it cannot avoid making.
pub fn split_blocks(text: &str, max_chars: usize) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if current.is_empty() {
            current.push_str(paragraph);
        } else if current.chars().count() + 2 + paragraph.chars().count() <= max_chars {
            current.push_str("\n\n");
            current.push_str(paragraph);
        } else {
            blocks.push(std::mem::take(&mut current));
            current.push_str(paragraph);
        }
    }

    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_paragraphs_travel_together() {
        let blocks = split_blocks("One.\n\nTwo.\n\nThree.", 100);
        assert_eq!(blocks, vec!["One.\n\nTwo.\n\nThree.".to_string()]);
    }

    #[test]
    fn paragraphs_that_do_not_fit_are_kept_apart() {
        let blocks = split_blocks("aaaa\n\nbbbb", 5);
        assert_eq!(blocks, vec!["aaaa".to_string(), "bbbb".to_string()]);
    }

    #[test]
    fn runs_of_blank_lines_collapse() {
        let blocks = split_blocks("a\n\n\n\nb", 100);
        assert_eq!(blocks, vec!["a\n\nb".to_string()]);
    }

    #[test]
    fn a_long_paragraph_is_not_cut() {
        let long = "x".repeat(200);
        let blocks = split_blocks(&format!("{long}\n\nshort"), 50);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].len(), 200);
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(split_blocks("   \n\n  ", 100).is_empty());
    }

    #[test]
    fn the_grammar_prompt_asks_for_exact_quotes_and_json() {
        let (system, user) = grammar_prompt("teh cat sat");
        assert!(system.contains("JSON only"));
        assert!(user.contains("\"original\" must be copied character for character"));
        assert!(user.contains("\"issues\":[]"));
        assert!(user.ends_with("teh cat sat"));
    }

    #[test]
    fn summarize_asks_for_a_third_of_the_length_and_no_invention() {
        let (_system, user) = paraphrase_prompt("a long report", "summarize", 2);
        assert!(user.contains("Summarise"));
        assert!(user.contains("at most a third of the length"));
        assert!(user.contains("nothing that is not already in the text"));
    }

    #[test]
    fn every_prompt_names_the_mode_and_the_text() {
        let (system, user) = paraphrase_prompt("hello there", "formal", 3);
        assert!(system.contains("only the rewritten text"));
        assert!(user.contains("formal register"));
        assert!(user.contains("Rewrite more freely"));
        assert!(user.ends_with("hello there"));
    }
}
