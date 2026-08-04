use crate::audio::pcm16_wav;
use crate::downloads;
#[cfg(windows)]
use crate::downloads::CudaRuntimeAvailability;
use crate::metrics;
use crate::model::{AppSettings, ComputeBackend, DictionaryEntry, DictionaryKind};
use crate::streaming::TimedWord;
use anyhow::{anyhow, Context, Result};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

const MAX_DICTIONARY_PROMPT_CHARS: usize = 800;

/// English-only Whisper models cannot decode other
/// languages. Feeding them a non-English `--language` flag makes whisper.cpp
/// hallucinate repeating English tokens. Coerce back to `en` and warn.
fn effective_language(model: &str, requested: &str) -> String {
    let is_en_only = model.ends_with(".en") || model == "distil-large-v3";
    let requested_norm = if requested.is_empty() {
        "auto"
    } else {
        requested
    };
    if is_en_only && requested_norm != "en" && requested_norm != "auto" {
        tracing::warn!(
            model,
            requested = requested_norm,
            "english-only model requested with non-english language; forcing en"
        );
        return "en".to_string();
    }
    requested_norm.to_string()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBackend {
    Cpu,
    Cuda,
    Metal,
}

pub struct WhisperServer {
    child: Child,
    client: reqwest::Client,
    endpoint: String,
    pub cold_load_ms: u128,
    pub backend: RuntimeBackend,
    pub cuda_pack_missing: bool,
}

impl WhisperServer {
    pub fn ready_message(&self) -> &'static str {
        if self.cuda_pack_missing {
            "Ready on CPU — CUDA runtime not installed"
        } else {
            match self.backend {
                RuntimeBackend::Cpu => "Ready on CPU",
                RuntimeBackend::Cuda => "Ready on CUDA",
                RuntimeBackend::Metal => "Ready on Metal",
            }
        }
    }

    pub fn activity_message(&self) -> &'static str {
        match self.backend {
            RuntimeBackend::Cpu => "Transcribing on CPU",
            RuntimeBackend::Cuda => "Transcribing on CUDA",
            RuntimeBackend::Metal => "Transcribing on Metal",
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            RuntimeBackend::Cpu => "CPU",
            RuntimeBackend::Cuda => "CUDA",
            RuntimeBackend::Metal => "Metal",
        }
    }
}

#[derive(Debug)]
pub struct AsrPass {
    pub words: Vec<TimedWord>,
    pub text: String,
    pub latency_ms: u128,
    /// True when a post-ASR transform rebuilt word timings rather than
    /// preserving measurements reported by whisper.cpp.
    pub timings_synthetic: bool,
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    segments: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    #[serde(default)]
    words: Vec<WhisperWord>,
}

#[derive(Debug, Deserialize)]
struct WhisperWord {
    word: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
}

fn dictionary_prompt(entries: &[DictionaryEntry]) -> (Option<String>, bool) {
    let mut prompt = String::new();
    let mut prompt_chars = 0usize;
    let mut truncated = false;

    for entry in entries
        .iter()
        .filter(|entry| entry.kind == DictionaryKind::Word)
    {
        let replacement = entry.replacement.trim();
        if replacement.is_empty() {
            continue;
        }
        let replacement_chars = replacement.chars().count();
        let separator_chars = usize::from(!prompt.is_empty()) * 2;
        if prompt_chars + separator_chars + replacement_chars > MAX_DICTIONARY_PROMPT_CHARS {
            truncated = true;
            break;
        }
        if !prompt.is_empty() {
            prompt.push_str(", ");
            prompt_chars += 2;
        }
        prompt.push_str(replacement);
        prompt_chars += replacement_chars;
    }

    ((!prompt.is_empty()).then_some(prompt), truncated)
}

