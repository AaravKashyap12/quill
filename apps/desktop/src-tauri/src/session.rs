use crate::asr::{AsrPass, WhisperServer};
use crate::audio::{AudioCapture, AudioSnapshot};
use crate::cleanup;
use crate::dictionary;
#[cfg(windows)]
use crate::downloads;
use crate::hotkeys;
use crate::injection;
use crate::metrics;
#[cfg(windows)]
use crate::model::ComputeBackend;
use crate::model::{AppSettings, HotkeyBehavior, HotkeyConfig, InjectionMode, Mode};
use crate::recovery;
use crate::register::Register;
use crate::streaming::{LocalAgreement, TimedWord};
use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const PASS_INTERVAL_MS: u64 = 700;
const MIN_AUDIO_MS: u64 = 800;
/// How often the audio recovery WAV is rewritten while a session is
/// actively recording. Writing the whole cumulative buffer every ASR pass
/// (~700 ms) makes long recordings quadratic in disk I/O; at 15 s cadence a
/// 20-min recording issues ~80 writes instead of ~1700, and each is
/// off-loaded to a background thread anyway.
const AUDIO_CHECKPOINT_INTERVAL_MS: u64 = 15_000;

#[derive(Default)]
struct SessionControlState {
    update_requested: bool,
    engine_stopped: bool,
    session_active: bool,
}

/// Coordinates the long-lived speech thread with the updater. Windows cannot
/// replace whisper.cpp DLLs while the sidecar has them mapped, so installation
/// waits for an explicit stopped acknowledgement rather than relying on app
/// process shutdown timing.
#[derive(Default)]
pub struct SessionControl {
    state: Mutex<SessionControlState>,
    changed: Condvar,
}

impl SessionControl {
    pub fn stop_for_update(&self, timeout: Duration) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The speech engine state is unavailable".to_string())?;
        if state.session_active {
            return Err(
                "Finish the current dictation before installing the update, then try again."
                    .to_string(),
            );
        }

        state.update_requested = true;
        self.changed.notify_all();
        let (mut state, result) = self
            .changed
            .wait_timeout_while(state, timeout, |state| !state.engine_stopped)
            .map_err(|_| "The speech engine state is unavailable".to_string())?;
        if result.timed_out() && !state.engine_stopped {
            state.update_requested = false;
            self.changed.notify_all();
            return Err(
                "Quill could not stop the local speech engine. Quit Quill from the tray and run the installer again."
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn resume_after_failed_update(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.update_requested = false;
            state.engine_stopped = false;
            self.changed.notify_all();
        }
    }

    fn stop_requested(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.update_requested)
            .unwrap_or(true)
    }

    fn park_engine_for_update(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.update_requested {
            return;
        }
        state.engine_stopped = true;
        self.changed.notify_all();
        while state.update_requested {
            let Ok(next) = self.changed.wait(state) else {
                return;
            };
            state = next;
        }
        state.engine_stopped = false;
    }

    fn try_start_session(self: &Arc<Self>) -> Option<SessionActivity> {
        let mut state = self.state.lock().ok()?;
        if state.update_requested || state.session_active {
            return None;
        }
        state.session_active = true;
        Some(SessionActivity(Arc::clone(self)))
    }

    fn finish_session(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.session_active = false;
            self.changed.notify_all();
        }
    }
}

struct SessionActivity(Arc<SessionControl>);

impl Drop for SessionActivity {
    fn drop(&mut self) {
        self.0.finish_session();
    }
}

pub fn spawn(
    app: AppHandle,
    settings: Arc<RwLock<AppSettings>>,
    hotkey_capture: Arc<AtomicBool>,
    control: Arc<SessionControl>,
) {
    std::thread::Builder::new()
        .name("quill-session".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else {
                emit_status(
                    &app,
                    "error",
                    None,
                    "Failed to create Quill's speech session runtime",
                    None,
                );
                return;
            };
            let mut last_failure = String::new();
            loop {
                control.park_engine_for_update();
                if let Err(error) = runtime.block_on(run(
                    app.clone(),
                    Arc::clone(&settings),
                    Arc::clone(&hotkey_capture),
                    Arc::clone(&control),
                )) {
                    if control.stop_requested() {
                        continue;
                    }
                    let error_text = error.to_string();
                    let waiting_for_model = error_text.contains("is not installed");
                    if error_text != last_failure {
                        if waiting_for_model {
                            tracing::info!("speech model is not installed; waiting for setup");
                        } else {
                            tracing::error!(%error, "speech session loop failed; restarting");
                        }
                        last_failure = error_text.clone();
                    }
                    hide_overlay(&app);
                    let status_message = if waiting_for_model {
                        "Waiting for the speech model download".to_owned()
                    } else {
                        format!("Restarting local speech engine: {error}")
                    };
                    emit_status(
                        &app,
                        if waiting_for_model {
                            "error"
                        } else {
                            "processing"
                        },
                        None,
                        &status_message,
                        None,
                    );
                    std::thread::sleep(Duration::from_secs(if waiting_for_model { 2 } else { 1 }));
                }
            }
        })
        .expect("failed to start Quill session thread");
}

