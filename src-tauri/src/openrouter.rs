//! The OpenRouter client: sign-in, and the streaming rewrite.
//!
//! The rewrite is provider-agnostic — base URL, key and model come from
//! [`crate::providers`] — but OpenRouter is the one that offers a hand-rolled OAuth
//! flow, so it stays the reference implementation here.
//!
//! Two things live here. The OAuth PKCE flow (no key is ever typed by hand unless
//! someone wants to) and the completion call the workspace actually makes.
//!
//! The sign-in binds a loopback listener on an ephemeral port, sends the browser to
//! `openrouter.ai/auth` with a SHA-256 challenge, and takes the `code` that comes
//! back on `/callback`. That code is exchanged for a *user-controlled* key, which is
//! written to the OS keychain and never returned to the webview — the frontend only
//! ever sees [`crate::KeyStatus`].
//!
//! The rewrite path takes an `Option<&AppHandle>` rather than an `AppHandle`, for one
//! concrete reason: [`crate::self_test`] runs it from the same binary the person
//! signed in with, and the keychain item is scoped to that binary. A test process is
//! a different identity and macOS would prompt or refuse.

use std::time::Duration;

use futures_util::StreamExt;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::prompt;
use crate::providers;
use crate::samples;
use crate::store;
use crate::DELTA_EVENT;

const API: &str = "https://openrouter.ai/api/v1";

/// How long one block may take before we give up on that model. Free models can be
/// slow, but a stream that has not finished in three minutes is not going to.
const BLOCK_TIMEOUT: Duration = Duration::from_secs(180);

/// How long a stream may go without a single byte before it counts as stalled.
///
/// The measured failure this exists for: a ~3,000-character block on the free router
/// opened the stream, sent nothing, and ate the whole three-minute budget. A stalled
/// stream is a failed block, and a failed block is the fallback's job.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub mod http {
    use std::time::Duration;

    use serde::de::DeserializeOwned;

    pub fn client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .user_agent("QuillNot/0.1.0 (Tauri desktop)")
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("could not build the HTTP client: {e}"))
    }

    pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
        let response = client()?
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(describe)?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("OpenRouter answered {status}: {}", clip(&body)));
        }
        response
            .json::<T>()
            .await
            .map_err(|e| format!("could not read OpenRouter's reply: {e}"))
    }

    fn describe(err: reqwest::Error) -> String {
        if err.is_timeout() {
            "OpenRouter did not answer in time.".to_string()
        } else if err.is_connect() {
            "could not reach openrouter.ai — check the connection.".to_string()
        } else {
            format!("request failed: {err}")
        }
    }

    pub fn clip(body: &str) -> String {
        let trimmed = body.trim();
        if trimmed.chars().count() > 280 {
            format!("{}…", trimmed.chars().take(280).collect::<String>())
        } else {
            trimmed.to_string()
        }
    }
}

/* ------------------------------------------------------------------ sign-in */

#[derive(Deserialize)]
struct KeyExchange {
    key: String,
}

pub async fn sign_in() -> Result<crate::KeyStatus, String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("could not open the local callback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("could not read the callback port: {e}"))?
        .port();

    let callback = format!("http://127.0.0.1:{port}/callback");
    let verifier = random_token(48);
    let challenge = pkce_challenge(&verifier);
    let url = format!(
        "https://openrouter.ai/auth?callback_url={callback}\
         &code_challenge={challenge}&code_challenge_method=S256&key_label=QuillNot"
    );

    open_browser(&url)?;

    let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(listener))
        .await
        .map_err(|_| {
            "signing in timed out — the browser window was left open for too long.".to_string()
        })??;

    let key = exchange_code(&code, &verifier).await?;
    // Both entries: the legacy one the header badge has always read, and the
    // per-provider one the provider list reads.
    store::save_key(&key)?;
    store::save_key_for("openrouter", &key)?;
    Ok(store::key_status())
}

