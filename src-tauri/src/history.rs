//! Every run, kept on this machine and nowhere else.
//!
//! The plan allows a JSON log while the volume stays small, and one line per run is
//! small enough to open in an editor, which a SQLite file is not. Each line is a
//! complete [`Run`], written newest-first, capped so the file cannot grow without
//! bound, and replaced atomically: a crash mid-write leaves the previous history
//! rather than half of a new one.
//!
//! A grammar check stores the text and what the checker found; a paraphrase stores
//! both sides, because restoring it means reopening the pair.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::store;

const FILE: &str = "history.jsonl";
/// Enough to cover months of casual use, small enough to stay a text file.
const CAP: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    /// Unix seconds.
    pub at: i64,
    /// `paraphrase` or `grammar`.
    pub tool: String,
    /// Empty for a grammar check, which has no modes.
    pub mode: String,
    pub model: String,
    pub original: String,
    /// None for a grammar check: the checker suggests, the person decides.
    pub result: Option<String>,
    /// What happened, in one line, e.g. "12 issue(s) · 1 unplaced".
    pub note: String,
    pub words_in: u32,
    pub words_out: u32,
    pub ms: u64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub runs: usize,
    pub words_in: u64,
    pub words_out: u64,
    pub per_tool: BTreeMap<String, usize>,
    pub per_mode: BTreeMap<String, usize>,
    pub models: Vec<ModelUse>,
    /// Words added (positive) or removed (negative) across rewrites.
    pub words_delta: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUse {
    pub model: String,
    pub runs: usize,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(store::data_dir(app)?.join(FILE))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|gap| gap.as_secs() as i64)
        .unwrap_or(0)
}

fn new_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|gap| format!("{:x}", gap.as_nanos()))
        .unwrap_or_else(|_| "0".to_string())
}

fn words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// Adds one run to the top of the history, dropping the oldest past the cap.
fn write_run(app: &AppHandle, run: Run) -> Result<(), String> {
    let mut runs = list(app)?;
    runs.insert(0, run);
    runs.truncate(CAP);

    let path = path(app)?;
    let mut body = String::new();
    for entry in &runs {
        body.push_str(&serde_json::to_string(entry).map_err(|e| e.to_string())?);
        body.push('\n');
    }

    // Write beside the real file, then rename: a failed write leaves the old history
    // intact instead of truncating it.
    let temp = path.with_extension("jsonl.tmp");
    std::fs::write(&temp, body).map_err(|e| format!("could not write the history: {e}"))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("could not replace the history: {e}"))
}

pub fn list(app: &AppHandle) -> Result<Vec<Run>, String> {
    let body = match std::fs::read_to_string(path(app)?) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("could not read the history: {err}")),
    };

    let mut runs: Vec<Run> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        // One unreadable line — a hand edit, a partial write from an old build — must
        // not cost the rest of the history.
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.at));
    Ok(runs)
}

pub fn delete(app: &AppHandle, id: &str) -> Result<Vec<Run>, String> {
    let mut runs = list(app)?;
    runs.retain(|run| run.id != id);
    write_all(app, &runs)?;
    Ok(runs)
}

pub fn clear(app: &AppHandle) -> Result<Vec<Run>, String> {
    match std::fs::remove_file(path(app)?) {
        Ok(()) => Ok(Vec::new()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("could not delete the history: {err}")),
    }
}

fn write_all(app: &AppHandle, runs: &[Run]) -> Result<(), String> {
    let path = path(app)?;
    let mut body = String::new();
    for entry in runs {
        body.push_str(&serde_json::to_string(entry).map_err(|e| e.to_string())?);
        body.push('\n');
    }
    let temp = path.with_extension("jsonl.tmp");
    std::fs::write(&temp, body).map_err(|e| format!("could not write the history: {e}"))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("could not replace the history: {e}"))
}

pub fn stats(app: &AppHandle) -> Result<Stats, String> {
    Ok(summarize(&list(app)?))
}

/// Kept separate from the file so the numbers can be tested without a window.
fn summarize(runs: &[Run]) -> Stats {
    let mut per_tool: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_mode: BTreeMap<String, usize> = BTreeMap::new();
    let mut models: BTreeMap<String, usize> = BTreeMap::new();
    let (mut words_in, mut words_out, mut delta) = (0u64, 0u64, 0i64);

    for run in runs {
        *per_tool.entry(run.tool.clone()).or_insert(0) += 1;
        if !run.mode.is_empty() {
            *per_mode.entry(run.mode.clone()).or_insert(0) += 1;
        }
        if !run.model.is_empty() {
            *models.entry(run.model.clone()).or_insert(0) += 1;
        }
        words_in += run.words_in as u64;
        words_out += run.words_out as u64;
        if run.tool == "paraphrase" {
            delta += run.words_out as i64 - run.words_in as i64;
        }
    }

    let mut models: Vec<ModelUse> = models
        .into_iter()
        .map(|(model, runs)| ModelUse { model, runs })
        .collect();
    models.sort_by(|a, b| b.runs.cmp(&a.runs).then_with(|| a.model.cmp(&b.model)));

    Stats {
        runs: runs.len(),
        words_in,
        words_out,
        per_tool,
        per_mode,
        models,
        words_delta: delta,
    }
}

