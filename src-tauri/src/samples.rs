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
//! A sample also carries what the ranking needs and the platform does not report:
//! which provider answered, how long the first token took, how many tokens went each
//! way, the HTTP status, and — once the person has decided — whether they kept the
//! reply, undid it, or ran it again. That last one is the quality term, and it cannot
//! be bought or synthesised: it is the only signal that says the model was *right*.
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

/// Recording is a read, insert and rewrite of one file, and a grammar check now answers
/// up to three blocks at once. Without this, two writers could each read the old log and
/// drop the other's line. One lock for the process is enough: nothing here is hot.
static RECORD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    RECORD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The two jobs are measured separately: a model that writes good prose can still
/// refuse to emit JSON, and a grammar check is nothing but JSON.
pub const PARAPHRASE: &str = "paraphrase";
pub const GRAMMAR: &str = "grammar";

pub const TASKS: [&str; 2] = [PARAPHRASE, GRAMMAR];

/// What the person did with a reply. A kept reply is the model being right; an undone
/// one is the model being wrong in a way the person had to fix; a rerun is "close, try
/// again", which is worth more than an undo and much less than a keep.
pub const KEPT: &str = "kept";
pub const UNDONE: &str = "undone";
pub const RERUN: &str = "rerun";

/// What a kept reply is worth, a rerun, and an undo, out of one. Rerun sits in the
/// middle because the text was usable enough to iterate on rather than discard.
const KEPT_VALUE: f64 = 1.0;
const RERUN_VALUE: f64 = 0.4;
const UNDONE_VALUE: f64 = 0.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix milliseconds, so two calls in the same second keep their order.
    pub at: i64,
    pub model: String,
    pub task: String,
    pub ok: bool,
    pub ms: u64,
    /// The provider that answered: a host for a plain endpoint, or the host of the
    /// base URL a provider was configured with. Older lines have none.
    #[serde(default)]
    pub provider: Option<String>,
    /// Time to the first token, which is when a rewrite starts to feel answered.
    #[serde(default)]
    pub ttft_ms: Option<u64>,
    #[serde(default)]
    pub tokens_in: Option<u64>,
    #[serde(default)]
    pub tokens_out: Option<u64>,
    /// The HTTP status, when there was one. A 429 is evidence about the endpoint that
    /// an `ok: false` alone does not carry.
    #[serde(default)]
    pub status: Option<u16>,
    /// `kept`, `undone` or `rerun`, once the person has decided. `None` means the call
    /// worked and nobody has said whether it was right.
    #[serde(default)]
    pub outcome: Option<String>,
}

/// Everything about a call that is not model, task, success or duration.
#[derive(Debug, Clone, Default)]
pub struct CallMeta {
    pub provider: Option<String>,
    pub ttft_ms: Option<u64>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub status: Option<u16>,
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
    /// Median time to first token, on the calls that reported one.
    pub median_ttft_ms: Option<u64>,
    /// How the person judged the replies that worked.
    pub kept: usize,
    pub undone: usize,
    pub rerun: usize,
    /// Calls that came back 429. The ranking fines an endpoint that keeps doing it.
    pub rate_limited: usize,
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

    /// Replies the person actually judged. A sample nobody has decided about says
    /// nothing about quality, so it is not counted either way.
    pub fn judged(&self) -> usize {
        self.kept + self.undone + self.rerun
    }