fn normalized_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn strip_prompt_echo_text(text: &str, prompt: &str) -> String {
    let prompt_tokens = normalized_tokens(prompt);
    if prompt_tokens.is_empty() {
        return text.trim().to_owned();
    }

    let mut response_tokens = Vec::<(String, usize)>::new();
    let mut token = String::new();
    let mut token_end = 0usize;
    for (offset, character) in text.char_indices() {
        if character.is_alphanumeric() {
            token.extend(character.to_lowercase());
            token_end = offset + character.len_utf8();
        } else if !token.is_empty() {
            response_tokens.push((std::mem::take(&mut token), token_end));
        }
    }
    if !token.is_empty() {
        response_tokens.push((token, token_end));
    }

    if response_tokens.len() < prompt_tokens.len()
        || !response_tokens
            .iter()
            .zip(prompt_tokens.iter())
            .all(|((actual, _), expected)| actual == expected)
    {
        return text.trim().to_owned();
    }

    let echo_end = response_tokens[prompt_tokens.len() - 1].1;
    text[echo_end..]
        .trim_start_matches(|character: char| {
            character.is_whitespace() || !character.is_alphanumeric()
        })
        .trim()
        .to_owned()
}

fn strip_prompt_echo_words(words: &mut Vec<TimedWord>, prompt: &str) {
    let prompt_tokens = normalized_tokens(prompt);
    if prompt_tokens.is_empty() {
        return;
    }

    let mut observed = Vec::<String>::new();
    let mut remove_through = None;
    for (index, word) in words.iter().enumerate() {
        observed.extend(normalized_tokens(&word.text));
        if observed.len() >= prompt_tokens.len() {
            if observed == prompt_tokens {
                remove_through = Some(index);
            }
            break;
        }
        if !prompt_tokens.starts_with(&observed) {
            break;
        }
    }
    if let Some(index) = remove_through {
        words.drain(..=index);
    }
}

