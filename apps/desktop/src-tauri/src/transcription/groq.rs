use crate::asr::{dictionary_prompt, AsrPass};
use crate::audio::pcm16_wav;
use crate::credentials::{get_key, CloudProvider};
use crate::metrics;
use crate::model::AppSettings;
use crate::streaming::TimedWord;
use anyhow::{anyhow, Result};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::{Duration, Instant};

const ENDPOINT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const MODEL: &str = "whisper-large-v3";
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const GROQ_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

fn ensure_upload_fits(byte_len: usize) -> Result<()> {
    if byte_len > MAX_UPLOAD_BYTES {
        return Err(anyhow!(
            "This recording is too long for Groq's 25 MB upload limit. The recovery audio was kept; shorten it or transcribe it locally."
        ));
    }
    Ok(())
}

pub struct GroqTranscriber {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct GroqResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<GroqWord>,
}

#[derive(Debug, Deserialize)]
struct GroqWord {
    word: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
}

impl GroqTranscriber {
    pub fn new() -> Result<Self> {
        if get_key(CloudProvider::Groq)
            .map_err(|error| anyhow!(error))?
            .is_none()
        {
            return Err(anyhow!(
                "Connect a Groq API key in Voice settings before selecting Groq transcription."
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
            return Ok(AsrPass {
                words: Vec::new(),
                text: String::new(),
                latency_ms: 0,
                timings_synthetic: false,
            });
        }
        let duration_ms = samples.len() as u64 * 1_000 / crate::audio::WHISPER_SAMPLE_RATE as u64;
        let bucket = metrics::audio_duration_bucket(duration_ms);
        let encode_started = Instant::now();
        let wav = pcm16_wav(samples);
        record_stage(
            "wavEncodeMs",
            encode_started.elapsed().as_millis(),
            mode,
            "success",
            bucket,
        );
        ensure_upload_fits(wav.len())?;
        let key = get_key(CloudProvider::Groq)
            .map_err(|error| anyhow!(error))?
            .ok_or_else(|| {
                anyhow!("The saved Groq API key is missing. Reconnect it in Voice settings.")
            })?;
        let (prompt, truncated) = dictionary_prompt(&settings.dictionary);
        if truncated {
            tracing::warn!(
                word_entries = settings.dictionary.len(),
                "dictionary bias prompt exceeded its budget; remaining entries were omitted"
            );
        }
        let part = Part::bytes(wav)
            .file_name("quill-recording.wav")
            .mime_str("audio/wav")?;
        let mut form = Form::new()
            .part("file", part)
            .text("model", MODEL)
            .text("response_format", "verbose_json")
            .text("timestamp_granularities[]", "word")
            .text("temperature", "0");
        if settings.language != "auto" && !settings.language.trim().is_empty() {
            form = form.text("language", settings.language.clone());
        }
        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }
        let started = Instant::now();
        let response = match self
            .client
            .post(ENDPOINT)
            .bearer_auth(key)
            .multipart(form)
            .timeout(GROQ_REQUEST_TIMEOUT)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                record_stage(
                    "groqRequestMs",
                    started.elapsed().as_millis(),
                    mode,
                    transport_outcome(&error),
                    bucket,
                );
                return Err(classify_transport_error(error));
            }
        };
        let status = response.status();
        if !status.is_success() {
            record_stage(
                "groqRequestMs",
                started.elapsed().as_millis(),
                mode,
                http_outcome(status),
                bucket,
            );
            return Err(classify_http_error(status));
        }
        let payload: GroqResponse = match response.json().await {
            Ok(payload) => payload,
            Err(_) => {
                record_stage(
                    "groqRequestMs",
                    started.elapsed().as_millis(),
                    mode,
                    "providerFailure",
                    bucket,
                );
                return Err(anyhow!(
                    "Groq returned a malformed transcription response. The recovery audio was kept."
                ));
            }
        };
        let request_ms = started.elapsed().as_millis();
        record_stage("groqRequestMs", request_ms, mode, "success", bucket);
        let text = payload.text.trim().to_owned();
        let (words, timings_synthetic) = words_or_synthetic(payload.words, &text, samples.len());
        Ok(AsrPass {
            words,
            text,
            latency_ms: request_ms,
            timings_synthetic,
        })
    }
}