async fn run(
    app: AppHandle,
    shared_settings: Arc<RwLock<AppSettings>>,
    hotkey_capture: Arc<AtomicBool>,
    control: Arc<SessionControl>,
) -> Result<()> {
    let startup_settings = read_settings(&shared_settings)?;
    emit_status(&app, "processing", None, "Loading whisper.cpp", None);
    let mut server = WhisperServer::start(&app, &startup_settings).await?;
    metrics::record(
        "whisperColdLoad",
        server.cold_load_ms,
        None,
        Some(server.backend_name()),
    )?;
    // Snapshot the engine-affecting settings the running server was booted
    // with, so we can detect drift each loop iteration and hot-swap the
    // server when the user changes model or compute backend.
    let mut loaded_model = startup_settings.whisper_model.clone();
    let mut loaded_backend = startup_settings.backend;
    #[cfg(windows)]
    let mut loaded_cuda_generation = downloads::cuda_runtime_generation(&app);
    emit_status(&app, "ready", None, server.ready_message(), None);
    // Clear any low-bit key history left from before this process/server
    // started so tap-to-lock never begins recording on launch or restart.
    let _ = hotkeys::poll_pair(
        &startup_settings.dictation_hotkey,
        &startup_settings.scribe_hotkey,
    );

    let mut dictation_key = KeyRuntime::default();
    let mut scribe_key = KeyRuntime::default();
    let mut active: Option<ActiveSession> = None;
    let mut deferred_insertions: Vec<DeferredInsertion> = Vec::new();

    loop {
        if control.stop_requested() {
            server.shutdown().await?;
            return Ok(());
        }
        let settings = read_settings(&shared_settings)?;

        // Hot-swap the whisper.cpp server when the user picks a different
        // model or compute backend. Guarded on `active.is_none()` because
        // reloading mid-utterance would drop the in-flight audio and produce
        // a mangled transcript. Language is passed per request, so it does
        // NOT require a restart. Failure to load the new engine bubbles up
        // to the outer supervisor loop, which will retry with the old
        // settings once they're re-persisted.
        #[cfg(windows)]
        let cuda_generation = downloads::cuda_runtime_generation(&app);
        #[cfg(windows)]
        let cuda_pack_changed = matches!(
            settings.backend,
            ComputeBackend::Auto | ComputeBackend::Cuda
        ) && cuda_generation != loaded_cuda_generation;
        #[cfg(not(windows))]
        let cuda_pack_changed = false;
        if active.is_none()
            && (settings.whisper_model != loaded_model
                || settings.backend != loaded_backend
                || cuda_pack_changed)
        {
            emit_status(
                &app,
                "processing",
                None,
                &format!("Reloading whisper.cpp with {}", settings.whisper_model),
                None,
            );
            server.shutdown().await?;
            server = WhisperServer::start(&app, &settings).await?;
            loaded_model = settings.whisper_model.clone();
            loaded_backend = settings.backend;
            #[cfg(windows)]
            {
                loaded_cuda_generation = cuda_generation;
            }
            emit_status(&app, "ready", None, server.ready_message(), None);
        }

        // Text that could not be safely delivered while its editor was in
        // the background is retried only after that original editor returns
        // to the foreground. It must never follow the user into Chrome (or
        // any other newly focused application).
        if active.is_none() {
            flush_deferred_insertions(&mut deferred_insertions, &app)?;
        }
        let (dictation_state, scribe_state) =
            hotkeys::poll_pair(&settings.dictation_hotkey, &settings.scribe_hotkey);
        // The settings UI captures a new shortcut by listening for a keypress
        // in a focused button. Actually firing the mode in parallel would open
        // the overlay every time the user pressed their current hotkey. While
        // capture is on we drop the poll result but leave the key runtime in
        // sync, so releasing the keys does not leave a stuck state.
        let capturing = hotkey_capture.load(Ordering::Relaxed);
        let (mut dictation_active, mut scribe_active) = if capturing {
            let _ = dictation_key.update(dictation_state, &settings.dictation_hotkey);
            let _ = scribe_key.update(scribe_state, &settings.scribe_hotkey);
            dictation_key.unlock();
            scribe_key.unlock();
            (false, false)
        } else {
            (
                dictation_key.update(dictation_state, &settings.dictation_hotkey),
                scribe_key.update(scribe_state, &settings.scribe_hotkey),
            )
        };
        // In tap-to-lock mode the hotkey itself is the mode switch. Starting
        // one mode must release the other mode's latch, otherwise stopping
        // Scribe can unexpectedly resume an older Dictation session.
        if dictation_key.just_pressed && dictation_active {
            scribe_key.unlock();
            scribe_active = false;
        } else if scribe_key.just_pressed && scribe_active {
            dictation_key.unlock();
            dictation_active = false;
        }
        let desired_mode = if scribe_active {
            Some(Mode::Scribe)
        } else if dictation_active {
            Some(Mode::Dictation)
        } else {
            None
        };
        let current_mode = active.as_ref().map(|session| session.mode);

        if desired_mode != current_mode {
            let mut carried_target = None;
            if let Some(mut session) = active.take() {
                session.finish(&server, &app).await?;
                carried_target = Some(session.target.clone());
                if let Some(deferred) = session.take_deferred_insertion() {
                    deferred_insertions.push(deferred);
                    emit_status(
                        &app,
                        "ready",
                        None,
                        "Text queued — return to the original editor to insert",
                        None,
                    );
                }
            }
            if let Some(mode) = desired_mode {
                if let Some(activity) = control.try_start_session() {
                    active = Some(ActiveSession::start(
                        mode,
                        settings.clone(),
                        &app,
                        carried_target,
                        activity,
                    )?);
                } else {
                    dictation_key.unlock();
                    scribe_key.unlock();
                }
            }
        }

        if let Some(session) = active.as_mut() {
            session.flush_pending_if_target_is_foreground(&app)?;
            if session.last_meter_emit.elapsed() >= Duration::from_millis(42) {
                session.emit_audio_levels(&app);
            }
            if session.last_pass.elapsed() >= Duration::from_millis(PASS_INTERVAL_MS) {
                let snapshot = session.audio.snapshot()?;
                if snapshot.duration_ms >= MIN_AUDIO_MS
                    && snapshot.duration_ms >= session.last_audio_ms + PASS_INTERVAL_MS / 2
                {
                    session.process_pass(&server, &app, snapshot).await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(12)).await;
    }
}

#[derive(Default)]
struct KeyRuntime {
    was_down: bool,
    locked: bool,
    just_pressed: bool,
}

impl KeyRuntime {
    fn update(&mut self, state: hotkeys::HotkeyState, config: &HotkeyConfig) -> bool {
        // Prefer the physical rising edge. The low GetAsyncKeyState bit is
        // retained only for taps that happen entirely between two polls.
        let pressed =
            (state.down && !self.was_down) || (state.pressed && !state.down && !self.was_down);
        self.was_down = state.down;
        self.just_pressed = pressed;
        match config.behavior {
            HotkeyBehavior::Hold => state.down,
            HotkeyBehavior::TapToLock => {
                if pressed {
                    self.locked = !self.locked;
                }
                self.locked
            }
        }
    }

    fn unlock(&mut self) {
        self.locked = false;
    }
}

struct ActiveSession {
    _activity: SessionActivity,
    recovery_id: String,
    mode: Mode,
    settings: AppSettings,
    target: injection::InsertionTarget,
    audio: AudioCapture,
    started: Instant,
    last_pass: Instant,
    last_meter_emit: Instant,
    last_audio_ms: u64,
    dictation_agreement: LocalAgreement,
    last_hypothesis: Vec<TimedWord>,
    transcript: String,
    pending_insertion: String,
    pending_notice_sent: bool,
    review_opened: bool,
    /// When the last audio-recovery WAV was written. Throttles the write so
    /// long recordings don't rewrite an ever-growing buffer every ASR pass.
    last_audio_checkpoint: Option<Instant>,
}

impl ActiveSession {
    fn start(
        mode: Mode,
        settings: AppSettings,
        app: &AppHandle,
        carried_target: Option<injection::InsertionTarget>,
        activity: SessionActivity,
    ) -> Result<Self> {
        // Capture before showing the overlay: the target is the text field
        // active at the hotkey, not Quill's own affordance or a later app.
        let target = match carried_target {
            Some(target) => target,
            None => injection::capture_target()?,
        };
        let audio = AudioCapture::start(settings.audio_input_device.as_deref())?;
        tracing::info!(
            mode = mode_name(mode),
            device = %audio.device_name,
            "session started"
        );
        show_overlay(app, mode, true);
        emit_status(
            app,
            "listening",
            Some(mode),
            if mode == Mode::Dictation {
                "Dictation listening"
            } else {
                "Scribe listening"
            },
            (mode == Mode::Scribe).then_some(settings.cleanup_base_url.as_str()),
        );
        Ok(Self {
            _activity: activity,
            recovery_id: recovery::new_recovery_id(),
            mode,
            settings,
            target,
            audio,
            started: Instant::now(),
            last_pass: Instant::now(),
            last_meter_emit: Instant::now(),
            last_audio_ms: 0,
            dictation_agreement: LocalAgreement::default(),
            last_hypothesis: Vec::new(),
            transcript: String::new(),
            pending_insertion: String::new(),
            pending_notice_sent: false,
            review_opened: false,
            last_audio_checkpoint: None,
        })
    }

    async fn process_pass(
        &mut self,
        server: &WhisperServer,
        app: &AppHandle,
        snapshot: AudioSnapshot,
    ) -> Result<()> {
        self.last_pass = Instant::now();
        self.last_audio_ms = snapshot.duration_ms;
        let peak = audio_peak(&snapshot);
        metrics::record(
            "audioSnapshot",
            snapshot.duration_ms.into(),
            Some(mode_name(self.mode)),
            Some(&format!("peak={peak:.5}")),
        )?;
        if peak < 0.002 {
            return Ok(());
        }
        emit_status(
            app,
            "processing",
            Some(self.mode),
            server.activity_message(),
            (self.mode == Mode::Scribe).then_some(self.settings.cleanup_base_url.as_str()),
        );
        let pass = server.transcribe(&self.settings, &snapshot.samples).await?;
        // Expand literal dictionary entries before Dictation and Scribe
        // diverge. Rebuilding the timed words here also lets LocalAgreement
        // stabilize the expanded form instead of injecting the spoken trigger.
        let pass = apply_dictionary_to_pass(pass, &self.settings.dictionary);
        metrics::record(
            "warmTranscription",
            pass.latency_ms,
            Some(mode_name(self.mode)),
            None,
        )?;
        self.transcript = pass.text.clone();
        self.last_hypothesis = pass.words.clone();
        match self.mode {
            Mode::Dictation => self.commit_dictation(pass, false, app)?,
            // Scribe must see the complete correction before it can safely
            // inject anything. Incremental per-chunk cleanup previously typed
            // the abandoned wording before "no/sorry" arrived.
            Mode::Scribe => {}
        }
        // Transcript is always checkpointed — the small, safer artefact,
        // written on-thread since serialising a few KB is cheap. Audio is
        // opt-in AND throttled AND off-threaded: writing the whole cumulative
        // buffer every pass would produce quadratic disk I/O on long
        // recordings and would block this single-threaded runtime while the
        // encode + flush completed. Toggling `keep_recovery_audio` off
        // opportunistically purges any residual WAV.
        let _ = recovery::write_transcript(
            &self.recovery_id,
            self.mode,
            &self.transcript,
            self.settings.keep_recovery_audio,
        );
        if self.settings.keep_recovery_audio {
            let due = self
                .last_audio_checkpoint
                .map(|last| last.elapsed().as_millis() as u64 >= AUDIO_CHECKPOINT_INTERVAL_MS)
                .unwrap_or(true);
            if due {
                recovery::write_audio_async(self.recovery_id.clone(), snapshot.samples.clone());
                self.last_audio_checkpoint = Some(Instant::now());
            }
        } else {
            recovery::purge_audio_if_disabled(false);
        }
        emit_status(
            app,
            "listening",
            Some(self.mode),
            if self.mode == Mode::Dictation {
                "Dictation listening"
            } else {
                "Scribe buffering corrections"
            },
            (self.mode == Mode::Scribe).then_some(self.settings.cleanup_base_url.as_str()),
        );
        Ok(())
    }

    async fn finish(&mut self, server: &WhisperServer, app: &AppHandle) -> Result<()> {
        // A stop tap must dismiss the recording affordance immediately. The
        // final CUDA/cleanup pass can continue without leaving a stale overlay
        // covering the user's editor.
        show_overlay(app, self.mode, false);
        let _ = app.emit(
            "runtime://audio-level",
            serde_json::json!({
                "mode": mode_name(self.mode),
                "levels": vec![0.0_f32; crate::audio::VISUALIZER_BARS],
            }),
        );
        emit_status(
            app,
            "processing",
            Some(self.mode),
            if self.mode == Mode::Dictation {
                "Finishing dictation"
            } else {
                "Resolving final wording"
            },
            (self.mode == Mode::Scribe).then_some(self.settings.cleanup_base_url.as_str()),
        );
        let snapshot = self.audio.snapshot()?;
        if snapshot.duration_ms >= 250 && audio_peak(&snapshot) >= 0.002 {
            if self.mode == Mode::Scribe {
                crate::review::show_processing(app, "Transcribing on your device")?;
            }
            let pass = match server.transcribe(&self.settings, &snapshot.samples).await {
                Ok(pass) => pass,
                Err(error) => {
                    if self.mode == Mode::Scribe {
                        crate::review::hide_processing(app);
                    }
                    return Err(error);
                }
            };
            let pass = apply_dictionary_to_pass(pass, &self.settings.dictionary);
            metrics::record(
                "warmTranscription",
                pass.latency_ms,
                Some(mode_name(self.mode)),
                Some("final pass"),
            )?;
            self.transcript = pass.text.clone();
            self.last_hypothesis = pass.words.clone();
            match self.mode {
                Mode::Dictation => self.commit_dictation(pass, true, app)?,
                Mode::Scribe => {
                    if let Err(error) = self.commit_scribe(pass, app).await {
                        crate::review::hide_processing(app);
                        return Err(error);
                    }
                }
            }
        }
        // Preserve the recovery checkpoint while text is waiting for the
        // captured editor. A crash must not turn safe queuing into data loss.
        if self.pending_insertion.is_empty() && !self.review_opened {
            match recovery::clear_if_matches(&self.recovery_id) {
                Ok(recovery::ClearOutcome::Cleared | recovery::ClearOutcome::Missing) => {}
                Ok(recovery::ClearOutcome::Stale) => tracing::info!(
                    recovery_id = %self.recovery_id,
                    "finished session did not clear a newer recovery checkpoint"
                ),
                Err(error) => tracing::warn!(%error, "failed to clear finished session recovery"),
            }
        }
        // Never log the transcript itself — that's user speech content and
        // has no place in a persistent log file. Word count is enough for
        // debugging session-completion issues.
        tracing::info!(
            mode = mode_name(self.mode),
            elapsed_ms = self.started.elapsed().as_millis(),
            transcript_words = self.transcript.split_whitespace().count(),
            "session finished"
        );
        if self.review_opened {
            emit_status(
                app,
                "processing",
                Some(Mode::Scribe),
                "Review your Scribe draft",
                None,
            );
        } else {
            emit_status(app, "ready", None, "Ready", None);
        }
        Ok(())
    }

    fn commit_dictation(&mut self, pass: AsrPass, final_pass: bool, app: &AppHandle) -> Result<()> {
        let committed = if final_pass {
            self.dictation_agreement.flush(pass.words)
        } else {
            // A rolling pass can stop between the words of a dictionary
            // trigger. Do not let LocalAgreement type that proper prefix: a
            // later expanded pass cannot retract text already sent to the
            // target editor. The final pass is deliberately not held back so
            // an unfinished phrase is never lost when the user stops.
            let stable_words = hold_back_dictionary_prefix(pass.words, &self.settings.dictionary);
            self.dictation_agreement.update(stable_words)
        };
        self.inject_words(&committed, "dictationWordCommit", app)
    }

    async fn commit_scribe(&mut self, pass: AsrPass, app: &AppHandle) -> Result<()> {
        if !pass.words.is_empty() {
            let source_words = &pass.words;
            let source = render_words(source_words);
            let detected_register = crate::register::resolve(&self.target);
            let register =
                resolve_scribe_register(detected_register, self.settings.default_register);
            crate::review::update_processing(app, "Resolving spoken corrections");
            let cleanup_started = Instant::now();
            let (cleaned, warning) = match cleanup::clean(&self.settings, &source, register).await {
                Ok(cleaned) => (cleaned, None),
                Err(error) => {
                    // Log only the error class and source-length. The source
                    // string is user speech and must never be persisted.
                    let detail = format!(
                        "source_words={}; error={error}",
                        source.split_whitespace().count()
                    );
                    metrics::record(
                        "scribeCleanupError",
                        cleanup_started.elapsed().as_millis(),
                        Some("scribe"),
                        Some(&detail),
                    )?;
                    tracing::error!(
                        source_words = source.split_whitespace().count(),
                        %error,
                        "Scribe cleanup failed; opening a safe editable draft"
                    );
                    (
                        cleanup::safe_fallback(&source),
                        Some(format!(
                            "Local cleanup was unavailable, so Quill preserved a safe draft: {error}"
                        )),
                    )
                }
            };
            crate::review::present(
                app,
                crate::review::ReviewRequest {
                    recovery_id: self.recovery_id.clone(),
                    source,
                    draft: cleaned,
                    warning,
                    register,
                    settings: self.settings.clone(),
                    target: self.target.clone(),
                },
            )?;
            self.review_opened = true;
            metrics::record(
                "scribeReviewReady",
                cleanup_started.elapsed().as_millis(),
                Some("scribe"),
                None,
            )?;
        } else {
            crate::review::hide_processing(app);
        }
        Ok(())
    }

    fn inject_words(&mut self, words: &[TimedWord], metric: &str, app: &AppHandle) -> Result<()> {
        if words.is_empty() {
            return Ok(());
        }
        let text = render_words(words);
        let last_word_end = words.last().map(|word| word.end_ms).unwrap_or(0);
        self.inject_text(&text, last_word_end, metric, app)
    }

    fn inject_text(
        &mut self,
        text: &str,
        last_word_end_ms: u64,
        metric: &str,
        app: &AppHandle,
    ) -> Result<()> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut insertion = trimmed.to_owned();
        insertion.push(' ');
        let mut batch = std::mem::take(&mut self.pending_insertion);
        batch.push_str(&insertion);
        let uses_clipboard = matches!(self.settings.injection_mode, InjectionMode::Clipboard);

        let outcome = injection::inject_text_to_target(&self.target, &batch, uses_clipboard);
        match outcome {
            Ok(injection::TargetInjection::Inserted) => {
                self.pending_notice_sent = false;
            }
            Ok(injection::TargetInjection::Queued) => {
                self.pending_insertion = batch;
                self.emit_pending_status(
                    app,
                    "Text queued — return to the original editor to insert",
                );
                return Ok(());
            }
            Ok(injection::TargetInjection::Unavailable) => {
                self.pending_insertion = batch;
                self.emit_pending_status(
                    app,
                    "Original editor closed — text is held safely and will not be sent elsewhere",
                );
                return Ok(());
            }
            Err(error) => {
                self.pending_insertion = batch;
                tracing::error!(%error, "could not deliver text to the pinned editor");
                self.emit_pending_status(
                    app,
                    "Text queued — Quill could not safely reach the original editor",
                );
                return Ok(());
            }
        }
        let lag_ms = self
            .started
            .elapsed()
            .as_millis()
            .saturating_sub(u128::from(last_word_end_ms));
        let injected_words = trimmed.split_whitespace().count();
        let metric_detail = format!("words={injected_words}");
        metrics::record(
            metric,
            lag_ms,
            Some(mode_name(self.mode)),
            Some(&metric_detail),
        )?;
        tracing::info!(
            mode = mode_name(self.mode),
            lag_ms,
            injected_words,
            "text injected"
        );
        Ok(())
    }

    fn flush_pending_if_target_is_foreground(&mut self, app: &AppHandle) -> Result<()> {
        if self.pending_insertion.is_empty() {
            return Ok(());
        }
        if !injection::target_is_available(&self.target) {
            self.emit_pending_status(
                app,
                "Original editor closed — text is held safely and will not be sent elsewhere",
            );
            return Ok(());
        }
        if !injection::target_is_foreground(&self.target) {
            return Ok(());
        }

        let batch = std::mem::take(&mut self.pending_insertion);
        let uses_clipboard = matches!(self.settings.injection_mode, InjectionMode::Clipboard);
        match injection::inject_text_to_target(&self.target, &batch, uses_clipboard) {
            Ok(injection::TargetInjection::Inserted) => {
                self.pending_notice_sent = false;
                tracing::info!("queued text inserted after the original editor regained focus");
            }
            Ok(injection::TargetInjection::Queued) => {
                self.pending_insertion = batch;
            }
            Ok(injection::TargetInjection::Unavailable) => {
                self.pending_insertion = batch;
                self.emit_pending_status(
                    app,
                    "Original editor closed — text is held safely and will not be sent elsewhere",
                );
            }
            Err(error) => {
                self.pending_insertion = batch;
                tracing::error!(%error, "could not flush text queued for the original editor");
                self.emit_pending_status(
                    app,
                    "Text remains queued — Quill could not safely reach the original editor",
                );
            }
        }
        Ok(())
    }

    fn emit_pending_status(&mut self, app: &AppHandle, message: &str) {
        if self.pending_notice_sent {
            return;
        }
        self.pending_notice_sent = true;
        emit_status(app, "listening", Some(self.mode), message, None);
    }

    fn emit_audio_levels(&mut self, app: &AppHandle) {
        self.last_meter_emit = Instant::now();
        let _ = app.emit(
            "runtime://audio-level",
            serde_json::json!({
                "mode": mode_name(self.mode),
                "levels": self.audio.visual_levels(),
            }),
        );
    }

    fn take_deferred_insertion(&mut self) -> Option<DeferredInsertion> {
        if self.pending_insertion.is_empty() {
            return None;
        }
        Some(DeferredInsertion {
            target: self.target.clone(),
            text: std::mem::take(&mut self.pending_insertion),
            use_clipboard: matches!(self.settings.injection_mode, InjectionMode::Clipboard),
            unavailable_notice_sent: false,
        })
    }
}

fn resolve_scribe_register(detected: Register, default_register: Register) -> Register {
    match detected {
        Register::Generic => default_register,
        detected => detected,
    }
}

struct DeferredInsertion {
    target: injection::InsertionTarget,
    text: String,
    use_clipboard: bool,
    unavailable_notice_sent: bool,
}

fn flush_deferred_insertions(
    deferred_insertions: &mut Vec<DeferredInsertion>,
    app: &AppHandle,
) -> Result<()> {
    let mut remaining = Vec::with_capacity(deferred_insertions.len());
    for mut deferred in std::mem::take(deferred_insertions) {
        if !injection::target_is_available(&deferred.target) {
            if !deferred.unavailable_notice_sent {
                emit_status(
                    app,
                    "ready",
                    None,
                    "Original editor closed — queued text will not be sent to another app",
                    None,
                );
                deferred.unavailable_notice_sent = true;
            }
            remaining.push(deferred);
            continue;
        }
        if !injection::target_is_foreground(&deferred.target) {
            remaining.push(deferred);
            continue;
        }

        match injection::inject_text_to_target(
            &deferred.target,
            &deferred.text,
            deferred.use_clipboard,
        ) {
            Ok(injection::TargetInjection::Inserted) => {
                tracing::info!("queued text inserted after the original editor regained focus");
                metrics::record(
                    "deferredTargetFlush",
                    0,
                    None,
                    Some("pinned target regained focus"),
                )?;
                emit_status(
                    app,
                    "ready",
                    None,
                    "Queued text inserted into the original editor",
                    None,
                );
            }
            Ok(injection::TargetInjection::Queued) => remaining.push(deferred),
            Ok(injection::TargetInjection::Unavailable) => {
                if !deferred.unavailable_notice_sent {
                    emit_status(
                        app,
                        "ready",
                        None,
                        "Original editor closed — queued text will not be sent to another app",
                        None,
                    );
                    deferred.unavailable_notice_sent = true;
                }
                remaining.push(deferred);
            }
            Err(error) => {
                tracing::error!(%error, "could not flush text queued for the original editor");
                remaining.push(deferred);
            }
        }
    }
    *deferred_insertions = remaining;
    Ok(())
}

#[cfg(test)]
fn common_prefix_len(left: &[TimedWord], right: &[TimedWord]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left.text.eq_ignore_ascii_case(&right.text))
        .count()
}

