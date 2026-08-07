use std::sync::Mutex;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

pub struct PendingUpdate(Mutex<Option<Update>>);

impl Default for PendingUpdate {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    version: String,
    current_version: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Downloaded,
}

/// Check the signed endpoint and retain the exact update object that was
/// offered. Keeping it in memory means the install action cannot accidentally
/// fetch a different release than the one the user approved.
#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMetadata>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let metadata = update.as_ref().map(|update| UpdateMetadata {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
    });
    *pending
        .0
        .lock()
        .map_err(|_| "The update state is unavailable".to_string())? = update;
    Ok(metadata)
}

/// Download, verify, install, and restart. Tauri verifies every artifact with
/// the updater public key before installation. On Windows the installer may
/// close Quill as part of the install; on macOS `restart` completes the handoff.
#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|_| "The update state is unavailable".to_string())?
        .take()
        .ok_or_else(|| "There is no pending Quill update".to_string())?;

    let mut started = false;
    if let Err(error) = update
        .download_and_install(
            |chunk_length, content_length| {
                if !started {
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                    started = true;
                }
                let _ = on_event.send(DownloadEvent::Progress { chunk_length });
            },
            || {
                let _ = on_event.send(DownloadEvent::Downloaded);
            },
        )
        .await
    {
        *pending
            .0
            .lock()
            .map_err(|_| "The update state is unavailable".to_string())? = Some(update);
        return Err(error.to_string());
    }

    app.restart();
}