fn record_stage(metric: &str, value_ms: u128, mode: &str, outcome: &str, bucket: &str) {
    if let Err(error) =
        metrics::record_cloud_stage(metric, value_ms, "groq", mode, outcome, Some(bucket))
    {
        tracing::warn!(%error, metric, "could not record cloud latency metric");
    }
}

fn transport_outcome(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else {
        "transportFailure"
    }
}

fn http_outcome(status: reqwest::StatusCode) -> &'static str {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        "quota"
    } else {
        "providerFailure"
    }
}

fn words_or_synthetic(
    words: Vec<GroqWord>,
    text: &str,
    sample_count: usize,
) -> (Vec<TimedWord>, bool) {
    if !words.is_empty() {
        return (
            words
                .into_iter()
                .map(|word| TimedWord {
                    text: word.word,
                    start_ms: (word.start.max(0.0) * 1_000.0) as u64,
                    end_ms: (word.end.max(0.0) * 1_000.0) as u64,
                })
                .collect(),
            false,
        );
    }
    let pieces = text.split_whitespace().collect::<Vec<_>>();
    if pieces.is_empty() {
        return (Vec::new(), false);
    }
    let duration_ms = sample_count as u64 * 1_000 / crate::audio::WHISPER_SAMPLE_RATE as u64;
    let count = pieces.len() as u64;
    (
        pieces
            .into_iter()
            .enumerate()
            .map(|(index, word)| TimedWord {
                text: word.to_owned(),
                start_ms: duration_ms * index as u64 / count,
                end_ms: duration_ms * (index as u64 + 1) / count,
            })
            .collect(),
        true,
    )
}

fn classify_transport_error(error: reqwest::Error) -> anyhow::Error {
    if error.is_timeout() {
        anyhow!("Groq transcription timed out. The recovery audio was kept so you can retry.")
    } else if error.is_connect() {
        anyhow!("Groq could not be reached. Check your connection; the recovery audio was kept.")
    } else {
        anyhow!("Groq transcription failed before a response was received. The recovery audio was kept.")
    }
}

fn classify_http_error(status: reqwest::StatusCode) -> anyhow::Error {
    match status.as_u16() {
        401 | 403 => anyhow!("Groq rejected the saved API key. Reconnect it in Voice settings."),
        413 => anyhow!("The recording is too large for Groq. The recovery audio was kept."),
        429 => anyhow!("Groq quota is exhausted. Try again later or transcribe locally."),
        code if code >= 500 => anyhow!("Groq is temporarily unavailable. The recovery audio was kept."),
        _ => anyhow!("Groq returned {status}. The recovery audio was kept and no response content was stored."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_word_timestamps_are_synthesized() {
        let (words, synthetic) = words_or_synthetic(Vec::new(), "hello world", 16_000);
        assert!(synthetic);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].start_ms, 0);
        assert_eq!(words[1].end_ms, 1_000);
    }

    #[test]
    fn provider_word_timestamps_are_preserved() {
        let (words, synthetic) = words_or_synthetic(
            vec![GroqWord {
                word: "hello".into(),
                start: 0.2,
                end: 0.7,
            }],
            "hello",
            16_000,
        );
        assert!(!synthetic);
        assert_eq!((words[0].start_ms, words[0].end_ms), (200, 700));
    }

    #[test]
    fn groq_upload_limit_is_rejected_before_request_construction() {
        assert!(ensure_upload_fits(MAX_UPLOAD_BYTES).is_ok());
        let error = ensure_upload_fits(MAX_UPLOAD_BYTES + 1)
            .unwrap_err()
            .to_string();
        assert!(error.contains("25 MB"));
        assert!(error.contains("transcribe it locally"));
    }

    #[test]
    fn groq_http_metrics_distinguish_quota_from_provider_failures() {
        assert_eq!(
            http_outcome(reqwest::StatusCode::TOO_MANY_REQUESTS),
            "quota"
        );
        assert_eq!(
            http_outcome(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            "providerFailure"
        );
    }

    #[test]
    fn groq_uses_the_accuracy_focused_whisper_model() {
        assert_eq!(MODEL, "whisper-large-v3");
    }
}
