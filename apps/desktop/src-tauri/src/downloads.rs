use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

pub const WHISPER_VERSION: &str = "v1.9.1";
pub const WHISPER_REVISION: &str = "f049fff95a089aa9969deb009cdd4892b3e74916";
pub const CUDA_RUNTIME_ID: &str = "cuda-runtime-windows-x64";
pub const CUDA_RUNTIME_DOWNLOAD_BYTES: u64 = 700_000_000;

const CUDA_ASSET_NAME: &str = "quill-cuda-runtime-windows-x64.zip";
const CUDA_RELEASE_BASE_URL: &str =
    "https://github.com/AaravKashyap12/quill/releases/latest/download";
const CUDA_MANIFEST_NAME: &str = "runtime-manifest.json";
const CUDA_RUNTIME_DIRECTORY_SUFFIX: &str = "-windows-x64-cuda";
const CUDA_VERSION_MISMATCH_MESSAGE: &str =
    "This GPU acceleration pack was built for a newer version of Quill. Update Quill, then try again.";
const MAX_CUDA_UNPACKED_BYTES: u64 = 1_400_000_000;
const REQUIRED_CUDA_FILES: &[&str] = &[
    "ggml-cuda.dll",
    "cublas64_11.dll",
    "cublasLt64_11.dll",
    "cudart32_110.dll",
    "cudart64_110.dll",
    "cuinj64_118.dll",
    "nvrtc-builtins64_118.dll",
    "nvrtc64_112_0.dll",
    "NVIDIA-CUDA-LICENSE.txt",
    "WHISPER-LICENSE.txt",
    CUDA_MANIFEST_NAME,
];

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
    cuda_generation: AtomicU64,
}

#[derive(Debug, Clone)]
pub enum CudaRuntimeAvailability {
    Missing,
    Ready(PathBuf),
    Invalid(String),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CudaRuntimeStatus {
    state: &'static str,
    expected_revision: &'static str,
    download_bytes: u64,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CudaRuntimeManifest {
    schema_version: u32,
    whisper_version: String,
    whisper_revision: String,
    platform: String,
    backend: String,
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

fn cuda_runtime_parent(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("runtimes")
        .join("whisper"))
}

fn cuda_runtime_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(cuda_runtime_parent(app)?.join(format!("{WHISPER_VERSION}-windows-x64-cuda")))
}

fn validate_cuda_runtime(directory: &Path) -> Result<(), String> {
    let manifest_bytes = std::fs::read(directory.join(CUDA_MANIFEST_NAME))
        .map_err(|_| "CUDA runtime manifest is missing".to_owned())?;
    let manifest: CudaRuntimeManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "CUDA runtime manifest is invalid".to_owned())?;
    if manifest.schema_version != 1
        || manifest.whisper_version != WHISPER_VERSION
        || manifest.whisper_revision != WHISPER_REVISION
        || manifest.platform != "windows-x64"
        || manifest.backend != "cuda"
    {
        return Err(CUDA_VERSION_MISMATCH_MESSAGE.to_owned());
    }
    for required in REQUIRED_CUDA_FILES {
        if !directory.join(required).is_file() {
            return Err("CUDA runtime pack is incomplete; remove and download it again".to_owned());
        }
    }
    Ok(())
}

pub fn cuda_runtime_availability(app: &AppHandle) -> Result<CudaRuntimeAvailability, String> {
    let directory = cuda_runtime_dir(app)?;
    if !directory.exists() {
        return Ok(CudaRuntimeAvailability::Missing);
    }
    Ok(match validate_cuda_runtime(&directory) {
        Ok(()) => CudaRuntimeAvailability::Ready(directory),
        Err(error) => CudaRuntimeAvailability::Invalid(error),
    })
}

pub fn cuda_runtime_generation(app: &AppHandle) -> u64 {
    app.state::<DownloadState>()
        .cuda_generation
        .load(Ordering::Relaxed)
}

