//! App-managed Scribe. Model weights live outside the app bundle; the engine
//! exists only during a Scribe request and never reads cloud credentials.
use crate::{
    downloads::{self, DownloadState},
    model::ProviderStatus,
};
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::{
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, State};

pub const MODEL_ID: &str = "qwen3-4b-instruct-2507-q4-k-m";
const MODEL_BYTES: u64 = 2_497_281_120;
const MODEL_SHA256: &str = "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597";
const MODEL_URL: &str = "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/a06e946bb6b655725eafa393f4a9745d460374c9/Qwen3-4B-Instruct-2507-Q4_K_M.gguf";
static MODEL_PATH: OnceLock<PathBuf> = OnceLock::new();
static REQUEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[cfg(windows)]
mod windows;
struct EngineChild {
    process: Child,
    #[cfg(windows)]
    _job: windows::KillOnCloseJob,
}
static CHILD: Mutex<Option<EngineChild>> = Mutex::new(None);
#[derive(PartialEq, Eq)]
struct VerifiedModel {
    path: PathBuf,
    length: u64,
    modified: SystemTime,
}
static VERIFIED_MODEL: Mutex<Option<VerifiedModel>> = Mutex::new(None);

pub fn initialise(app: &AppHandle) -> Result<(), String> {
    let _ = MODEL_PATH.set(downloads::user_model_dir(app)?.join(format!("{MODEL_ID}.gguf")));
    Ok(())
}
fn model_path() -> Result<PathBuf, String> {
    MODEL_PATH
        .get()
        .cloned()
        .ok_or_else(|| "Local Scribe is not initialized".to_owned())
}
fn engine_path() -> Result<PathBuf> {
    #[cfg(target_arch = "x86_64")]
    if !(std::is_x86_feature_detected!("avx2")
        && std::is_x86_feature_detected!("fma")
        && std::is_x86_feature_detected!("f16c")
        && std::is_x86_feature_detected!("bmi2"))
    {
        return Err(anyhow!("Offline Scribe requires a CPU with AVX2, FMA, F16C and BMI2. Choose a cloud or external Scribe provider on this computer."));
    }
    let root = std::env::current_exe()?
        .parent()
        .context("Missing app directory")?
        .to_owned();
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let bundled = root.join(name);
    if bundled.is_file() {
        return Ok(bundled);
    }
    #[cfg(debug_assertions)]
    {
        let triple = if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!(
                "llama-server-{triple}{}",
                if cfg!(windows) { ".exe" } else { "" }
            ));
        if dev.is_file() {
            return Ok(dev);
        }
    }
    Err(anyhow!(
        "The local Scribe engine is missing. Reinstall Quill to restore it."
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalScribeStatus {
    installed: bool,
    download_bytes: u64,
    partial_bytes: u64,
    downloading: bool,
    engine_available: bool,
    engine_error: Option<String>,
}
fn installed() -> bool {
    model_path()
        .ok()
        .and_then(|p| p.metadata().ok())
        .is_some_and(|m| m.is_file() && m.len() == MODEL_BYTES)
}
pub fn provider_status() -> ProviderStatus {
    let available = installed() && engine_path().is_ok();
    ProviderStatus {
        kind: "local".to_owned(),
        base_url: String::new(),
        available,
        models: if available {
            vec![MODEL_ID.to_owned()]
        } else {
            vec![]
        },
    }
}
#[tauri::command]
pub fn get_local_scribe_status(
    state: State<'_, DownloadState>,
) -> Result<LocalScribeStatus, String> {
    let path = model_path()?;
    Ok(LocalScribeStatus {
        installed: installed(),
        download_bytes: MODEL_BYTES,
        partial_bytes: path
            .with_extension("gguf.part")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0),
        downloading: state
            .in_flight
            .read()
            .map_err(|e| e.to_string())?
            .contains(MODEL_ID),
        engine_available: engine_path().is_ok(),
        engine_error: engine_path().err().map(|error| error.to_string()),
    })
}
#[tauri::command]
pub async fn download_local_scribe(
    app: AppHandle,
    state: State<'_, DownloadState>,
) -> Result<(), String> {
    engine_path().map_err(|e| e.to_string())?;
    downloads::begin_download(&state, MODEL_ID)?;
    let outcome = async {
        let path = model_path()?;
        let temp = path.with_extension("gguf.part");
        downloads::download_to_file(
            |downloaded, total| downloads::emit_progress(&app, MODEL_ID, downloaded, total),
            MODEL_ID,
            MODEL_URL,
            MODEL_BYTES,
            MODEL_SHA256,
            &temp,
            &Arc::clone(&state.cancelled),
        )
        .await?;
        tokio::fs::rename(temp, path)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    downloads::finish_download(&state, MODEL_ID);
    downloads::emit_download_complete(&app, MODEL_ID, &outcome);
    outcome
}
#[tauri::command]
pub fn pause_local_scribe(state: State<'_, DownloadState>) -> Result<(), String> {
    state
        .cancelled
        .write()
        .map_err(|e| e.to_string())?
        .insert(MODEL_ID.to_owned());
    Ok(())
}
#[tauri::command]
pub async fn delete_local_scribe(state: State<'_, DownloadState>) -> Result<(), String> {
    let _request = REQUEST
        .try_lock()
        .map_err(|_| "Scribe is in use. Try removing the model after it finishes.")?;
    // Reserve the download slot while deleting, so download/remove cannot race.
    downloads::begin_download(&state, MODEL_ID)?;
    let outcome = async {
        let path = model_path()?;
        for file in [&path, &path.with_extension("gguf.part")] {
            match tokio::fs::remove_file(file).await {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    .await;
    downloads::finish_download(&state, MODEL_ID);
    if outcome.is_ok() {
        *VERIFIED_MODEL.lock().map_err(|e| e.to_string())? = None;
    }
    outcome
}

pub fn shutdown() {
    if let Ok(mut guard) = CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.process.kill();
            let _ = child.process.wait();
        }
    }
}
struct RunningEngine;
impl Drop for RunningEngine {
    fn drop(&mut self) {
        shutdown();
    }
}

async fn verify_model(path: &std::path::Path) -> Result<()> {
    let metadata = path
        .metadata()
        .context("Download the offline Scribe model in Voice settings before using it")?;
    let stamp = metadata.modified().ok().map(|modified| VerifiedModel {
        path: path.to_owned(),
        length: metadata.len(),
        modified,
    });
    if stamp.is_some()
        && *VERIFIED_MODEL
            .lock()
            .map_err(|_| anyhow!("Scribe model lock failed"))?
            == stamp
    {
        return Ok(());
    }
    let checked_path = path.to_owned();
    tokio::task::spawn_blocking(move || downloads::verify_model_file(&checked_path, MODEL_ID, MODEL_BYTES, MODEL_SHA256))
        .await?.map_err(|_| anyhow!("The offline Scribe model failed its integrity check. Remove it in Voice settings and download it again."))?;
    *VERIFIED_MODEL
        .lock()
        .map_err(|_| anyhow!("Scribe model lock failed"))? = stamp;
    Ok(())
}

fn start_engine(command: &mut Command) -> Result<RunningEngine> {
    let mut guard = CHILD
        .lock()
        .map_err(|_| anyhow!("Scribe engine lock failed"))?;
    #[cfg(windows)]
    let job = windows::KillOnCloseJob::new().context("Could not protect the Scribe process")?;
    let process = command
        .spawn()
        .context("Could not start the local Scribe engine")?;
    #[cfg(windows)]
    let process = {
        let mut process = process;
        if let Err(error) = job.assign(&process) {
            let _ = process.kill();
            let _ = process.wait();
            return Err(error).context("Could not protect the Scribe process");
        }
        process
    };
    *guard = Some(EngineChild {
        process,
        #[cfg(windows)]
        _job: job,
    });
    Ok(RunningEngine)
}

pub async fn complete(system: &str, user: &str) -> Result<serde_json::Value> {
    let _request = REQUEST.lock().await;
    let path = model_path().map_err(|e| anyhow!(e))?;
    verify_model(&path).await?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random)
        .map_err(|_| anyhow!("Unable to secure the local Scribe connection"))?;
    let key: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut command = Command::new(engine_path()?);
    command.env_clear();
    for name in ["SystemRoot", "WINDIR", "HOME", "TEMP", "TMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("LLAMA_API_KEY", &key)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--ctx-size",
            "2048",
            "--parallel",
            "1",
            "--jinja",
            "--no-webui",
            "--log-disable",
            "--n-gpu-layers",
            if cfg!(target_os = "macos") { "99" } else { "0" },
            "--model",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    drop(listener);
    let _engine = start_engine(&mut command)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .build()?;
    let base = format!("http://127.0.0.1:{port}");
    wait_until_ready(&client, &base, Duration::from_secs(90)).await?;
    let response = client.post(format!("{base}/v1/chat/completions")).bearer_auth(key)
        .json(&serde_json::json!({"model": MODEL_ID, "temperature": 0, "max_tokens": 512,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]}))
        .send().await.context("Local Scribe request failed")?;
    if !response.status().is_success() {
        return Err(anyhow!("Local Scribe could not process this draft"));
    }
    Ok(response.json().await?)
}

async fn wait_until_ready(client: &reqwest::Client, base: &str, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        loop {
            {
                let mut guard = CHILD
                    .lock()
                    .map_err(|_| anyhow!("Scribe engine lock failed"))?;
                let child = guard.as_mut().context("Local Scribe stopped")?;
                if child.process.try_wait()?.is_some() {
                    return Err(anyhow!(
                        "Local Scribe could not load. Free some memory and try again."
                    ));
                }
            }
            if let Ok(response) = client
                .get(format!("{base}/health"))
                .timeout(Duration::from_secs(1))
                .send()
                .await
            {
                if response.status().is_success() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        Ok(())
    })
    .await
    .context("Local Scribe took too long to load")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_provider_never_probes_or_requires_external_servers() {
        let _request = REQUEST.lock().await;
        let settings = crate::model::AppSettings {
            cleanup_provider: crate::model::CleanupProvider::Local,
            cleanup_model: String::new(),
            ..Default::default()
        };
        let (protocol, endpoint) = crate::cleanup::resolve_endpoint(&settings).await.unwrap();
        assert_eq!(protocol, crate::cleanup::Protocol::Local);
        assert!(endpoint.is_empty());
        crate::cleanup::warm_up(settings).await;
        assert!(CHILD.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn failed_or_unresponsive_engine_is_reaped() {
        let _request = REQUEST.lock().await;
        for exits in [true, false] {
            #[cfg(unix)]
            let mut command = if exits {
                Command::new("/usr/bin/false")
            } else {
                let mut command = Command::new("/bin/sleep");
                command.arg("30");
                command
            };
            #[cfg(windows)]
            let mut command = {
                let mut command = Command::new("powershell.exe");
                command.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    if exits {
                        "exit 9"
                    } else {
                        "Start-Sleep -Seconds 30"
                    },
                ]);
                command
            };
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", reservation.local_addr().unwrap());
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let result = async {
                let _engine = start_engine(&mut command)?;
                wait_until_ready(
                    &client,
                    &base,
                    if exits {
                        Duration::from_secs(10)
                    } else {
                        Duration::from_millis(300)
                    },
                )
                .await
            }
            .await;
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains(if exits {
                    "could not load"
                } else {
                    "too long to load"
                }),
                "{error}"
            );
            assert!(CHILD.lock().unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn changed_model_does_not_reuse_a_verified_stamp() {
        let _request = REQUEST.lock().await;
        let path = std::env::temp_dir().join(format!("quill-scribe-stamp-{}", std::process::id()));
        std::fs::write(&path, b"bad model").unwrap();
        let metadata = path.metadata().unwrap();
        *VERIFIED_MODEL.lock().unwrap() = Some(VerifiedModel {
            path: path.clone(),
            length: metadata.len(),
            modified: metadata.modified().unwrap(),
        });
        verify_model(&path).await.unwrap();
        std::fs::write(&path, b"new model").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(metadata.modified().unwrap() + Duration::from_secs(5))
            .unwrap();
        assert!(verify_model(&path)
            .await
            .unwrap_err()
            .to_string()
            .contains("integrity check"));
        *VERIFIED_MODEL.lock().unwrap() = None;
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires the pinned 2.50 GB model and built engine; set QUILL_SCRIBE_TEST_MODEL"]
    async fn managed_engine_synthetic_draft() {
        let path = PathBuf::from(
            std::env::var_os("QUILL_SCRIBE_TEST_MODEL").expect("Set QUILL_SCRIBE_TEST_MODEL"),
        );
        MODEL_PATH.set(path).unwrap();
        let settings = crate::model::AppSettings {
            cleanup_provider: crate::model::CleanupProvider::Local,
            ..Default::default()
        };
        let cases = [
            (
                "The demo is on Sunday, sorry Monday, at 11 am.",
                vec!["monday", "11"],
                vec!["sunday"],
            ),
            (
                "We need sixteen, sorry sixty copies for the workshop.",
                vec!["sixty"],
                vec!["sixteen"],
            ),
            (
                "Let's meet Tuesday, sorry Thursday at 3 pm.",
                vec!["thursday", "3"],
                vec!["tuesday", "can't", "cannot"],
            ),
            (
                "The budget is 300 dollars, no, 450 dollars.",
                vec!["450"],
                vec!["300"],
            ),
            (
                "Send the invoice to Mira, I mean Priya, by Friday.",
                vec!["priya", "friday"],
                vec!["mira"],
            ),
            (
                "I cannot attend on Monday because the train is cancelled.",
                vec!["monday", "train"],
                vec!["i can attend", "i will attend"],
            ),
            (
                "We tested version 2.4 on port 8080 and found 17 errors.",
                vec!["2.4", "8080", "17"],
                vec![],
            ),
            (
                "Please explain why the Rust compiler rejects this lifetime.",
                vec!["rust", "lifetime"],
                vec!["certainly", "because", "borrow checker ensures"],
            ),
        ];
        let mut failures = Vec::new();
        for (intent, keep, omit) in cases {
            let request = crate::cleanup::ScribeRequest {
                intent,
                register: if intent.starts_with("Please explain") {
                    crate::register::Register::Prompt
                } else {
                    crate::register::Register::Generic
                },
                action: crate::cleanup::ScribeAction::Compose,
                selected_text: None,
                surrounding_before: None,
                surrounding_after: None,
                style: None,
            };
            let (draft, unguarded) = crate::cleanup::evaluate_managed_draft(&settings, &request)
                .await
                .unwrap();
            println!("Synthetic input: {intent}\nSynthetic Scribe output: {draft}");
            let lower = draft.to_lowercase();
            if !unguarded
                || keep.iter().any(|value| !lower.contains(value))
                || omit.iter().any(|value| lower.contains(value))
            {
                failures.push(intent);
            }
            assert!(
                CHILD.lock().unwrap().is_none(),
                "engine must exit after processing"
            );
        }
        assert!(failures.is_empty(), "Semantic failures: {failures:?}");
    }
}