/// Accepts the redirect, answers it with a sentence the person can read, and hands
/// back the `code`. A browser may fetch `/favicon.ico` first, so a handful of
/// connections are tolerated before giving up.
async fn wait_for_code(listener: tokio::net::TcpListener) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    for _ in 0..8 {
        let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let read = socket.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..read]).to_string();
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_string();

        let code = query_param(&target, "code");
        let (status, body) = match code {
            Some(_) => (
                "200 OK",
                "<!doctype html><title>QuillNot</title><body style=\"font:16px -apple-system,sans-serif;padding:3rem;line-height:1.6\">\
                 <h2 style=\"font-weight:600\">QuillNot is connected</h2>\
                 <p>You can close this tab and go back to the app.</p></body>",
            ),
            None => (
                "400 Bad Request",
                "<!doctype html><title>QuillNot</title><body style=\"font:16px -apple-system,sans-serif;padding:3rem;line-height:1.6\">\
                 <h2 style=\"font-weight:600\">No code came back</h2>\
                 <p>Close this tab and try signing in again from QuillNot.</p></body>",
            ),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;

        if let Some(code) = code {
            return Ok(code);
        }
    }

    Err("the browser never came back with an authorization code.".to_string())
}

fn query_param(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == name {
            return percent_decode(value);
        }
    }
    None
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

async fn exchange_code(code: &str, verifier: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct Body<'a> {
        code: &'a str,
        code_verifier: &'a str,
        code_challenge_method: &'a str,
    }

    let response = http::client()?
        .post(format!("{API}/auth/keys"))
        .json(&Body {
            code,
            code_verifier: verifier,
            code_challenge_method: "S256",
        })
        .send()
        .await
        .map_err(|e| format!("could not reach OpenRouter to finish signing in: {e}"))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "OpenRouter refused the sign-in: {}",
            http::clip(&body)
        ));
    }

    let body: KeyExchange = response
        .json()
        .await
        .map_err(|e| format!("could not read the key OpenRouter returned: {e}"))?;
    if body.key.trim().is_empty() {
        return Err("OpenRouter returned an empty key.".to_string());
    }
    Ok(body.key)
}

fn random_token(bytes: usize) -> String {
    use base64::Engine;
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(target_os = "macos")]
pub(crate) fn open_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the browser: {e}"))
}

#[cfg(target_os = "windows")]
pub(crate) fn open_browser(url: &str) -> Result<(), String> {
    // The empty argument after `start` is the window title; without it a URL
    // containing an ampersand would be read as a command separator.
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the browser: {e}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn open_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the browser: {e}"))
}

