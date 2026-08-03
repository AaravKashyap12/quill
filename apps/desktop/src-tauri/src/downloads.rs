use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Registry of whisper.cpp GGML models Quill knows how to fetch.
/// URLs resolve to the raw `ggml-<id>.bin` on Hugging Face.
const REGISTRY: &[(&str, u64)] = &[
    // (id, approximate size in bytes — used for a progress fallback if the
    //   server does not report Content-Length)
    ("tiny.en", 77_691_713),
    ("tiny", 77_691_713),
    ("base.en", 147_951_465),
    ("base", 147_951_465),
    ("small.en", 487_593_953),
    ("small", 487_593_953),
    ("medium.en", 1_533_763_425),
    ("medium", 1_533_763_425),
    ("distil-large-v3", 1_520_000_000),
    ("large-v3-turbo", 1_624_555_275),
];

/// Tracks in-flight downloads so the frontend can cancel and the model
/// scanner can avoid returning half-written files.
#[derive(Default)]
pub struct DownloadState {
    pub in_flight: Arc<RwLock<HashSet<String>>>,
    pub cancelled: Arc<RwLock<HashSet<String>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    id: String,
    bytes_downloaded: u64,
    bytes_total: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    id: String,
    ok: bool,
    error: Option<String>,
}

/// Writable per-user directory for downloaded models.
///   Windows:  %APPDATA%\quill\models\
///   macOS:    ~/Library/Application Support/quill/models/
///   Linux:    ~/.local/share/quill/models/
pub fn user_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("models");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn is_known(id: &str) -> bool {
    REGISTRY.iter().any(|(known, _)| *known == id)
}

fn expected_size(id: &str) -> u64 {
    REGISTRY
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, size)| *size)
        .unwrap_or(0)
}

fn model_url(id: &str) -> String {
    if id == "distil-large-v3" {
        return "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/ggml-distil-large-v3.bin"
            .to_owned();
    }
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{id}.bin")
}

fn model_file(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(user_model_dir(app)?.join(format!("ggml-{id}.bin")))
}

#[tauri::command]
pub async fn download_whisper_model(
    app: AppHandle,
    state: State<'_, DownloadState>,
    id: String,
) -> Result<(), String> {
    if !is_known(&id) {
        return Err(format!("unknown model: {id}"));
    }

    // Guard against duplicate downloads for the same id.
    {
        let mut in_flight = state.in_flight.write().map_err(|e| e.to_string())?;
        if in_flight.contains(&id) {
            return Err(format!("{id} is already downloading"));
        }
        in_flight.insert(id.clone());
    }
    {
        let mut cancelled = state.cancelled.write().map_err(|e| e.to_string())?;
        cancelled.remove(&id);
    }

    let in_flight = Arc::clone(&state.in_flight);
    let cancelled = Arc::clone(&state.cancelled);
    let app_for_emit = app.clone();
    let id_for_emit = id.clone();

    let outcome = perform_download(app, id.clone(), &cancelled).await;

    // Clean up the in-flight marker regardless of outcome.
    if let Ok(mut set) = in_flight.write() {
        set.remove(&id);
    }
    if let Ok(mut set) = cancelled.write() {
        set.remove(&id);
    }

    match &outcome {
        Ok(()) => {
            let _ = app_for_emit.emit(
                "model-download://complete",
                DoneEvent {
                    id: id_for_emit,
                    ok: true,
                    error: None,
                },
            );
        }
        Err(err) => {
            let _ = app_for_emit.emit(
                "model-download://complete",
                DoneEvent {
                    id: id_for_emit,
                    ok: false,
                    error: Some(err.clone()),
                },
            );
        }
    }
    outcome
}

async fn perform_download(
    app: AppHandle,
    id: String,
    cancelled: &Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    let destination = model_file(&app, &id)?;
    let temp = destination.with_extension("bin.part");
    let _ = std::fs::remove_file(&temp);

    let client = reqwest::Client::builder()
        .user_agent("quill-desktop/0.1 (+https://github.com)")
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(model_url(&id))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "download failed: HTTP {} for {}",
            response.status(),
            id
        ));
    }
    let total = response
        .content_length()
        .unwrap_or_else(|| expected_size(&id));

    // Stream to a temp file so a partial download never masquerades as installed.
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&temp)
        .await
        .map_err(|e| e.to_string())?;

    let mut received: u64 = 0;
    let mut last_emit = Instant::now();
    let throttle = Duration::from_millis(120);

    while let Some(bytes) = response.chunk().await.map_err(|e| e.to_string())? {
        if cancelled
            .read()
            .map(|set| set.contains(&id))
            .unwrap_or(false)
        {
            drop(file);
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(format!("{id} download cancelled"));
        }
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
        received += bytes.len() as u64;
        if last_emit.elapsed() >= throttle {
            last_emit = Instant::now();
            let _ = app.emit(
                "model-download://progress",
                ProgressEvent {
                    id: id.clone(),
                    bytes_downloaded: received,
                    bytes_total: total,
                },
            );
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    // Emit one final progress event so the UI reads 100% before the completion event lands.
    let _ = app.emit(
        "model-download://progress",
        ProgressEvent {
            id: id.clone(),
            bytes_downloaded: received,
            bytes_total: if total == 0 { received } else { total },
        },
    );

    tokio::fs::rename(&temp, &destination)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cancel_whisper_download(state: State<'_, DownloadState>, id: String) -> Result<(), String> {
    let mut cancelled = state.cancelled.write().map_err(|e| e.to_string())?;
    cancelled.insert(id);
    Ok(())
}

#[tauri::command]
pub fn delete_whisper_model(app: AppHandle, id: String) -> Result<(), String> {
    let path = model_file(&app, &id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distil_large_v3_uses_its_official_ggml_repository() {
        assert!(is_known("distil-large-v3"));
        assert_eq!(
            model_url("distil-large-v3"),
            "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/ggml-distil-large-v3.bin"
        );
    }

    #[test]
    fn standard_models_keep_the_whisper_cpp_repository() {
        assert_eq!(
            model_url("base.en"),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
        );
    }
}
