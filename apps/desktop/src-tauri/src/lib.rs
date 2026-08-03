mod asr;
mod audio;
mod cleanup;
mod dictionary;
mod downloads;
mod hotkeys;
mod injection;
mod metrics;
mod model;
mod ollama;
mod recovery;
mod register;
mod review;
mod session;
mod settings;
mod streaming;

use crate::model::{AppSettings, Mode, ProviderStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_autostart::ManagerExt;

struct QuillState {
    settings: Arc<RwLock<AppSettings>>,
    /// When true, the session loop ignores the current hotkey state so the
    /// settings UI can capture a new shortcut without the mode firing at the
    /// same time. Toggled by the `set_hotkey_capture` command.
    hotkey_capture: Arc<AtomicBool>,
    _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

#[cfg(windows)]
struct SingleInstanceGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE},
    };

    let mutex_name: Vec<u16> = "Local\\Quill.Desktop.SingleInstance\0"
        .encode_utf16()
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if handle.is_null() {
        tracing::error!("could not create the Quill single-instance mutex");
        return None;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let title: Vec<u16> = "Quill\0".encode_utf16().collect();
        let window = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !window.is_null() {
            unsafe {
                ShowWindow(window, SW_RESTORE);
                SetForegroundWindow(window);
            }
        }
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        return None;
    }
    Some(SingleInstanceGuard(handle))
}

#[tauri::command]
fn get_settings(state: State<'_, QuillState>) -> AppSettings {
    state
        .settings
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, QuillState>,
    mut settings: AppSettings,
) -> Result<(), String> {
    settings.cap_dismissed_suggestions();
    // Capture whether cleanup-affecting or autostart fields changed, so we can
    // rewarm the LLM / update the OS autostart entry only when needed.
    let (should_rewarm, autostart_changed) = state
        .settings
        .read()
        .map(|current| {
            let rewarm = current.cleanup_model != settings.cleanup_model
                || current.cleanup_provider != settings.cleanup_provider
                || current.cleanup_base_url != settings.cleanup_base_url;
            let autostart = current.launch_at_startup != settings.launch_at_startup;
            (rewarm, autostart)
        })
        .unwrap_or((true, true));

    crate::settings::save(&settings).map_err(|error| error.to_string())?;
    if let Ok(mut current) = state.settings.write() {
        *current = settings.clone();
    }
    if should_rewarm {
        let warmup_settings = settings.clone();
        tauri::async_runtime::spawn(async move {
            cleanup::warm_up(warmup_settings).await;
        });
    }
    if autostart_changed {
        apply_autostart(&app, settings.launch_at_startup);
    }
    app.emit("settings://changed", settings)
        .map_err(|error| error.to_string())
}

/// Enable or disable the OS-level autostart entry to match the setting.
/// Best-effort — failures are logged but never surfaced to the user, because
/// nothing user-actionable is lost (they can toggle it again to retry).
fn apply_autostart(app: &AppHandle, enable: bool) {
    let manager = app.autolaunch();
    let result = if enable {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(error) = result {
        tracing::warn!(%error, enable, "failed to update OS autostart entry");
    }
}

#[tauri::command]
async fn detect_local_providers(
    state: State<'_, QuillState>,
) -> Result<Vec<ProviderStatus>, String> {
    // Snapshot settings synchronously — State's lock isn't held across the
    // await below (Tauri would refuse to compile that anyway).
    let settings_snapshot = state.settings.read().ok().map(|s| s.clone());
    let providers = cleanup::detect_providers().await;
    // If a local provider just became available (typically Ollama coming online
    // after we started), pre-warm the configured cleanup model so the next
    // Scribe call doesn't take 30s+ to return.
    if providers.iter().any(|p| p.available) {
        if let Some(settings) = settings_snapshot {
            tauri::async_runtime::spawn(async move {
                cleanup::warm_up(settings).await;
            });
        }
    }
    Ok(providers)
}

#[tauri::command]
fn list_audio_input_devices() -> Result<Vec<String>, String> {
    audio::input_devices().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_pending_recovery() -> Result<Option<recovery::RecoveryManifest>, String> {
    recovery::load_pending().map_err(|error| error.to_string())
}

#[tauri::command]
fn discard_recovery(id: String) -> Result<(), String> {
    match recovery::accept_or_discard(&id).map_err(|error| error.to_string())? {
        recovery::ClearOutcome::Cleared | recovery::ClearOutcome::Missing => Ok(()),
        recovery::ClearOutcome::Stale => Err(
            "A newer recording has replaced this recovery checkpoint. Its recovery data was kept."
                .to_owned(),
        ),
    }
}

#[tauri::command]
fn list_installed_whisper_models(app: AppHandle) -> Result<Vec<String>, String> {
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    let mut model_directories = vec![
        resource_root.join("resources").join("models"),
        resource_root.join("models"),
    ];
    if let Ok(user_dir) = downloads::user_model_dir(&app) {
        model_directories.push(user_dir);
    }
    let mut models = Vec::new();
    for directory in model_directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Some(model) = file_name
                .strip_prefix("ggml-")
                .and_then(|name| name.strip_suffix(".bin"))
            {
                models.push(model.to_owned());
            }
        }
    }
    models.sort();
    models.dedup();
    Ok(models)
}

