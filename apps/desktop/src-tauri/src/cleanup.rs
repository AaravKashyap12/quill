use crate::metrics;
use crate::model::{AppSettings, CleanupProvider, ProviderStatus};
use crate::register::Register;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};

const OLLAMA_URL: &str = "http://127.0.0.1:11434";
const MODEL_CONTEXT_TOKENS: usize = 2_048;
const MAX_GENERATED_TOKENS: usize = 512;
const CONTEXT_SAFETY_MARGIN_TOKENS: usize = 128;
const MAX_PROMPT_TOKEN_ESTIMATE: usize =
    MODEL_CONTEXT_TOKENS - MAX_GENERATED_TOKENS - CONTEXT_SAFETY_MARGIN_TOKENS;
const COMPATIBLE_URLS: [&str; 3] = [
    "http://127.0.0.1:1234",
    "http://127.0.0.1:8080",
    "http://127.0.0.1:39281",
];

/// Wire-protocol dispatch is decoupled from base_url substring sniffing so
/// that picking "Ollama" in the dropdown genuinely uses Ollama's REST API
/// and picking "OpenAI-compatible" genuinely uses the OpenAI shape — even
/// when the user leaves the default URL pointing at Ollama.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Ollama,
    OpenAiCompatible,
}

/// Resolve which protocol Quill should speak and which base URL to talk to,
/// given the user's explicit setting and (for auto-detect) the current
/// results of `detect_providers`. Called from `clean`, `warm_up`, and
/// `resolve_model`, so every code path stays consistent.
pub async fn resolve_endpoint(settings: &AppSettings) -> Result<(Protocol, String)> {
    match settings.cleanup_provider {
        CleanupProvider::Disabled => Err(anyhow!("Scribe cleanup is disabled")),
        CleanupProvider::Ollama => {
            let url = if settings.cleanup_base_url.contains(":11434") {
                settings.cleanup_base_url.clone()
            } else {
                OLLAMA_URL.to_string()
            };
            Ok((Protocol::Ollama, url))
        }
        CleanupProvider::OpenaiCompatible => {
            // Default URL is Ollama's port; if user picked OpenAI-compatible
            // but never overrode the URL, snap to the LM Studio default so
            // requests actually reach an OpenAI-shaped server.
            let url = if settings.cleanup_base_url.is_empty()
                || settings.cleanup_base_url.contains(":11434")
            {
                COMPATIBLE_URLS[0].to_string()
            } else {
                settings.cleanup_base_url.clone()
            };
            Ok((Protocol::OpenAiCompatible, url))
        }
        CleanupProvider::Auto => {
            let providers = detect_providers().await;
            if let Some(p) = providers.iter().find(|p| p.kind == "ollama" && p.available) {
                return Ok((Protocol::Ollama, p.base_url.clone()));
            }
            if let Some(p) = providers
                .iter()
                .find(|p| p.kind == "openai-compatible" && p.available)
            {
                return Ok((Protocol::OpenAiCompatible, p.base_url.clone()));
            }
            Err(anyhow!(
                "No local LLM server detected. Start Ollama or LM Studio and try again."
            ))
        }
    }
}

const SHARED_PREAMBLE: &str = r#"You are refining dictated speech into well-formed written text.
The user spoke roughly and wants it to read well.

ABSOLUTE RULE — never invent facts:
Times, dates, names, numbers, prices, URLs, commitments and
obligations must appear in the output only if the user said them.
Never add a promise, offer or constraint the user did not make.
You may freely rephrase, reorder, and restructure. You may fix
grammar, vocabulary and word choice. Resolve any self-correction
the speaker makes — keep only what they settled on — however they
phrase it.
Output only the refined text. No preamble, no quotes, no markdown."#;

fn register_instructions(register: Register) -> &'static str {
    match register {
        Register::Email => {
            r#"Target: an email.
Add an appropriate greeting and sign-off. Expand terse fragments
into full sentences. Use paragraphs. Courteous but not flowery.
Do not add commitments — "I'm flexible on timing" is a new promise
unless they said it. Do not add next steps, alternative times,
invitations, requests for a reply, or offers to help."#
        }
        Register::Chat => {
            r#"Target: a chat message (Slack/Discord/Teams).
No greeting, no sign-off. Short. Contractions fine. Usually one
paragraph. Keep it casual and direct."#
        }
        Register::Prompt => {
            r#"Target: a prompt for an AI assistant.
Add NOTHING. No greeting, no framing and no new requirements.
Remove politeness, filler and framing that the user spoke —
"please", "could you", "I was wondering" — they add nothing to a
prompt. Fix grammar and false starts, resolve self-corrections, and
make the wording clear. Preserve every technical term exactly.
Return one plain-text paragraph with no markdown formatting."#
        }
        Register::Notes => {
            r#"Target: a note or document.
No greeting, no addressee. Clean prose. Preserve the user's
structure."#
        }
        Register::Generic => {
            r#"Target: general text. Light touch — fix grammar, filler and
self-corrections. Do not add greetings or sign-offs."#
        }
    }
}