impl WhisperServer {
    pub async fn start(app: &AppHandle, settings: &AppSettings) -> Result<Self> {
        let resource_root = app
            .path()
            .resource_dir()
            .context("Tauri did not provide the packaged resource directory")?;
        let (runtime, executable) = locate_whisper_runtime(&resource_root)?;
        let model = locate_whisper_model(app, &resource_root, &settings.whisper_model)?;
        if !executable.is_file() {
            return Err(anyhow!(
                "Could not find the packaged whisper.cpp server at {}. Reinstall Quill from a complete release build.",
                executable.display()
            ));
        }
        if !model.is_file() {
            return Err(anyhow!(
                "packaged whisper model is missing: {}",
                model.display()
            ));
        }

        #[cfg(windows)]
        let wants_cuda = matches!(
            settings.backend,
            ComputeBackend::Auto | ComputeBackend::Cuda
        );
        #[cfg(windows)]
        let (cuda_runtime, cuda_pack_missing) = if wants_cuda {
            match downloads::cuda_runtime_availability(app).map_err(anyhow::Error::msg)? {
                CudaRuntimeAvailability::Missing => (None, true),
                CudaRuntimeAvailability::Ready(directory) => (Some(directory), false),
                CudaRuntimeAvailability::Invalid(error) => return Err(anyhow!(error)),
            }
        } else {
            (None, false)
        };
        #[cfg(windows)]
        let require_cuda = cuda_runtime.is_some();
        #[cfg(windows)]
        let active_backend = if require_cuda {
            RuntimeBackend::Cuda
        } else {
            RuntimeBackend::Cpu
        };

        #[cfg(target_os = "macos")]
        let (active_backend, cuda_pack_missing) = match settings.backend {
            ComputeBackend::Cpu => (RuntimeBackend::Cpu, false),
            ComputeBackend::Auto | ComputeBackend::Metal => (RuntimeBackend::Metal, false),
            ComputeBackend::Cuda => {
                return Err(anyhow!(
                    "CUDA is only available on Windows. Open Voice settings and choose Metal, CPU, or Auto."
                ));
            }
        };

        #[cfg(not(any(windows, target_os = "macos")))]
        let (active_backend, cuda_pack_missing) = (RuntimeBackend::Cpu, false);

        let port = reserve_local_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(&executable);
        #[cfg(windows)]
        let working_directory = cuda_runtime.as_deref().unwrap_or(&runtime);
        #[cfg(not(windows))]
        let working_directory = runtime.as_path();
        command
            .current_dir(working_directory)
            .arg("--model")
            .arg(&model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--device")
            .arg("0")
            .arg("--language")
            .arg(effective_language(
                &settings.whisper_model,
                &settings.language,
            ))
            .arg("--no-language-probabilities")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        if active_backend == RuntimeBackend::Cpu {
            command.arg("--no-gpu");
        }

        let started = Instant::now();
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to launch packaged {}", executable.display()))?;
        let stderr = child
            .stderr
            .take()
            .context("whisper.cpp stderr pipe was unavailable")?;
        let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // Keep raw sidecar diagnostics in memory only long enough for
                // CUDA readiness checks. Inference diagnostics can contain an
                // initial dictionary prompt, so never persist or trace lines.
                let _ = line_tx.send(line);
            }
        });

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(90))
            .build()?;
        let mut cuda_device_detected = false;
        let mut cuda_backend_active = false;
        let mut metal_backend_active = false;
        let mut ready = false;
        while started.elapsed() < Duration::from_secs(90) {
            if let Some(status) = child.try_wait()? {
                return Err(anyhow!(
                    "whisper.cpp exited during model load with status {status}"
                ));
            }
            if let Ok(Some(line)) =
                tokio::time::timeout(Duration::from_millis(200), line_rx.recv()).await
            {
                if line.contains("found 1 CUDA devices") && line.contains("VRAM") {
                    cuda_device_detected = true;
                }
                if line.contains("using CUDA0 backend") || line.contains("loaded CUDA backend") {
                    cuda_backend_active = true;
                }
                if line.contains("ggml_metal_init: picking default device") {
                    metal_backend_active = true;
                }
            }
            if client.get(&endpoint).send().await.is_ok() {
                ready = true;
                let backend_verified = match active_backend {
                    RuntimeBackend::Cpu => true,
                    RuntimeBackend::Cuda => cuda_device_detected && cuda_backend_active,
                    RuntimeBackend::Metal => metal_backend_active,
                };
                if backend_verified {
                    break;
                }
            }
        }
        if !ready {
            let _ = child.start_kill();
            return Err(anyhow!(
                "whisper.cpp did not become ready within 90 seconds"
            ));
        }
        if active_backend == RuntimeBackend::Cuda && !(cuda_device_detected && cuda_backend_active)
        {
            let _ = child.start_kill();
            return Err(anyhow!(
                "CUDA verification failed: whisper.cpp did not report a detected GPU and active CUDA0 backend"
            ));
        }
        if active_backend == RuntimeBackend::Metal && !metal_backend_active {
            let _ = child.start_kill();
            return Err(anyhow!(
                "Metal verification failed: whisper.cpp became ready but did not report an active Metal device. Try CPU in Voice settings and send the Quill log with your macOS and Mac model details."
            ));
        }

        if active_backend == RuntimeBackend::Cuda {
            // The first CUDA inference initializes/JITs kernels and is much slower
            // than subsequent passes on a GTX 1650. Pay that cost during startup so
            // the first spoken word cannot time out the live session.
            let warmup = Form::new()
                .part(
                    "file",
                    Part::bytes(pcm16_wav(&vec![0.0; 4_000]))
                        .file_name("quill-cuda-warmup.wav")
                        .mime_str("audio/wav")?,
                )
                .text("response_format", "verbose_json")
                .text("temperature", "0")
                .text("temperature_inc", "0.2")
                .text("suppress_nst", "true")
                .text("no_context", "true")
                .text("split_on_word", "true")
                .text("language", "en");
            client
                .post(format!("{endpoint}/inference"))
                .multipart(warmup)
                .send()
                .await
                .context("whisper.cpp CUDA warmup request failed")?
                .error_for_status()
                .context("whisper.cpp CUDA warmup returned an error")?;
        }

        let cold_load_ms = started.elapsed().as_millis();
        if active_backend == RuntimeBackend::Cuda {
            metrics::record(
                "whisperCudaVerified",
                cold_load_ms,
                None,
                Some("device detected; CUDA backend active"),
            )?;
        }
        tracing::info!(
            version = downloads::WHISPER_VERSION,
            revision = downloads::WHISPER_REVISION,
            cold_load_ms,
            cuda_device_detected,
            cuda_backend_active,
            metal_backend_active,
            active_backend = ?active_backend,
            cuda_pack_missing,
            "whisper.cpp server ready"
        );
        Ok(Self {
            child,
            client,
            endpoint,
            cold_load_ms,
            backend: active_backend,
            cuda_pack_missing,
        })
    }

    pub async fn transcribe(&self, settings: &AppSettings, samples: &[f32]) -> Result<AsrPass> {
        if samples.is_empty() {
            return Ok(AsrPass {
                words: Vec::new(),
                text: String::new(),
                latency_ms: 0,
                timings_synthetic: false,
            });
        }
        let wav = pcm16_wav(samples);
        let (bias_prompt, prompt_truncated) = dictionary_prompt(&settings.dictionary);
        if prompt_truncated {
            tracing::warn!(
                word_entries = settings
                    .dictionary
                    .iter()
                    .filter(|entry| entry.kind == DictionaryKind::Word)
                    .count(),
                prompt_chars = bias_prompt
                    .as_deref()
                    .map(|prompt| prompt.chars().count())
                    .unwrap_or(0),
                max_prompt_chars = MAX_DICTIONARY_PROMPT_CHARS,
                "dictionary bias prompt exceeded its budget; remaining entries were omitted"
            );
        }
        let mut form = Form::new()
            .part(
                "file",
                Part::bytes(wav)
                    .file_name("quill-live.wav")
                    .mime_str("audio/wav")?,
            )
            .text("response_format", "verbose_json")
            // Temperature fallback: whisper.cpp starts greedy at 0.0 and steps up
            // if the decoder trips its logprob/no-speech thresholds. This is what
            // breaks repetition loops on short or ambiguous audio. Setting
            // temperature_inc to 0 disables the fallback entirely — do not.
            .text("temperature", "0")
            .text("temperature_inc", "0.2")
            // Non-speech tokens like "[music]" and "(inaudible)" are common
            // seeds for decoder loops; suppress them.
            .text("suppress_nst", "true")
            // Don't feed previous-segment context back in — a bad segment
            // then poisons every following one until end of utterance.
            .text("no_context", "true")
            .text("split_on_word", "true")
            .text(
                "language",
                effective_language(&settings.whisper_model, &settings.language),
            );
        if let Some(prompt) = bias_prompt.as_ref() {
            form = form.text("prompt", prompt.clone());
        }
        let started = Instant::now();
        let response = self
            .client
            .post(format!("{}/inference", self.endpoint))
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .json::<WhisperResponse>()
            .await?;
        let latency_ms = started.elapsed().as_millis();
        let word_pieces = response
            .segments
            .into_iter()
            .flat_map(|segment| segment.words)
            .collect::<Vec<_>>();
        let mut words = merge_word_pieces(word_pieces);
        words.retain(|word| !is_control_token(word.text.trim()));
        let mut text = response.text.trim().to_owned();
        if let Some(prompt) = bias_prompt.as_deref() {
            text = strip_prompt_echo_text(&text, prompt);
            strip_prompt_echo_words(&mut words, prompt);
        }
        tracing::info!(
            latency_ms,
            samples = samples.len(),
            transcript_words = text.split_whitespace().count(),
            "warm whisper transcription completed"
        );
        Ok(AsrPass {
            words,
            text,
            latency_ms,
            timings_synthetic: false,
        })
    }
}