fn render_words(words: &[TimedWord]) -> String {
    let mut output = String::new();
    for word in words {
        let token = word.text.trim();
        if token.is_empty() {
            continue;
        }
        if !output.is_empty() && !starts_with_tight_punctuation(token) {
            output.push(' ');
        }
        output.push_str(token);
    }
    output
}

fn apply_dictionary_to_pass(
    mut pass: AsrPass,
    entries: &[crate::model::DictionaryEntry],
) -> AsrPass {
    if entries.is_empty() || pass.words.is_empty() {
        return pass;
    }
    let source = render_words(&pass.words);
    let replaced = dictionary::apply(&source, entries);
    if replaced == source {
        return pass;
    }

    let start_ms = pass.words.first().map(|word| word.start_ms).unwrap_or(0);
    let end_ms = pass
        .words
        .last()
        .map(|word| word.end_ms)
        .unwrap_or(start_ms);
    let tokens: Vec<&str> = replaced.split_whitespace().collect();
    let token_count = tokens.len() as u64;
    let duration_ms = end_ms.saturating_sub(start_ms);
    pass.words = tokens
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let index = index as u64;
            TimedWord {
                text: text.to_owned(),
                start_ms: start_ms + duration_ms.saturating_mul(index) / token_count,
                end_ms: start_ms + duration_ms.saturating_mul(index + 1) / token_count,
            }
        })
        .collect();
    pass.text = replaced;
    // All words currently receive interpolated timings after a replacement.
    // Keep that limitation explicit so future pause/prosody consumers cannot
    // accidentally treat these values as measurements from whisper.cpp.
    pass.timings_synthetic = true;
    pass
}

