use crate::cleanup;
use crate::injection::{self, TargetInjection};
use crate::model::{AppSettings, DictionarySuggestion, InjectionMode};
use crate::recovery;
use crate::register::Register;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State};
use unicode_segmentation::UnicodeSegmentation;

const REVIEW_WIDTH: f64 = 420.0;
const REVIEW_HEIGHT: f64 = 326.0;
const REVIEW_MIN_WIDTH: f64 = 380.0;
const REVIEW_MIN_HEIGHT: f64 = 300.0;
const SUGGESTION_HEIGHT: f64 = 86.0;
const SUGGESTION_MIN_HEIGHT: f64 = 76.0;

#[derive(Default)]
pub struct ReviewStore {
    current: Mutex<Option<ReviewSession>>,
    next_id: AtomicU64,
}

struct ReviewSession {
    id: u64,
    recovery_id: String,
    source: String,
    draft: String,
    warning: Option<String>,
    register: Register,
    settings: AppSettings,
    target: injection::InsertionTarget,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPayload {
    id: u64,
    source: String,
    draft: String,
    warning: Option<String>,
    register: Register,
}

pub struct ReviewRequest {
    pub recovery_id: String,
    pub source: String,
    pub draft: String,
    pub warning: Option<String>,
    pub register: Register,
    pub settings: AppSettings,
    pub target: injection::InsertionTarget,
}

impl ReviewSession {
    fn payload(&self) -> ReviewPayload {
        ReviewPayload {
            id: self.id,
            source: self.source.clone(),
            draft: self.draft.clone(),
            warning: self.warning.clone(),
            register: self.register,
        }
    }
}

pub fn show_processing(app: &AppHandle, message: &str) -> Result<()> {
    if let Ok(mut current) = app.state::<ReviewStore>().current.lock() {
        current.take();
    }
    let window = app
        .get_webview_window("scribe-review")
        .ok_or_else(|| anyhow!("Scribe review window is unavailable"))?;
    restore_review_window(&window);
    position_review(&window);
    window.show()?;
    window.set_focus()?;
    let _ = window.emit(
        "scribe-review://processing",
        serde_json::json!({ "message": message }),
    );
    Ok(())
}

pub fn hide_processing(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("scribe-review") {
        let _ = window.hide();
    }
}

pub fn update_processing(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window("scribe-review") {
        let _ = window.emit(
            "scribe-review://processing",
            serde_json::json!({ "message": message }),
        );
    }
}

pub fn present(app: &AppHandle, request: ReviewRequest) -> Result<()> {
    let store = app.state::<ReviewStore>();
    let id = store.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let session = ReviewSession {
        id,
        recovery_id: request.recovery_id,
        source: request.source,
        draft: request.draft,
        warning: request.warning,
        register: request.register,
        settings: request.settings,
        target: request.target,
    };
    let payload = session.payload();
    *store
        .current
        .lock()
        .map_err(|_| anyhow!("Scribe review state was poisoned"))? = Some(session);

    let window = app
        .get_webview_window("scribe-review")
        .ok_or_else(|| anyhow!("Scribe review window is unavailable"))?;
    restore_review_window(&window);
    position_review(&window);
    window.show()?;
    window.set_focus()?;
    let _ = window.emit("scribe-review://updated", payload);
    Ok(())
}

#[tauri::command]
pub fn get_scribe_review(state: State<'_, ReviewStore>) -> Result<Option<ReviewPayload>, String> {
    state
        .current
        .lock()
        .map(|current| current.as_ref().map(ReviewSession::payload))
        .map_err(|_| "Scribe review state was poisoned".to_owned())
}

#[tauri::command]
pub async fn regenerate_scribe_review(
    app: AppHandle,
    state: State<'_, ReviewStore>,
    register: Option<Register>,
) -> Result<ReviewPayload, String> {
    let (id, source, settings, selected_register) = {
        let current = state
            .current
            .lock()
            .map_err(|_| "Scribe review state was poisoned".to_owned())?;
        let session = current
            .as_ref()
            .ok_or_else(|| "There is no Scribe draft to regenerate".to_owned())?;
        (
            session.id,
            session.source.clone(),
            session.settings.clone(),
            register.unwrap_or(session.register),
        )
    };

    let regenerated = cleanup::clean(&settings, &source, selected_register).await;
    let payload = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "Scribe review state was poisoned".to_owned())?;
        let session = current
            .as_mut()
            .filter(|session| session.id == id)
            .ok_or_else(|| "This Scribe draft is no longer active".to_owned())?;
        session.register = selected_register;
        match regenerated {
            Ok(draft) => {
                session.draft = draft;
                session.warning = None;
            }
            Err(error) => {
                session.warning = Some(format!(
                    "Local cleanup could not regenerate this draft: {error}"
                ));
            }
        }
        session.payload()
    };
    if let Some(window) = app.get_webview_window("scribe-review") {
        let _ = window.emit("scribe-review://updated", payload.clone());
    }
    Ok(payload)
}

