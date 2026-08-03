//! Crash recovery.
//!
//! Two artefacts live under `%LOCALAPPDATA%/quill/recovery/`:
//!
//! - `latest.json` — the transcript checkpoint. Always written every pass so
//!   even if the process is killed mid-utterance, we retain the words that had
//!   been recognised. Small, always-safe, atomic-renamed to prevent
//!   half-written state.
//! - `latest.wav` — the raw audio buffer. Written only when the user has
//!   `keepRecoveryAudio` enabled, throttled to every ~15s, and issued on a
//!   background thread so the session loop never blocks on disk. Rewriting
//!   the whole cumulative buffer every pass would produce quadratic I/O on
//!   long recordings; the throttle bounds that.
//!
//! The two are decoupled by design (per the release review): disabling audio
//! must not silently disable the safer, smaller transcript checkpoint.
//!
//! Both files persist until the user either accepts the recovered transcript
//! (successful insertion elsewhere in the app) or explicitly discards it. A
//! crash-then-relaunch loop preserves the checkpoint indefinitely, so the
//! user can decide at their pace whether the recovered content is worth
//! keeping.

use crate::audio::pcm16_wav;
use crate::model::Mode;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

/// Serializes every recovery-file mutation and every read/compare/delete
/// sequence. This makes an ID check and its deletion one atomic operation,
/// so a stale consumer cannot clear a checkpoint replaced by a new session.
static RECOVERY_MUTEX: Mutex<()> = Mutex::new(());

/// Monotonic counter bumped by a matching clear and by
/// `purge_audio_if_disabled(false)`.
/// Background writers snapshot the current value at dispatch time and abort
/// (silently) if the counter has changed by the time they acquire the mutex.
/// This closes the race where a spawned writer wins the mutex *after* clear
/// deletes the file and recreates a stale WAV.
static AUDIO_SESSION: AtomicUsize = AtomicUsize::new(0);
static NEXT_RECOVERY_ID: AtomicU64 = AtomicU64::new(1);

fn invalidate_pending_audio_writes() {
    AUDIO_SESSION.fetch_add(1, Ordering::Release);
}

const MANIFEST_NAME: &str = "latest.json";
const AUDIO_NAME: &str = "latest.wav";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryManifest {
    /// Stable identifier for the recording that owns this checkpoint. Clear
    /// operations must supply this value so stale UI cannot delete a newer
    /// recording's recovery data.
    #[serde(default)]
    pub id: String,
    pub updated_at_unix_ms: u128,
    pub mode: String,
    pub transcript: String,
    /// Path to the raw audio WAV, present only when the user had
    /// `keepRecoveryAudio` enabled at the time of the checkpoint.
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearOutcome {
    Cleared,
    Missing,
    Stale,
}

pub fn new_recovery_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = NEXT_RECOVERY_ID.fetch_add(1, Ordering::Relaxed);
    format!("{}-{timestamp}-{counter}", std::process::id())
}

fn default_directory() -> Result<PathBuf> {
    dirs::data_local_dir()
        .context("the operating system did not provide a local data directory")
        .map(|dir| dir.join("quill").join("recovery"))
}

fn ensure_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).map_err(Into::into)
}

fn recovery_directory() -> Result<PathBuf> {
    let dir = default_directory()?;
    ensure_dir(&dir)?;
    Ok(dir)
}

fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Dictation => "dictation",
        Mode::Scribe => "scribe",
    }
}

/// Persist the current transcript. Fast, safe to call every pass. Writes to a
/// `.pending` sibling then atomically renames, so a crash in the middle of
/// serialising can never leave a truncated manifest for the next launch to
/// choke on.
pub fn write_transcript(id: &str, mode: Mode, transcript: &str, keep_audio: bool) -> Result<()> {
    let dir = recovery_directory()?;
    let _guard = RECOVERY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    write_transcript_in(&dir, id, mode, transcript, keep_audio)
}

fn write_transcript_in(
    dir: &Path,
    id: &str,
    mode: Mode,
    transcript: &str,
    keep_audio: bool,
) -> Result<()> {
    ensure_dir(dir)?;
    let audio_file = dir.join(AUDIO_NAME);
    let audio = if keep_audio && audio_file.exists() {
        Some(audio_file.to_string_lossy().into_owned())
    } else {
        None
    };
    let manifest = RecoveryManifest {
        id: id.to_owned(),
        updated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        mode: mode_str(mode).to_owned(),
        transcript: transcript.to_owned(),
        audio_path: audio,
    };
    let pending = dir.join(format!("{MANIFEST_NAME}.pending"));
    let committed = dir.join(MANIFEST_NAME);
    fs::write(&pending, serde_json::to_vec_pretty(&manifest)?)
        .context("failed to write recovery checkpoint")?;
    fs::rename(pending, committed).context("failed to commit recovery checkpoint")?;
    Ok(())
}

