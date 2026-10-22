use crate::asr::{dictionary_prompt, AsrPass};
use crate::audio::pcm16_wav;
use crate::credentials::{get_key, CloudProvider};
use crate::metrics;
use crate::model::AppSettings;
use crate::streaming::TimedWord;
use crate::transcription::speech::{detect_speech_window, SpeechWindow};
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
    words: Vec<GroqWord>,
    #[serde(default)]
    segments: Vec<GroqSegment>,
}

#[derive(Debug, Deserialize)]
struct GroqWord {
    word: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
}

#[derive(Debug, Deserialize)]
struct GroqSegment {
    #[serde(default)]
    text: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    #[serde(default)]
    avg_logprob: f64,
    #[serde(default)]
    compression_ratio: f64,
    #[serde(default)]
    no_speech_prob: f64,
}

impl GroqTranscriber {
    pub async fn new() -> Result<Self> {
        if get_key(CloudProvider::Groq)
            .await
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
        let Some(speech) = detect_speech_window(samples) else {
            return Ok(AsrPass {
                words: Vec::new(),
                text: String::new(),
                latency_ms: 0,
                timings_synthetic: false,
            });
        };
        let speech_samples = speech.samples(samples);
        let encode_started = Instant::now();
        let wav = pcm16_wav(speech_samples);
        record_stage(
            "wavEncodeMs",
            encode_started.elapsed().as_millis(),
            mode,
            "success",
            bucket,
        );
        ensure_upload_fits(wav.len())?;
        let key = get_key(CloudProvider::Groq)
            .await
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
            .text("timestamp_granularities[]", "segment")
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
        let (text, accepted_ranges) = accepted_text_and_ranges(&payload, &speech);
        let words = retain_words_in_ranges(payload.words, accepted_ranges.as_deref());
        let (mut words, timings_synthetic) = words_or_synthetic(words, &text, speech_samples.len());
        let offset_ms = speech.start_ms();
        for word in &mut words {
            word.start_ms = word.start_ms.saturating_add(offset_ms);
            word.end_ms = word.end_ms.saturating_add(offset_ms);
        }
        Ok(AsrPass {
            words,
            text,
            latency_ms: request_ms,
            timings_synthetic,
        })
    }
}

fn segment_is_trustworthy(segment: &GroqSegment) -> bool {
    if segment.text.trim().is_empty() {
        return false;
    }
    if segment.no_speech_prob >= 0.85 || segment.avg_logprob <= -1.0 {
        return false;
    }
    if segment.no_speech_prob >= 0.6 && segment.avg_logprob <= -0.5 {
        return false;
    }
    !(segment.compression_ratio >= 2.4 && segment.avg_logprob <= -0.3)
}

fn segment_is_trustworthy_with_audio(segment: &GroqSegment, speech: &SpeechWindow) -> bool {
    segment_is_trustworthy(segment) && speech.supports_interval(segment.start, segment.end)
}

fn accepted_text_and_ranges(
    payload: &GroqResponse,
    speech: &SpeechWindow,
) -> (String, Option<Vec<(f64, f64)>>) {
    if payload.segments.is_empty() {
        return (String::new(), Some(Vec::new()));
    }
    let accepted = payload
        .segments
        .iter()
        .filter(|segment| segment_is_trustworthy_with_audio(segment, speech))
        .collect::<Vec<_>>();
    let text = accepted
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let ranges = accepted
        .into_iter()
        .map(|segment| (segment.start.max(0.0), segment.end.max(segment.start)))
        .collect();
    (text, Some(ranges))
}

