#[cfg(target_os = "macos")]
mod apple;
mod gemini;
mod groq;
mod speech;

use crate::asr::{AsrPass, WhisperServer};
use crate::model::{AppSettings, TranscriptionProvider};
use crate::streaming::TimedWord;
use anyhow::Result;
use tauri::AppHandle;

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

#[tauri::command]
pub fn apple_speech_supported(language: String) -> bool {
    #[cfg(target_os = "macos")]
    {
        apple::supported(&language)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = language;
        false
    }
}

pub enum TranscriptionRuntime {
    #[cfg(target_os = "macos")]
    Apple,
    Local(Box<WhisperServer>),
    Groq(groq::GroqTranscriber),
    Gemini(gemini::GeminiTranscriber),
}

#[cfg(target_os = "macos")]
fn apple_language(settings: &AppSettings) -> &str {
    if settings.onboarding_completed {
        &settings.language
    } else {
        "auto"
    }
}

#[cfg(target_os = "macos")]
pub async fn transcribe_recovery_apple(settings: AppSettings, samples: Vec<f32>) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let pass = apple::transcribe(apple_language(&settings), &samples)?;
        Ok(crate::dictionary::apply(&pass.text, &settings.dictionary))
    })
    .await?
}

pub async fn transcribe_recovery_groq(settings: &AppSettings, samples: &[f32]) -> Result<String> {
    let transcriber = groq::GroqTranscriber::new()?;
    let pass = transcriber
        .transcribe(settings, samples, "recovery")
        .await?;
    Ok(crate::dictionary::apply(&pass.text, &settings.dictionary))
}

pub async fn transcribe_recovery_gemini(
    settings: &AppSettings,
    samples: &[f32],
    mode: &str,
) -> Result<String> {
    let transcriber = gemini::GeminiTranscriber::new()?;
    let pass = transcriber.transcribe(settings, samples, mode).await?;
    Ok(crate::dictionary::apply(&pass.text, &settings.dictionary))
}

pub async fn transcribe_recovery_local(
    app: &AppHandle,
    settings: &AppSettings,
    samples: &[f32],
) -> Result<String> {
    let mut server = WhisperServer::start(app, settings).await?;
    let result = server.transcribe(settings, samples).await;
    let _ = server.shutdown().await;
    let pass = result?;
    Ok(crate::dictionary::apply(&pass.text, &settings.dictionary))
}

impl TranscriptionRuntime {
    pub async fn start(app: &AppHandle, settings: &AppSettings) -> Result<Self> {
        match settings.transcription_provider {
            TranscriptionProvider::Apple => {
                #[cfg(target_os = "macos")]
                {
                    Ok(Self::Apple)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    anyhow::bail!(
                        "Apple Speech requires macOS. Choose a speech provider in Voice settings."
                    )
                }
            }
            TranscriptionProvider::Local => Ok(Self::Local(Box::new(
                WhisperServer::start(app, settings).await?,
            ))),
            TranscriptionProvider::Groq => Ok(Self::Groq(groq::GroqTranscriber::new()?)),
            TranscriptionProvider::Gemini => Ok(Self::Gemini(gemini::GeminiTranscriber::new()?)),
        }
    }

    pub fn supports_rolling(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    pub fn ready_message(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => "Ready with Apple Speech",
            Self::Local(server) => server.ready_message(),
            Self::Groq(_) => "Ready with Groq — audio uploads after release",
            Self::Gemini(_) => "Ready with Gemini — audio uploads after release",
        }
    }

    pub fn activity_message(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => "Transcribing with Apple Speech",
            Self::Local(server) => server.activity_message(),
            Self::Groq(_) => "Transcribing with Groq",
            Self::Gemini(_) => "Transcribing with Gemini",
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => "Apple Speech",
            Self::Local(server) => server.backend_name(),
            Self::Groq(_) => "Groq",
            Self::Gemini(_) => "Gemini",
        }
    }

    pub fn cloud_provider_id(&self) -> Option<&'static str> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => None,
            Self::Local(_) => None,
            Self::Groq(_) => Some("groq"),
            Self::Gemini(_) => Some("gemini"),
        }
    }

    pub fn cloud_label(&self) -> Option<&'static str> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => None,
            Self::Local(_) => None,
            Self::Groq(_) => Some("Groq Cloud"),
            Self::Gemini(_) => Some("Gemini Cloud"),
        }
    }

    pub fn listening_message(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => "Listening",
            Self::Local(_) => "Listening",
            Self::Groq(_) => "Listening — Groq transcribes after release",
            Self::Gemini(_) => "Listening — Gemini transcribes after release",
        }
    }

    pub fn cold_load_ms(&self) -> u128 {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => 0,
            Self::Local(server) => server.cold_load_ms,
            Self::Groq(_) => 0,
            Self::Gemini(_) => 0,
        }
    }

    pub async fn transcribe(
        &self,
        settings: &AppSettings,
        samples: &[f32],
        mode: &str,
    ) -> Result<AsrPass> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Apple => apple::transcribe(apple_language(settings), samples),
            Self::Local(server) => server.transcribe(settings, samples).await,
            Self::Groq(transcriber) => transcriber.transcribe(settings, samples, mode).await,
            Self::Gemini(transcriber) => transcriber.transcribe(settings, samples, mode).await,
        }
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if let Self::Local(server) = self {
            server.shutdown().await?;
        }
        Ok(())
    }
}