/// Remove the longest trailing proper prefix of any multi-word dictionary
/// trigger from an incremental hypothesis. The words are reconsidered on the
/// next cumulative ASR pass; this only delays them, it never deletes final
/// dictated text.
fn hold_back_dictionary_prefix(
    mut words: Vec<TimedWord>,
    entries: &[crate::model::DictionaryEntry],
) -> Vec<TimedWord> {
    if words.is_empty() || entries.is_empty() {
        return words;
    }

    let hypothesis: Vec<String> = words
        .iter()
        .map(|word| normalize_dictionary_token(&word.text))
        .collect();
    let mut hold_back = 0usize;

    for entry in entries {
        let trigger: Vec<String> = entry
            .spoken
            .split_whitespace()
            .map(normalize_dictionary_token)
            .filter(|token| !token.is_empty())
            .collect();
        if trigger.len() < 2 {
            continue;
        }

        let max_prefix = (trigger.len() - 1).min(hypothesis.len());
        for prefix_len in (1..=max_prefix).rev() {
            let tail_start = hypothesis.len() - prefix_len;
            if hypothesis[tail_start..] == trigger[..prefix_len] {
                hold_back = hold_back.max(prefix_len);
                break;
            }
        }
    }

    words.truncate(words.len().saturating_sub(hold_back));
    words
}

