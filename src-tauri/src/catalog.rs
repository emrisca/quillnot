//! The catalogue: every zero-priced model, and the per-provider numbers behind the
//! ranking.
//!
//! Two GETs, neither needing a key. `/models` says what exists and what it costs;
//! `/models/{id}/endpoints` says which provider serves it, with `uptime_last_5m`,
//! `uptime_last_30m`, `uptime_last_1d`, `latency_last_30m` and
//! `throughput_last_30m`. Those come back `null` on a free endpoint nobody has
//! called recently, which is why they stay `Option` all the way through and why the
//! score says how much evidence stands behind it: a missing number is *no
//! evidence*, never zero.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::openrouter::http;
use crate::store;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const ENDPOINTS_BASE: &str = "https://openrouter.ai/api/v1/models";

/// How many endpoint lookups are in flight at once. Seventeen models is a fine
/// number of requests; seventeen at once is rude to a free API.
const CONCURRENCY: usize = 6;

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

/// A catalogue row as the table shows it: what the model is, how many providers
/// serve it, the best numbers any provider reported, and a score with the evidence
/// count that score rests on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedModel {
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
    pub score: Option<f64>,
    /// 0, 1 or 2: how many of availability and speed contributed to the score.
    pub evidence: usize,
}

/// Ids that are text-to-text by modality but are not chat models. A music generator
/// and a content-safety classifier both passed the modality check, and neither can
/// paraphrase a sentence. This is a blocklist because the catalogue carries no "this
/// is a chat model" flag; M7 replaces it with per-task measurement, which a classifier
/// cannot fake.
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

/// The score out of 100, and how many components stood behind it.
///
/// Unknown components are left out of the weighted mean instead of dragging it
/// down: a brand-new endpoint has no 30-minute window, and ranking it last for that
/// would be a lie about the model. `evidence` carries that uncertainty to the table.
pub fn score(
    availability: Option<f64>,
    latency_ms: Option<f64>,
    throughput: Option<f64>,
) -> (Option<f64>, usize) {
    let speed = {
        let by_latency = latency_ms.map(|ms| (1.0 - ms / 3000.0).clamp(0.0, 1.0));
        let by_throughput = throughput.map(|tps| (tps / 150.0).clamp(0.0, 1.0));
        match (by_latency, by_throughput) {
            (Some(a), Some(b)) => Some(0.6 * a + 0.4 * b),
            (Some(only), None) | (None, Some(only)) => Some(only),
            (None, None) => None,
        }
    };

    let parts: Vec<(f64, f64)> = [
        (availability.map(normalise_uptime), 0.6),
        (speed, 0.4),
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

/// Every free text model, with the best availability and speed any of its providers
/// reported, scored and sorted. A provider that fails to answer contributes nothing
/// rather than failing the whole table — the model is still listed, with less
/// evidence behind it.
pub async fn ranked_models() -> Result<Vec<RankedModel>, String> {
    let models = free_models_headless().await?;

    let mut rows: Vec<RankedModel> = futures_util::stream::iter(models)
        .map(|model| async move {
            let endpoints = endpoints(&model.id).await.unwrap_or_default();

            // Best uptime from the most recent window that reported anything, plus the
            // fastest latency and best throughput across providers.
            let (uptime, uptime_window) = best_uptime(&endpoints);
            let latency_ms = endpoints
                .iter()
                .filter_map(|e| e.latency_last_30m)
                .filter(|value| *value > 0.0)
                .reduce(f64::min);
            let throughput = endpoints
                .iter()
                .filter_map(|e| e.throughput_last_30m)
                .reduce(f64::max);
            let (score, evidence) = score(uptime, latency_ms, throughput);

            RankedModel {
                id: model.id,
                name: model.name,
                context_length: model.context_length,
                providers: endpoints.len(),
                uptime,
                uptime_window: uptime_window.map(str::to_string),
                latency_ms,
                throughput,
                score,
                evidence,
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    // Best first, then the best-evidenced, then alphabetical so the order is stable
    // between two runs that produced the same numbers.
    rows.sort_by(|a, b| {
        let left = a.score.unwrap_or(-1.0);
        let right = b.score.unwrap_or(-1.0);
        right
            .partial_cmp(&left)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.evidence.cmp(&a.evidence))
            .then(a.id.cmp(&b.id))
    });

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