/// Persist the raw audio buffer to `latest.wav`. Only called when
/// `keepRecoveryAudio` is on. Best-effort — an audio-write failure never
/// blocks a transcript checkpoint because the transcript is the primary,
/// smaller, always-safer artefact.
///
#[cfg(test)]
fn write_audio_in(dir: &Path, samples: &[f32]) -> Result<()> {
    ensure_dir(dir)?;
    let pending = dir.join(format!("{AUDIO_NAME}.pending"));
    let committed = dir.join(AUDIO_NAME);
    fs::write(&pending, pcm16_wav(samples)).context("failed to write recovery audio")?;
    fs::rename(pending, committed).context("failed to commit recovery audio")?;
    Ok(())
}

/// Fire-and-forget write of the audio buffer on a background thread. The
/// session loop hands off ownership of a cloned sample vec and never waits
/// for the encode or disk write, so a slow disk cannot stall dictation.
///
/// The writer takes `RECOVERY_MUTEX` around the actual file operations and
/// snapshots `AUDIO_SESSION` at dispatch, aborting if a matching clear
/// or `purge_audio_if_disabled(false)` bumped the counter while it was
/// encoding or waiting for the mutex. Without that gate, a slow writer
/// could recreate `latest.wav` after the user had already cleared or
/// disabled recovery — a data-integrity bug the reviewer flagged.
pub fn write_audio_async(recovery_id: String, samples: Vec<f32>) {
    let dir = match recovery_directory() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let _ = dispatch_audio_write(dir, recovery_id, samples);
}

fn dispatch_audio_write(dir: PathBuf, recovery_id: String, samples: Vec<f32>) -> JoinHandle<()> {
    let session_at_dispatch = AUDIO_SESSION.load(Ordering::Acquire);
    std::thread::spawn(move || {
        // Encode off-lock so slow disks don't hold up other writers, but
        // sample first — the caller may have already cleared before we
        // even started.
        if AUDIO_SESSION.load(Ordering::Acquire) != session_at_dispatch {
            return;
        }
        let bytes = pcm16_wav(&samples);
        // Take the recovery mutex. Any concurrent clear is either already
        // done (and bumped the session) or is queued behind us; either way
        // the counter check below is authoritative.
        let _guard = RECOVERY_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if AUDIO_SESSION.load(Ordering::Acquire) != session_at_dispatch {
            // A clear/purge invalidated our write while we were queued —
            // silently drop the bytes rather than recreate a stale file.
            return;
        }
        let manifest_matches = load_pending_in(&dir)
            .ok()
            .flatten()
            .is_some_and(|manifest| manifest.id == recovery_id);
        if !manifest_matches {
            return;
        }
        if let Err(error) = write_audio_bytes_in(&dir, &bytes) {
            tracing::warn!(%error, "background audio checkpoint failed");
        }
    })
}

fn write_audio_bytes_in(dir: &Path, bytes: &[u8]) -> Result<()> {
    ensure_dir(dir)?;
    let pending = dir.join(format!("{AUDIO_NAME}.pending"));
    let committed = dir.join(AUDIO_NAME);
    fs::write(&pending, bytes).context("failed to write recovery audio")?;
    fs::rename(pending, committed).context("failed to commit recovery audio")?;
    Ok(())
}