#[cfg(windows)]
const WINDOWS_WHISPER_RUNTIME_DIR: &str = "resources/whisper/windows-x64-cpu";

fn locate_whisper_runtime(resource_root: &Path) -> Result<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    {
        let runtime = locate_resource(resource_root, Path::new(WINDOWS_WHISPER_RUNTIME_DIR))?;
        let executable = runtime.join("whisper-server.exe");
        Ok((runtime, executable))
    }

    #[cfg(target_os = "macos")]
    {
        // Tauri's externalBin sidecar mechanism places the executable next to
        // Quill in Contents/MacOS and preserves its executable permission bit.
        // Do not resolve it through bundle.resources (Contents/Resources),
        // which is intended for data files rather than executable sidecars.
        let quill_executable = std::env::current_exe()
            .context("macOS did not provide the path to the running Quill executable")?;
        let runtime = quill_executable
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                anyhow!(
                    "Could not resolve the macOS sidecar directory from {}",
                    quill_executable.display()
                )
            })?;
        let executable = runtime.join("whisper-server");
        Ok((runtime, executable))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = resource_root;
        Err(anyhow!(
            "Quill speech recognition is not packaged for this operating system"
        ))
    }
}

fn merge_word_pieces(pieces: Vec<WhisperWord>) -> Vec<TimedWord> {
    let mut words = Vec::<TimedWord>::new();
    for piece in pieces {
        let text = piece.word.trim();
        if text.is_empty() || is_control_token(text) {
            continue;
        }
        let start_ms = (piece.start.max(0.0) * 1_000.0).round() as u64;
        let end_ms = (piece.end.max(0.0) * 1_000.0).round() as u64;
        let starts_word = piece.word.chars().next().is_some_and(char::is_whitespace);
        if starts_word || words.is_empty() {
            words.push(TimedWord {
                text: text.to_owned(),
                start_ms,
                end_ms,
            });
        } else if let Some(word) = words.last_mut() {
            word.text.push_str(text);
            word.end_ms = end_ms;
        }
    }
    words
}