fn normalize_dictionary_token(token: &str) -> String {
    token
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn starts_with_tight_punctuation(text: &str) -> bool {
    text.starts_with([',', '.', '!', '?', ':', ';', ')', ']', '}'])
}

fn audio_peak(snapshot: &AudioSnapshot) -> f32 {
    snapshot
        .samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

fn read_settings(settings: &Arc<RwLock<AppSettings>>) -> Result<AppSettings> {
    settings
        .read()
        .map(|settings| settings.clone())
        .map_err(|_| anyhow!("settings lock was poisoned"))
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Dictation => "dictation",
        Mode::Scribe => "scribe",
    }
}

fn emit_status(
    app: &AppHandle,
    state: &str,
    mode: Option<Mode>,
    message: &str,
    provider: Option<&str>,
) {
    let payload = serde_json::json!({
        "state": state,
        "mode": mode.map(mode_name),
        "message": message,
        "provider": provider,
    });
    let _ = app.emit("runtime://status", payload);
}

fn show_overlay(app: &AppHandle, mode: Mode, visible: bool) {
    if visible {
        let selected_label = match mode {
            Mode::Dictation => "overlay",
            Mode::Scribe => "scribe-overlay",
        };
        let other_label = match mode {
            Mode::Dictation => "scribe-overlay",
            Mode::Scribe => "overlay",
        };
        if let Some(other) = app.get_webview_window(other_label) {
            let _ = other.hide();
        }
        if let Some(overlay) = app.get_webview_window(selected_label) {
            crate::position_overlay_bottom_center(&overlay);
            let _ = overlay.show();
        }
    } else {
        hide_overlay(app);
    }
}

fn hide_overlay(app: &AppHandle) {
    for label in ["overlay", "scribe-overlay"] {
        if let Some(overlay) = app.get_webview_window(label) {
            let _ = overlay.hide();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_refuses_to_interrupt_an_active_session() {
        let control = Arc::new(SessionControl::default());
        let activity = control.try_start_session().expect("session should start");

        let error = control
            .stop_for_update(Duration::from_millis(10))
            .expect_err("active speech must block installation");
        assert!(error.contains("Finish the current dictation"));

        drop(activity);
        assert!(control.try_start_session().is_some());
    }

    #[test]
    fn updater_waits_for_engine_stopped_acknowledgement() {
        let control = Arc::new(SessionControl::default());
        let worker_control = Arc::clone(&control);
        let worker = std::thread::spawn(move || {
            while !worker_control.stop_requested() {
                std::thread::yield_now();
            }
            worker_control.park_engine_for_update();
        });

        control
            .stop_for_update(Duration::from_secs(1))
            .expect("engine should acknowledge update stop");
        control.resume_after_failed_update();
        worker.join().expect("engine worker should resume");
    }
    use crate::model::{DictionaryEntry, DictionaryKind};

    #[test]
    fn detected_register_overrides_the_default_profile() {
        assert_eq!(
            resolve_scribe_register(Register::Email, Register::Chat),
            Register::Email
        );
    }

    #[test]
    fn generic_detection_uses_the_default_profile() {
        assert_eq!(
            resolve_scribe_register(Register::Generic, Register::Email),
            Register::Email
        );
    }

    #[test]
    fn generic_detection_with_generic_profile_preserves_existing_behavior() {
        assert_eq!(
            resolve_scribe_register(Register::Generic, Register::Generic),
            Register::Generic
        );
    }

    fn words(text: &str) -> Vec<TimedWord> {
        text.split_whitespace()
            .enumerate()
            .map(|(index, text)| TimedWord {
                text: text.to_owned(),
                start_ms: index as u64 * 250,
                end_ms: (index as u64 + 1) * 250,
            })
            .collect()
    }

    #[test]
    fn scribe_prefix_requires_two_matching_passes() {
        assert_eq!(
            common_prefix_len(&words("one two three five"), &words("one two three four")),
            3
        );
    }

    #[test]
    fn punctuation_is_not_prefixed_with_a_space() {
        let mut tokens = words("Hello world");
        tokens.push(TimedWord {
            text: ".".into(),
            start_ms: 500,
            end_ms: 600,
        });
        assert_eq!(render_words(&tokens), "Hello world.");
    }

    #[test]
    fn dictionary_is_applied_to_the_timed_pass_before_mode_dispatch() {
        let pass = AsrPass {
            words: words("send the invoice to my email"),
            text: "send the invoice to my email".into(),
            latency_ms: 12,
            timings_synthetic: false,
        };
        let entries = [DictionaryEntry {
            id: "email".into(),
            spoken: "my email".into(),
            replacement: "aarav@example.com".into(),
            kind: DictionaryKind::Snippet,
        }];

        let replaced = apply_dictionary_to_pass(pass, &entries);

        assert_eq!(
            render_words(&replaced.words),
            "send the invoice to aarav@example.com"
        );
        assert_eq!(replaced.text, "send the invoice to aarav@example.com");
        assert_eq!(replaced.latency_ms, 12);
        assert!(replaced.timings_synthetic);
        assert_eq!(replaced.words.first().unwrap().start_ms, 0);
        assert_eq!(replaced.words.last().unwrap().end_ms, 1_500);
    }

    #[test]
    fn incremental_dictation_never_commits_a_bare_dictionary_prefix() {
        let entries = [DictionaryEntry {
            id: "email".into(),
            spoken: "my email".into(),
            replacement: "aarav@example.com".into(),
            kind: DictionaryKind::Snippet,
        }];
        let mut agreement = LocalAgreement::default();
        let mut emitted = Vec::<TimedWord>::new();

        // Two matching partial passes would normally make LocalAgreement type
        // "my". Two complete passes then stabilize the expanded address.
        for transcript in [
            "send it to my",
            "send it to my",
            "send it to my email",
            "send it to my email",
        ] {
            let pass = apply_dictionary_to_pass(
                AsrPass {
                    words: words(transcript),
                    text: transcript.into(),
                    latency_ms: 1,
                    timings_synthetic: false,
                },
                &entries,
            );
            emitted.extend(agreement.update(hold_back_dictionary_prefix(pass.words, &entries)));
        }

        let emitted_text = render_words(&emitted);
        assert_eq!(emitted_text, "send it to aarav@example.com");
        assert!(!emitted
            .iter()
            .any(|word| { normalize_dictionary_token(&word.text) == "my" }));
    }

    #[test]
    fn final_dictation_flush_releases_an_unfinished_dictionary_prefix() {
        let entries = [DictionaryEntry {
            id: "email".into(),
            spoken: "my email".into(),
            replacement: "aarav@example.com".into(),
            kind: DictionaryKind::Snippet,
        }];
        let mut agreement = LocalAgreement::default();
        let partial = words("send it to my");

        agreement.update(hold_back_dictionary_prefix(partial.clone(), &entries));
        let committed = agreement.update(hold_back_dictionary_prefix(partial.clone(), &entries));
        assert_eq!(render_words(&committed), "send it to");

        // commit_dictation deliberately sends the unfiltered final pass to
        // flush(), preserving "my" if the user stops before saying "email".
        assert_eq!(render_words(&agreement.flush(partial)), "my");
    }

    #[test]
    fn tap_to_lock_second_press_stops() {
        let config = HotkeyConfig {
            modifiers: vec!["Ctrl".into()],
            key: "Space".into(),
            behavior: HotkeyBehavior::TapToLock,
        };
        let mut runtime = KeyRuntime::default();

        assert!(runtime.update(
            hotkeys::HotkeyState {
                down: true,
                pressed: true,
            },
            &config,
        ));
        assert!(runtime.update(hotkeys::HotkeyState::default(), &config));
        assert!(!runtime.update(
            hotkeys::HotkeyState {
                down: true,
                pressed: true,
            },
            &config,
        ));
    }
}
