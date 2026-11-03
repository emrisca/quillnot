//! The catalogue: every zero-priced model, the per-provider numbers behind the
//! ranking, and the ranking itself.
//!
//! Two GETs, neither needing a key. `/models` says what exists and what it costs;
//! `/models/{id}/endpoints` says which provider serves it, with `uptime_last_5m`,
//! `uptime_last_30m`, `uptime_last_1d`, `latency_last_30m` and
//! `throughput_last_30m`. Those come back `null` on a free endpoint nobody has
//! called recently, which is why they stay `Option` all the way through and why the
//! score says how much evidence stands behind it: a missing number is *no
//! evidence*, never zero.
//!
//! The platform's numbers are only half of it. Free endpoints rarely report latency
//! at all, so [`crate::samples`] times every call this app makes and the score folds
//! those in: our success rate and median latency for *this task* count for more as
//! the sample count grows, and three failures in a row demote a model no matter what
//! it says about its uptime. That is what makes Auto's choice a measurement rather
//! than an alphabet.
//!
//! The ranked table is cached, because Auto has to answer in the middle of a run
//! without waiting on seventeen HTTP requests. The cache is refreshed whenever the
//! AI Models view is opened.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::openrouter::http;
use crate::samples::{self, Evidence};
use crate::store;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const ENDPOINTS_BASE: &str = "https://openrouter.ai/api/v1/models";

/// How many endpoint lookups are in flight at once. Seventeen models is a fine
/// number of requests; seventeen at once is rude to a free API.
const CONCURRENCY: usize = 6;

/// The weights, in one place. Availability is what the platform knows best, speed is
/// where our own timing beats it, and the confidence shrink stops a single number
/// from reading like a verdict.
const AVAILABILITY_SHARE: f64 = 0.6;
const SPEED_SHARE: f64 = 0.4;
const PLATFORM_LATENCY_CEILING: f64 = 3000.0;
const THROUGHPUT_CEILING: f64 = 150.0;
/// Our latency ceiling is looser than the platform's: a free model that answers a
/// whole paragraph in six seconds is still usable, and scoring it by a 3s ceiling
/// would flatten every real measurement to zero.
const OUR_LATENCY_CEILING: f64 = 6000.0;
/// How many of our own calls it takes before our numbers count as much as the
/// platform's prior. Below it the platform's uptime still steadies the estimate.
const SAMPLE_PRIOR: f64 = 3.0;
/// This many failures in a row and the model is demoted, whatever it reports.
pub const FAIL_STREAK: usize = 3;
/// Demoted models keep half their score: still tried when nothing else answers, never
/// chosen first.
const DEMOTED_SHARE: f64 = 0.5;
/// Auto will not act on a ranking older than this. A week-old table would keep
/// sending runs to a model that has since gone dark.
const CACHE_MAX_AGE: i64 = 7 * 24 * 3600;
/// The gateway router: the one catalogue row that is not a single endpoint. It is
/// what Auto falls back to when we have measured nothing, because a guess from the
/// alphabet is not a measurement.
pub const ROUTER: &str = "openrouter/free";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub completion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Architecture {
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRow {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub architecture: Option<Architecture>,
    #[serde(default)]
    pub pricing: Option<Pricing>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRow {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub provider_name: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub max_completion_tokens: Option<u64>,
    #[serde(default)]
    pub pricing: Option<Pricing>,
    #[serde(default)]
    pub uptime_last_5m: Option<f64>,
    #[serde(default)]
    pub uptime_last_30m: Option<f64>,
    #[serde(default)]
    pub uptime_last_1d: Option<f64>,
    #[serde(default)]
    pub latency_last_30m: Option<f64>,
    #[serde(default)]
    pub throughput_last_30m: Option<f64>,
    #[serde(default)]
    pub quantization: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EndpointsResponse {
    data: EndpointsBody,
}

#[derive(Debug, Deserialize)]
struct EndpointsBody {
    #[serde(default)]
    endpoints: Vec<EndpointRow>,
}

/// What OpenRouter reported about a model, with nothing of ours mixed in. This is the
/// shape the ranking cache holds, so a refresh replaces the platform's half and the
/// sample log keeps supplying the other half.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformRow {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<u64>,
    pub providers: usize,
    pub uptime: Option<f64>,
    /// Which window the uptime came from: `5m`, `30m` or `1d`. Shown in the table, so
    /// a day-long average cannot pass for a live reading.
    pub uptime_window: Option<String>,
    pub latency_ms: Option<f64>,
    pub throughput: Option<f64>,
}

/// A catalogue row as the table shows it: what the model is, what the platform reports,
/// what we measured, and the score those produce together.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<u64>,
    pub providers: usize,
    pub uptime: Option<f64>,
    pub uptime_window: Option<String>,
    pub latency_ms: Option<f64>,
    pub throughput: Option<f64>,
    pub score: Option<f64>,
    /// 0, 1 or 2: how many of availability and speed contributed to the score.
    pub evidence: usize,
    /// Our own calls for this task.
    pub runs: usize,
    pub success_rate: Option<f64>,
    /// Median of our successful calls, in milliseconds.
    pub our_latency_ms: Option<u64>,
    /// Failures in a row, newest first.
    pub streak: usize,
    pub pinned: bool,
    pub chosen: bool,
}

impl RankedModel {
    pub fn demoted(&self) -> bool {
        self.streak >= FAIL_STREAK
    }
}

/// The table, plus the choice it implies: what Auto would pick and what is pinned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub task: String,
    pub auto: bool,
    pub pinned: Option<String>,
    /// The model Auto picks right now, from this ranking.
    pub auto_choice: Option<String>,
    pub rows: Vec<RankedModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RankingCache {
    /// Unix seconds.
    at: i64,
    rows: Vec<PlatformRow>,
}

/// Ids that are text-to-text by modality but are not chat models. A music generator
/// and a content-safety classifier both passed the modality check, and neither can
/// paraphrase a sentence. This is a blocklist because the catalogue carries no "this
/// is a chat model" flag; per-task measurement is the real answer, since a classifier
/// cannot fake a working rewrite.
const NOT_CHAT: &[&str] = &[
    "content-safety",
    "lyria",
    "moderation",
    "guard",
    "embed",
    "rerank",
    "whisper",
    "tts",
];

fn is_general_chat_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    !NOT_CHAT.iter().any(|hint| id.contains(hint))
}