/* --------------------------------------------------------------- paraphrase */

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaphraseRequest {
    pub text: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub strength: Option<u8>,
    #[serde(default)]
    pub model: Option<String>,
    /// A skill written by the person, bundled with every block of this run. Present
    /// means the humanizer sent it, and the mode is then whatever the skill says.
    #[serde(default)]
    pub skill: Option<String>,
    /// A target language, which turns the request into a translation.
    #[serde(default)]
    pub target: Option<String>,
    /// The register a translation keeps: Neutral, Formal or Informal.
    #[serde(default)]
    pub formality: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaphraseResult {
    pub text: String,
    pub model: String,
    pub blocks: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Delta {
    block: usize,
    text: String,
    model: String,
}

/// The command the workspace calls.
pub async fn paraphrase(app: AppHandle, request: ParaphraseRequest) -> Result<ParaphraseResult, String> {
    paraphrase_with(Some(&app), request).await
}

/// The same work without a window, for `--self-test`.
pub async fn paraphrase_with(
    app: Option<&AppHandle>,
    request: ParaphraseRequest,
) -> Result<ParaphraseResult, String> {
    let text = request.text.trim();
    if text.is_empty() {
        return Err("There is nothing to paraphrase.".to_string());
    }

    // Whichever provider is active in Settings answers this: its base URL, its key,
    // its model. The failover chain is built in `providers`, so a free tier that is
    // rate limited hands the block to the next one instead of ending the run.
    let route = match app {
        Some(handle) => providers::resolve(handle, request.model.clone(), samples::PARAPHRASE)?,
        None => providers::resolve_headless(request.model.clone())?,
    };

    let skill = request
        .skill
        .map(|skill| skill.trim().to_string())
        .filter(|skill| !skill.is_empty());

    let target = request
        .target
        .map(|target| target.trim().to_string())
        .filter(|target| !target.is_empty());
    let formality = request
        .formality
        .map(|formality| formality.trim().to_string())
        .filter(|formality| !formality.is_empty())
        .unwrap_or_else(|| "Neutral".to_string());

    // A skill replaces the mode instructions rather than joining them: the humanizer's
    // whole promise is that the pane shows exactly what the model is told. A target
    // language does the same job for Translate.
    let mode = if skill.is_some() {
        "humanize".to_string()
    } else if target.is_some() {
        "translate".to_string()
    } else {
        request.mode.unwrap_or_else(|| "standard".to_string())
    };
    let strength = request.strength.unwrap_or(2).clamp(1, 4);

    let started = std::time::Instant::now();

    // 2,500 rather than 4,000 characters per block: the free models stall far less
    // often on a smaller prompt, and a failed block costs a fraction of the time.
    let blocks = prompt::split_blocks(text, 2500);
    let mut rewritten: Vec<String> = Vec::with_capacity(blocks.len());
    let mut used = route.model.clone();

    for (index, block) in blocks.iter().enumerate() {
        // The separator is emitted as a delta too, so the pane grows exactly as the
        // text will be assembled instead of running paragraphs together mid-stream.
        if index > 0 {
            if let Some(handle) = app {
                let _ = handle.emit(
                    DELTA_EVENT,
                    Delta {
                        block: index,
                        text: "\n\n".to_string(),
                        model: used.clone(),
                    },
                );
            }
        }

        let (system, user) = match (skill.as_deref(), target.as_deref()) {
            (Some(skill), _) => prompt::skill_prompt(skill, block),
            (None, Some(target)) => prompt::translate_prompt(block, target, &formality),
            (None, None) => prompt::paraphrase_prompt(block, &mode, strength),
        };
        let (model, body) =
            stream_block(app, samples::PARAPHRASE, index, &route.attempts, &system, &user).await?;
        used = model;
        rewritten.push(body);
    }

    let result = ParaphraseResult {
        text: rewritten.join("\n\n"),
        model: used,
        blocks: blocks.len(),
    };

    // Recorded only when a window exists: `--self-test` exercises the same path and
    // must not fill the history with test runs.
    if let Some(handle) = app {
        crate::history::record_paraphrase(
            handle,
            &mode,
            &result.model,
            text,
            &result.text,
            started.elapsed().as_millis() as u64,
        );
    }

    Ok(result)
}

/// The event the header badge listens on for the provider's remaining requests.
pub const QUOTA_EVENT: &str = "quota:update";

/// What a provider says is left of this window's allowance. Both numbers are optional
/// because providers disagree about which headers they send, and a provider that sends
/// neither should leave the last reading alone rather than blank it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub provider: String,
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
}

fn note_quota(app: Option<&AppHandle>, provider: &str, headers: &reqwest::header::HeaderMap) {
    let number = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    };
    let remaining = number("x-ratelimit-remaining-requests").or_else(|| number("x-ratelimit-remaining"));
    let limit = number("x-ratelimit-limit-requests").or_else(|| number("x-ratelimit-limit"));
    if remaining.is_none() && limit.is_none() {
        return;
    }
    if let Some(handle) = app {
        let _ = handle.emit(
            QUOTA_EVENT,
            Quota {
                provider: provider.to_string(),
                remaining,
                limit,
            },
        );
    }
}

/// The body of one finished call, with everything the sample log wants to keep.
struct Call {
    body: String,
    meta: samples::CallMeta,
}