fn retain_words_in_ranges(words: Vec<GroqWord>, ranges: Option<&[(f64, f64)]>) -> Vec<GroqWord> {
    let Some(ranges) = ranges else {
        return words;
    };
    words
        .into_iter()
        .filter(|word| {
            let midpoint = (word.start + word.end) / 2.0;
            ranges
                .iter()
                .any(|(start, end)| midpoint >= *start && midpoint <= *end)
        })
        .collect()
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
    use crate::transcription::speech::SpeechWindow;

    fn segment(
        text: &str,
        no_speech_prob: f64,
        avg_logprob: f64,
        compression_ratio: f64,
    ) -> GroqSegment {
        GroqSegment {
            text: text.to_owned(),
            start: 0.0,
            end: 1.0,
            avg_logprob,
            compression_ratio,
            no_speech_prob,
        }
    }

    fn acoustic_window(activity: Vec<bool>) -> SpeechWindow {
        SpeechWindow {
            start_sample: 0,
            end_sample: activity.len() * 320,
            activity,
        }
    }

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

    #[test]
    fn confident_segments_are_kept_even_when_they_contain_a_url() {
        let segment = segment("Open youtube.com", 0.02, -0.08, 1.1);
        assert!(segment_is_trustworthy(&segment));
    }

    #[test]
    fn confident_provider_text_without_sustained_audio_is_rejected() {
        let segment = GroqSegment {
            text: "thank you for watching".into(),
            start: 0.0,
            end: 2.0,
            avg_logprob: -0.05,
            compression_ratio: 1.0,
            no_speech_prob: 0.01,
        };
        let mut activity = vec![false; 100];
        activity[4] = true;
        activity[35] = true;
        activity[70] = true;
        assert!(!segment_is_trustworthy_with_audio(
            &segment,
            &acoustic_window(activity),
        ));
    }

    #[test]
    fn provider_text_without_segment_metadata_is_not_accepted() {
        let payload = GroqResponse {
            words: vec![GroqWord {
                word: "unverified".into(),
                start: 0.0,
                end: 0.5,
            }],
            segments: Vec::new(),
        };
        let (text, ranges) = accepted_text_and_ranges(&payload, &acoustic_window(vec![true; 50]));
        assert!(text.is_empty());
        assert_eq!(ranges, Some(Vec::new()));
    }

    #[test]
    fn silent_or_low_confidence_segments_are_rejected() {
        assert!(!segment_is_trustworthy(&segment(
            "invented subtitle",
            0.9,
            -0.1,
            1.0,
        )));
        assert!(!segment_is_trustworthy(&segment(
            "uncertain output",
            0.65,
            -0.7,
            1.0,
        )));
        assert!(!segment_is_trustworthy(&segment(
            "decoder failure",
            0.1,
            -1.1,
            1.0,
        )));
    }

    #[test]
    fn repetitive_low_confidence_segments_are_rejected() {
        assert!(!segment_is_trustworthy(&segment(
            "thank you thank you thank you",
            0.2,
            -0.4,
            2.5,
        )));
        assert!(segment_is_trustworthy(&segment(
            "a deliberately repetitive phrase",
            0.05,
            -0.1,
            2.5,
        )));
    }

    #[test]
    fn rejected_segments_and_their_words_do_not_reach_the_transcript() {
        let payload = GroqResponse {
            words: vec![
                GroqWord {
                    word: "hello".into(),
                    start: 0.0,
                    end: 0.5,
                },
                GroqWord {
                    word: "invented".into(),
                    start: 1.2,
                    end: 1.5,
                },
            ],
            segments: vec![
                GroqSegment {
                    text: "hello".into(),
                    start: 0.0,
                    end: 0.8,
                    avg_logprob: -0.1,
                    compression_ratio: 1.0,
                    no_speech_prob: 0.02,
                },
                GroqSegment {
                    text: "invented outro".into(),
                    start: 1.0,
                    end: 2.0,
                    avg_logprob: -0.7,
                    compression_ratio: 1.0,
                    no_speech_prob: 0.7,
                },
            ],
        };
        let (text, ranges) = accepted_text_and_ranges(&payload, &acoustic_window(vec![true; 100]));
        let words = retain_words_in_ranges(payload.words, ranges.as_deref());
        assert_eq!(text, "hello");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].word, "hello");
    }

    #[test]
    fn rejecting_every_segment_also_rejects_every_word() {
        let payload = GroqResponse {
            words: vec![GroqWord {
                word: "invented".into(),
                start: 0.0,
                end: 0.5,
            }],
            segments: vec![segment("invented outro", 0.92, -0.6, 1.0)],
        };
        let (text, ranges) = accepted_text_and_ranges(&payload, &acoustic_window(vec![false; 100]));
        let words = retain_words_in_ranges(payload.words, ranges.as_deref());
        assert!(text.is_empty());
        assert!(words.is_empty());
    }
}