#[tauri::command]
pub fn get_cuda_runtime_status(app: AppHandle) -> Result<CudaRuntimeStatus, String> {
    let (state, error) = match cuda_runtime_availability(&app)? {
        CudaRuntimeAvailability::Missing => ("missing", None),
        CudaRuntimeAvailability::Ready(_) => ("installed", None),
        CudaRuntimeAvailability::Invalid(error) => ("invalid", Some(error)),
    };
    Ok(CudaRuntimeStatus {
        state,
        expected_revision: WHISPER_REVISION,
        download_bytes: CUDA_RUNTIME_DOWNLOAD_BYTES,
        error,
    })
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

fn begin_download(state: &DownloadState, id: &str) -> Result<(), String> {
    {
        let mut in_flight = state.in_flight.write().map_err(|e| e.to_string())?;
        if in_flight.contains(id) {
            return Err(format!("{id} is already downloading"));
        }
        in_flight.insert(id.to_owned());
    }
    state
        .cancelled
        .write()
        .map_err(|e| e.to_string())?
        .remove(id);
    Ok(())
}

fn finish_download(state: &DownloadState, id: &str) {
    if let Ok(mut set) = state.in_flight.write() {
        set.remove(id);
    }
    if let Ok(mut set) = state.cancelled.write() {
        set.remove(id);
    }
}

fn emit_download_complete(app: &AppHandle, id: &str, outcome: &Result<(), String>) {
    let (ok, error) = match outcome {
        Ok(()) => (true, None),
        Err(error) => (false, Some(error.clone())),
    };
    let _ = app.emit(
        "model-download://complete",
        DoneEvent {
            id: id.to_owned(),
            ok,
            error,
        },
    );
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
    begin_download(&state, &id)?;
    let cancelled = Arc::clone(&state.cancelled);
    let outcome = perform_model_download(&app, &id, &cancelled).await;
    finish_download(&state, &id);
    emit_download_complete(&app, &id, &outcome);
    outcome
}

async fn perform_model_download(
    app: &AppHandle,
    id: &str,
    cancelled: &Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    let destination = model_file(app, id)?;
    let temp = destination.with_extension("bin.part");
    download_to_file(
        app,
        id,
        &model_url(id),
        expected_size(id),
        None,
        &temp,
        cancelled,
    )
    .await?;
    tokio::fs::rename(&temp, &destination)
        .await
        .map_err(|e| e.to_string())
}

async fn download_to_file(
    app: &AppHandle,
    id: &str,
    url: &str,
    fallback_size: u64,
    expected_sha256: Option<&str>,
    temp: &Path,
    cancelled: &Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    let _ = tokio::fs::remove_file(temp).await;

    let client = reqwest::Client::builder()
        .user_agent("quill-desktop/0.1 (+https://github.com/AaravKashyap12/quill)")
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "download failed: HTTP {} for {}",
            response.status(),
            id
        ));
    }
    let total = response.content_length().unwrap_or(fallback_size);

    // Stream to a temp file so a partial download never masquerades as installed.
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(temp)
        .await
        .map_err(|e| e.to_string())?;

    let mut received: u64 = 0;
    let mut hasher = Sha256::new();
    let mut last_emit = Instant::now();
    let throttle = Duration::from_millis(120);

    while let Some(bytes) = response.chunk().await.map_err(|e| e.to_string())? {
        if cancelled
            .read()
            .map(|set| set.contains(id))
            .unwrap_or(false)
        {
            drop(file);
            let _ = tokio::fs::remove_file(temp).await;
            return Err(format!("{id} download cancelled"));
        }
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
        hasher.update(&bytes);
        received += bytes.len() as u64;
        if last_emit.elapsed() >= throttle {
            last_emit = Instant::now();
            let _ = app.emit(
                "model-download://progress",
                ProgressEvent {
                    id: id.to_owned(),
                    bytes_downloaded: received,
                    bytes_total: total,
                },
            );
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    if let Some(expected) = expected_sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(temp).await;
            return Err(format!("SHA-256 verification failed for {id}"));
        }
    }

    // Emit one final progress event so the UI reads 100% before the completion event lands.
    let _ = app.emit(
        "model-download://progress",
        ProgressEvent {
            id: id.to_owned(),
            bytes_downloaded: received,
            bytes_total: if total == 0 { received } else { total },
        },
    );

    Ok(())
}

fn parse_sha256_file(contents: &str) -> Result<String, String> {
    let mut fields = contents.split_whitespace();
    let hash = fields
        .next()
        .ok_or_else(|| "CUDA runtime checksum file is empty".to_owned())?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("CUDA runtime checksum file is invalid".to_owned());
    }
    if let Some(file_name) = fields.next() {
        if file_name.trim_start_matches('*') != CUDA_ASSET_NAME {
            return Err("CUDA runtime checksum names an unexpected asset".to_owned());
        }
    }
    Ok(hash.to_ascii_lowercase())
}

async fn fetch_cuda_checksum() -> Result<String, String> {
    let response = reqwest::Client::builder()
        .user_agent("quill-desktop/0.1 (+https://github.com/AaravKashyap12/quill)")
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("{CUDA_RELEASE_BASE_URL}/{CUDA_ASSET_NAME}.sha256"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "CUDA runtime checksum download failed: HTTP {}",
            response.status()
        ));
    }
    let contents = response.text().await.map_err(|error| error.to_string())?;
    parse_sha256_file(&contents)
}

