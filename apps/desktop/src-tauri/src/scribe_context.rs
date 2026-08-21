use crate::cleanup::{ScribeAction, ScribeRequest};
use crate::injection::{self, InsertionTarget};
use crate::model::{AppSettings, StyleProfile};
use crate::register::{Register, TargetApp};

pub const MAX_CONTEXT_CHARS_PER_SIDE: i32 = 1_200;

/// Text captured from the editor at the instant Scribe starts. This value is
/// intentionally not serializable: nearby text may contain private messages,
/// document content, or customer data and must remain in memory only.
#[derive(Clone, Debug)]
pub struct CapturedScribeContext {
    pub target_app: TargetApp,
    pub action: ScribeAction,
    pub selected_text: Option<String>,
    pub surrounding_before: Option<String>,
    pub surrounding_after: Option<String>,
    pub context_enabled: bool,
}

impl CapturedScribeContext {
    pub fn capture(settings: &AppSettings, target: &InsertionTarget) -> Self {
        let target_app = crate::register::resolve_target_app(target);
        if !settings.scribe_context_enabled {
            return Self::empty(target_app, false);
        }

        let captured = injection::capture_editor_context(target, MAX_CONTEXT_CHARS_PER_SIDE)
            .ok()
            .flatten()
            .unwrap_or_default();
        let action = infer_action(
            target_app,
            captured.selected_text.as_deref(),
            captured.surrounding_before.as_deref(),
            captured.surrounding_after.as_deref(),
        );
        Self {
            target_app,
            action,
            selected_text: captured.selected_text,
            surrounding_before: captured.surrounding_before,
            surrounding_after: captured.surrounding_after,
            context_enabled: true,
        }
    }

    pub fn empty(target_app: TargetApp, context_enabled: bool) -> Self {
        Self {
            target_app,
            action: ScribeAction::Compose,
            selected_text: None,
            surrounding_before: None,
            surrounding_after: None,
            context_enabled,
        }
    }

    pub fn register(&self, fallback: Register) -> Register {
        let detected = self.target_app.register();
        if detected == Register::Generic {
            fallback
        } else {
            detected
        }
    }

    pub fn style<'a>(&self, settings: &'a AppSettings) -> Option<&'a StyleProfile> {
        settings
            .style_profiles
            .iter()
            .find(|profile| profile.target_app == self.target_app)
    }

    pub fn request<'a>(&'a self, intent: &'a str, settings: &'a AppSettings) -> ScribeRequest<'a> {
        ScribeRequest {
            intent,
            register: self.register(settings.default_register),
            action: self.action,
            selected_text: self.selected_text.as_deref(),
            surrounding_before: self.surrounding_before.as_deref(),
            surrounding_after: self.surrounding_after.as_deref(),
            style: self.style(settings),
        }
    }

    pub fn has_nearby_text(&self) -> bool {
        self.selected_text.is_some()
            || self.surrounding_before.is_some()
            || self.surrounding_after.is_some()
    }

    pub fn without_nearby_text(&self) -> Self {
        let keeps_selection = self.selected_text.is_some();
        Self {
            target_app: self.target_app,
            action: if keeps_selection {
                ScribeAction::Rewrite
            } else {
                ScribeAction::Compose
            },
            selected_text: self.selected_text.clone(),
            surrounding_before: None,
            surrounding_after: None,
            context_enabled: self.context_enabled,
        }
    }

    pub fn context_label(&self) -> String {
        if !self.context_enabled {
            return "Nearby text off".to_owned();
        }
        if !self.has_nearby_text() {
            return format!("{} · no nearby text available", self.target_app.label());
        }
        format!(
            "{} · {} using nearby text",
            self.target_app.label(),
            self.action.label()
        )
    }
}

fn infer_action(
    target_app: TargetApp,
    selected_text: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) -> ScribeAction {
    if selected_text.is_some_and(|text| !text.trim().is_empty()) {
        return ScribeAction::Rewrite;
    }
    let has_nearby_text = before.is_some_and(|text| !text.trim().is_empty())
        || after.is_some_and(|text| !text.trim().is_empty());
    if has_nearby_text && matches!(target_app.register(), Register::Email | Register::Chat) {
        ScribeAction::Reply
    } else {
        ScribeAction::Compose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_always_a_rewrite() {
        assert_eq!(
            infer_action(TargetApp::Code, Some("old wording"), None, None),
            ScribeAction::Rewrite
        );
    }

    #[test]
    fn nearby_conversation_becomes_a_reply_but_prompt_context_stays_compose() {
        assert_eq!(
            infer_action(TargetApp::Slack, None, Some("Earlier message"), None),
            ScribeAction::Reply
        );
        assert_eq!(
            infer_action(TargetApp::Chatgpt, None, Some("Earlier prompt"), None),
            ScribeAction::Compose
        );
    }

    #[test]
    fn captured_context_has_no_serde_contract() {
        let context = CapturedScribeContext {
            target_app: TargetApp::Gmail,
            action: ScribeAction::Reply,
            selected_text: None,
            surrounding_before: Some("private thread".to_owned()),
            surrounding_after: None,
            context_enabled: true,
        };
        assert_eq!(context.context_label(), "Gmail · Reply using nearby text");
    }
}
