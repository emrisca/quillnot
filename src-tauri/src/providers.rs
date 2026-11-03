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

use crate::{catalog, store};

pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    /// What the free tier actually is, in one line.
    pub free_tier: &'static str,
    pub docs: &'static str,
    /// The prefixes this provider issues keys under, newest first: the cheapest
    /// possible check for a pasted-the-wrong-thing mistake. Google has two live
    /// shapes at once (`AQ.` Auth keys and the older `AIza`), so this is a list.
    /// Empty means "do not guess", which is right for a local server and for Mistral.
    pub key_prefixes: &'static [&'static str],
    /// Gemini's native endpoint, whose body and stream are its own shape. It is used
    /// for Google because the `AQ.` keys AI Studio now issues are rejected on the
    /// OpenAI-compatible route.
    pub gemini: bool,
    pub supports_oauth: bool,
    /// Model used when nobody has overridden it. Editable per provider in Settings,
    /// because these ids move faster than any app release.
    pub default_model: &'static str,
    /// The provider can be called without a key: a local server.
    pub needs_key: bool,
    /// Where this provider lists its models, relative to its base URL. Empty for
    /// OpenRouter, whose catalogue has a module of its own.
    pub models_path: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        free_tier: "Zero-priced models, 17 of them when this was written, plus a gateway router.",
        docs: "https://openrouter.ai/docs",
        key_prefixes: &["sk-or-"],
        gemini: false,
        supports_oauth: true,
        default_model: "openrouter/free",
        needs_key: true,
        models_path: "",
    },
    Provider {
        id: "google",
        label: "Google AI Studio",
        // The native base, not `/v1beta/openai`: an `AQ.` key authenticates on this
        // route and comes back as an invalid key on the OpenAI-compatible one.
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        free_tier: "Free tier on the Gemini API, rate-limited per model.",
        docs: "https://ai.google.dev/gemini-api/docs",
        // Newest first: AI Studio hands out `AQ.` Auth keys now, and a Standard
        // `AIza` key keeps working until Google retires the format.
        key_prefixes: &["AQ.", "AIza"],
        gemini: true,
        supports_oauth: false,
        default_model: "gemini-2.5-flash",
        needs_key: true,
        models_path: "/models",
    },
    Provider {
        id: "groq",
        label: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        free_tier: "Free tier with generous requests per minute on open models.",
        docs: "https://console.groq.com/docs/openai",
        key_prefixes: &["gsk_"],
        gemini: false,
        supports_oauth: false,
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
        models_path: "/models",
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        free_tier: "Free experimentation tier on La Plateforme.",
        docs: "https://docs.mistral.ai",
        key_prefixes: &[],
        gemini: false,
        supports_oauth: false,
        default_model: "mistral-small-latest",
        needs_key: true,
        models_path: "/models",
    },
    Provider {
        id: "custom",
        label: "Your own endpoint",
        base_url: "",
        free_tier: "Anything OpenAI-compatible: Ollama, LM Studio, llama.cpp, vLLM.",
        docs: "https://platform.openai.com/docs/api-reference/chat",
        key_prefixes: &[],
        gemini: false,
        supports_oauth: false,
        default_model: "",
        // A local server usually wants no Authorization header at all.
        needs_key: false,
        models_path: "/models",
    },
];

pub fn get(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|provider| provider.id == id)
}