fn is_control_token(text: &str) -> bool {
    (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('<') && text.ends_with('>'))
}

impl Drop for WhisperServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Whisper models can be bundled inside the app's `resources/models` folder
/// OR downloaded at runtime into the user's app-data folder by the
/// `downloads` module. Check both, preferring the user download because that's
/// the only place non-bundled models (multilingual variants, larger sizes)
/// can live.
fn locate_whisper_model(app: &AppHandle, resource_root: &Path, model_id: &str) -> Result<PathBuf> {
    let file_name = format!("ggml-{model_id}.bin");

    if let Ok(user_dir) = crate::downloads::user_model_dir(app) {
        let user_path = user_dir.join(&file_name);
        if user_path.is_file() {
            return Ok(user_path);
        }
    }

    let bundled = resource_root
        .join("resources")
        .join("models")
        .join(&file_name);
    if bundled.is_file() {
        return Ok(bundled);
    }
    let bundled_flat = resource_root.join("models").join(&file_name);
    if bundled_flat.is_file() {
        return Ok(bundled_flat);
    }

    Err(anyhow!(
        "whisper model '{}' is not installed. Download it from Voice → Compare and download models.",
        model_id
    ))
}

#[cfg(windows)]
fn locate_resource(resource_root: &Path, relative: &Path) -> Result<PathBuf> {
    let direct = resource_root.join(relative);
    if direct.exists() {
        return Ok(direct);
    }
    let without_resources = relative
        .strip_prefix("resources")
        .map(|path| resource_root.join(path))
        .unwrap_or_else(|_| direct.clone());
    if without_resources.exists() {
        return Ok(without_resources);
    }
    Err(anyhow!(
        "packaged resource not found: {} (resource root: {})",
        relative.display(),
        resource_root.display()
    ))
}

