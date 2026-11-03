//! The provider registry.
//!
//! One idea taken from FreeLLMAPI (github.com/tashfeenahmed/freellmapi): every free
//! tier is an OpenAI-compatible endpoint, so a single client works against all of
//! them if the base URL, the auth header and a default model are the only things that
//! differ. This module holds that table, plus the routing decision — which provider
//! answers a rewrite, and who takes over when it does not.
//!
//! It does *not* reimplement the aggregator. There is no local router, no unified
//! bearer token, no encrypted key database: keys live in the OS keychain and requests
//! go straight from the app to the provider the person chose.

use serde::Serialize;
use tauri::AppHandle;

use crate::store;

pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    /// What the free tier actually is, in one line.
    pub free_tier: &'static str,
    pub docs: &'static str,
    /// Every key from this provider starts with this, which is the cheapest possible
    /// check for a pasted-the-wrong-thing mistake. None means "do not guess".
    pub key_prefix: Option<&'static str>,
    pub supports_oauth: bool,
    /// Model used when nobody has overridden it. Editable per provider in Settings,
    /// because these ids move faster than any app release.
    pub default_model: &'static str,
    /// The provider can be called without a key: a local server.
    pub needs_key: bool,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        free_tier: "Zero-priced models, 17 of them when this was written, plus a gateway router.",
        docs: "https://openrouter.ai/docs",
        key_prefix: Some("sk-or-"),
        supports_oauth: true,
        default_model: "openrouter/free",
        needs_key: true,
    },
    Provider {
        id: "google",
        label: "Google AI Studio",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        free_tier: "Free tier on the Gemini API, rate-limited per model.",
        docs: "https://ai.google.dev/gemini-api/docs/openai",
        key_prefix: Some("AIza"),
        supports_oauth: false,
        default_model: "gemini-2.5-flash",
        needs_key: true,
    },
    Provider {
        id: "groq",
        label: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        free_tier: "Free tier with generous requests per minute on open models.",
        docs: "https://console.groq.com/docs/openai",
        key_prefix: Some("gsk_"),
        supports_oauth: false,
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
    },
    Provider {
        id: "cerebras",
        label: "Cerebras",
        base_url: "https://api.cerebras.ai/v1",
        free_tier: "Free tier on wafer-scale inference, very fast.",
        docs: "https://inference-docs.cerebras.ai",
        key_prefix: Some("csk-"),
        supports_oauth: false,
        default_model: "llama3.3-70b",
        needs_key: true,
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        free_tier: "Free experimentation tier on La Plateforme.",
        docs: "https://docs.mistral.ai",
        key_prefix: None,
        supports_oauth: false,
        default_model: "mistral-small-latest",
        needs_key: true,
    },
    Provider {
        id: "custom",
        label: "Your own endpoint",
        base_url: "",
        free_tier: "Anything OpenAI-compatible: Ollama, LM Studio, llama.cpp, vLLM.",
        docs: "https://platform.openai.com/docs/api-reference/chat",
        key_prefix: None,
        supports_oauth: false,
        default_model: "",
        // A local server usually wants no Authorization header at all.
        needs_key: false,
    },
];

pub fn get(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|provider| provider.id == id)
}

pub const DEFAULT_PROVIDER: &str = "openrouter";

/// One attempt at answering a block.
#[derive(Debug, Clone)]
pub struct Attempt {
    pub base_url: String,
    pub key: Option<String>,
    pub model: String,
    /// OpenRouter wants attribution headers and has its own router model; the others
    /// take the plain OpenAI body.
    pub openrouter: bool,
}

/// Where a rewrite goes: the chosen provider first, then whatever else can answer.
#[derive(Debug, Clone)]
pub struct Route {
    pub model: String,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub free_tier: String,
    pub docs: String,
    pub key_prefix: Option<String>,
    pub supports_oauth: bool,
    pub default_model: String,
    pub needs_key: bool,
    pub connected: bool,
    pub hint: Option<String>,
    pub active: bool,
    /// The model this provider will use, override or default.
    pub model: String,
}

fn configured_model(settings: &store::Settings, provider: &Provider) -> String {
    settings
        .provider_models
        .get(provider.id)
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| provider.default_model.to_string())
}

fn base_url_for(settings: &store::Settings, provider: &Provider) -> String {
    if provider.id == "custom" {
        return settings.custom_base_url.clone().unwrap_or_default();
    }
    provider.base_url.to_string()
}

pub fn statuses(app: &AppHandle) -> Vec<ProviderStatus> {
    let settings = store::read_settings(app);
    let active = settings
        .active_provider
        .clone()
        .unwrap_or_else(|| DEFAULT_PROVIDER.to_string());

    PROVIDERS
        .iter()
        .map(|provider| {
            let key = store::load_key_for(provider.id);
            ProviderStatus {
                id: provider.id.to_string(),
                label: provider.label.to_string(),
                base_url: base_url_for(&settings, provider),
                free_tier: provider.free_tier.to_string(),
                docs: provider.docs.to_string(),
                key_prefix: provider.key_prefix.map(str::to_string),
                supports_oauth: provider.supports_oauth,
                default_model: provider.default_model.to_string(),
                needs_key: provider.needs_key,
                connected: !provider.needs_key || key.is_some(),
                hint: key.as_deref().map(store::hint),
                active: provider.id == active,
                model: configured_model(&settings, provider),
            }
        })
        .collect()
}

