//! The catalogue: every zero-priced model, and the per-provider numbers behind the
//! ranking.
//!
//! Two GETs, neither needing a key. `/models` says what exists and what it costs;
//! `/models/{id}/endpoints` says which provider serves it, with `uptime_last_30m`,
//! `latency_last_30m` and `throughput_last_30m`. Those three come back `null` on a
//! free endpoint nobody has called recently, which is why the model here keeps them
//! as `Option` all the way through: a missing number is *no evidence*, never zero.
//!
//! M7 scores these rows per task. This module deliberately stops at fetching,
//! filtering and caching.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::openrouter::http;
use crate::store;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const ENDPOINTS_BASE: &str = "https://openrouter.ai/api/v1/models";

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
    pub uptime_last_30m: Option<f64>,
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
    let Some(arch) = row.architecture.as_ref() else {
        return true;
    };
    let text_out = arch.output_modalities.is_empty()
        || arch.output_modalities.iter().any(|m| m == "text");
    let text_in = arch.input_modalities.is_empty()
        || arch.input_modalities.iter().any(|m| m == "text");
    text_in && text_out
}

/// The same fetch without a window or a cache, for `--self-test`.
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
        // Offline is not an error while a cache exists: the picker keeps working
        // and says how old its list is.
        Err(err) => match store::models_cache_path(app) {
            Ok(path) => match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<Vec<ModelRow>>(&bytes)
                    .map_err(|_| err),
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
}