    /// The quality term out of one: the mean of the verdicts, or None before anyone
    /// has judged this model on this task. Undecided calls are left out rather than
    /// counted as good, because that would flatter a model nobody has used.
    pub fn quality(&self) -> Option<f64> {
        let judged = self.judged();
        if judged == 0 {
            return None;
        }
        let total = self.kept as f64 * KEPT_VALUE
            + self.rerun as f64 * RERUN_VALUE
            + self.undone as f64 * UNDONE_VALUE;
        Some(total / judged as f64)
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

pub fn record(app: &AppHandle, model: &str, task: &str, ok: bool, ms: u64, meta: CallMeta) {
    if model.trim().is_empty() {
        return;
    }
    let _guard = locked();
    let sample = Sample {
        at: now(),
        model: model.trim().to_string(),
        task: task.to_string(),
        ok,
        ms,
        provider: meta.provider,
        ttft_ms: meta.ttft_ms,
        tokens_in: meta.tokens_in,
        tokens_out: meta.tokens_out,
        status: meta.status,
        outcome: None,
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

/// The plain-logic half of recording a verdict, so it can be tested without a window.
///
/// The newest sample for this model and task that nobody has judged is the one the
/// verdict belongs to: runs are recorded newest first, so that is the call the person
/// just looked at. A second verdict on the same call is ignored rather than overwriting
/// the first, because the first is what they actually did.
pub fn apply_outcome(samples: &mut [Sample], model: &str, task: &str, outcome: &str) -> bool {
    if !matches!(outcome, KEPT | UNDONE | RERUN) {
        return false;
    }
    let found = samples
        .iter_mut()
        .find(|sample| sample.model == model && sample.task == task && sample.outcome.is_none());
    match found {
        Some(sample) => {
            sample.outcome = Some(outcome.to_string());
            true
        }
        None => false,
    }
}

/// Records what the person did with the newest reply from this model on this task.
/// Returns whether there was a sample to judge.
pub fn set_outcome(app: &AppHandle, model: &str, task: &str, outcome: &str) -> bool {
    let _guard = locked();
    let Ok(mut samples) = load(app) else {
        return false;
    };
    if !apply_outcome(&mut samples, model, task, outcome) {
        return false;
    }
    if let Err(err) = write(app, &samples) {
        eprintln!("samples: {err}");
        return false;
    }
    true
}

fn median(values: &mut [u64]) -> Option<u64> {
    values.sort_unstable();
    if values.is_empty() {
        None
    } else {
        Some(values[values.len() / 2])
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
    let median_ms = median(&mut times);

    let mut ttfts: Vec<u64> = mine
        .iter()
        .filter(|sample| sample.ok)
        .filter_map(|sample| sample.ttft_ms)
        .collect();
    let median_ttft_ms = median(&mut ttfts);

    let count = |outcome: &str| {
        mine.iter()
            .filter(|sample| sample.outcome.as_deref() == Some(outcome))
            .count()
    };

    Evidence {
        runs,
        ok,
        streak,
        median_ms,
        median_ttft_ms,
        kept: count(KEPT),
        undone: count(UNDONE),
        rerun: count(RERUN),
        rate_limited: mine
            .iter()
            .filter(|sample| sample.status == Some(429))
            .count(),
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
            provider: Some("openrouter.ai".into()),
            ttft_ms: None,
            tokens_in: None,
            tokens_out: None,
            status: None,
            outcome: None,
        }
    }

    fn judged(at: i64, model: &str, outcome: &str) -> Sample {
        Sample {
            outcome: Some(outcome.to_string()),
            ..sample(at, model, PARAPHRASE, true, 500)
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

    #[test]
    fn an_older_line_without_the_new_fields_still_parses() {
        let line = "{\"at\":7,\"model\":\"m\",\"task\":\"paraphrase\",\"ok\":true,\"ms\":512}";
        let parsed: Sample = serde_json::from_str(line).expect("a 0.2.2 sample still parses");
        assert_eq!(parsed.provider, None);
        assert_eq!(parsed.outcome, None);
        assert_eq!(parsed.status, None);
    }

    #[test]
    fn a_sample_records_what_the_call_cost_and_where_it_went() {
        let meta = CallMeta {
            provider: Some("api.groq.com".into()),
            ttft_ms: Some(180),
            tokens_in: Some(220),
            tokens_out: Some(190),
            status: Some(200),
        };
        let log = vec![Sample {
            provider: meta.provider.clone(),
            ttft_ms: meta.ttft_ms,
            tokens_in: meta.tokens_in,
            tokens_out: meta.tokens_out,
            status: meta.status,
            ..sample(1, "m", GRAMMAR, true, 900)
        }];
        let found = evidence(&log, "m", GRAMMAR);
        assert_eq!(found.median_ttft_ms, Some(180));
        assert_eq!(found.rate_limited, 0);

        let mut limited = log.clone();
        limited[0].status = Some(429);
        assert_eq!(evidence(&limited, "m", GRAMMAR).rate_limited, 1);
    }

    #[test]
    fn quality_is_the_mean_of_the_verdicts_and_none_before_any() {
        let unjudged = vec![sample(1, "m", PARAPHRASE, true, 100)];
        assert_eq!(evidence(&unjudged, "m", PARAPHRASE).quality(), None);

        let log = vec![
            judged(4, "m", KEPT),
            judged(3, "m", KEPT),
            judged(2, "m", RERUN),
            judged(1, "m", UNDONE),
            sample(0, "m", PARAPHRASE, true, 100),
        ];
        let found = evidence(&log, "m", PARAPHRASE);
        assert_eq!(found.judged(), 4);
        // (1 + 1 + 0.4 + 0) / 4
        assert!((found.quality().unwrap() - 0.6).abs() < 1e-9);
        // The undecided call is not counted as good or bad.
        assert_eq!(found.runs, 5);
    }

    #[test]
    fn a_kept_reply_outranks_an_undone_one_on_quality_alone() {
        let kept = vec![judged(3, "a", KEPT), judged(2, "a", KEPT)];
        let undone = vec![judged(3, "b", UNDONE), judged(2, "b", UNDONE)];
        let a = evidence(&kept, "a", PARAPHRASE).quality().unwrap();
        let b = evidence(&undone, "b", PARAPHRASE).quality().unwrap();
        assert!(a > b, "{a} should beat {b}");
    }

    #[test]
    fn a_verdict_lands_on_the_newest_unjudged_call_and_only_once() {
        let mut log = vec![
            judged(5, "m", KEPT),
            sample(4, "m", PARAPHRASE, true, 300),
            sample(1, "m", PARAPHRASE, true, 300),
        ];
        assert!(apply_outcome(&mut log, "m", PARAPHRASE, UNDONE));
        assert_eq!(log[1].outcome.as_deref(), Some(UNDONE));
        assert_eq!(log[2].outcome, None, "only the newest call is judged");

        // A second verdict has nothing left to attach to at the top.
        assert!(apply_outcome(&mut log, "m", PARAPHRASE, KEPT));
        assert_eq!(log[2].outcome.as_deref(), Some(KEPT));
        assert_eq!(log[1].outcome.as_deref(), Some(UNDONE), "the first verdict stands");
    }

    #[test]
    fn an_unknown_verdict_is_refused() {
        let mut log = vec![sample(1, "m", PARAPHRASE, true, 100)];
        assert!(!apply_outcome(&mut log, "m", PARAPHRASE, "liked"));
        assert_eq!(log[0].outcome, None);
    }
}
