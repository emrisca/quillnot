//! Our own per-call samples: the one measurement the platform will not give us.
//!
//! `/models/{id}/endpoints` reports `latency_last_30m` and `throughput_last_30m`, but
//! on free endpoints those come back `null` — nobody is paying, so nothing is
//! measured. When the live table was first built, eleven of fourteen free endpoints
//! reported only a day-long uptime. That is enough to say "reachable" and not enough
//! to rank by speed.
//!
//! So every call this app makes is timed here, per model and per task, and the ranking
//! folds these in. A call is a sample whether it worked or not: three failures in a
//! row is what demotes a model, and the first success after that clears the streak.
//! Recording never fails a run — a full disk costs a sample, not a rewrite.
//!
//! The log is one JSON line per call beside the history, with the same guarantees:
//! newest first, capped, replaced atomically, and one unreadable line costs only that
//! line.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::store;

const FILE: &str = "samples.jsonl";
/// Four hundred calls is weeks of ordinary use in a file small enough to read.
const CAP: usize = 400;

/// The two jobs are measured separately: a model that writes good prose can still
/// refuse to emit JSON, and a grammar check is nothing but JSON.
pub const PARAPHRASE: &str = "paraphrase";
pub const GRAMMAR: &str = "grammar";

pub const TASKS: [&str; 2] = [PARAPHRASE, GRAMMAR];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix milliseconds, so two calls in the same second keep their order.
    pub at: i64,
    pub model: String,
    pub task: String,
    pub ok: bool,
    pub ms: u64,
}

/// What the log says about one model on one task.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub runs: usize,
    pub ok: usize,
    /// Failures at the top of the log. Three in a row is the demotion; a success
    /// clears it, which is how a recovered model climbs back.
    pub streak: usize,
    /// Median of the calls that worked. A failed call is not slow, it is absent, so
    /// its time is left out.
    pub median_ms: Option<u64>,
}

impl Evidence {
    /// The share of calls that came back with something, or None before any call.
    pub fn success_rate(&self) -> Option<f64> {
        if self.runs == 0 {
            None
        } else {
            Some(self.ok as f64 / self.runs as f64)
        }
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(store::data_dir(app)?.join(FILE))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|gap| gap.as_millis() as i64)
        .unwrap_or(0)
}

/// Newest first, capped. One unreadable line — a hand edit, a partial write — must not
/// cost the rest of the log.
pub fn load(app: &AppHandle) -> Result<Vec<Sample>, String> {
    let body = match std::fs::read_to_string(path(app)?) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("could not read the samples: {err}")),
    };

    let mut samples: Vec<Sample> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    samples.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(samples)
}

fn write(app: &AppHandle, samples: &[Sample]) -> Result<(), String> {
    let path = path(app)?;
    let mut body = String::new();
    for sample in samples {
        body.push_str(&serde_json::to_string(sample).map_err(|e| e.to_string())?);
        body.push('\n');
    }

    // Beside the real file, then rename: a failed write leaves the previous log.
    let temp = path.with_extension("jsonl.tmp");
    std::fs::write(&temp, body).map_err(|e| format!("could not write the samples: {e}"))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("could not replace the samples: {e}"))
}

pub fn record(app: &AppHandle, model: &str, task: &str, ok: bool, ms: u64) {
    if model.trim().is_empty() {
        return;
    }
    let sample = Sample {
        at: now(),
        model: model.trim().to_string(),
        task: task.to_string(),
        ok,
        ms,
    };

    let written = load(app).and_then(|mut samples| {
        samples.insert(0, sample);
        samples.truncate(CAP);
        write(app, &samples)
    });

    if let Err(err) = written {
        eprintln!("samples: {err}");
    }
}

/// Everything the log knows about one model on one task.
pub fn evidence(samples: &[Sample], model: &str, task: &str) -> Evidence {
    let mine: Vec<&Sample> = samples
        .iter()
        .filter(|sample| sample.model == model && sample.task == task)
        .collect();

    let runs = mine.len();
    let ok = mine.iter().filter(|sample| sample.ok).count();
    // Newest first, so the streak is however many failures come before the first
    // success.
    let streak = mine.iter().take_while(|sample| !sample.ok).count();

    let mut times: Vec<u64> = mine.iter().filter(|sample| sample.ok).map(|sample| sample.ms).collect();
    times.sort_unstable();
    let median_ms = if times.is_empty() {
        None
    } else {
        Some(times[times.len() / 2])
    };

    Evidence {
        runs,
        ok,
        streak,
        median_ms,
    }
}

