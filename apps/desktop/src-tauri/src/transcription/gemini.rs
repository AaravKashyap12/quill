use crate::asr::AsrPass;
use crate::audio::pcm16_wav;
use crate::credentials::{get_key, CloudProvider};
use crate::metrics;
use crate::model::{AppSettings, DictionaryKind};
use crate::streaming::TimedWord;
use crate::transcription::speech::detect_speech_window;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const MODEL: &str = "gemini-3.5-transcribe";
const UPLOAD_START_URL: &str = "https://generativelanguage.googleapis.com/upload/v1beta/files";
const INTERACTIONS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";
const FILES_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const GEMINI_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_UPLOAD_BYTES: usize = 125 * 1024 * 1024;
const MAX_VOCABULARY_TERMS: usize = 100;

pub struct GeminiTranscriber {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct UploadedFileResponse {
    file: UploadedFile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadedFile {
    name: String,
    uri: String,
    mime_type: String,
}

impl GeminiTranscriber {
    pub fn new() -> Result<Self> {
        if get_key(CloudProvider::Gemini)
            .map_err(|error| anyhow!(error))?
            .is_none()
        {
            return Err(anyhow!(
                "Connect a Gemini API key in Voice settings before selecting Gemini transcription."
            ));
        }
        Ok(Self {
            client: crate::credentials::cloud_client().map_err(|error| anyhow!(error))?,
        })
    }

    pub async fn transcribe(
        &self,
        settings: &AppSettings,
        samples: &[f32],
        mode: &str,
    ) -> Result<AsrPass> {
        if samples.is_empty() {
            return Ok(empty_pass());
        }
        let duration_ms = samples.len() as u64 * 1_000 / crate::audio::WHISPER_SAMPLE_RATE as u64;
        let bucket = metrics::audio_duration_bucket(duration_ms);
        let Some(speech) = detect_speech_window(samples) else {
            return Ok(empty_pass());
        };
        let speech_samples = speech.samples(samples);
        let wav = pcm16_wav(speech_samples);
        if wav.len() > MAX_UPLOAD_BYTES {
            return Err(anyhow!(
                "This recording is too long for Gemini transcription. The recovery audio was kept; shorten it or transcribe it locally."
            ));
        }
        let key = get_key(CloudProvider::Gemini)
            .map_err(|error| anyhow!(error))?
            .ok_or_else(|| {
                anyhow!("The saved Gemini API key is missing. Reconnect it in Voice settings.")
            })?;
        let started = Instant::now();
        let uploaded = match self.upload_audio(&key, wav).await {
            Ok(file) => file,
            Err(error) => {
                record_stage(
                    started.elapsed().as_millis(),
                    mode,
                    error_outcome(&error),
                    bucket,
                );
                return Err(error);
            }
        };

        let result = self
            .transcribe_uploaded(&key, &uploaded, settings, mode)
            .await;
        let delete_client = self.client.clone();
        let delete_key = key;
        let delete_name = uploaded.name.clone();
        tauri::async_runtime::spawn(async move {
            if Self::delete_uploaded_file(&delete_client, &delete_key, &delete_name)
                .await
                .is_err()
            {
                // File identifiers can be user-correlated provider data, so
                // keep this warning content-free. Google automatically removes
                // Files API uploads after 48 hours if immediate deletion fails.
                tracing::warn!("Gemini audio upload could not be deleted immediately");
            }
        });
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                record_stage(
                    started.elapsed().as_millis(),
                    mode,
                    error_outcome(&error),
                    bucket,
                );
                return Err(error);
            }
        };
        let request_ms = started.elapsed().as_millis();
        record_stage(request_ms, mode, "success", bucket);
        let mut words = synthetic_words(&text, speech_samples.len());
        let offset_ms = speech.start_ms();
        for word in &mut words {
            word.start_ms = word.start_ms.saturating_add(offset_ms);
            word.end_ms = word.end_ms.saturating_add(offset_ms);
        }
        Ok(AsrPass {
            text,
            words,
            latency_ms: request_ms,
            timings_synthetic: true,
        })
    }