fn price_is_zero(price: Option<&str>) -> bool {
    price
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(|value| value == 0.0)
        .unwrap_or(false)
}

/// A model is worth listing when its prompt costs nothing and it takes and returns
/// text. The music and image models in the free tier are excluded by the modality
/// check rather than by name, so a new one cannot slip in.
pub fn is_free_text_model(row: &ModelRow) -> bool {
    if !price_is_zero(row.pricing.as_ref().and_then(|p| p.prompt.as_deref())) {
        return false;
    }
    if !is_general_chat_model(&row.id) {
        return false;
    }
    let Some(arch) = row.architecture.as_ref() else {
        return true;
    };
    let text_out =
        arch.output_modalities.is_empty() || arch.output_modalities.iter().any(|m| m == "text");
    let text_in =
        arch.input_modalities.is_empty() || arch.input_modalities.iter().any(|m| m == "text");
    text_in && text_out
}

/// OpenRouter reports uptime as a fraction today, but the field has appeared as a
/// percentage in older payloads. Accept both rather than showing "9700%".
fn normalise_uptime(value: f64) -> f64 {
    if value > 1.0 {
        (value / 100.0).clamp(0.0, 1.0)
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// The best uptime any provider reported, from the most recent window that had any
/// value, and which window that was.
///
/// Free endpoints frequently report `null` for the 5-minute and 30-minute windows
/// while carrying a day-long figure. Falling straight through to the day is right —
/// the number is real — as long as the table says which one it is.
pub fn best_uptime(endpoints: &[EndpointRow]) -> (Option<f64>, Option<&'static str>) {
    let windows: [(&'static str, fn(&EndpointRow) -> Option<f64>); 3] = [
        ("5m", |row| row.uptime_last_5m),
        ("30m", |row| row.uptime_last_30m),
        ("1d", |row| row.uptime_last_1d),
    ];

    for (label, pick) in windows {
        if let Some(best) = endpoints.iter().filter_map(pick).reduce(f64::max) {
            return (Some(normalise_uptime(best)), Some(label));
        }
    }
    (None, None)
}

/// Speed out of 100 as a fraction: latency counts for more than throughput, because a
/// rewrite is judged by when the first useful sentence arrives.
fn speed(latency_ms: Option<f64>, throughput: Option<f64>) -> Option<f64> {
    let by_latency = latency_ms.map(|ms| (1.0 - ms / PLATFORM_LATENCY_CEILING).clamp(0.0, 1.0));
    let by_throughput = throughput.map(|tps| (tps / THROUGHPUT_CEILING).clamp(0.0, 1.0));
    match (by_latency, by_throughput) {
        (Some(a), Some(b)) => Some(0.6 * a + 0.4 * b),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The weighted mean of whatever components we have, and how many there were.
///
/// Unknown components are left out instead of dragging the mean down: a brand-new
/// endpoint has no 30-minute window, and ranking it last for that would be a lie
/// about the model. `evidence` carries that uncertainty to the table.
fn blend(availability: Option<f64>, speed: Option<f64>) -> (Option<f64>, usize) {
    let parts: Vec<(f64, f64)> = [
        (availability.map(normalise_uptime), AVAILABILITY_SHARE),
        (speed, SPEED_SHARE),
    ]
    .into_iter()
    .filter_map(|(value, weight)| value.map(|value| (value, weight)))
    .collect();

    if parts.is_empty() {
        return (None, 0);
    }
    let weight: f64 = parts.iter().map(|(_, weight)| weight).sum();
    let weighted: f64 = parts.iter().map(|(value, weight)| value * weight).sum();
    let mean = 100.0 * weighted / weight;

    // Thin evidence shrinks the score towards the middle. Without this, thirteen free
    // models that report nothing but a 100% day-long uptime all score 100 and the
    // table ranks alphabetically, which looks like a ranking and is not one. With it,
    // a model we have only one signal for is promising rather than proven.
    let confidence = 0.6 + 0.2 * parts.len() as f64;
    (Some(mean * confidence), parts.len())
}

/// The score out of 100 from the platform's numbers alone, and how many components
/// stood behind it.
pub fn score(
    availability: Option<f64>,
    latency_ms: Option<f64>,
    throughput: Option<f64>,
) -> (Option<f64>, usize) {
    blend(availability, speed(latency_ms, throughput))
}

/// The score with our own calls folded in.
///
/// Availability becomes a blend of our success rate and the platform's uptime, with the
/// platform's number acting as a prior worth [`SAMPLE_PRIOR`] calls — so one good run
/// does not erase a model's record, and our own history outweighs it once there is
/// enough of it. Speed prefers our median, which was measured on this machine against
/// this task, over a provider average nobody has updated in weeks. A model that failed
/// [`FAIL_STREAK`] times in a row is demoted even if the platform still calls it up.
pub fn rank(
    availability: Option<f64>,
    latency_ms: Option<f64>,
    throughput: Option<f64>,
    ours: Option<&Evidence>,
) -> (Option<f64>, usize) {
    let Some(ours) = ours.filter(|evidence| evidence.runs > 0) else {
        return score(availability, latency_ms, throughput);
    };

    let reported = availability.map(normalise_uptime);
    let availability = match ours.success_rate() {
        Some(rate) => {
            let prior = reported.unwrap_or(0.5);
            let count = ours.runs as f64;
            Some((rate * count + prior * SAMPLE_PRIOR) / (count + SAMPLE_PRIOR))
        }
        None => reported,
    };

    let our_speed = ours
        .median_ms
        .map(|ms| (1.0 - ms as f64 / OUR_LATENCY_CEILING).clamp(0.0, 1.0));
    let speed = our_speed.or_else(|| speed(latency_ms, throughput));

    let (value, evidence) = blend(availability, speed);
    let value = value.map(|value| {
        if ours.streak >= FAIL_STREAK {
            value * DEMOTED_SHARE
        } else {
            value
        }
    });
    (value, evidence)
}

/// The same fetch without a window or a cache, for `--self-test` and `--rank`.
pub async fn free_models_headless() -> Result<Vec<ModelRow>, String> {
    let body: ModelsResponse = http::get_json(MODELS_URL).await?;
    let mut rows: Vec<ModelRow> = body.data.into_iter().filter(is_free_text_model).collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

pub async fn free_models(app: &AppHandle) -> Result<Vec<ModelRow>, String> {
    match free_models_headless().await {
        Ok(rows) => {
            if let Ok(path) = store::models_cache_path(app) {
                if let Ok(bytes) = serde_json::to_vec(&rows) {
                    let _ = std::fs::write(path, bytes);
                }
            }
            Ok(rows)
        }
        // Offline is not an error while a cache exists: the picker keeps working.
        Err(err) => match store::models_cache_path(app) {
            Ok(path) => match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<Vec<ModelRow>>(&bytes).map_err(|_| err),
                Err(_) => Err(err),
            },
            Err(_) => Err(err),
        },
    }
}

pub async fn endpoints(id: &str) -> Result<Vec<EndpointRow>, String> {
    let url = format!("{ENDPOINTS_BASE}/{id}/endpoints");
    let body: EndpointsResponse = http::get_json(&url).await?;
    Ok(body.data.endpoints)
}

/// Every free text model with the best availability and speed any of its providers
/// reported. A provider that fails to answer contributes nothing rather than failing
/// the whole table — the model is still listed, with less evidence behind it.
pub async fn fetch_platform() -> Result<Vec<PlatformRow>, String> {
    let models = free_models_headless().await?;

    let rows: Vec<PlatformRow> = futures_util::stream::iter(models)
        .map(|model| async move {
            let endpoints = endpoints(&model.id).await.unwrap_or_default();

            // Best uptime from the most recent window that reported anything, plus the
            // fastest latency and best throughput across providers.
            let (uptime, uptime_window) = best_uptime(&endpoints);
            let latency_ms = endpoints
                .iter()
                .filter_map(|endpoint| endpoint.latency_last_30m)
                .filter(|value| *value > 0.0)
                .reduce(f64::min);
            let throughput = endpoints
                .iter()
                .filter_map(|endpoint| endpoint.throughput_last_30m)
                .reduce(f64::max);

            PlatformRow {
                id: model.id,
                name: model.name,
                context_length: model.context_length,
                providers: endpoints.len(),
                uptime,
                uptime_window: uptime_window.map(str::to_string),
                latency_ms,
                throughput,
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    Ok(rows)
}

/// Folds what we measured into what the platform reported, sorts the table, and says
/// which row Auto picks.
fn decorate(
    platform: Vec<PlatformRow>,
    samples: &[samples::Sample],
    task: &str,
    settings: &store::Settings,
) -> Catalogue {
    let known = samples::by_model(samples, task);

    let mut rows: Vec<RankedModel> = platform
        .into_iter()
        .map(|row| {
            let evidence = known.get(&row.id).cloned().unwrap_or_default();
            let (score, parts) = rank(
                row.uptime,
                row.latency_ms,
                row.throughput,
                Some(&evidence),
            );

            RankedModel {
                id: row.id,
                name: row.name,
                context_length: row.context_length,
                providers: row.providers,
                uptime: row.uptime,
                uptime_window: row.uptime_window,
                latency_ms: row.latency_ms,
                throughput: row.throughput,
                score,
                evidence: parts,
                runs: evidence.runs,
                success_rate: evidence.success_rate(),
                our_latency_ms: evidence.median_ms,
                streak: evidence.streak,
                pinned: false,
                chosen: false,
            }
        })
        .collect();

    // Best first, then the better-evidenced, then alphabetical so two runs that
    // produced the same numbers produce the same table.
    rows.sort_by(|a, b| {
        let left = a.score.unwrap_or(-1.0);
        let right = b.score.unwrap_or(-1.0);
        right
            .partial_cmp(&left)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.runs.cmp(&a.runs))
            // More providers is weak evidence, but it is evidence: three endpoints
            // behind a model is a better bet than one, and it breaks the ties that
            // would otherwise fall to the alphabet.
            .then(b.providers.cmp(&a.providers))
            .then(a.id.cmp(&b.id))
    });

    // Auto's pick is the best model we have actually measured for this task. Nothing
    // measured means no opinion, and the honest fallback is the gateway router rather
    // than whichever id sorts first: thirteen endpoints reporting the same day-long
    // uptime are indistinguishable, and the alphabet is not evidence. Pinning is a
    // person overriding all of this, so the pin is the chosen row wherever it ranks.
    let auto_choice = rows
        .iter()
        .find(|row| row.runs > 0 && !row.demoted() && row.score.is_some())
        .or_else(|| rows.iter().find(|row| row.id == ROUTER))
        .map(|row| row.id.clone());

    let pinned = settings
        .pinned_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string);

    for row in rows.iter_mut() {
        row.pinned = pinned.as_deref() == Some(row.id.as_str());
        row.chosen = row.pinned || pinned.is_none() && auto_choice.as_deref() == Some(row.id.as_str());
    }

    Catalogue {
        task: task.to_string(),
        auto: settings.auto_enabled(),
        pinned,
        auto_choice,
        rows,
    }
}

fn cache_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    store::ranking_cache_path(app)
}

fn read_cache(app: &AppHandle) -> Option<Vec<PlatformRow>> {
    let path = cache_path(app).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let cache: RankingCache = serde_json::from_slice(&bytes).ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|gap| gap.as_secs() as i64)
        .unwrap_or(0);
    if now.saturating_sub(cache.at) > CACHE_MAX_AGE {
        return None;
    }
    Some(cache.rows)
}

fn write_cache(app: &AppHandle, rows: &[PlatformRow]) {
    let written = (|| -> Result<(), String> {
        let path = cache_path(app)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|gap| gap.as_secs() as i64)
            .unwrap_or(0);
        let bytes = serde_json::to_vec(&RankingCache {
            at: now,
            rows: rows.to_vec(),
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    })();
    if let Err(err) = written {
        eprintln!("catalogue cache: {err}");
    }
}

/// The table for one task, freshly fetched and cached.
pub async fn catalogue(app: &AppHandle, task: &str) -> Result<Catalogue, String> {
    let task = normalise_task(task);
    let platform = fetch_platform().await?;
    write_cache(app, &platform);

    let samples = samples::load(app).unwrap_or_default();
    Ok(decorate(platform, &samples, &task, &store::read_settings(app)))
}

/// The last ranking this app measured, with today's samples and the current settings
/// folded in. None when there is no usable cache — Auto then leaves the choice to the
/// provider's configured model.
pub fn ranked_from_cache(app: &AppHandle, task: &str) -> Option<Catalogue> {
    let platform = read_cache(app)?;
    let samples = samples::load(app).unwrap_or_default();
    let task = normalise_task(task);
    Some(decorate(
        platform,
        &samples,
        &task,
        &store::read_settings(app),
    ))
}

/// The same table without a window, for `--rank`. No cache and no sample log, so it
/// reports the platform half of the score.
pub async fn ranked_headless(task: &str) -> Result<Catalogue, String> {
    let platform = fetch_platform().await?;
    let task = normalise_task(task);
    Ok(decorate(
        platform,
        &[],
        &task,
        &store::Settings::default(),
    ))
}

/// Only the two measured tasks have a ranking; anything else falls back to the
/// paraphraser's, which is what most runs are.
pub fn normalise_task(task: &str) -> String {
    let task = task.trim().to_ascii_lowercase();
    if samples::TASKS.contains(&task.as_str()) {
        task
    } else {
        samples::PARAPHRASE.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples::Sample;

    fn row(prompt: &str, input: &[&str], output: &[&str]) -> ModelRow {
        ModelRow {
            id: "test/model".into(),
            name: Some("Test".into()),
            context_length: Some(4096),
            architecture: Some(Architecture {
                input_modalities: input.iter().map(|m| m.to_string()).collect(),
                output_modalities: output.iter().map(|m| m.to_string()).collect(),
            }),
            pricing: Some(Pricing {
                prompt: Some(prompt.into()),
                completion: Some(prompt.into()),
            }),
        }
    }

    fn platform(id: &str, uptime: Option<f64>) -> PlatformRow {
        PlatformRow {
            id: id.to_string(),
            name: Some(id.to_string()),
            context_length: Some(8192),
            providers: 1,
            uptime,
            uptime_window: uptime.map(|_| "1d".to_string()),
            latency_ms: None,
            throughput: None,
        }
    }

    fn sample(at: i64, model: &str, ok: bool, ms: u64) -> Sample {
        Sample {
            at,
            model: model.to_string(),
            task: samples::PARAPHRASE.to_string(),
            ok,
            ms,
        }
    }

    fn evidence(runs: usize, ok: usize, streak: usize, median: u64) -> Evidence {
        Evidence {
            runs,
            ok,
            streak,
            median_ms: Some(median),
        }
    }

    #[test]
    fn a_zero_priced_text_model_is_listed() {
        assert!(is_free_text_model(&row("0", &["text"], &["text"])));
    }

    #[test]
    fn a_priced_model_is_not() {
        assert!(!is_free_text_model(&row("0.000001", &["text"], &["text"])));
    }

    #[test]
    fn a_music_or_image_model_is_not() {
        assert!(!is_free_text_model(&row("0", &["text"], &["audio"])));
        assert!(!is_free_text_model(&row("0", &["image"], &["text"])));
    }

    #[test]
    fn a_model_without_pricing_is_not_listed() {
        let mut model = row("0", &["text"], &["text"]);
        model.pricing = None;
        assert!(!is_free_text_model(&model));
    }

    #[test]
    fn a_perfect_endpoint_scores_high_with_two_pieces_of_evidence() {
        let (score, evidence) = score(Some(1.0), Some(100.0), Some(150.0));
        assert_eq!(evidence, 2);
        assert!(score.unwrap() > 85.0, "got {score:?}");
    }

    #[test]
    fn no_numbers_means_no_score_rather_than_zero() {
        assert_eq!(score(None, None, None), (None, 0));
    }

    #[test]
    fn a_quiet_endpoint_still_scores_on_speed_with_one_piece_of_evidence() {
        let (score, evidence) = score(None, Some(300.0), None);
        assert_eq!(evidence, 1);
        // 300ms on a 3s ceiling is 90% of the latency component, shrunk to 80% because
        // a single signal is not proof.
        assert!((score.unwrap() - 72.0).abs() < 0.5, "got {score:?}");
    }

    #[test]
    fn one_signal_scores_below_two_even_when_it_looks_perfect() {
        let only_uptime = score(Some(1.0), None, None).0.unwrap();
        let both = score(Some(1.0), Some(300.0), None).0.unwrap();
        assert!(both > only_uptime, "{both} should beat {only_uptime}");
        assert!((only_uptime - 80.0).abs() < 0.01, "got {only_uptime}");
    }

    #[test]
    fn the_most_recent_uptime_window_wins() {
        let mut quiet = endpoint(None, None, Some(0.4));
        assert_eq!(best_uptime(&[quiet.clone()]), (Some(0.4), Some("1d")));

        quiet.uptime_last_30m = Some(0.9);
        assert_eq!(best_uptime(&[quiet.clone()]), (Some(0.9), Some("30m")));

        quiet.uptime_last_5m = Some(0.75);
        assert_eq!(best_uptime(&[quiet]), (Some(0.75), Some("5m")));
    }

    #[test]
    fn no_window_reports_means_no_uptime_rather_than_zero() {
        assert_eq!(best_uptime(&[endpoint(None, None, None)]), (None, None));
        assert_eq!(best_uptime(&[]), (None, None));
    }

    #[test]
    fn classifiers_and_music_generators_are_not_chat_models() {
        assert!(!is_general_chat_model("nvidia/nemotron-3.5-content-safety:free"));
        assert!(!is_general_chat_model("google/lyria-3-clip-preview"));
        assert!(is_general_chat_model("google/gemma-4-31b-it:free"));
    }

    fn endpoint(
        uptime_last_5m: Option<f64>,
        uptime_last_30m: Option<f64>,
        uptime_last_1d: Option<f64>,
    ) -> EndpointRow {
        EndpointRow {
            name: Some("test".into()),
            provider_name: Some("Provider".into()),
            context_length: Some(8192),
            max_completion_tokens: None,
            pricing: None,
            uptime_last_5m,
            uptime_last_30m,
            uptime_last_1d,
            latency_last_30m: None,
            throughput_last_30m: None,
            quantization: None,
        }
    }

    #[test]
    fn a_percentage_uptime_is_normalised() {
        assert!((normalise_uptime(97.0) - 0.97).abs() < f64::EPSILON);
        assert!((normalise_uptime(0.97) - 0.97).abs() < f64::EPSILON);
    }

    #[test]
    fn a_slow_model_scores_below_a_fast_one_on_the_same_evidence() {
        let fast = score(Some(0.99), Some(200.0), Some(120.0)).0.unwrap();
        let slow = score(Some(0.99), Some(2500.0), Some(10.0)).0.unwrap();
        assert!(fast > slow, "fast {fast} should beat slow {slow}");
    }

    #[test]
    fn our_own_calls_outweigh_a_perfect_uptime_report() {
        let unmeasured = rank(Some(1.0), None, None, None).0.unwrap();
        let measured = rank(Some(1.0), None, None, Some(&evidence(5, 5, 0, 600)))
            .0
            .unwrap();
        assert!(
            measured > unmeasured,
            "measured {measured} should beat unmeasured {unmeasured}"
        );
    }

    #[test]
    fn three_failures_in_a_row_demote_a_model_that_reports_perfect_uptime() {
        let healthy = rank(Some(1.0), None, None, Some(&evidence(3, 3, 0, 900)))
            .0
            .unwrap();
        let failing = rank(Some(1.0), None, None, Some(&evidence(3, 0, 3, 900)))
            .0
            .unwrap();
        assert!(failing < healthy / 2.0, "failing {failing} vs healthy {healthy}");
        assert!(failing < 40.0, "a failing model should not stay near the top: {failing}");
    }

    #[test]
    fn a_recovered_model_climbs_back() {
        let failing = rank(Some(1.0), None, None, Some(&evidence(3, 0, 3, 900)))
            .0
            .unwrap();
        let recovered = rank(Some(1.0), None, None, Some(&evidence(4, 1, 0, 700)))
            .0
            .unwrap();
        assert!(
            recovered > failing * 2.0,
            "recovered {recovered} should clear failing {failing}"
        );
    }

    #[test]
    fn auto_picks_the_measured_model_rather_than_the_first_in_the_list() {
        let platform = vec![platform("aaa/first", Some(1.0)), platform("zzz/measured", Some(1.0))];
        let samples = vec![
            sample(4, "zzz/measured", true, 400),
            sample(3, "zzz/measured", true, 500),
            sample(2, "zzz/measured", true, 600),
        ];
        let catalogue = decorate(platform, &samples, samples::PARAPHRASE, &store::Settings::default());

        assert_eq!(catalogue.rows[0].id, "zzz/measured");
        assert_eq!(catalogue.auto_choice.as_deref(), Some("zzz/measured"));
        assert!(catalogue.rows[0].chosen);
        assert!(!catalogue.rows[1].chosen);
    }

    #[test]
    fn auto_skips_a_model_that_keeps_failing() {
        let platform = vec![platform("aaa/failing", Some(1.0)), platform("zzz/ok", Some(1.0))];
        let samples = vec![
            sample(4, "zzz/ok", true, 1200),
            sample(3, "aaa/failing", false, 10),
            sample(2, "aaa/failing", false, 10),
            sample(1, "aaa/failing", false, 10),
        ];
        let catalogue = decorate(platform, &samples, samples::PARAPHRASE, &store::Settings::default());
        assert_eq!(catalogue.auto_choice.as_deref(), Some("zzz/ok"));
    }

    #[test]
    fn with_nothing_measured_auto_falls_to_the_router_rather_than_the_alphabet() {
        let platform = vec![
            platform("aaa/first", Some(1.0)),
            platform(ROUTER, None),
            platform("zzz/last", Some(1.0)),
        ];
        let catalogue = decorate(platform, &[], samples::PARAPHRASE, &store::Settings::default());
        assert_eq!(catalogue.auto_choice.as_deref(), Some(ROUTER));
    }

    #[test]
    fn with_nothing_measured_and_no_router_auto_has_no_opinion() {
        let catalogue = decorate(
            vec![platform("aaa/first", Some(1.0))],
            &[],
            samples::PARAPHRASE,
            &store::Settings::default(),
        );
        assert_eq!(catalogue.auto_choice, None);
    }

    #[test]
    fn a_pin_beats_auto_and_is_marked_as_the_choice() {
        let platform = vec![platform("aaa/first", Some(1.0)), platform("zzz/measured", Some(1.0))];
        let samples = vec![sample(1, "zzz/measured", true, 400)];
        let settings = store::Settings {
            pinned_model: Some("aaa/first".into()),
            ..store::Settings::default()
        };
        let catalogue = decorate(platform, &samples, samples::PARAPHRASE, &settings);

        assert_eq!(catalogue.pinned.as_deref(), Some("aaa/first"));
        let pinned = catalogue.rows.iter().find(|row| row.id == "aaa/first").unwrap();
        assert!(pinned.pinned && pinned.chosen);
        // Auto's own pick is still reported, so the view can say what unpinning means.
        assert_eq!(catalogue.auto_choice.as_deref(), Some("zzz/measured"));
    }

    #[test]
    fn more_providers_break_a_tie_before_the_name() {
        let mut first = platform("aaa/first", Some(1.0));
        let mut second = platform("zzz/second", Some(1.0));
        first.providers = 1;
        second.providers = 3;
        let catalogue = decorate(
            vec![first, second],
            &[],
            samples::PARAPHRASE,
            &store::Settings::default(),
        );
        assert_eq!(catalogue.rows[0].id, "zzz/second");
    }

    #[test]
    fn samples_for_the_other_task_do_not_count() {
        let mut grammar_sample = sample(1, "zzz/measured", true, 400);
        grammar_sample.task = samples::GRAMMAR.to_string();
        let catalogue = decorate(
            vec![platform("zzz/measured", Some(1.0))],
            &[grammar_sample],
            samples::PARAPHRASE,
            &store::Settings::default(),
        );
        assert_eq!(catalogue.rows[0].runs, 0);
        assert_eq!(catalogue.rows[0].our_latency_ms, None);
    }

    #[test]
    fn an_unknown_task_falls_back_to_the_paraphraser() {
        assert_eq!(normalise_task(""), "paraphrase");
        assert_eq!(normalise_task("Grammar"), "grammar");
    }

    #[test]
    fn auto_is_on_unless_it_was_turned_off() {
        let catalogue = decorate(
            vec![platform("aaa/only", Some(1.0))],
            &[],
            samples::PARAPHRASE,
            &store::Settings::default(),
        );
        assert!(catalogue.auto);

        let off = store::Settings {
            auto_model: Some(false),
            ..store::Settings::default()
        };
        let catalogue = decorate(vec![platform("aaa/only", Some(1.0))], &[], samples::PARAPHRASE, &off);
        assert!(!catalogue.auto);
    }
}