fn install_cuda_archive(
    archive_path: &Path,
    staging: &Path,
    destination: &Path,
    cancelled: &Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    if staging.exists() {
        std::fs::remove_dir_all(staging)
            .map_err(|_| "could not reset CUDA runtime staging directory".to_owned())?;
    }
    std::fs::create_dir_all(staging)
        .map_err(|_| "could not create CUDA runtime staging directory".to_owned())?;

    let archive_file = File::open(archive_path)
        .map_err(|_| "could not open the downloaded CUDA runtime".to_owned())?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|_| "downloaded CUDA runtime is not a valid ZIP archive".to_owned())?;
    let allowed = REQUIRED_CUDA_FILES.iter().copied().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut unpacked_bytes = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "CUDA runtime archive entry could not be read".to_owned())?;
        let name = entry.name().replace('\\', "/");
        if entry.is_dir() || name.contains('/') || !allowed.contains(name.as_str()) {
            return Err("CUDA runtime archive contains an unexpected entry".to_owned());
        }
        if !seen.insert(name.clone()) {
            return Err("CUDA runtime archive contains a duplicate entry".to_owned());
        }
        unpacked_bytes = unpacked_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "CUDA runtime archive size overflowed".to_owned())?;
        if unpacked_bytes > MAX_CUDA_UNPACKED_BYTES {
            return Err("CUDA runtime archive expands beyond its safety limit".to_owned());
        }

        let mut output = File::create(staging.join(&name))
            .map_err(|_| "CUDA runtime file could not be created".to_owned())?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            if cancelled
                .read()
                .map(|set| set.contains(CUDA_RUNTIME_ID))
                .unwrap_or(false)
            {
                return Err("CUDA runtime download cancelled".to_owned());
            }
            let count = entry
                .read(&mut buffer)
                .map_err(|_| "CUDA runtime file could not be extracted".to_owned())?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|_| "CUDA runtime file could not be extracted".to_owned())?;
        }
        output
            .flush()
            .map_err(|_| "CUDA runtime file could not be flushed".to_owned())?;
    }

    validate_cuda_runtime(staging)?;
    if destination.exists() {
        std::fs::remove_dir_all(destination).map_err(|_| {
            "CUDA runtime is currently in use; switch to CPU and try again".to_owned()
        })?;
    }
    std::fs::rename(staging, destination)
        .map_err(|_| "CUDA runtime could not be activated after extraction".to_owned())?;
    Ok(())
}

fn remove_stale_cuda_runtimes(parent: &Path, current: &Path) -> Result<(usize, usize), String> {
    let entries = std::fs::read_dir(parent)
        .map_err(|_| "could not inspect installed CUDA runtimes".to_owned())?;
    let mut removed = 0;
    let mut failed = 0;

    for entry in entries {
        let entry = entry.map_err(|_| "could not inspect installed CUDA runtimes".to_owned())?;
        let path = entry.path();
        if path == current {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let is_owned_runtime = file_type.is_dir()
            && !file_type.is_symlink()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(CUDA_RUNTIME_DIRECTORY_SUFFIX));
        if !is_owned_runtime {
            continue;
        }
        match std::fs::remove_dir_all(path) {
            Ok(()) => removed += 1,
            Err(_) => failed += 1,
        }
    }

    Ok((removed, failed))
}

async fn perform_cuda_download(
    app: &AppHandle,
    cancelled: &Arc<RwLock<HashSet<String>>>,
) -> Result<(), String> {
    let expected_sha256 = fetch_cuda_checksum().await?;
    let parent = cuda_runtime_parent(app)?;
    tokio::fs::create_dir_all(&parent)
        .await
        .map_err(|error| error.to_string())?;
    let archive_path = parent.join(format!(".{CUDA_ASSET_NAME}.part"));
    download_to_file(
        app,
        CUDA_RUNTIME_ID,
        &format!("{CUDA_RELEASE_BASE_URL}/{CUDA_ASSET_NAME}"),
        CUDA_RUNTIME_DOWNLOAD_BYTES,
        Some(&expected_sha256),
        &archive_path,
        cancelled,
    )
    .await?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let staging = parent.join(format!(".cuda-installing-{}-{nonce}", std::process::id()));
    let destination = cuda_runtime_dir(app)?;
    let archive_for_install = archive_path.clone();
    let staging_for_install = staging.clone();
    let cancelled_for_install = Arc::clone(cancelled);
    let install_result = tokio::task::spawn_blocking(move || {
        install_cuda_archive(
            &archive_for_install,
            &staging_for_install,
            &destination,
            &cancelled_for_install,
        )
    })
    .await;

    let _ = tokio::fs::remove_file(&archive_path).await;
    let install_result = match install_result {
        Ok(result) => result,
        Err(_) => Err("CUDA runtime installer stopped unexpectedly".to_owned()),
    };
    if install_result.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
    } else {
        match remove_stale_cuda_runtimes(&parent, &cuda_runtime_dir(app)?) {
            Ok((removed, failed)) => {
                if removed > 0 {
                    tracing::info!(removed_count = removed, "removed stale CUDA runtimes");
                }
                if failed > 0 {
                    tracing::warn!(
                        failed_count = failed,
                        "some stale CUDA runtimes could not be removed"
                    );
                }
            }
            Err(_) => tracing::warn!("stale CUDA runtime cleanup could not run"),
        }
    }
    install_result
}