#[tauri::command]
pub fn accept_scribe_review(
    app: AppHandle,
    state: State<'_, ReviewStore>,
    text: String,
) -> Result<Option<DictionarySuggestion>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("The Scribe draft is empty".to_owned());
    }

    let session = state
        .current
        .lock()
        .map_err(|_| "Scribe review state was poisoned".to_owned())?
        .take()
        .ok_or_else(|| "There is no active Scribe draft".to_owned())?;
    let mut insertion = trimmed.to_owned();
    insertion.push(' ');
    let use_clipboard = matches!(session.settings.injection_mode, InjectionMode::Clipboard);
    // The candidate exists only for this successful review acceptance. It is
    // never traced or measured; only an explicit Add or Dismiss persists it.
    let suggestion = qualify_dictionary_suggestion(&session.draft, trimmed, &session.settings);

    match injection::inject_review_text(&session.target, &insertion, use_clipboard) {
        Ok(TargetInjection::Inserted) => {
            if let Some(window) = app.get_webview_window("scribe-review") {
                if suggestion.is_some() {
                    show_suggestion_window(&window);
                } else {
                    let _ = window.hide();
                }
            }
            clear_review_recovery(&session.recovery_id);
            emit_runtime_status(&app, "ready", "Scribe draft inserted");
            Ok(suggestion)
        }
        Ok(TargetInjection::Queued) => {
            restore_session(&state, session);
            refocus_review(&app);
            Err(
                "Quill could not focus the original editor. Keep it open and press Done again."
                    .to_owned(),
            )
        }
        Ok(TargetInjection::Unavailable) => {
            restore_session(&state, session);
            refocus_review(&app);
            Err("The original editor was closed, so Quill kept your draft here.".to_owned())
        }
        Err(error) => {
            restore_session(&state, session);
            refocus_review(&app);
            Err(format!(
                "Could not insert into the original editor: {error}"
            ))
        }
    }
}

fn qualify_dictionary_suggestion(
    generated_draft: &str,
    accepted_text: &str,
    settings: &AppSettings,
) -> Option<DictionarySuggestion> {
    let original_words = generated_draft.unicode_words().collect::<Vec<_>>();
    let accepted_words = accepted_text.unicode_words().collect::<Vec<_>>();
    if original_words.len() != accepted_words.len() {
        return None;
    }

    let mut substitution = None;
    for (original, accepted) in original_words.iter().zip(&accepted_words) {
        if original == accepted {
            continue;
        }
        if substitution.is_some() {
            return None;
        }
        substitution = Some(((*original).to_owned(), (*accepted).to_owned()));
    }
    let (spoken, replacement) = substitution?;
    let spoken_key = normalized_dictionary_word(&spoken);
    let replacement_key = normalized_dictionary_word(&replacement);

    if accepted_words
        .iter()
        .any(|word| normalized_dictionary_word(word) == spoken_key)
    {
        return None;
    }

    let word_is_known = |candidate: &str| {
        let candidate = normalized_dictionary_word(candidate);
        settings.dictionary.iter().any(|entry| {
            normalized_dictionary_word(&entry.spoken) == candidate
                || normalized_dictionary_word(&entry.replacement) == candidate
        })
    };
    if word_is_known(&spoken) || word_is_known(&replacement) {
        return None;
    }

    if settings.dismissed_suggestions.iter().any(|dismissed| {
        normalized_dictionary_word(&dismissed.spoken) == spoken_key
            && normalized_dictionary_word(&dismissed.replacement) == replacement_key
    }) {
        return None;
    }

    Some(DictionarySuggestion {
        spoken,
        replacement,
    })
}

fn normalized_dictionary_word(word: &str) -> String {
    word.trim().to_lowercase()
}

#[tauri::command]
pub fn discard_scribe_review(app: AppHandle, state: State<'_, ReviewStore>) -> Result<(), String> {
    let session = state
        .current
        .lock()
        .map_err(|_| "Scribe review state was poisoned".to_owned())?
        .take();
    if let Some(window) = app.get_webview_window("scribe-review") {
        let _ = window.hide();
    }
    if let Some(session) = session {
        clear_review_recovery(&session.recovery_id);
    }
    emit_runtime_status(&app, "ready", "Scribe draft discarded");
    Ok(())
}