fn reserve_local_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary_entry(id: &str, replacement: &str, kind: DictionaryKind) -> DictionaryEntry {
        DictionaryEntry {
            id: id.into(),
            spoken: format!("spoken-{id}"),
            replacement: replacement.into(),
            kind,
        }
    }

    #[test]
    fn merges_whisper_token_pieces_into_words() {
        let words = merge_word_pieces(vec![
            WhisperWord {
                word: " Qu".into(),
                start: 0.1,
                end: 0.2,
            },
            WhisperWord {
                word: "ill".into(),
                start: 0.2,
                end: 0.4,
            },
            WhisperWord {
                word: " dict".into(),
                start: 0.4,
                end: 0.7,
            },
            WhisperWord {
                word: "ation".into(),
                start: 0.7,
                end: 1.0,
            },
        ]);
        assert_eq!(
            words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Quill", "dictation"]
        );
        assert_eq!(words[0].end_ms, 400);
    }

    #[test]
    fn drops_whisper_control_tokens() {
        let words = merge_word_pieces(vec![WhisperWord {
            word: " [BLANK_AUDIO]".into(),
            start: 0.0,
            end: 1.0,
        }]);
        assert!(words.is_empty());
    }

    #[test]
    fn distil_large_v3_is_treated_as_english_only() {
        assert_eq!(effective_language("distil-large-v3", "fr"), "en");
        assert_eq!(effective_language("distil-large-v3", "en"), "en");
    }

    #[test]
    fn dictionary_prompt_contains_only_word_replacements() {
        let entries = vec![
            dictionary_entry("1", "Tauri", DictionaryKind::Word),
            dictionary_entry("2", "aarav@example.com", DictionaryKind::Snippet),
            dictionary_entry("3", "whisper.cpp", DictionaryKind::Word),
        ];
        let (prompt, truncated) = dictionary_prompt(&entries);
        assert_eq!(prompt.as_deref(), Some("Tauri, whisper.cpp"));
        assert!(!truncated);
    }

    #[test]
    fn dictionary_prompt_is_omitted_when_no_word_entries_exist() {
        let entries = vec![dictionary_entry(
            "1",
            "aarav@example.com",
            DictionaryKind::Snippet,
        )];
        assert_eq!(dictionary_prompt(&entries), (None, false));
    }

    #[test]
    fn dictionary_prompt_stops_before_exceeding_the_character_budget() {
        let first = "a".repeat(MAX_DICTIONARY_PROMPT_CHARS - 3);
        let entries = vec![
            dictionary_entry("1", &first, DictionaryKind::Word),
            dictionary_entry("2", "Tauri", DictionaryKind::Word),
        ];
        let (prompt, truncated) = dictionary_prompt(&entries);
        assert!(truncated);
        assert_eq!(
            prompt.unwrap().chars().count(),
            MAX_DICTIONARY_PROMPT_CHARS - 3
        );
    }

    #[test]
    fn strips_normalized_prompt_echo_from_text_and_words() {
        let prompt = "Tauri, whisper.cpp";
        assert_eq!(
            strip_prompt_echo_text("TAURI whisper CPP. The sidecar started.", prompt),
            "The sidecar started."
        );
        assert_eq!(strip_prompt_echo_text("Tauri, whisper.cpp.", prompt), "");

        let mut words = vec![
            TimedWord {
                text: "Tauri,".into(),
                start_ms: 0,
                end_ms: 100,
            },
            TimedWord {
                text: "whisper.cpp".into(),
                start_ms: 100,
                end_ms: 200,
            },
            TimedWord {
                text: "sidecar".into(),
                start_ms: 200,
                end_ms: 300,
            },
        ];
        strip_prompt_echo_words(&mut words, prompt);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "sidecar");
    }

    #[test]
    fn leaves_non_echo_transcripts_untouched() {
        let transcript = "The Tauri sidecar spawns whisper.cpp.";
        assert_eq!(
            strip_prompt_echo_text(transcript, "Tauri, whisper.cpp"),
            transcript
        );
    }
}