/// The evidence for every model on one task, keyed by model id, so a table of
/// seventeen models does not rescan the log seventeen times.
pub fn by_model(samples: &[Sample], task: &str) -> std::collections::BTreeMap<String, Evidence> {
    let mut out = std::collections::BTreeMap::new();
    // `evidence` needs the whole log to see which failure came first, so it is asked
    // once per model instead of accumulated in one pass.
    for sample in samples.iter().filter(|sample| sample.task == task) {
        if !out.contains_key(&sample.model) {
            out.insert(sample.model.clone(), evidence(samples, &sample.model, task));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: i64, model: &str, task: &str, ok: bool, ms: u64) -> Sample {
        Sample {
            at,
            model: model.to_string(),
            task: task.to_string(),
            ok,
            ms,
        }
    }

    #[test]
    fn evidence_counts_only_this_model_and_this_task() {
        let log = vec![
            sample(5, "a/model", PARAPHRASE, true, 400),
            sample(4, "b/model", PARAPHRASE, false, 20),
            sample(3, "a/model", GRAMMAR, false, 30),
        ];
        let found = evidence(&log, "a/model", PARAPHRASE);
        assert_eq!(found.runs, 1);
        assert_eq!(found.ok, 1);
        assert_eq!(found.median_ms, Some(400));
    }

    #[test]
    fn the_streak_is_the_failures_at_the_top_of_the_log() {
        let log = vec![
            sample(6, "m", PARAPHRASE, false, 10),
            sample(5, "m", PARAPHRASE, false, 10),
            sample(4, "m", PARAPHRASE, false, 10),
            sample(3, "m", PARAPHRASE, true, 500),
        ];
        let found = evidence(&log, "m", PARAPHRASE);
        assert_eq!(found.streak, 3);
        assert_eq!(found.runs, 4);
    }

    #[test]
    fn a_success_clears_the_streak() {
        let log = vec![
            sample(6, "m", PARAPHRASE, true, 900),
            sample(5, "m", PARAPHRASE, false, 10),
            sample(4, "m", PARAPHRASE, false, 10),
        ];
        assert_eq!(evidence(&log, "m", PARAPHRASE).streak, 0);
    }

    #[test]
    fn the_median_leaves_out_the_calls_that_failed() {
        let log = vec![
            sample(9, "m", PARAPHRASE, true, 1000),
            sample(8, "m", PARAPHRASE, false, 30_000),
            sample(7, "m", PARAPHRASE, true, 200),
            sample(6, "m", PARAPHRASE, true, 600),
        ];
        assert_eq!(evidence(&log, "m", PARAPHRASE).median_ms, Some(600));
    }

    #[test]
    fn a_log_with_no_successes_has_no_median_and_a_zero_rate() {
        let log = vec![sample(2, "m", GRAMMAR, false, 15)];
        let found = evidence(&log, "m", GRAMMAR);
        assert_eq!(found.median_ms, None);
        assert_eq!(found.success_rate(), Some(0.0));
        assert!(evidence(&[], "m", GRAMMAR).success_rate().is_none());
    }

    #[test]
    fn a_sample_round_trips_through_the_line_format() {
        let line = serde_json::to_string(&sample(7, "m", PARAPHRASE, true, 512)).expect("serialise");
        let parsed: Sample = serde_json::from_str(&line).expect("parse");
        assert_eq!(parsed.ms, 512);
        assert!(line.contains("\"task\":\"paraphrase\""));
    }

    #[test]
    fn the_by_model_map_agrees_with_asking_one_at_a_time() {
        let log = vec![
            sample(3, "a", PARAPHRASE, true, 100),
            sample(2, "b", PARAPHRASE, true, 300),
            sample(1, "a", PARAPHRASE, false, 0),
        ];
        let map = by_model(&log, PARAPHRASE);
        assert_eq!(map.len(), 2);
        assert_eq!(map["a"], evidence(&log, "a", PARAPHRASE));
        assert_eq!(map["b"].runs, 1);
    }
}
