mod gemini;
mod groq;
mod speech;

use crate::asr::{AsrPass, WhisperServer};
use crate::model::{AppSettings, TranscriptionProvider};
use anyhow::Result;
use tauri::AppHandle;

pub enum TranscriptionRuntime {
    Local(Box<WhisperServer>),
    Groq(groq::GroqTranscriber),
    Gemini(gemini::GeminiTranscriber),
}

pub async fn transcribe_recovery_groq(settings: &AppSettings, samples: &[f32]) -> Result<String> {
    let transcriber = groq::GroqTranscriber::new().await?;
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
    let transcriber = gemini::GeminiTranscriber::new().await?;
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
            TranscriptionProvider::Local => Ok(Self::Local(Box::new(
                WhisperServer::start(app, settings).await?,
            ))),
            TranscriptionProvider::Groq => Ok(Self::Groq(groq::GroqTranscriber::new().await?)),
            TranscriptionProvider::Gemini => {
                Ok(Self::Gemini(gemini::GeminiTranscriber::new().await?))
            }
        }
    }

    pub fn supports_rolling(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    pub fn ready_message(&self) -> &'static str {
        match self {
            Self::Local(server) => server.ready_message(),
            Self::Groq(_) => "Ready with Groq — audio uploads after release",
            Self::Gemini(_) => "Ready with Gemini — audio uploads after release",
        }
    }

    pub fn activity_message(&self) -> &'static str {
        match self {
            Self::Local(server) => server.activity_message(),
            Self::Groq(_) => "Transcribing with Groq",
            Self::Gemini(_) => "Transcribing with Gemini",
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(server) => server.backend_name(),
            Self::Groq(_) => "Groq",
            Self::Gemini(_) => "Gemini",
        }
    }

    pub fn cloud_provider_id(&self) -> Option<&'static str> {
        match self {
            Self::Local(_) => None,
            Self::Groq(_) => Some("groq"),
            Self::Gemini(_) => Some("gemini"),
        }
    }

    pub fn cloud_label(&self) -> Option<&'static str> {
        match self {
            Self::Local(_) => None,
            Self::Groq(_) => Some("Groq Cloud"),
            Self::Gemini(_) => Some("Gemini Cloud"),
        }
    }

    pub fn listening_message(&self) -> &'static str {
        match self {
            Self::Local(_) => "Listening",
            Self::Groq(_) => "Listening — Groq transcribes after release",
            Self::Gemini(_) => "Listening — Gemini transcribes after release",
        }
    }

    pub fn cold_load_ms(&self) -> u128 {
        match self {
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