/// Front-end lets the session know a hotkey recorder is capturing keys.
/// While captured, the session skips mode evaluation so pressing the same
/// combination writes it into the shortcut rather than triggering it.
#[tauri::command]
fn set_hotkey_capture(state: State<'_, QuillState>, capturing: bool) {
    state.hotkey_capture.store(capturing, Ordering::Relaxed);
}

#[tauri::command]
fn preview_mode(app: AppHandle, mode: Mode, active: bool) -> Result<(), String> {
    let selected_label = match mode {
        Mode::Dictation => "overlay",
        Mode::Scribe => "scribe-overlay",
    };
    let other_label = match mode {
        Mode::Dictation => "scribe-overlay",
        Mode::Scribe => "overlay",
    };
    let overlay = app
        .get_webview_window(selected_label)
        .ok_or_else(|| format!("{selected_label} window is unavailable"))?;
    if active {
        if let Some(other) = app.get_webview_window(other_label) {
            let _ = other.hide();
        }
        position_overlay_bottom_center(&overlay);
        overlay.show().map_err(|error| error.to_string())
    } else {
        for label in ["overlay", "scribe-overlay"] {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.hide();
            }
        }
        Ok(())
    }
}

/// Anchor a window to the bottom-centre of the primary monitor, with a
/// small margin above the taskbar/dock. Called each time the overlay is
/// shown so it follows resolution changes.
pub(crate) fn position_overlay_bottom_center(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let Ok(win) = window.outer_size() else {
        return;
    };
    let screen = monitor.size();
    let scale = monitor.scale_factor();
    let margin = (46.0 * scale).round() as i32;
    let x = ((screen.width as i32 - win.width as i32) / 2).max(0);
    let y = (screen.height as i32 - win.height as i32 - margin).max(0);
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Quill", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Quill", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .tooltip("Quill — local voice dictation")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state,
                ..
            } = event
            {
                if button_state == tauri::tray::MouseButtonState::Up {
                    show_main_window(tray.app_handle());
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Tint the Windows caption bar to match Quill's warm paper canvas.
///
/// Without this the native title bar renders as a solid dark band that becomes
/// the highest-contrast element on screen and visually belongs to a different
/// application. `DWMWA_CAPTION_COLOR` / `DWMWA_TEXT_COLOR` are Windows 11
/// (build 22000+) attributes; on Windows 10 the calls simply return a failure
/// HRESULT which we ignore, leaving the default frame intact.
#[cfg(windows)]
fn theme_window_caption(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    let Ok(handle) = window.hwnd() else {
        return;
    };
    let hwnd = handle.0 as HWND;

    // DWM takes 0x00BBGGRR, which is byte-reversed from CSS hex.
    // Caption #FCFBF8 -> 0x00F8FBFC, text #1C1D1C -> 0x001C1D1C.
    let caption: u32 = 0x00F8_FBFC;
    let text: u32 = 0x001C_1D1C;
    let border: u32 = 0x00E3_EAED;

    unsafe {
        for (attribute, value) in [
            (DWMWA_CAPTION_COLOR, caption),
            (DWMWA_TEXT_COLOR, text),
            (DWMWA_BORDER_COLOR, border),
        ] {
            // Ignore the HRESULT: unsupported on Windows 10, which is fine.
            DwmSetWindowAttribute(
                hwnd,
                attribute as u32,
                &value as *const u32 as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

/// True when Quill was launched from the OS autostart entry, which passes
/// `--minimized` so we know to skip popping the settings window into the
/// user's face immediately after login. The app is still reachable via the
/// tray icon, so `--minimized` is a strict UX preference, not a functional
/// difference.
fn started_minimized() -> bool {
    std::env::args().any(|arg| arg == "--minimized")
}

pub fn run() {
    let log_directory = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("quill")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_directory);
    // Daily rotation with a 7-file cap keeps the log directory bounded. The
    // previous `rolling::never` writer let quill.log grow forever, which is
    // fine for a debug tool but wrong for an always-on background app that
    // handles user speech.
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("quill")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_directory)
        .unwrap_or_else(|_| tracing_appender::rolling::never(&log_directory, "quill.log"));
    let (file_writer, log_guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_ansi(false)
        .with_writer(file_writer)
        .init();

    #[cfg(windows)]
    let Some(_single_instance) = acquire_single_instance() else {
        return;
    };

    let shared_settings = Arc::new(RwLock::new(settings::load()));
    #[cfg(windows)]
    let session_settings = Arc::clone(&shared_settings);
    let warmup_settings = Arc::clone(&shared_settings);
    let hotkey_capture = Arc::new(AtomicBool::new(false));
    let session_hotkey_capture = Arc::clone(&hotkey_capture);
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Quill")
                .args(["--minimized".to_string()])
                .build(),
        )
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(QuillState {
            settings: shared_settings,
            hotkey_capture,
            _log_guard: log_guard,
        })
        .manage(review::ReviewStore::default())
        .manage(downloads::DownloadState::default())
        .manage(ollama::OllamaPullState::default())
        .setup(move |app| {
            setup_tray(app)?;

            // Bring the native frame into the design rather than inheriting a
            // dark caption bar that clashes with the warm paper interface.
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                theme_window_caption(&window);
            }

            // When launched from OS autostart (`--minimized`), keep the main
            // window hidden so the user doesn't get an unsolicited settings
            // window popping up at login. Tray icon remains the entry point.
            if started_minimized() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            #[cfg(windows)]
            session::spawn(
                app.handle().clone(),
                Arc::clone(&session_settings),
                Arc::clone(&session_hotkey_capture),
            );
            #[cfg(not(windows))]
            let _ = &session_hotkey_capture;

            // Reconcile OS autostart with the persisted setting on every
            // launch, so a manually-cleared registry entry re-arms itself.
            if let Ok(guard) = warmup_settings.read() {
                apply_autostart(app.handle(), guard.launch_at_startup);
            }

            // If a recovery checkpoint survived a prior crash, announce it so
            // the UI can show a Recover/Discard banner. Payload is delivered
            // via event so the frontend doesn't need to poll; a Tauri command
            // (`get_pending_recovery`) is also available for the initial
            // render before the event listener attaches.
            //
            // A corrupt manifest is quarantined (renamed with a timestamp
            // suffix) so every subsequent launch doesn't re-hit the same
            // parse failure — the previous `if let Ok(Some(...))` silently
            // dropped the error and left the bad file in place forever.
            match recovery::load_pending() {
                Ok(Some(pending)) => {
                    let _ = app.handle().emit("recovery://pending", pending);
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(
                        %error,
                        "recovery manifest could not be parsed at startup"
                    );
                    match recovery::quarantine_corrupt_manifest() {
                        Ok(dest) => tracing::warn!(
                            quarantined = %dest.display(),
                            "corrupt recovery manifest quarantined"
                        ),
                        Err(rename_error) => tracing::error!(
                            %rename_error,
                            "failed to quarantine corrupt recovery manifest"
                        ),
                    }
                }
            }

            // Warm the cleanup model in the background so the first Scribe
            // call after startup doesn't wait 10–40s for the model to load.
            // 2s delay lets the UI paint and settings settle first.
            let warmup_settings = Arc::clone(&warmup_settings);
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let snapshot = match warmup_settings.read() {
                    Ok(guard) => guard.clone(),
                    Err(_) => return,
                };
                cleanup::warm_up(snapshot).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            detect_local_providers,
            get_pending_recovery,
            discard_recovery,
            list_audio_input_devices,
            list_installed_whisper_models,
            downloads::get_cuda_runtime_status,
            preview_mode,
            set_hotkey_capture,
            review::get_scribe_review,
            review::regenerate_scribe_review,
            review::accept_scribe_review,
            review::discard_scribe_review,
            downloads::download_whisper_model,
            downloads::cancel_whisper_download,
            downloads::delete_whisper_model,
            downloads::download_cuda_runtime,
            downloads::cancel_cuda_runtime_download,
            downloads::delete_cuda_runtime,
            ollama::pull_ollama_model,
            ollama::cancel_ollama_pull
        ])
        .run(tauri::generate_context!())
        .expect("error while running Quill");
}