/* ------------------------------------------------------- what the commands call */

/// A completed rewrite. History is a convenience, so a failed write is reported to
/// stderr and never turns a good rewrite into an error.
pub fn record_paraphrase(
    app: &AppHandle,
    mode: &str,
    model: &str,
    original: &str,
    result: &str,
    ms: u64,
) {
    let run = Run {
        id: new_id(),
        at: now(),
        tool: "paraphrase".to_string(),
        mode: mode.to_string(),
        model: model.to_string(),
        original: original.to_string(),
        result: Some(result.to_string()),
        note: String::new(),
        words_in: words(original),
        words_out: words(result),
        ms,
    };
    if let Err(err) = write_run(app, run) {
        eprintln!("history: {err}");
    }
}

/// A completed check. The result is the person's own text — the checker only offered
/// suggestions, and recording them as the outcome would misreport the run.
pub fn record_grammar(
    app: &AppHandle,
    original: &str,
    model: &str,
    issues: usize,
    unplaced: usize,
    dropped: usize,
    ms: u64,
) {
    let mut note = format!("{issues} issue(s)");
    if unplaced > 0 {
        note.push_str(&format!(" · {unplaced} unplaced"));
    }
    if dropped > 0 {
        note.push_str(&format!(" · {dropped} dropped"));
    }

    let run = Run {
        id: new_id(),
        at: now(),
        tool: "grammar".to_string(),
        mode: String::new(),
        model: model.to_string(),
        original: original.to_string(),
        result: None,
        note,
        words_in: words(original),
        words_out: words(original),
        ms,
    };
    if let Err(err) = write_run(app, run) {
        eprintln!("history: {err}");
    }
}

/// Opens the data directory in the system file manager. Settings → Data points here so
/// the history is a file the person can read, copy or delete without this app.
pub fn open_folder(app: &AppHandle) -> Result<(), String> {
    let dir = store::data_dir(app)?;
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the data folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(tool: &str, mode: &str, model: &str, words_in: u32, words_out: u32) -> Run {
        Run {
            id: "x".to_string(),
            at: 1,
            tool: tool.to_string(),
            mode: mode.to_string(),
            model: model.to_string(),
            original: "a b".to_string(),
            result: None,
            note: String::new(),
            words_in,
            words_out,
            ms: 10,
        }
    }

    #[test]
    fn stats_count_each_tool_mode_and_model() {
        let runs = vec![
            run("paraphrase", "standard", "openrouter/free", 100, 80),
            run("paraphrase", "shorten", "openrouter/free", 50, 30),
            run("grammar", "", "google/gemma:free", 40, 40),
        ];
        let stats = summarize(&runs);
        assert_eq!(stats.runs, 3);
        assert_eq!(stats.words_in, 190);
        assert_eq!(stats.words_out, 150);
        assert_eq!(stats.per_tool.get("paraphrase"), Some(&2));
        assert_eq!(stats.per_mode.get("standard"), Some(&1));
        // A grammar check has no mode, so it adds no mode row.
        assert_eq!(stats.per_mode.len(), 2);
        assert_eq!(stats.models[0].model, "openrouter/free");
        assert_eq!(stats.models[0].runs, 2);
    }

    #[test]
    fn the_word_delta_is_rewrites_only() {
        // The grammar run cannot change the text, so it must not cancel out a shorten.
        let runs = vec![
            run("paraphrase", "shorten", "m", 100, 60),
            run("grammar", "", "m", 500, 500),
        ];
        assert_eq!(summarize(&runs).words_delta, -40);
    }

    #[test]
    fn an_empty_history_summarises_to_zeros() {
        let stats = summarize(&[]);
        assert_eq!(stats.runs, 0);
        assert!(stats.models.is_empty());
        assert_eq!(stats.words_delta, 0);
    }

    #[test]
    fn a_run_round_trips_through_the_line_format() {
        let original = run("grammar", "", "m", 12, 12);
        let line = serde_json::to_string(&original).expect("serialise");
        let parsed: Run = serde_json::from_str(&line).expect("parse");
        assert_eq!(parsed.words_in, 12);
        assert_eq!(parsed.tool, "grammar");
        // camelCase on the wire, so the TypeScript type and this struct agree.
        assert!(line.contains("\"wordsIn\""));
    }
}
