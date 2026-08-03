use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

const OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// Tracks in-flight `ollama pull` requests so the UI can cancel them and
/// so the same model isn't pulled twice concurrently.
#[derive(Default)]
pub struct OllamaPullState {
    pub in_flight: Arc<RwLock<HashSet<String>>>,
    pub cancelled: Arc<RwLock<HashSet<String>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    name: String,
    status: String,
    bytes_downloaded: u64,
    bytes_total: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    name: String,
    ok: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct PullLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    completed: u64,
    #[serde(default)]
    error: Option<String>,
}

#[tauri::command]
pub async fn pull_ollama_model(
    app: AppHandle,
    state: State<'_, OllamaPullState>,
    name: String,
) -> Result<(), String> {
    {
        let mut in_flight = state.in_flight.write().map_err(|e| e.to_string())?;
        if in_flight.contains(&name) {
            return Err(format!("{name} is already being pulled"));
        }
        in_flight.insert(name.clone());
    }
    {
        let mut cancelled = state.cancelled.write().map_err(|e| e.to_string())?;
        cancelled.remove(&name);
    }

    let cancelled_flag = Arc::clone(&state.cancelled);
    let in_flight = Arc::clone(&state.in_flight);
    let outcome = perform_pull(app.clone(), name.clone(), cancelled_flag).await;

    if let Ok(mut set) = in_flight.write() {
        set.remove(&name);
    }
    if let Ok(mut set) = state.cancelled.write() {
        set.remove(&name);
    }

    match &outcome {
        Ok(()) => {
            let _ = app.emit(
                "ollama-pull://complete",
                DoneEvent {
                    name,
                    ok: true,
                    error: None,
                },
            );
        }
        Err(err) => {
            let _ = app.emit(
                "ollama-pull://complete",
                DoneEvent {
                    name,
                    ok: false,
                    error: Some(err.clone()),
                },
            );
        }
    }
    outcome
}

async fn perform_pull(
    app: AppHandle,
    name: String,
    cancelled: Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|e| e.to_string())?;

    let mut response = client
        .post(format!("{OLLAMA_URL}/api/pull"))
        .json(&serde_json::json!({
            "name": name,
            "stream": true,
        }))
        .send()
        .await
        .map_err(|e| format!("could not reach Ollama at {OLLAMA_URL} — is it running? ({e})"))?;

    if !response.status().is_success() {
        return Err(format!("ollama pull failed: HTTP {}", response.status()));
    }

    // Ollama returns NDJSON — one JSON object per line, streaming.
    // Track the largest per-digest total we've seen so we can approximate an
    // overall percentage. Model pulls often ship a manifest + multiple
    // layers; the weights layer dwarfs the rest.
    let mut buffer: Vec<u8> = Vec::with_capacity(4096);
    let mut biggest_total: u64 = 0;
    let mut current_completed: u64 = 0;
    let mut last_emit = Instant::now();
    let throttle = Duration::from_millis(120);
    let mut latest_status = String::from("starting");

    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if cancelled
            .read()
            .map(|set| set.contains(&name))
            .unwrap_or(false)
        {
            return Err(format!("{name} pull cancelled"));
        }
        buffer.extend_from_slice(&chunk);

        // Consume complete lines from the buffer.
        while let Some(newline) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=newline).collect();
            let line_str = std::str::from_utf8(&line[..line.len() - 1])
                .unwrap_or("")
                .trim();
            if line_str.is_empty() {
                continue;
            }
            let parsed: PullLine = match serde_json::from_str(line_str) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if let Some(err) = parsed.error {
                return Err(err);
            }
            if parsed.total > biggest_total {
                biggest_total = parsed.total;
                current_completed = parsed.completed;
            } else if !parsed.digest.is_empty() && parsed.total == biggest_total {
                current_completed = parsed.completed;
            }
            if !parsed.status.is_empty() {
                latest_status = parsed.status.clone();
            }
            if latest_status == "success" {
                let _ = app.emit(
                    "ollama-pull://progress",
                    ProgressEvent {
                        name: name.clone(),
                        status: "success".into(),
                        bytes_downloaded: biggest_total,
                        bytes_total: biggest_total,
                    },
                );
                return Ok(());
            }
            if last_emit.elapsed() >= throttle {
                last_emit = Instant::now();
                let _ = app.emit(
                    "ollama-pull://progress",
                    ProgressEvent {
                        name: name.clone(),
                        status: latest_status.clone(),
                        bytes_downloaded: current_completed,
                        bytes_total: biggest_total,
                    },
                );
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_ollama_pull(state: State<'_, OllamaPullState>, name: String) -> Result<(), String> {
    let mut cancelled = state.cancelled.write().map_err(|e| e.to_string())?;
    cancelled.insert(name);
    Ok(())
}