fn clear_review_recovery(recovery_id: &str) {
    match recovery::clear_if_matches(recovery_id) {
        Ok(recovery::ClearOutcome::Cleared | recovery::ClearOutcome::Missing) => {}
        Ok(recovery::ClearOutcome::Stale) => tracing::info!(
            recovery_id,
            "Scribe review did not clear a newer recovery checkpoint"
        ),
        Err(error) => tracing::warn!(%error, "failed to clear Scribe review recovery"),
    }
}

fn restore_session(state: &State<'_, ReviewStore>, session: ReviewSession) {
    if let Ok(mut current) = state.current.lock() {
        *current = Some(session);
    }
}

fn refocus_review(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("scribe-review") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn emit_runtime_status(app: &AppHandle, state: &str, message: &str) {
    let _ = app.emit(
        "runtime://status",
        serde_json::json!({
            "state": state,
            "mode": null,
            "message": message,
            "provider": null,
        }),
    );
}

fn restore_review_window(window: &tauri::WebviewWindow) {
    let _ = window.set_min_size(Some(LogicalSize::new(REVIEW_MIN_WIDTH, REVIEW_MIN_HEIGHT)));
    let _ = window.set_size(LogicalSize::new(REVIEW_WIDTH, REVIEW_HEIGHT));
}

fn show_suggestion_window(window: &tauri::WebviewWindow) {
    let _ = window.set_min_size(Some(LogicalSize::new(
        REVIEW_MIN_WIDTH,
        SUGGESTION_MIN_HEIGHT,
    )));
    let _ = window.set_size(LogicalSize::new(REVIEW_WIDTH, SUGGESTION_HEIGHT));
    position_review(window);
}

fn position_review(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let screen = monitor.size();
    let scale = monitor.scale_factor();
    let bottom_margin = (86.0 * scale).round() as i32;
    let x = ((screen.width as i32 - size.width as i32) / 2).max(0);
    let y = (screen.height as i32 - size.height as i32 - bottom_margin).max(0);
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DictionaryEntry, DictionaryKind};

    fn candidate(
        draft: &str,
        accepted: &str,
        settings: &AppSettings,
    ) -> Option<DictionarySuggestion> {
        qualify_dictionary_suggestion(draft, accepted, settings)
    }

    #[test]
    fn single_word_substitution_qualifies() {
        assert_eq!(
            candidate(
                "The Tori sidecar is ready.",
                "The Tauri sidecar is ready.",
                &AppSettings::default(),
            ),
            Some(DictionarySuggestion {
                spoken: "Tori".into(),
                replacement: "Tauri".into(),
            })
        );
    }

    #[test]
    fn multi_word_change_does_not_qualify() {
        assert_eq!(
            candidate(
                "Use Tori.",
                "Use the Tauri framework.",
                &AppSettings::default()
            ),
            None
        );
    }

    #[test]
    fn two_single_word_substitutions_do_not_qualify() {
        assert_eq!(
            candidate(
                "Tori uses Wisp.",
                "Tauri uses Whisper.",
                &AppSettings::default()
            ),
            None
        );
    }

    #[test]
    fn word_already_in_dictionary_does_not_qualify() {
        let mut settings = AppSettings::default();
        settings.dictionary.push(DictionaryEntry {
            id: "tauri".into(),
            spoken: "Tory".into(),
            replacement: "Tauri".into(),
            kind: DictionaryKind::Word,
        });
        assert_eq!(candidate("Use Tori.", "Use Tauri.", &settings), None);

        let mut settings = AppSettings::default();
        settings.dictionary.push(DictionaryEntry {
            id: "tori".into(),
            spoken: "Tori".into(),
            replacement: "Tory".into(),
            kind: DictionaryKind::Word,
        });
        assert_eq!(candidate("Use Tori.", "Use Tauri.", &settings), None);
    }

    #[test]
    fn previously_dismissed_pair_does_not_qualify() {
        let mut settings = AppSettings::default();
        settings.dismissed_suggestions.push(DictionarySuggestion {
            spoken: "tori".into(),
            replacement: "tauri".into(),
        });
        assert_eq!(candidate("Use Tori.", "Use Tauri.", &settings), None);
    }

    #[test]
    fn original_word_still_present_elsewhere_does_not_qualify() {
        assert_eq!(
            candidate(
                "Tori works with Tori.",
                "Tauri works with Tori.",
                &AppSettings::default(),
            ),
            None
        );
    }

    #[test]
    fn accepting_without_edits_produces_no_suggestion() {
        assert_eq!(
            candidate(
                "The Tori sidecar is ready.",
                "The Tori sidecar is ready.",
                &AppSettings::default(),
            ),
            None
        );
    }
}