/// Delete any audio file if `keepRecoveryAudio` is off. Called opportunistically
/// so toggling the setting off actually purges the residual WAV rather than
/// leaving it stale on disk forever. Bumps the session counter and takes the
/// audio mutex so a background writer dispatched before the toggle cannot
/// recreate the file after purge.
pub fn purge_audio_if_disabled(keep_audio: bool) {
    if keep_audio {
        return;
    }
    invalidate_pending_audio_writes();
    let _guard = RECOVERY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Ok(dir) = default_directory() {
        let path = dir.join(AUDIO_NAME);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn clear_if_matches(expected_id: &str) -> Result<ClearOutcome> {
    let dir = recovery_directory()?;
    let _guard = RECOVERY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_if_matches_in(&dir, expected_id)
}

fn clear_if_matches_in(dir: &Path, expected_id: &str) -> Result<ClearOutcome> {
    let Some(manifest) = load_pending_in(dir)? else {
        return Ok(ClearOutcome::Missing);
    };
    if manifest.id != expected_id {
        return Ok(ClearOutcome::Stale);
    }
    invalidate_pending_audio_writes();
    clear_in(dir)?;
    Ok(ClearOutcome::Cleared)
}

fn clear_in(dir: &Path) -> Result<()> {
    let manifest = dir.join(MANIFEST_NAME);
    if manifest.exists() {
        fs::remove_file(manifest)?;
    }
    let audio = dir.join(AUDIO_NAME);
    if audio.exists() {
        fs::remove_file(audio)?;
    }
    Ok(())
}

/// Rename a corrupt `latest.json` out of the way so subsequent launches don't
/// keep hitting the same parse failure. The file is preserved with a
/// timestamp suffix in case the user (or Anthropic support) wants to inspect
/// it for forensic purposes rather than deleting it outright.
pub fn quarantine_corrupt_manifest() -> Result<PathBuf> {
    let dir = recovery_directory()?;
    let _guard = RECOVERY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    quarantine_corrupt_manifest_in(&dir)
}

fn quarantine_corrupt_manifest_in(dir: &Path) -> Result<PathBuf> {
    let manifest = dir.join(MANIFEST_NAME);
    if !manifest.exists() {
        anyhow::bail!("no recovery manifest to quarantine");
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dest = dir.join(format!("{MANIFEST_NAME}.corrupt-{ts}"));
    fs::rename(&manifest, &dest).with_context(|| {
        format!(
            "failed to quarantine corrupt manifest {} → {}",
            manifest.display(),
            dest.display()
        )
    })?;
    Ok(dest)
}

/// Read a pending checkpoint from disk. Called at startup to decide whether
/// to prompt the user with a Recover/Discard banner. A corrupt or
/// schema-mismatched manifest returns an Err so the caller can log it and
/// treat the recovery as gone; it never panics or returns partial data.
pub fn load_pending() -> Result<Option<RecoveryManifest>> {
    let dir = recovery_directory()?;
    let _guard = RECOVERY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    load_pending_in(&dir)
}

fn load_pending_in(dir: &Path) -> Result<Option<RecoveryManifest>> {
    let path = dir.join(MANIFEST_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read recovery checkpoint at {}", path.display()))?;
    let mut manifest: RecoveryManifest = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "recovery checkpoint at {} was not valid JSON",
            path.display()
        )
    })?;
    if manifest.id.trim().is_empty() {
        manifest.id = format!("legacy-{}", manifest.updated_at_unix_ms);
    }
    Ok(Some(manifest))
}

/// Called after the user has successfully consumed the recovered transcript
/// (copied it, inserted it manually, whatever) OR explicitly discarded it.
/// The supplied ID must match the manifest currently on disk.
pub fn accept_or_discard(expected_id: &str) -> Result<ClearOutcome> {
    clear_if_matches(expected_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("quill-recovery-test-{name}-{unique}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn missing_manifest_loads_as_none() {
        let dir = scratch_dir("missing");
        let result = load_pending_in(&dir).expect("load did not error");
        assert!(result.is_none(), "no manifest → Ok(None)");
    }

    #[test]
    fn round_trip_preserves_transcript_and_mode() {
        let dir = scratch_dir("round-trip");
        write_transcript_in(&dir, "round-trip-id", Mode::Dictation, "hello world", false)
            .expect("write ok");
        let loaded = load_pending_in(&dir).expect("load ok").expect("some");
        assert_eq!(loaded.id, "round-trip-id");
        assert_eq!(loaded.transcript, "hello world");
        assert_eq!(loaded.mode, "dictation");
        assert!(loaded.audio_path.is_none(), "no audio was written");
    }

    #[test]
    fn corrupt_manifest_returns_err_not_panic() {
        let dir = scratch_dir("corrupt");
        fs::write(dir.join(MANIFEST_NAME), b"{ not valid json").expect("seed corrupt manifest");
        let result = load_pending_in(&dir);
        assert!(result.is_err(), "corrupt JSON must surface as Err");
    }

    #[test]
    fn clear_removes_manifest_and_audio() {
        let dir = scratch_dir("clear");
        write_transcript_in(&dir, "clear-id", Mode::Scribe, "keep me", false).expect("write ok");
        write_audio_in(&dir, &[0.0, 0.1, -0.1]).expect("audio write ok");
        clear_in(&dir).expect("clear ok");
        assert!(!dir.join(MANIFEST_NAME).exists(), "manifest removed");
        assert!(!dir.join(AUDIO_NAME).exists(), "audio removed");
    }

    #[test]
    fn manifest_records_audio_path_when_wav_exists_and_keep_flag_is_on() {
        let dir = scratch_dir("audio-linked");
        write_audio_in(&dir, &[0.0; 8]).expect("audio write ok");
        write_transcript_in(&dir, "audio-id", Mode::Scribe, "with audio", true).expect("write ok");
        let loaded = load_pending_in(&dir).expect("load ok").expect("some");
        assert!(
            loaded
                .audio_path
                .as_deref()
                .is_some_and(|p| p.ends_with(AUDIO_NAME)),
            "audio_path should reference latest.wav"
        );
    }

    #[test]
    fn clear_defeats_racing_audio_write() {
        // Reviewer scenario: a background write is dispatched but is still in
        // its encode/queue phase when a matching clear runs. Without the counter
        // gate, the writer would recreate latest.wav *after* clear deletes it.
        //
        // We deterministically force the race by taking RECOVERY_MUTEX externally,
        // dispatching a write (which will encode fast then block on the
        // mutex), then bumping the session counter and releasing the mutex.
        // The writer must see the mismatched counter and abort without
        // touching the disk.
        let dir = scratch_dir("clear-race");
        let recovery_id = "clear-race-id";

        // Put a matching manifest and a real WAV on disk so the writer would
        // be eligible to commit if the invalidation guard were absent.
        write_transcript_in(&dir, recovery_id, Mode::Dictation, "seed", true)
            .expect("seed manifest");
        write_audio_in(&dir, &[0.0; 8]).expect("seed wav");
        assert!(dir.join(AUDIO_NAME).exists(), "seed present");

        // Hold the mutex externally.
        let guard = RECOVERY_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Dispatch a write while we hold the lock — writer will encode, then
        // block on the mutex.
        let handle = dispatch_audio_write(dir.clone(), recovery_id.to_owned(), vec![0.5; 32_000]);

        // Give the writer a moment to reach the mutex-wait state.
        std::thread::sleep(std::time::Duration::from_millis(60));

        // Now simulate a clear happening while the writer is queued: bump
        // the session counter and, still holding the mutex, delete the file.
        invalidate_pending_audio_writes();
        let _ = fs::remove_file(dir.join(AUDIO_NAME));

        // Release the mutex so the writer can proceed. It should observe the
        // bumped counter and abort before writing.
        drop(guard);
        handle.join().expect("writer thread joined");

        assert!(
            !dir.join(AUDIO_NAME).exists(),
            "writer must not recreate latest.wav after a concurrent clear invalidated its session"
        );
    }

    #[test]
    fn quarantine_moves_manifest_out_of_the_way() {
        let dir = scratch_dir("quarantine");
        fs::write(dir.join(MANIFEST_NAME), b"{ not valid json").expect("seed corrupt manifest");
        let dest = quarantine_corrupt_manifest_in(&dir).expect("quarantine ok");
        assert!(!dir.join(MANIFEST_NAME).exists(), "original moved");
        assert!(dest.exists(), "quarantined copy exists");
        assert!(
            dest.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("latest.json.corrupt-")),
            "quarantined file uses timestamp suffix; got {}",
            dest.display()
        );
    }

    #[test]
    fn manifest_omits_audio_path_when_keep_flag_is_off_even_if_wav_present() {
        // Regression for the "disable audio in settings but leave a stale
        // WAV" case: the manifest must not point at an audio file we're
        // supposed to be ignoring.
        let dir = scratch_dir("audio-ignored");
        write_audio_in(&dir, &[0.0; 8]).expect("audio write ok");
        write_transcript_in(&dir, "no-audio-id", Mode::Dictation, "no audio", false)
            .expect("write ok");
        let loaded = load_pending_in(&dir).expect("load ok").expect("some");
        assert!(loaded.audio_path.is_none());
    }

    #[test]
    fn stale_clear_keeps_newer_checkpoint() {
        let dir = scratch_dir("stale-clear");
        write_transcript_in(&dir, "older", Mode::Dictation, "old words", false)
            .expect("write older checkpoint");
        write_transcript_in(&dir, "newer", Mode::Scribe, "new words", false)
            .expect("replace with newer checkpoint");

        let outcome = clear_if_matches_in(&dir, "older").expect("clear check");
        assert_eq!(outcome, ClearOutcome::Stale);
        let loaded = load_pending_in(&dir).expect("load ok").expect("some");
        assert_eq!(loaded.id, "newer");
        assert_eq!(loaded.transcript, "new words");
    }

    #[test]
    fn matching_clear_removes_checkpoint() {
        let dir = scratch_dir("matching-clear");
        write_transcript_in(&dir, "owner", Mode::Dictation, "owned words", false)
            .expect("write checkpoint");
        write_audio_in(&dir, &[0.0; 8]).expect("write audio");

        let outcome = clear_if_matches_in(&dir, "owner").expect("clear check");
        assert_eq!(outcome, ClearOutcome::Cleared);
        assert!(!dir.join(MANIFEST_NAME).exists());
        assert!(!dir.join(AUDIO_NAME).exists());
    }
}