/// One block, with the fallback the plan promised: the preferred model first, the
/// gateway router after it. An empty rewrite counts as a failure — a model that
/// answers with nothing has not answered.
pub async fn stream_block(
    app: Option<&AppHandle>,
    task: &str,
    index: usize,
    attempts: &[providers::Attempt],
    system: &str,
    user: &str,
) -> Result<(String, String), String> {
    let mut last_error = String::new();
    for (attempt_index, attempt) in attempts.iter().enumerate() {
        if attempt_index > 0 {
            backoff(attempt_index).await;
        }

        let model = attempt.model.clone();
        let started = std::time::Instant::now();
        // When the first token arrived. Measured in the sink itself, so it is the moment
        // the text started to exist rather than when the call returned.
        // Atomic rather than a Cell: the sink is held across an await and the future has
        // to be Send, which a Cell is not.
        const UNSET: u64 = u64::MAX;
        let first_token = std::sync::atomic::AtomicU64::new(UNSET);
        let mut sink = |piece: &str| {
            if first_token.load(std::sync::atomic::Ordering::Relaxed) == UNSET {
                let _ = first_token.compare_exchange(
                    UNSET,
                    started.elapsed().as_millis() as u64,
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
            if let Some(handle) = app {
                let _ = handle.emit(
                    DELTA_EVENT,
                    Delta {
                        block: index,
                        text: piece.to_string(),
                        model: model.clone(),
                    },
                );
            }
        };

        let outcome =
            tokio::time::timeout(BLOCK_TIMEOUT, stream_once(attempt, system, user, &mut sink)).await;
        let elapsed = started.elapsed();
        let measured = first_token.load(std::sync::atomic::Ordering::Relaxed);
        let meta = samples::CallMeta {
            provider: Some(host_of(&attempt.base_url)),
            ttft_ms: (measured != UNSET).then_some(measured),
            status: Some(200),
            ..samples::CallMeta::default()
        };

        match outcome {
            Ok(Ok(body)) if !body.trim().is_empty() => {
                record_sample(app, task, &model, true, elapsed, meta);
                return Ok((model, body.trim().to_string()));
            }
            Ok(Ok(_)) => {
                record_sample(app, task, &model, false, elapsed, meta);
                last_error = format!("{model} returned an empty rewrite");
            }
            Ok(Err(err)) => {
                record_sample(app, task, &model, false, elapsed, meta);
                last_error = err;
            }
            Err(_) => {
                record_sample(app, task, &model, false, elapsed, meta);
                last_error = format!("{model} took longer than three minutes");
            }
        }
    }
    Err(last_error)
}

/// Waits before the next provider in the chain.
///
/// Doubling rather than fixed: a rate limit that clears in a second is worth waiting
/// for, and one that does not clear is worth giving up on. Capped at five seconds so a
/// chain of three models is a delay, not a hang.
async fn backoff(attempt: usize) {
    let step = 700u64.saturating_mul(1u64 << attempt.min(3));
    tokio::time::sleep(Duration::from_millis(step.min(5_000))).await;
}

/// One line in the sample log, whether the call worked or not: a failure is evidence
/// too, and three in a row is what demotes a model.
///
/// No window means `--self-test`, which must not fill the log with test calls.
fn record_sample(
    app: Option<&AppHandle>,
    task: &str,
    model: &str,
    ok: bool,
    elapsed: Duration,
    meta: samples::CallMeta,
) {
    if let Some(handle) = app {
        samples::record(handle, model, task, ok, elapsed.as_millis() as u64, meta);
    }
}

/// `api.groq.com` out of `https://api.groq.com/openai/v1`, for error messages that
/// name the thing that actually refused.
fn host_of(base_url: &str) -> String {
    base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or(base_url)
        .to_string()
}

async fn stream_once(
    attempt: &providers::Attempt,
    system: &str,
    user: &str,
    // `+ Send` because a Tauri command's future must be `Send`, and this sink is
    // held across an await. It captures nothing but an AppHandle and a model name,
    // so it is Send in practice; the bound just has to say so.
    on_delta: &mut (dyn FnMut(&str) + Send),
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Message<'a> {
        role: &'a str,
        content: &'a str,
    }

    #[derive(Serialize)]
    struct Body<'a> {
        model: &'a str,
        messages: Vec<Message<'a>>,
        stream: bool,
        temperature: f32,
    }

    let model = attempt.model.as_str();
    let body = Body {
        model,
        messages: vec![
            Message {
                role: "system",
                content: system,
            },
            Message {
                role: "user",
                content: user,
            },
        ],
        stream: true,
        temperature: 0.5,
    };

    let host = host_of(&attempt.base_url);
    let mut outgoing = http::client()?
        .post(format!(
            "{}/chat/completions",
            attempt.base_url.trim_end_matches('/')
        ))
        .json(&body);

    // A local server usually takes no Authorization header at all, which is the one
    // real difference between "bring your own endpoint" and a hosted free tier.
    if let Some(key) = attempt.key.as_deref() {
        outgoing = outgoing.bearer_auth(key);
    }
    if attempt.openrouter {
        // OpenRouter uses these for attribution; they cost nothing and are honest.
        outgoing = outgoing
            .header("HTTP-Referer", "https://github.com/emrisca/quillnot")
            .header("X-Title", "QuillNot");
    }

    let response = outgoing
        .send()
        .await
        .map_err(|e| format!("could not reach {host}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let body = response.text().await.unwrap_or_default();

        return Err(match status.as_u16() {
            429 => match retry_after {
                Some(seconds) => format!("{model} is rate limited right now (retry after {seconds}s)."),
                None => format!("{model} is rate limited right now."),
            },
            401 | 403 => format!("{host} rejected the key — check it in Settings."),
            404 => format!("{model} is not available on this account."),
            _ => format!("{host} answered {status}: {}", http::clip(&body)),
        });
    }

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    let mut out = String::new();

    loop {
        let chunk = match tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await {
            Ok(Some(chunk)) => chunk.map_err(|e| format!("the reply stopped early: {e}"))?,
            Ok(None) => break,
            Err(_) => {
                return Err(format!(
                    "{model} stopped sending for {}s",
                    IDLE_TIMEOUT.as_secs()
                ))
            }
        };
        pending.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = pending.find("\n\n") {
            let event: String = pending.drain(..end + 2).collect();
            for line in event.lines() {
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload == "[DONE]" {
                    return Ok(out);
                }
                let Ok(parsed) = serde_json::from_str::<CompletionChunk>(payload) else {
                    continue;
                };
                if let Some(message) = parsed.error {
                    return Err(format!("{model}: {}", http::clip(&message.message)));
                }
                if let Some(delta) = parsed
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|choice| choice.delta.content)
                {
                    out.push_str(&delta);
                    on_delta(&delta);
                }
            }
        }
    }

    Ok(out)
}