pub fn prompt(transcript: &str, register: Register) -> String {
    format!(
        "{SHARED_PREAMBLE}\n\n{}\n\nDictation:\n{transcript}",
        register_instructions(register)
    )
}

/// Conservative tokenizer-independent estimate used before contacting a local
/// model. ASCII word runs are charged at one token per three characters,
/// punctuation at one token each, and non-ASCII characters by their UTF-8
/// byte length. Byte-level BPE cannot emit more tokens than input bytes, so
/// this remains an upper bound even for sparse scripts and emoji. This
/// intentionally rejects before the model-specific tokenizer's hard context
/// cliff rather than risking left-truncation of the safety preamble.
fn conservative_token_estimate(value: &str) -> usize {
    let mut tokens = 0usize;
    let mut ascii_word_length = 0usize;
    let flush_ascii_word = |tokens: &mut usize, length: &mut usize| {
        if *length > 0 {
            *tokens += (*length).div_ceil(3);
            *length = 0;
        }
    };

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            ascii_word_length += 1;
            continue;
        }
        flush_ascii_word(&mut tokens, &mut ascii_word_length);
        if character.is_whitespace() {
            continue;
        }
        tokens += if character.is_ascii() {
            1
        } else {
            character.len_utf8()
        };
    }
    flush_ascii_word(&mut tokens, &mut ascii_word_length);
    tokens
}

fn resolve_simple_corrections(transcript: &str) -> String {
    let tokens = transcript.split_whitespace().collect::<Vec<_>>();
    let mut output = Vec::<&str>::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let normalized = tokens[index]
            .trim_matches(|character: char| !character.is_alphanumeric())
            .to_ascii_lowercase();
        if matches!(normalized.as_str(), "no" | "sorry" | "rather")
            && !output.is_empty()
            && index + 1 < tokens.len()
        {
            output.pop();
            index += 1;
            if normalized == "no"
                && index < tokens.len()
                && tokens[index]
                    .trim_matches(|character: char| !character.is_alphanumeric())
                    .eq_ignore_ascii_case("wait")
            {
                index += 1;
            }
            continue;
        }
        output.push(tokens[index]);
        index += 1;
    }
    output.join(" ")
}