    async fn upload_audio(&self, key: &str, wav: Vec<u8>) -> Result<UploadedFile> {
        let started = self
            .client
            .post(UPLOAD_START_URL)
            .header("x-goog-api-key", key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", wav.len())
            .header("X-Goog-Upload-Header-Content-Type", "audio/wav")
            .json(&json!({"file": {"display_name": "quill-recording.wav"}}))
            .timeout(GEMINI_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(classify_transport_error)?;
        if !started.status().is_success() {
            return Err(classify_http_error(started.status()));
        }
        let upload_url = started
            .headers()
            .get("x-goog-upload-url")
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.starts_with("https://"))
            .ok_or_else(|| {
                anyhow!("Gemini did not provide a valid audio upload destination. The recovery audio was kept.")
            })?
            .to_owned();
        let uploaded = self
            .client
            .post(upload_url)
            .header("Content-Length", wav.len())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .header("Content-Type", "audio/wav")
            .body(wav)
            .timeout(GEMINI_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(classify_transport_error)?;
        if !uploaded.status().is_success() {
            return Err(classify_http_error(uploaded.status()));
        }
        uploaded
            .json::<UploadedFileResponse>()
            .await
            .context("Gemini returned a malformed audio upload response")
            .map(|response| response.file)
    }

    async fn transcribe_uploaded(
        &self,
        key: &str,
        file: &UploadedFile,
        settings: &AppSettings,
        mode: &str,
    ) -> Result<String> {
        let body = interaction_body(file, settings, mode);
        let response = self
            .client
            .post(INTERACTIONS_URL)
            .header("x-goog-api-key", key)
            .json(&body)
            .timeout(GEMINI_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(classify_transport_error)?;
        if !response.status().is_success() {
            return Err(classify_http_error(response.status()));
        }
        let payload = response.json::<Value>().await.map_err(|_| {
            anyhow!(
                "Gemini returned a malformed transcription response. The recovery audio was kept."
            )
        })?;
        extract_output_text(&payload).ok_or_else(|| {
            anyhow!("Gemini returned no transcription text. The recovery audio was kept.")
        })
    }

    async fn delete_uploaded_file(client: &reqwest::Client, key: &str, name: &str) -> Result<()> {
        if !name.starts_with("files/") {
            return Err(anyhow!(
                "Gemini returned an invalid uploaded-file identifier"
            ));
        }
        let response = client
            .delete(format!("{FILES_BASE_URL}/{name}"))
            .header("x-goog-api-key", key)
            .timeout(Duration::from_secs(12))
            .send()
            .await
            .map_err(classify_transport_error)?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(anyhow!("Gemini did not confirm uploaded-file deletion"))
        }
    }
}

fn interaction_body(file: &UploadedFile, settings: &AppSettings, mode: &str) -> Value {
    let (vocabulary, truncated) = custom_vocabulary(settings);
    if truncated {
        tracing::warn!(
            word_entries = settings
                .dictionary
                .iter()
                .filter(|entry| entry.kind == DictionaryKind::Word)
                .count(),
            "Gemini custom vocabulary exceeded 100 terms; remaining entries were omitted"
        );
    }
    let mut config = json!({
        "language_codes": [],
        "mode": {"type": if mode == "scribe" { "smart" } else { "verbatim" }}
    });
    if !vocabulary.is_empty() {
        config["custom_vocabulary"] = json!(vocabulary);
    }
    json!({
        "model": MODEL,
        "input": [{
            "type": "audio",
            "uri": file.uri,
            "mime_type": file.mime_type
        }],
        "store": false,
        "generation_config": {"transcription_config": config}
    })
}

fn custom_vocabulary(settings: &AppSettings) -> (Vec<String>, bool) {
    let mut terms = Vec::new();
    let mut truncated = false;
    for replacement in settings
        .dictionary
        .iter()
        .filter(|entry| entry.kind == DictionaryKind::Word)
        .map(|entry| entry.replacement.trim())
        .filter(|replacement| !replacement.is_empty())
    {
        if terms
            .iter()
            .any(|term: &String| term.eq_ignore_ascii_case(replacement))
        {
            continue;
        }
        if terms.len() == MAX_VOCABULARY_TERMS {
            truncated = true;
            break;
        }
        terms.push(replacement.to_owned());
    }
    (terms, truncated)
}

fn extract_output_text(payload: &Value) -> Option<String> {
    payload
        .get("output_text")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("outputs")
                .and_then(Value::as_array)
                .and_then(|outputs| {
                    outputs.iter().rev().find_map(|output| {
                        (output.get("type").and_then(Value::as_str) == Some("text"))
                            .then(|| output.get("text").and_then(Value::as_str))
                            .flatten()
                    })
                })
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn synthetic_words(text: &str, sample_count: usize) -> Vec<TimedWord> {
    let pieces = text.split_whitespace().collect::<Vec<_>>();
    let count = pieces.len() as u64;
    if count == 0 {
        return Vec::new();
    }
    let duration_ms = sample_count as u64 * 1_000 / crate::audio::WHISPER_SAMPLE_RATE as u64;
    pieces
        .into_iter()
        .enumerate()
        .map(|(index, word)| TimedWord {
            text: word.to_owned(),
            start_ms: duration_ms * index as u64 / count,
            end_ms: duration_ms * (index as u64 + 1) / count,
        })
        .collect()
}

fn empty_pass() -> AsrPass {
    AsrPass {
        words: Vec::new(),
        text: String::new(),
        latency_ms: 0,
        timings_synthetic: false,
    }
}

fn record_stage(value_ms: u128, mode: &str, outcome: &str, bucket: &str) {
    if let Err(error) = metrics::record_cloud_stage(
        "geminiTranscribeRequestMs",
        value_ms,
        "gemini-transcribe",
        mode,
        outcome,
        Some(bucket),
    ) {
        tracing::warn!(%error, "could not record Gemini transcription latency metric");
    }
}

fn error_outcome(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("timed out") {
        "timeout"
    } else if message.contains("quota") {
        "quota"
    } else if message.contains("reached") {
        "transportFailure"
    } else {
        "providerFailure"
    }
}

fn classify_transport_error(error: reqwest::Error) -> anyhow::Error {
    if error.is_timeout() {
        anyhow!("Gemini transcription timed out. The recovery audio was kept so you can retry.")
    } else if error.is_connect() {
        anyhow!("Gemini could not be reached. Check your connection; the recovery audio was kept.")
    } else {
        anyhow!("Gemini transcription failed before a response was received. The recovery audio was kept.")
    }
}

fn classify_http_error(status: reqwest::StatusCode) -> anyhow::Error {
    match status.as_u16() {
        401 | 403 => anyhow!("Gemini rejected the saved API key. Reconnect it in Voice settings."),
        413 => anyhow!("The recording is too large for Gemini. The recovery audio was kept."),
        429 => anyhow!("Gemini transcription quota is exhausted. Try again later or transcribe locally."),
        code if code >= 500 => anyhow!("Gemini transcription is temporarily unavailable. The recovery audio was kept."),
        _ => anyhow!("Gemini transcription returned {status}. The recovery audio was kept and no response content was stored."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DictionaryEntry, DictionaryKind};

    fn file() -> UploadedFile {
        UploadedFile {
            name: "files/test".into(),
            uri: "https://example.invalid/file".into(),
            mime_type: "audio/wav".into(),
        }
    }

    #[test]
    fn dictation_is_verbatim_and_scribe_is_smart() {
        let settings = AppSettings::default();
        let dictation = interaction_body(&file(), &settings, "dictation");
        let scribe = interaction_body(&file(), &settings, "scribe");
        assert_eq!(
            dictation["generation_config"]["transcription_config"]["mode"]["type"],
            "verbatim"
        );
        assert_eq!(
            scribe["generation_config"]["transcription_config"]["mode"]["type"],
            "smart"
        );
        assert_eq!(dictation["store"], false);
    }

    #[test]
    fn custom_vocabulary_uses_only_unique_word_entries() {
        let settings = AppSettings {
            dictionary: vec![
                DictionaryEntry {
                    id: "1".into(),
                    spoken: "tory".into(),
                    replacement: "Tauri".into(),
                    kind: DictionaryKind::Word,
                },
                DictionaryEntry {
                    id: "2".into(),
                    spoken: "my email".into(),
                    replacement: "aarav@example.com".into(),
                    kind: DictionaryKind::Snippet,
                },
                DictionaryEntry {
                    id: "3".into(),
                    spoken: "tauri".into(),
                    replacement: "tauri".into(),
                    kind: DictionaryKind::Word,
                },
            ],
            ..AppSettings::default()
        };
        let (terms, truncated) = custom_vocabulary(&settings);
        assert_eq!(terms, vec!["Tauri"]);
        assert!(!truncated);
    }

    #[test]
    fn rest_output_text_is_extracted_without_fallback_to_input() {
        assert_eq!(
            extract_output_text(&json!({"outputs": [{"type": "text", "text": " hello "}]})),
            Some("hello".into())
        );
        assert_eq!(extract_output_text(&json!({"outputs": []})), None);
    }

    #[test]
    fn gemini_uses_the_dedicated_transcription_model() {
        assert_eq!(MODEL, "gemini-3.5-transcribe");
    }
}