/// One whole answer, not a stream.
///
/// The grammar checker needs the complete JSON before it can verify a single quote, so
/// there is nothing to show halfway. Same provider chain and same block budget as a
/// rewrite; a model that returns nothing, times out or errors hands over to the next.
pub async fn complete(
    app: Option<&AppHandle>,
    task: &str,
    attempts: &[providers::Attempt],
    system: &str,
    user: &str,
) -> Result<(String, String), String> {
    let mut last_error = String::new();
    for (index, attempt) in attempts.iter().enumerate() {
        if index > 0 {
            backoff(index).await;
        }

        let started = std::time::Instant::now();
        let outcome =
            tokio::time::timeout(BLOCK_TIMEOUT, complete_once(app, attempt, system, user)).await;
        let elapsed = started.elapsed();

        match outcome {
            Ok(Ok(call)) if !call.body.trim().is_empty() => {
                record_sample(app, task, &attempt.model, true, elapsed, call.meta);
                return Ok((attempt.model.clone(), call.body));
            }
            Ok(Ok(call)) => {
                record_sample(app, task, &attempt.model, false, elapsed, call.meta);
                last_error = format!("{} returned an empty answer", attempt.model);
            }
            Ok(Err(err)) => {
                record_sample(
                    app,
                    task,
                    &attempt.model,
                    false,
                    elapsed,
                    samples::CallMeta::default(),
                );
                last_error = err;
            }
            Err(_) => {
                record_sample(
                    app,
                    task,
                    &attempt.model,
                    false,
                    elapsed,
                    samples::CallMeta::default(),
                );
                last_error = format!("{} took longer than three minutes", attempt.model);
            }
        }
    }
    Err(last_error)
}