/// Builds the attempt chain for a provider: itself first, then OpenRouter's router if
/// a key for it exists and it is not already the one being tried. That is the useful
/// half of the aggregator idea — a free tier that hits its daily cap should not be a
/// dead end, it should hand the request to the next free tier the person has a key
/// for.
fn build_route(
    provider: &Provider,
    settings: &store::Settings,
    model_override: Option<String>,
) -> Result<Route, String> {
    let model = model_override
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| configured_model(settings, provider));

    let key = store::load_key_for(provider.id);
    if provider.needs_key && key.is_none() {
        return Err(format!(
            "Connect {} in Settings first — add its API key there, then try again.",
            provider.label
        ));
    }

    let base_url = base_url_for(settings, provider);
    if base_url.trim().is_empty() {
        return Err("Add a base URL for your own endpoint in Settings first.".to_string());
    }
    if model.is_empty() {
        return Err(format!("Pick a model for {} in Settings first.", provider.label));
    }

    let mut attempts = vec![Attempt {
        base_url,
        key: key.clone(),
        model: model.clone(),
        openrouter: provider.id == "openrouter",
    }];

    if provider.id != "openrouter" {
        if let Some(openrouter_key) = store::load_key_for("openrouter") {
            attempts.push(Attempt {
                base_url: get("openrouter").expect("registry entry").base_url.to_string(),
                key: Some(openrouter_key),
                model: "openrouter/free".to_string(),
                openrouter: true,
            });
        }
    } else if model != "openrouter/free" {
        // The router itself is the cheapest second opinion on OpenRouter.
        attempts.push(Attempt {
            base_url: get("openrouter").expect("registry entry").base_url.to_string(),
            key,
            model: "openrouter/free".to_string(),
            openrouter: true,
        });
    }

    Ok(Route { model, attempts })
}

pub fn resolve(app: &AppHandle, model_override: Option<String>) -> Result<Route, String> {
    let settings = store::read_settings(app);
    let id = settings
        .active_provider
        .clone()
        .unwrap_or_else(|| DEFAULT_PROVIDER.to_string());
    let provider = get(&id).ok_or_else(|| format!("Unknown provider “{id}”."))?;
    // A model named per request wins, then the older default-model setting, then the
    // provider's own choice.
    let model = model_override.or_else(|| settings.default_model.clone());
    build_route(provider, &settings, model)
}

/// The same resolution without a window, for `--self-test`: OpenRouter, whatever key
/// is stored, the gateway router unless a model is named.
pub fn resolve_headless(model_override: Option<String>) -> Result<Route, String> {
    let provider = get(DEFAULT_PROVIDER).expect("registry entry");
    let model = model_override
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| provider.default_model.to_string());
    let key = store::load_key_for(provider.id)
        .ok_or_else(|| "Connect an OpenRouter account in Settings first.".to_string())?;

    Ok(Route {
        model: model.clone(),
        attempts: vec![Attempt {
            base_url: provider.base_url.to_string(),
            key: Some(key),
            model,
            openrouter: true,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn settings(models: &[(&str, &str)], active: Option<&str>) -> store::Settings {
        let mut provider_models = BTreeMap::new();
        for (id, model) in models {
            provider_models.insert(id.to_string(), model.to_string());
        }
        store::Settings {
            default_model: None,
            active_provider: active.map(str::to_string),
            provider_models,
            custom_base_url: None,
            default_mode: None,
        }
    }

    #[test]
    fn every_registered_provider_resolves_by_id() {
        for provider in PROVIDERS {
            assert!(get(provider.id).is_some(), "{} missing", provider.id);
        }
        assert!(get("nope").is_none());
    }

    #[test]
    fn an_override_beats_the_default_model() {
        let settings = settings(&[("groq", "llama-3.1-8b-instant")], Some("groq"));
        let route = build_route(get("groq").unwrap(), &settings, None).unwrap_or_else(|err| {
            // No key stored on this machine: the model choice is what is under test.
            assert!(err.contains("Connect Groq"), "unexpected error: {err}");
            Route {
                model: String::new(),
                attempts: vec![],
            }
        });
        if !route.model.is_empty() {
            assert_eq!(route.model, "llama-3.1-8b-instant");
        }
        assert_eq!(configured_model(&settings, get("groq").unwrap()), "llama-3.1-8b-instant");
    }

    #[test]
    fn a_provider_without_an_override_uses_its_default() {
        let settings = settings(&[], Some("groq"));
        assert_eq!(
            configured_model(&settings, get("groq").unwrap()),
            "llama-3.3-70b-versatile"
        );
    }

    #[test]
    fn the_custom_endpoint_takes_its_url_from_settings() {
        let mut settings = settings(&[], Some("custom"));
        assert_eq!(base_url_for(&settings, get("custom").unwrap()), "");
        settings.custom_base_url = Some("http://127.0.0.1:11434/v1".into());
        assert_eq!(
            base_url_for(&settings, get("custom").unwrap()),
            "http://127.0.0.1:11434/v1"
        );
    }

    #[test]
    fn a_provider_that_needs_a_key_without_one_names_itself() {
        let settings = settings(&[], Some("mistral"));
        // Only meaningful when no Mistral key is stored; keeps the message honest.
        if let Err(err) = build_route(get("mistral").unwrap(), &settings, None) {
            assert!(err.contains("Mistral"), "unexpected error: {err}");
        }
    }

    #[test]
    fn the_custom_endpoint_needs_no_key() {
        assert!(!get("custom").unwrap().needs_key);
        assert!(get("openrouter").unwrap().needs_key);
    }
}