/// Whether a pasted key has the shape this provider issues. An empty list means the
/// provider has no prefix worth guessing at, and everything is let through.
pub fn key_accepted(provider: &Provider, key: &str) -> bool {
    provider.key_prefixes.is_empty()
        || provider
            .key_prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

/// `“AQ.” (or the older “AIza”)` — the one message a wrong paste gets.
pub fn key_prefix_list(provider: &Provider) -> String {
    match provider.key_prefixes {
        [] => "no particular prefix".to_string(),
        [one] => format!("“{one}”"),
        [first, rest @ ..] => format!(
            "“{first}” (or the older {})",
            rest.iter()
                .map(|prefix| format!("“{prefix}”"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
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
    /// Speak Gemini's native `contents` shape rather than OpenAI's `messages`.
    pub gemini: bool,
}

/// Where a rewrite goes: the chosen provider first, then whatever else can answer.
#[derive(Debug, Clone)]
pub struct Route {
    pub model: String,
    pub attempts: Vec<Attempt>,
}

/// One model at one provider. `id` is the string the table and the sample log use: the
/// model id on its own for OpenRouter (as it always was), `provider::model` for every
/// other provider, so two providers serving the same id stay two rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub provider: String,
    pub model: String,
}

impl Selected {
    pub fn id(&self) -> String {
        catalog_id(&self.provider, &self.model)
    }
}

pub fn catalog_id(provider: &str, model: &str) -> String {
    if provider == DEFAULT_PROVIDER {
        model.to_string()
    } else {
        format!("{provider}::{model}")
    }
}

/// The reverse of [`catalog_id`]: an id with no `::` belongs to OpenRouter, which is
/// what every measurement stored before the providers were merged looks like.
pub fn parse_catalog_id(id: &str) -> Selected {
    match id.split_once("::") {
        Some((provider, model)) => Selected {
            provider: provider.to_string(),
            model: model.to_string(),
        },
        None => Selected {
            provider: DEFAULT_PROVIDER.to_string(),
            model: id.to_string(),
        },
    }
}

/// What the ranking contributes to a route: the model it measures as best, and the
/// order to fall back through when that one refuses — across every provider.
#[derive(Debug, Default, Clone)]
pub struct Picks {
    pub auto: Option<Selected>,
    /// Best first, demoted models already removed.
    pub ranked: Vec<Selected>,
}

/// A block tries at most this many models before giving up. The first, two measured
/// alternates, and the gateway router: past that the run has cost more patience than
/// a rewrite is worth.
const MAX_ATTEMPTS: usize = 4;

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
                key_prefix: provider.key_prefixes.first().map(|prefix| (*prefix).to_string()),
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

/// One attempt, or none when this provider cannot answer now: no key where one is
/// required, no base URL, no model.
fn attempt_for(
    settings: &store::Settings,
    provider: &Provider,
    model: String,
) -> Option<Attempt> {
    let key = store::load_key_for(provider.id);
    if provider.needs_key && key.is_none() {
        return None;
    }
    let base_url = base_url_for(settings, provider);
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return None;
    }
    Some(Attempt {
        base_url,
        key,
        model,
        openrouter: provider.id == DEFAULT_PROVIDER,
        gemini: provider.gemini,
    })
}

/// The order to try, before any of it is turned into a request: a model named for this
/// run, then the pin, then the ranking's best, then every ranked fallback in order,
/// then the active provider's configured model and finally OpenRouter's router.
///
/// Nothing here is limited to one provider. A free tier that hits its cap should hand
/// the block to the next best model the person has a key for, wherever it lives.
fn wanted_order(
    provider: &Provider,
    settings: &store::Settings,
    model_override: Option<String>,
    picks: &Picks,
) -> Vec<Selected> {
    let mut wanted = Vec::new();

    if let Some(model) = model_override
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
    {
        wanted.push(Selected {
            provider: provider.id.to_string(),
            model,
        });
    }

    // A pin is a person overriding every measurement, so it comes before the ranking.
    if let Some(pinned) = settings
        .pinned_model
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        wanted.push(parse_catalog_id(pinned));
    }

    if let Some(auto) = picks.auto.clone() {
        wanted.push(auto);
    }
    wanted.extend(picks.ranked.iter().cloned());

    // The older "default model" setting is an OpenRouter id typed by hand. The ranking
    // outranks it because the ranking is a measurement and this is a preference, but it
    // still gets a turn above the provider's configured model, which is where it sat
    // before the providers were merged.
    if let Some(model) = settings
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        wanted.push(Selected {
            provider: DEFAULT_PROVIDER.to_string(),
            model: model.to_string(),
        });
    }

    // The active provider still gets a turn when nothing has been measured yet, and the
    // gateway router is the last thing that can answer.
    wanted.push(Selected {
        provider: provider.id.to_string(),
        model: configured_model(settings, provider),
    });
    if provider.id != DEFAULT_PROVIDER {
        wanted.push(Selected {
            provider: DEFAULT_PROVIDER.to_string(),
            model: "openrouter/free".to_string(),
        });
    }

    // Auto's choice is also the first ranked row, and the configured model can be a
    // fallback already. Trying the same model twice would spend a retry on a model that
    // has just refused, so the first mention of each is the only one kept.
    let mut seen: Vec<String> = Vec::new();
    wanted.retain(|pick| {
        let id = pick.id();
        if seen.contains(&id) {
            false
        } else {
            seen.push(id);
            true
        }
    });

    wanted
}

/// Builds the attempt chain: the wanted order, minus anything that cannot answer (no
/// key, no URL), capped so one block does not cost more patience than a rewrite is
/// worth.
fn build_route(
    provider: &Provider,
    settings: &store::Settings,
    model_override: Option<String>,
    picks: &Picks,
) -> Result<Route, String> {
    let mut attempts: Vec<Attempt> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut first = String::new();

    for pick in wanted_order(provider, settings, model_override, picks) {
        if attempts.len() >= MAX_ATTEMPTS {
            break;
        }
        let Some(info) = get(&pick.provider) else {
            continue;
        };
        let id = pick.id();
        if seen.contains(&id) {
            continue;
        }
        if let Some(attempt) = attempt_for(settings, info, pick.model) {
            if first.is_empty() {
                first = id.clone();
            }
            seen.push(id);
            attempts.push(attempt);
        }
    }

    if attempts.is_empty() {
        return Err(format!(
            "Connect {} in Settings first — add its API key there, then try again.",
            provider.label
        ));
    }

    Ok(Route {
        model: first,
        attempts,
    })
}

/// Reads the last ranking and turns it into routing: which model came out best across
/// every connected provider, and the order behind it. No cache means no opinion, and
/// the provider's own configured model answers.
fn picks_for(app: &AppHandle, settings: &store::Settings, task: &str) -> Picks {
    let Some(catalogue) = catalog::ranked_from_cache(app, task) else {
        return Picks::default();
    };

    // A pin means "not Auto": the ranking still supplies the fallbacks, never the
    // first attempt.
    let auto = if settings.auto_enabled() && settings.pinned_model.is_none() {
        catalogue.auto_choice.map(|id| parse_catalog_id(&id))
    } else {
        None
    };

    let ranked = catalogue
        .rows
        .into_iter()
        .filter(|row| row.available && !row.demoted() && row.score.is_some())
        .map(|row| parse_catalog_id(&row.id))
        .collect();

    Picks { auto, ranked }
}

/// `task` is `paraphrase` or `grammar`: the two are measured separately, and a model
/// that writes prose well can still refuse to emit the JSON a check needs.
pub fn resolve(
    app: &AppHandle,
    model_override: Option<String>,
    task: &str,
) -> Result<Route, String> {
    let settings = store::read_settings(app);
    let id = settings
        .active_provider
        .clone()
        .unwrap_or_else(|| DEFAULT_PROVIDER.to_string());
    let provider = get(&id).ok_or_else(|| format!("Unknown provider “{id}”."))?;
    let picks = picks_for(app, &settings, task);
    build_route(provider, &settings, model_override, &picks)
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
            gemini: false,
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
            auto_model: None,
            pinned_model: None,
            humanizer_skill: None,
            humanizer_preset: None,
            typer_stop_shortcut: None,
            ..store::Settings::default()
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
        let route = build_route(get("groq").unwrap(), &settings, None, &Picks::default())
            .unwrap_or_else(|err| {
            // No key stored on this machine: the model choice is what is under test.
            assert!(err.contains("Connect Groq"), "unexpected error: {err}");
            Route {
                model: String::new(),
                attempts: vec![],
            }
        });
        if !route.model.is_empty() {
            // The catalogue id names the provider, so two providers serving the same
            // model id stay two rows.
            assert_eq!(route.model, "groq::llama-3.1-8b-instant");
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
        if let Err(err) = build_route(get("mistral").unwrap(), &settings, None, &Picks::default()) {
            assert!(err.contains("Mistral"), "unexpected error: {err}");
        }
    }

    fn picks(auto: Option<(&str, &str)>, ranked: &[(&str, &str)]) -> Picks {
        let selected = |(provider, model): (&str, &str)| Selected {
            provider: provider.to_string(),
            model: model.to_string(),
        };
        Picks {
            auto: auto.map(selected),
            ranked: ranked.iter().copied().map(selected).collect(),
        }
    }

    fn order(settings: &store::Settings, picks: &Picks, active: &str) -> Vec<String> {
        wanted_order(get(active).unwrap(), settings, None, picks)
            .iter()
            .map(Selected::id)
            .collect()
    }

    #[test]
    fn the_best_model_answers_first_and_the_second_best_follows() {
        let settings = settings(&[], Some("openrouter"));
        let picks = picks(
            Some(("groq", "best")),
            &[("groq", "best"), ("google", "second"), ("openrouter", "third")],
        );
        let ids = order(&settings, &picks, "openrouter");
        assert_eq!(ids[0], "groq::best");
        assert_eq!(ids[1], "google::second");
        // An OpenRouter id carries no provider prefix: that is how every measurement
        // stored before the providers were merged is keyed.
        assert_eq!(ids[2], "third");
    }

    #[test]
    fn a_pin_beats_the_ranking_but_the_ranking_stays_behind_it() {
        let mut settings = settings(&[], Some("openrouter"));
        settings.pinned_model = Some("groq::pinned".into());
        let picks = picks(Some(("google", "best")), &[("google", "best")]);
        let ids = order(&settings, &picks, "openrouter");
        assert_eq!(ids[0], "groq::pinned");
        assert_eq!(ids[1], "google::best");
    }

    #[test]
    fn a_run_override_still_beats_a_pin() {
        let mut settings = settings(&[], Some("openrouter"));
        settings.pinned_model = Some("groq::pinned".into());
        let ids: Vec<String> = wanted_order(
            get("openrouter").unwrap(),
            &settings,
            Some("typed".into()),
            &Picks::default(),
        )
        .iter()
        .map(Selected::id)
        .collect();
        assert_eq!(ids[0], "typed");
    }

    #[test]
    fn the_older_default_model_setting_is_still_a_fallback() {
        let mut settings = settings(&[], Some("groq"));
        settings.default_model = Some("openai/gpt-oss-20b:free".into());
        let picks = picks(Some(("groq", "best")), &[("groq", "best")]);
        let ids = order(&settings, &picks, "groq");

        let default_at = ids
            .iter()
            .position(|id| id == "openai/gpt-oss-20b:free")
            .unwrap_or_else(|| panic!("the setting has to survive: {ids:?}"));
        let configured_at = ids
            .iter()
            .position(|id| id == "groq::llama-3.3-70b-versatile")
            .expect("the configured model is the last local resort");
        assert!(
            default_at < configured_at,
            "the ranking comes first, then the old setting: {ids:?}"
        );
    }

    #[test]
    fn the_active_provider_and_the_router_are_the_last_resorts() {
        let settings = settings(&[], Some("mistral"));
        let ids = order(&settings, &Picks::default(), "mistral");
        assert_eq!(
            ids[0],
            "mistral::mistral-small-latest",
            "nothing measured means the configured model answers: {ids:?}"
        );
        assert_eq!(ids.last().unwrap(), "openrouter/free");
    }

    #[test]
    fn a_model_id_without_a_provider_belongs_to_openrouter() {
        // Every measurement stored before the providers were merged looks like this.
        let parsed = parse_catalog_id("google/gemma-4-31b-it:free");
        assert_eq!(parsed.provider, "openrouter");
        assert_eq!(parsed.model, "google/gemma-4-31b-it:free");
        assert_eq!(parsed.id(), "google/gemma-4-31b-it:free");
    }

    #[test]
    fn a_provider_prefixed_id_round_trips() {
        let parsed = parse_catalog_id("google::gemini-2.5-flash");
        assert_eq!(parsed.provider, "google");
        assert_eq!(parsed.model, "gemini-2.5-flash");
        assert_eq!(parsed.id(), "google::gemini-2.5-flash");
    }

    #[test]
    fn the_custom_endpoint_needs_no_key() {
        assert!(!get("custom").unwrap().needs_key);
        assert!(get("openrouter").unwrap().needs_key);
    }

    #[test]
    fn both_google_key_shapes_are_accepted() {
        let google = get("google").unwrap();
        // The Auth key AI Studio issues now, and the Standard key it used to.
        assert!(key_accepted(google, "AQ.Ab8RN6IXJ4hL"));
        assert!(key_accepted(google, "AIzaSyD-0123456789"));
        assert!(!key_accepted(google, "sk-or-v1-abc"));
        assert!(!key_accepted(google, ""));
    }

    #[test]
    fn the_bad_paste_message_names_the_current_prefix_first() {
        let message = key_prefix_list(get("google").unwrap());
        assert!(message.starts_with("“AQ.”"), "{message}");
        assert!(message.contains("AIza"), "{message}");
        assert_eq!(key_prefix_list(get("groq").unwrap()), "“gsk_”");
    }

    #[test]
    fn a_provider_without_a_guessable_prefix_takes_anything() {
        assert!(key_accepted(get("custom").unwrap(), "anything at all"));
        assert!(key_accepted(get("mistral").unwrap(), ""));
    }

    #[test]
    fn googles_first_attempt_speaks_gemini_on_the_native_base() {
        let settings = settings(&[], Some("google"));
        if let Ok(route) = build_route(get("google").unwrap(), &settings, None, &Picks::default()) {
            let first = &route.attempts[0];
            assert!(first.gemini, "Google has to use its own shape");
            assert!(first.base_url.ends_with("/v1beta"), "{}", first.base_url);
            assert!(!first.base_url.contains("/openai"), "{}", first.base_url);
        }
    }
}