async fn complete_once(
    app: Option<&AppHandle>,
    attempt: &providers::Attempt,
    system: &str,
    user: &str,
) -> Result<Call, String> {
    let call_started = std::time::Instant::now();

    #[derive(Serialize)]
    struct Message<'a> {
        role: &'a str,
        content: &'a str,
    }

    #[derive(Serialize)]
    struct Body<'a> {
        model: &'a str,
        messages: Vec<Message<'a>>,
        stream: bool,
        temperature: f32,
    }

    #[derive(Deserialize)]
    struct Reply {
        #[serde(default)]
        choices: Vec<ReplyChoice>,
        #[serde(default)]
        usage: Option<Usage>,
    }

    #[derive(Deserialize, Default)]
    struct Usage {
        #[serde(default)]
        prompt_tokens: Option<u64>,
        #[serde(default)]
        completion_tokens: Option<u64>,
    }

    #[derive(Deserialize)]
    struct ReplyChoice {
        #[serde(default)]
        message: ReplyMessage,
    }

    #[derive(Deserialize, Default)]
    struct ReplyMessage {
        #[serde(default)]
        content: Option<String>,
    }

    let body = Body {
        model: attempt.model.as_str(),
        messages: vec![
            Message {
                role: "system",
                content: system,
            },
            Message {
                role: "user",
                content: user,
            },
        ],
        stream: false,
        // Editing wants the model's most careful answer, not its most varied one.
        temperature: 0.2,
    };

    let host = host_of(&attempt.base_url);
    let mut outgoing = http::client()?
        .post(format!(
            "{}/chat/completions",
            attempt.base_url.trim_end_matches('/')
        ))
        .json(&body);
    if let Some(key) = attempt.key.as_deref() {
        outgoing = outgoing.bearer_auth(key);
    }
    if attempt.openrouter {
        outgoing = outgoing
            .header("HTTP-Referer", "https://github.com/emrisca/quillnot")
            .header("X-Title", "QuillNot");
    }

    let response = outgoing
        .send()
        .await
        .map_err(|e| format!("could not reach {host}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.text().await.unwrap_or_default();
        return Err(status_error(&host, &attempt.model, status, retry_after, &body));
    }

    // Time to the headers, which for a non-streaming call is when the reply started
    // arriving. Recorded before the body is read, because reading it moves the response.
    let ttft_ms = Some(call_started.elapsed().as_millis() as u64);
    let quota_headers = response.headers().clone();
    note_quota(app, &host, &quota_headers);

    let reply: Reply = response
        .json()
        .await
        .map_err(|e| format!("could not read {host}'s reply: {e}"))?;
    let Reply { choices, usage } = reply;
    let body = choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| format!("{host} answered without any content"))?;

    Ok(Call {
        body,
        meta: samples::CallMeta {
            provider: Some(host),
            ttft_ms,
            tokens_in: usage.as_ref().and_then(|usage| usage.prompt_tokens),
            tokens_out: usage.as_ref().and_then(|usage| usage.completion_tokens),
            status: Some(status.as_u16()),
        },
    })
}

fn status_error(
    host: &str,
    model: &str,
    status: reqwest::StatusCode,
    retry_after: Option<String>,
    body: &str,
) -> String {
    match status.as_u16() {
        429 => match retry_after {
            Some(seconds) => format!("{model} is rate limited right now (retry after {seconds}s)."),
            None => format!("{model} is rate limited right now."),
        },
        401 | 403 => format!("{host} rejected the key — check it in Settings."),
        404 => format!("{model} is not available on this account."),
        _ => format!("{host} answered {status}: {}", http::clip(body)),
    }
}

#[derive(Deserialize)]
struct CompletionChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    error: Option<ChunkError>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
}

#[derive(Deserialize, Default)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChunkError {
    #[serde(default)]
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_parameter_is_found_and_decoded() {
        assert_eq!(
            query_param("/callback?code=abc123&state=xyz", "code"),
            Some("abc123".to_string())
        );
        assert_eq!(
            query_param("/callback?code=a%2Bb%20c", "code"),
            Some("a+b c".to_string())
        );
    }

    #[test]
    fn a_missing_parameter_is_none() {
        assert_eq!(query_param("/callback", "code"), None);
        assert_eq!(query_param("/callback?state=x", "code"), None);
    }

    #[test]
    fn pkce_challenge_is_the_base64url_of_the_sha256() {
        // The RFC 7636 appendix B vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_streamed_delta_is_read_out_of_the_sse_frame() {
        let frame = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\r\n\r\n";
        let payload = frame
            .lines()
            .find_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .expect("a data line");
        let parsed: CompletionChunk = serde_json::from_str(payload).unwrap();
        assert_eq!(
            parsed.choices.into_iter().next().unwrap().delta.content,
            Some("Hel".to_string())
        );
    }

    #[test]
    fn an_error_frame_is_recognised() {
        let parsed: CompletionChunk =
            serde_json::from_str("{\"error\":{\"message\":\"no credits\"}}").unwrap();
        assert_eq!(parsed.error.unwrap().message, "no credits");
    }
}