/// A conservative local fallback used whenever the cleanup model is missing,
/// times out, or invents content. It resolves the correction markers Quill
/// understands, removes only unmistakable hesitation sounds, and otherwise
/// preserves the user's words for editing in the Scribe review window.
pub fn safe_fallback(transcript: &str) -> String {
    let resolved = resolve_simple_corrections(transcript);
    let mut tokens = resolved
        .split_whitespace()
        .filter(|token| {
            let normalized = token
                .trim_matches(|character: char| !character.is_alphanumeric())
                .to_ascii_lowercase();
            !matches!(normalized.as_str(), "um" | "uh" | "erm" | "hmm")
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return String::new();
    }
    let mut output = tokens.join(" ");
    if let Some(first) = output.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    tokens.clear();
    output
}

const COMMITMENT_SIGNAL_GROUPS: &[&[&str]] = &[
    &["i will", "we will", "i am going to", "we are going to"],
    &["i can", "we can", "i am able to", "we are able to"],
    &[
        "available",
        "flexible",
        "works for me",
        "work for me",
        "accommodate",
    ],
    &[
        "another time",
        "alternative time",
        "different time",
        "find a time",
        "find another",
        "reschedule",
        "schedule another",
        "arrange another",
    ],
    &[
        "follow up",
        "reach out",
        "get back to you",
        "keep you posted",
        "let you know",
        "tell you",
        "inform you",
        "update you",
    ],
    &[
        "let me know",
        "tell me",
        "inform me",
        "update me",
        "feel free",
        "at your convenience",
    ],
    &["happy to", "glad to", "look forward to", "would be happy"],
    &["promise", "commit to", "guarantee", "offer to"],
];

/// Detect obligation, availability, or proposal language that appears only in
/// the model output. This deliberately works on semantic signal groups rather
/// than exact sentences so harmless contraction changes still compare equal.
/// It is conservative by design: a false positive yields the user's local
/// fallback draft in the review window; a false negative could silently create
/// a commitment the user never made.
fn introduces_commitment(source: &str, cleaned: &str) -> bool {
    let source = normalize_for_commitment_guard(source);
    let cleaned = normalize_for_commitment_guard(cleaned);

    COMMITMENT_SIGNAL_GROUPS.iter().any(|signals| {
        signals
            .iter()
            .any(|signal| contains_phrase(&cleaned, signal))
            && !signals
                .iter()
                .any(|signal| contains_phrase(&source, signal))
    })
}

fn normalize_for_commitment_guard(value: &str) -> String {
    let expanded = value
        .to_lowercase()
        .replace('’', "'")
        .replace("can't", "cannot")
        .replace("won't", "will not")
        .replace("i'll", "i will")
        .replace("we'll", "we will")
        .replace("i'm", "i am")
        .replace("we're", "we are")
        .replace("let's", "let us");
    let normalized = expanded
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!(" {normalized} ")
}

fn contains_phrase(normalized: &str, phrase: &str) -> bool {
    normalized.contains(&format!(" {phrase} "))
}

#[derive(Deserialize)]
struct OllamaTags {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

const PREFERRED_COMPOSE_MODELS: &[&str] = &["qwen2.5:7b"];

fn select_ollama_model(models: Vec<OllamaModel>) -> Option<String> {
    for preferred in PREFERRED_COMPOSE_MODELS {
        if let Some(model) = models.iter().find(|model| model.name == *preferred) {
            return Some(model.name.clone());
        }
    }
    models.into_iter().map(|model| model.name).next()
}

#[derive(Deserialize)]
struct OpenAiModels {
    data: Vec<OpenAiModel>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: String,
}

pub async fn detect_providers() -> Vec<ProviderStatus> {
    let Ok(client) = Client::builder()
        // Ignore system HTTP_PROXY / HTTPS_PROXY. All targets are localhost;
        // routing them through a corporate/dev proxy just breaks the connection.
        .no_proxy()
        .timeout(Duration::from_millis(600))
        .build()
    else {
        return Vec::new();
    };
    let mut providers = Vec::with_capacity(4);

    let ollama = client
        .get(format!("{OLLAMA_URL}/api/tags"))
        .send()
        .await
        .ok()
        .and_then(|response| response.error_for_status().ok());
    let ollama_models = match ollama {
        Some(response) => response
            .json::<OllamaTags>()
            .await
            .map(|tags| tags.models.into_iter().map(|model| model.name).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    providers.push(ProviderStatus {
        kind: "ollama".into(),
        base_url: OLLAMA_URL.into(),
        available: !ollama_models.is_empty(),
        models: ollama_models,
    });

    for base_url in COMPATIBLE_URLS {
        let models = client
            .get(format!("{base_url}/v1/models"))
            .send()
            .await
            .ok()
            .and_then(|response| response.error_for_status().ok());
        let model_ids = match models {
            Some(response) => response
                .json::<OpenAiModels>()
                .await
                .map(|payload| payload.data.into_iter().map(|model| model.id).collect())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        providers.push(ProviderStatus {
            kind: "openai-compatible".into(),
            base_url: base_url.into(),
            available: !model_ids.is_empty(),
            models: model_ids,
        });
    }
    providers
}

/// Fire-and-forget request that loads the configured cleanup model into RAM
/// without generating any tokens, so the first real Scribe request doesn't
/// pay the 10–40s cold-load penalty. Called at app startup, when settings
/// change, and any time a fresh provider is detected. Silent-fails on any
/// error (Ollama not running, model missing, disabled, etc.) because pre-warm
/// is best-effort — real errors surface on the actual cleanup call.
pub async fn warm_up(settings: AppSettings) {
    let Ok((protocol, base_url)) = resolve_endpoint(&settings).await else {
        return;
    };
    if ensure_loopback(&base_url).is_err() {
        return;
    }
    let Ok(client) = Client::builder()
        .no_proxy()
        // Generous ceiling — warm-up on a slow disk with a 2 GB model can
        // legitimately take 60s. It's a background task so latency is fine.
        .timeout(Duration::from_secs(120))
        .build()
    else {
        return;
    };

    let model = match resolve_model(&client, &settings, protocol, &base_url).await {
        Ok(m) => m,
        Err(err) => {
            tracing::debug!(%err, "cleanup warm-up skipped: could not resolve a model");
            return;
        }
    };

    tracing::info!(%model, ?protocol, "pre-warming cleanup model");
    let started = Instant::now();

    let result = match protocol {
        Protocol::Ollama => {
            // /api/generate with num_predict=0 loads weights but skips decoding.
            // keep_alive pins the model in RAM for 10 minutes.
            client
                .post(format!("{base_url}/api/generate"))
                .json(&serde_json::json!({
                    "model": model,
                    "prompt": "",
                    "stream": false,
                    "keep_alive": "10m",
                    "options": { "num_predict": 0 }
                }))
                .send()
                .await
        }
        Protocol::OpenAiCompatible => {
            // LM Studio, llama.cpp server, Jan: send a 1-token completion to
            // force model load.
            client
                .post(format!("{base_url}/v1/chat/completions"))
                .json(&serde_json::json!({
                    "model": model,
                    "temperature": 0,
                    "max_tokens": 1,
                    "messages": [{ "role": "user", "content": "hi" }]
                }))
                .send()
                .await
        }
    };

    match result {
        Ok(response) if response.status().is_success() => {
            tracing::info!(
                %model,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "cleanup model warmed"
            );
        }
        Ok(response) => {
            tracing::debug!(%model, status = %response.status(), "cleanup warm-up returned a non-success status");
        }
        Err(err) => {
            tracing::debug!(%model, %err, "cleanup warm-up transport error");
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardReason {
    InputTooLong,
    MalformedResponse,
    Truncated,
    Empty,
    RunawayLength,
    AddedCommitment,
}

impl GuardReason {
    fn metric_detail(self) -> &'static str {
        match self {
            Self::InputTooLong => "inputTooLong",
            Self::MalformedResponse => "malformedResponse",
            Self::Truncated => "truncated",
            Self::Empty => "empty",
            Self::RunawayLength => "runawayLength",
            Self::AddedCommitment => "addedCommitment",
        }
    }
}

struct CleanOutcome {
    delivered: String,
    model_output: String,
    guard_reason: Option<GuardReason>,
}

struct ProviderOutput {
    text: String,
    truncated: bool,
}

fn guarded_outcome(
    transcript: &str,
    model_output: String,
    guard_reason: GuardReason,
) -> CleanOutcome {
    CleanOutcome {
        delivered: safe_fallback(transcript),
        model_output,
        guard_reason: Some(guard_reason),
    }
}

fn parse_ollama_output(payload: &serde_json::Value) -> Option<ProviderOutput> {
    let text = payload.get("response")?.as_str()?.trim().to_owned();
    let truncated = payload
        .get("done_reason")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|reason| reason.eq_ignore_ascii_case("length"));
    Some(ProviderOutput { text, truncated })
}

fn parse_openai_output(payload: &serde_json::Value) -> Option<ProviderOutput> {
    let choice = payload.get("choices")?.as_array()?.first()?;
    let text = choice
        .get("message")?
        .get("content")?
        .as_str()?
        .trim()
        .to_owned();
    let truncated = choice
        .get("finish_reason")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|reason| reason.eq_ignore_ascii_case("length"));
    Some(ProviderOutput { text, truncated })
}

fn apply_output_guards(transcript: &str, provider_output: ProviderOutput) -> CleanOutcome {
    let ProviderOutput {
        text: model_output,
        truncated,
    } = provider_output;
    let source_words = transcript.split_whitespace().count().max(1);
    let model_words = model_output.split_whitespace().count();
    let guard_reason = if truncated {
        Some(GuardReason::Truncated)
    } else if model_output.is_empty() {
        Some(GuardReason::Empty)
    } else if model_words > source_words * 3 + 8 {
        Some(GuardReason::RunawayLength)
    } else if introduces_commitment(transcript, &model_output) {
        Some(GuardReason::AddedCommitment)
    } else {
        None
    };
    let delivered = if guard_reason.is_some() {
        safe_fallback(transcript)
    } else {
        model_output.clone()
    };
    CleanOutcome {
        delivered,
        model_output,
        guard_reason,
    }
}

pub async fn clean(settings: &AppSettings, transcript: &str, register: Register) -> Result<String> {
    let outcome = clean_with_outcome(settings, transcript, register).await?;
    if let Some(reason) = outcome.guard_reason {
        let source_words = transcript.split_whitespace().count();
        let model_words = outcome.model_output.split_whitespace().count();
        tracing::warn!(
            source_words,
            model_words,
            guard_reason = reason.metric_detail(),
            "Scribe cleanup output failed a safety guard; using the safe local draft"
        );
        if let Err(error) = metrics::increment(
            "scribeCleanupSafetyFallback",
            Some("scribe"),
            Some(reason.metric_detail()),
        ) {
            tracing::warn!(%error, "could not record Scribe safety fallback metric");
        }
    }
    Ok(outcome.delivered)
}

async fn clean_with_outcome(
    settings: &AppSettings,
    transcript: &str,
    register: Register,
) -> Result<CleanOutcome> {
    if transcript.trim().is_empty() {
        return Ok(CleanOutcome {
            delivered: String::new(),
            model_output: String::new(),
            guard_reason: None,
        });
    }
    let request_prompt = prompt(transcript, register);
    if conservative_token_estimate(&request_prompt) > MAX_PROMPT_TOKEN_ESTIMATE {
        return Ok(guarded_outcome(
            transcript,
            String::new(),
            GuardReason::InputTooLong,
        ));
    }
    let (protocol, base_url) = resolve_endpoint(settings).await?;
    ensure_loopback(&base_url)?;
    // Cold-loading a 400 MB–2 GB local model into RAM can easily take 30–60s on
    // a slow disk, especially on the very first generate after the server
    // started. Warm requests remain fast; the bound only guards against a dead
    // server.
    let client = Client::builder()
        // System proxy env vars must NOT apply to loopback traffic. Reqwest
        // would otherwise try to route `POST /api/generate` through a
        // corporate/VPN proxy that has no route to 127.0.0.1, producing a
        // generic "error sending request" that looks like Ollama is down.
        .no_proxy()
        .timeout(Duration::from_secs(90))
        .build()?;
    let model = resolve_model(&client, settings, protocol, &base_url).await?;

    let provider_output = match protocol {
        Protocol::Ollama => {
            let response = client
                .post(format!("{base_url}/api/generate"))
                .json(&serde_json::json!({
                    "model": model,
                    "prompt": request_prompt,
                    "stream": false,
                    "keep_alive": "10m",
                    "options": {
                        "temperature": 0.0,
                        "num_ctx": MODEL_CONTEXT_TOKENS,
                        "num_predict": MAX_GENERATED_TOKENS
                    }
                }))
                .send()
                .await
                .map_err(|error| classify_transport_error(&base_url, &model, error))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(classify_http_error(&model, status, &body));
            }
            let payload: serde_json::Value = response.json().await?;
            parse_ollama_output(&payload)
        }
        Protocol::OpenAiCompatible => {
            // Unlike Ollama's explicit `num_ctx`, this protocol uses whatever
            // context window the user configured in LM Studio, Jan, or their
            // llama.cpp server. Quill still applies its conservative 2,048-
            // token input budget before sending; the remote loopback server
            // must be configured with at least that much context.
            let response = client
                .post(format!("{base_url}/v1/chat/completions"))
                .json(&serde_json::json!({
                    "model": model,
                    "temperature": 0,
                    "max_tokens": MAX_GENERATED_TOKENS,
                    "messages": [{ "role": "user", "content": request_prompt }]
                }))
                .send()
                .await
                .map_err(|error| classify_transport_error(&base_url, &model, error))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(classify_http_error(&model, status, &body));
            }
            let payload: serde_json::Value = response.json().await?;
            parse_openai_output(&payload)
        }
    };

    let Some(provider_output) = provider_output else {
        return Ok(guarded_outcome(
            transcript,
            String::new(),
            GuardReason::MalformedResponse,
        ));
    };
    // Lightweight sanity guards, replacing the old strict word-provenance
    // check which blocked legitimate rewrites like "hei" → "Hey". Reject
    // newly introduced commitment/proposal concepts as well as pathological
    // empty or runaway outputs. Never log either body: both contain user text.
    Ok(apply_output_guards(transcript, provider_output))
}

fn classify_transport_error(base_url: &str, model: &str, error: reqwest::Error) -> anyhow::Error {
    if error.is_timeout() {
        anyhow!(
            "Ollama is taking too long to respond. It may still be loading the '{model}' model into memory — try again in a moment."
        )
    } else if error.is_connect() {
        anyhow!(
            "Can't reach a local LLM at {base_url}. Ollama isn't running, or a proxy is blocking loopback traffic. Start Ollama and try again."
        )
    } else {
        anyhow!("Local cleanup request failed: {error}")
    }
}

fn classify_http_error(model: &str, status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let lower = body.to_lowercase();
    if status == reqwest::StatusCode::NOT_FOUND
        || lower.contains("not found")
        || lower.contains("no such")
    {
        anyhow!(
            "Model '{model}' isn't installed in Ollama. Install it from Voice → Install a recommended cleanup model, or run `ollama pull {model}` in a terminal."
        )
    } else {
        // The provider body is untrusted and can echo the request prompt, which
        // contains the transcript. Keep it out of errors because callers persist
        // the error class in tracing and metrics.
        anyhow!("Local LLM returned {status}")
    }
}

fn ensure_loopback(base_url: &str) -> Result<()> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|error| anyhow!("invalid cleanup server URL: {error}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("cleanup server URL has no host"))?;
    if !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        return Err(anyhow!(
            "Scribe cleanup is restricted to a loopback server; rejected host: {host}"
        ));
    }
    Ok(())
}

async fn resolve_model(
    client: &Client,
    settings: &AppSettings,
    protocol: Protocol,
    base_url: &str,
) -> Result<String> {
    if !settings.cleanup_model.trim().is_empty() {
        return Ok(settings.cleanup_model.clone());
    }
    match protocol {
        Protocol::Ollama => {
            let tags = client
                .get(format!("{base_url}/api/tags"))
                .send()
                .await?
                .error_for_status()?
                .json::<OllamaTags>()
                .await?;
            select_ollama_model(tags.models)
                .ok_or_else(|| anyhow!("Ollama is running but has no local cleanup model"))
        }
        Protocol::OpenAiCompatible => {
            let models = client
                .get(format!("{base_url}/v1/models"))
                .send()
                .await?
                .error_for_status()?
                .json::<OpenAiModels>()
                .await?;
            models
                .data
                .into_iter()
                .map(|model| model.id)
                .next()
                .ok_or_else(|| anyhow!("local cleanup server has no loaded model"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVAL_REGISTERS: [Register; 4] = [
        Register::Email,
        Register::Chat,
        Register::Prompt,
        Register::Notes,
    ];

    const EVAL_UTTERANCES: [&str; 20] = [
        "schedule it Tuesday, no wait, Thursday",
        "move the review to Tuesday, no wait, Thursday, so you can meet Tuesday",
        "can't make Tuesday, conflict",
        "please could you add retries to fetchUser without changing the TypeScript API",
        "send the invoice for 480 dollars to Priya on August 14",
        "the release is version 2.7.1 and the URL is https://quill.dev/releases",
        "um tell Marcus the deployment starts at 4:30 PM",
        "book room Cedar for twelve people, actually make that fourteen",
        "I think the price is 89 dollars, sorry, 98 dollars",
        "draft the migration steps first back up Postgres then run the schema update",
        "hey team the build is green and staging is ready",
        "remind me to call Anika after the design review",
        "use QueryFullProcessImageNameW and preserve the Windows API name exactly",
        "the customer said no, rather, they asked for a revised quote",
        "we shipped the mac build, scratch that, the Windows build",
        "one two three five, no, four and then five",
        "can you summarize this without adding recommendations",
        "meeting notes colon latency improved by twelve percent next check memory use",
        "email support at help@example.com with ticket 1842",
        "uh the deadline is Friday and there is no extension",
    ];

    #[test]
    fn fallback_resolves_single_item_spoken_corrections() {
        assert_eq!(
            resolve_simple_corrections("write 1, 2, 3, 5, No, 4 and then 5"),
            "write 1, 2, 3, 4 and then 5"
        );
        assert_eq!(
            resolve_simple_corrections("send it Tuesday no wait Wednesday"),
            "send it Wednesday"
        );
    }

    #[test]
    fn normal_prompts_keep_correction_language_for_the_model() {
        let transcript = "schedule it Tuesday, no wait, Thursday";
        for register in [
            Register::Email,
            Register::Chat,
            Register::Prompt,
            Register::Notes,
            Register::Generic,
        ] {
            let request = prompt(transcript, register);
            assert!(request.ends_with(transcript));
            assert!(request.contains("Tuesday, no wait, Thursday"));
        }
    }

    #[test]
    fn malformed_provider_payloads_never_substitute_the_transcript() {
        assert!(parse_ollama_output(&serde_json::json!({
            "response": null,
            "done_reason": "stop"
        }))
        .is_none());
        assert!(parse_ollama_output(&serde_json::json!({
            "done_reason": "stop"
        }))
        .is_none());
        assert!(parse_openai_output(&serde_json::json!({
            "choices": [{
                "message": { "content": null },
                "finish_reason": "stop"
            }]
        }))
        .is_none());
        assert!(parse_openai_output(&serde_json::json!({ "choices": [] })).is_none());
    }

    #[test]
    fn provider_error_bodies_are_not_persisted_in_errors() {
        let echoed_transcript = "private transcript text";
        let error = classify_http_error(
            "local-model",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            echoed_transcript,
        );
        let message = error.to_string();
        assert_eq!(message, "Local LLM returned 500 Internal Server Error");
        assert!(!message.contains(echoed_transcript));
    }

    #[test]
    fn provider_length_reasons_force_a_truncated_fallback() {
        let ollama = parse_ollama_output(&serde_json::json!({
            "response": "This draft ends halfway through a",
            "done_reason": "length"
        }))
        .unwrap();
        let outcome = apply_output_guards("this is the source", ollama);
        assert_eq!(outcome.guard_reason, Some(GuardReason::Truncated));
        assert_eq!(outcome.delivered, safe_fallback("this is the source"));

        let openai = parse_openai_output(&serde_json::json!({
            "choices": [{
                "message": { "content": "This also ends halfway" },
                "finish_reason": "length"
            }]
        }))
        .unwrap();
        assert!(openai.truncated);
    }

    #[test]
    fn normal_provider_finish_reasons_preserve_the_model_draft() {
        let output = parse_ollama_output(&serde_json::json!({
            "response": "A complete cleaned draft.",
            "done_reason": "stop"
        }))
        .unwrap();
        let outcome = apply_output_guards("a complete draft", output);
        assert_eq!(outcome.guard_reason, None);
        assert_eq!(outcome.delivered, "A complete cleaned draft.");
    }

    #[tokio::test]
    async fn oversized_input_falls_back_before_provider_discovery() {
        let transcript = "dictation ".repeat(MAX_PROMPT_TOKEN_ESTIMATE);
        let outcome = clean_with_outcome(&AppSettings::default(), &transcript, Register::Notes)
            .await
            .unwrap();
        assert_eq!(outcome.guard_reason, Some(GuardReason::InputTooLong));
        assert!(outcome.model_output.is_empty());
        assert_eq!(outcome.delivered, safe_fallback(&transcript));
    }

    #[test]
    fn ordinary_prompts_fit_inside_the_reserved_context_budget() {
        let request = prompt(
            "draft a short update about the Thursday release",
            Register::Email,
        );
        assert!(conservative_token_estimate(&request) <= MAX_PROMPT_TOKEN_ESTIMATE);
    }

    #[test]
    fn non_ascii_estimates_use_the_utf8_byte_upper_bound() {
        let devanagari = "नमस्ते दुनिया";
        let non_whitespace_bytes = devanagari
            .chars()
            .filter(|character| !character.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        assert_eq!(
            conservative_token_estimate(devanagari),
            non_whitespace_bytes
        );

        let emoji = "👩‍💻";
        assert_eq!(conservative_token_estimate(emoji), emoji.len());
    }

    #[test]
    fn every_register_has_the_shared_fact_rule_and_specific_target() {
        for register in [
            Register::Email,
            Register::Chat,
            Register::Prompt,
            Register::Notes,
            Register::Generic,
        ] {
            let request = prompt("test dictation", register);
            assert!(request.contains(SHARED_PREAMBLE));
            assert!(request.contains(register_instructions(register)));
        }
        let prompt_request = prompt("please explain this", Register::Prompt);
        assert!(prompt_request.contains("Remove politeness"));
        assert!(prompt_request.contains("no markdown formatting"));
    }

    #[test]
    fn commitment_guard_rejects_new_promises_and_proposals() {
        assert!(introduces_commitment(
            "can't make Tuesday, conflict",
            "I'm sorry, I can't make Tuesday due to a conflict. Let's find another time."
        ));
        assert!(introduces_commitment(
            "can't make Tuesday, conflict",
            "I can't make Tuesday, but I'm flexible on timing."
        ));
        assert!(introduces_commitment(
            "the build failed",
            "The build failed. I'll follow up tomorrow."
        ));
        assert!(introduces_commitment(
            "Thursday does not work",
            "Thursday doesn't work. Please let me know another time."
        ));
    }

    #[test]
    fn commitment_guard_allows_language_already_present_in_the_source() {
        assert!(!introduces_commitment(
            "I'm flexible on timing",
            "I am flexible on timing."
        ));
        assert!(!introduces_commitment(
            "I'll follow up tomorrow",
            "I will follow up tomorrow."
        ));
        assert!(!introduces_commitment(
            "let's find another time",
            "Let's find another time."
        ));
        assert!(!introduces_commitment(
            "can't make Tuesday, conflict",
            "I'm sorry, I cannot make Tuesday due to a conflict."
        ));
        assert!(!introduces_commitment(
            "we should ship it tomorrow",
            "Let's ship it tomorrow."
        ));
        assert!(!introduces_commitment(
            "tell me when you're free",
            "Let me know when you're free."
        ));
    }

    #[test]
    fn automatic_ollama_selection_prefers_a_compose_capable_model() {
        let selected = select_ollama_model(vec![
            OllamaModel {
                name: "llama3.1:8b".into(),
            },
            OllamaModel {
                name: "qwen2.5:3b".into(),
            },
            OllamaModel {
                name: "qwen2.5:7b".into(),
            },
        ]);
        assert_eq!(selected.as_deref(), Some("qwen2.5:7b"));

        let fallback = select_ollama_model(vec![OllamaModel {
            name: "qwen2.5:3b".into(),
        }]);
        assert_eq!(fallback.as_deref(), Some("qwen2.5:3b"));
    }

    #[test]
    fn fixed_twenty_by_four_prompt_matrix_is_complete_and_raw() {
        assert_eq!(EVAL_UTTERANCES.len(), 20);
        assert_eq!(EVAL_REGISTERS.len(), 4);
        let mut prompts = 0;
        for utterance in EVAL_UTTERANCES {
            for register in EVAL_REGISTERS {
                let request = prompt(utterance, register);
                assert!(request.ends_with(utterance));
                assert!(request.contains(register_instructions(register)));
                prompts += 1;
            }
        }
        assert_eq!(prompts, 80);
    }

    /// Opt-in semantic evaluation against the configured local model. This is
    /// intentionally ignored in ordinary CI: it makes 80 local LLM requests.
    /// All 80 responses must be non-empty and must pass without a safety
    /// fallback; cases 0–3 add ten targeted semantic assertions. The assertions
    /// inspect the raw pre-guard model output, so the guard cannot conceal a
    /// model regression. Run it before tuning register prompts with:
    /// `cargo test register_model_eval_matrix -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "requires a running local cleanup model and makes 80 requests"]
    async fn register_model_eval_matrix() {
        let settings = AppSettings::default();
        for (index, utterance) in EVAL_UTTERANCES.iter().enumerate() {
            for register in EVAL_REGISTERS {
                let outcome = clean_with_outcome(&settings, utterance, register)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("cleanup failed for case {index} in {register:?}: {error}")
                    });
                println!(
                    "case {index:02} {register:?}: guard={:?}",
                    outcome.guard_reason
                );
                assert!(
                    !outcome.model_output.trim().is_empty(),
                    "case {index} in {register:?} returned an empty model response"
                );
                assert_eq!(
                    outcome.guard_reason, None,
                    "case {index} in {register:?} silently degraded to the fallback draft"
                );
                let output = outcome.model_output;
                let lower = output.to_lowercase();

                if index == 0 {
                    assert!(lower.contains("thursday"));
                    assert!(!lower.contains("tuesday"));
                }
                if index == 1 {
                    let review = lower
                        .find("review")
                        .unwrap_or_else(|| panic!("case 1 lost the review subject: {output}"));
                    assert!(lower[review..].contains("thursday"));
                }
                if index == 2 && register == Register::Email {
                    assert!(!lower.contains("flexible"));
                    assert!(!lower.contains("another time"));
                }
                if index == 3 && register == Register::Prompt {
                    assert!(!lower.contains("please"));
                    assert!(!lower.contains("could you"));
                    assert!(lower.contains("fetchuser"));
                    assert!(output.contains("TypeScript"));
                }
            }
        }
    }

    #[test]
    fn rejects_non_loopback_cleanup_hosts() {
        assert!(ensure_loopback("https://api.example.com").is_err());
        assert!(ensure_loopback("http://127.0.0.1:11434").is_ok());
        assert!(ensure_loopback("http://localhost:1234").is_ok());
    }

    #[test]
    fn safe_fallback_keeps_spoken_content_and_resolves_corrections() {
        assert_eq!(
            safe_fallback("um write one two three five no wait four and five"),
            "Write one two three four and five"
        );
    }
}