#[tauri::command]
pub async fn download_cuda_runtime(
    app: AppHandle,
    state: State<'_, DownloadState>,
) -> Result<(), String> {
    begin_download(&state, CUDA_RUNTIME_ID)?;
    let cancelled = Arc::clone(&state.cancelled);
    let outcome = perform_cuda_download(&app, &cancelled).await;
    if outcome.is_ok() {
        state.cuda_generation.fetch_add(1, Ordering::Relaxed);
    }
    finish_download(&state, CUDA_RUNTIME_ID);
    emit_download_complete(&app, CUDA_RUNTIME_ID, &outcome);
    outcome
}

#[tauri::command]
pub fn cancel_cuda_runtime_download(state: State<'_, DownloadState>) -> Result<(), String> {
    state
        .cancelled
        .write()
        .map_err(|error| error.to_string())?
        .insert(CUDA_RUNTIME_ID.to_owned());
    Ok(())
}

#[tauri::command]
pub fn delete_cuda_runtime(app: AppHandle, state: State<'_, DownloadState>) -> Result<(), String> {
    let directory = cuda_runtime_dir(&app)?;
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|_| {
            "CUDA runtime is currently in use; switch to CPU, save, and try again".to_owned()
        })?;
        state.cuda_generation.fetch_add(1, Ordering::Relaxed);
    }
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

    fn runtime_fixture(revision: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow the Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "quill-cuda-runtime-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("fixture directory should be created");
        for required in REQUIRED_CUDA_FILES {
            if *required != CUDA_MANIFEST_NAME {
                std::fs::write(directory.join(required), [])
                    .expect("fixture file should be written");
            }
        }
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "whisperVersion": WHISPER_VERSION,
            "whisperRevision": revision,
            "platform": "windows-x64",
            "backend": "cuda"
        });
        std::fs::write(
            directory.join(CUDA_MANIFEST_NAME),
            serde_json::to_vec(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");
        directory
    }

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

    #[test]
    fn checksum_file_names_the_expected_release_asset() {
        let hash = "a".repeat(64);
        assert_eq!(
            parse_sha256_file(&format!("{hash}  {CUDA_ASSET_NAME}\n")),
            Ok(hash)
        );
        assert!(parse_sha256_file(&format!("{}  another.zip", "b".repeat(64))).is_err());
    }

    #[test]
    fn installed_cuda_runtime_requires_the_exact_whisper_revision() {
        let valid = runtime_fixture(WHISPER_REVISION);
        assert_eq!(validate_cuda_runtime(&valid), Ok(()));
        std::fs::remove_dir_all(&valid).expect("valid fixture should be removed");

        let mismatched = runtime_fixture("different-revision");
        let error = validate_cuda_runtime(&mismatched).expect_err("mismatch should be refused");
        assert_eq!(error, CUDA_VERSION_MISMATCH_MESSAGE);
        std::fs::remove_dir_all(&mismatched).expect("mismatched fixture should be removed");
    }

    #[test]
    fn successful_install_cleanup_removes_only_stale_owned_runtime_directories() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow the Unix epoch")
            .as_nanos();
        let parent = std::env::temp_dir().join(format!(
            "quill-cuda-cleanup-test-{}-{nonce}",
            std::process::id()
        ));
        let current = parent.join(format!("{WHISPER_VERSION}{CUDA_RUNTIME_DIRECTORY_SUFFIX}"));
        let stale = parent.join(format!("v0.0.1{CUDA_RUNTIME_DIRECTORY_SUFFIX}"));
        let unrelated = parent.join("other-runtime");
        std::fs::create_dir_all(&current).expect("current runtime should be created");
        std::fs::create_dir_all(&stale).expect("stale runtime should be created");
        std::fs::create_dir_all(&unrelated).expect("unrelated runtime should be created");

        assert_eq!(remove_stale_cuda_runtimes(&parent, &current), Ok((1, 0)));
        assert!(current.is_dir());
        assert!(!stale.exists());
        assert!(unrelated.is_dir());

        std::fs::remove_dir_all(&parent).expect("cleanup fixture should be removed");
    }
}
