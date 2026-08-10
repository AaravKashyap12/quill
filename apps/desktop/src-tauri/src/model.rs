use crate::register::Register;
use serde::{Deserialize, Serialize};

pub const MAX_DISMISSED_SUGGESTIONS: usize = 200;

#[cfg(any(target_os = "macos", test))]
fn has_legacy_windows_hotkeys(settings: &AppSettings) -> bool {
    settings.dictation_hotkey.modifiers.len() == 1
        && settings.dictation_hotkey.modifiers[0] == "Ctrl"
        && settings.dictation_hotkey.key == "Space"
        && settings.scribe_hotkey.modifiers.len() == 2
        && settings.scribe_hotkey.modifiers[0] == "Ctrl"
        && settings.scribe_hotkey.modifiers[1] == "Shift"
        && settings.scribe_hotkey.key == "Space"
}

fn generic_register() -> Register {
    Register::Generic
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyConfig {
    pub modifiers: Vec<String>,
    pub key: String,
    pub behavior: HotkeyBehavior,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyBehavior {
    Hold,
    TapToLock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DictionaryKind {
    Word,
    Snippet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: String,
    pub spoken: String,
    pub replacement: String,
    pub kind: DictionaryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DictionarySuggestion {
    pub spoken: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub dictation_hotkey: HotkeyConfig,
    pub scribe_hotkey: HotkeyConfig,
    #[serde(default)]
    pub audio_input_device: Option<String>,
    pub whisper_model: String,
    pub backend: ComputeBackend,
    pub language: String,
    #[serde(default)]
    pub transcription_provider: TranscriptionProvider,
    #[serde(default = "generic_register")]
    pub default_register: Register,
    pub cleanup_provider: CleanupProvider,
    pub cleanup_model: String,
    pub cleanup_base_url: String,
    pub trailing_buffer_ms: u64,
    pub launch_at_startup: bool,
    pub keep_recovery_audio: bool,
    pub injection_mode: InjectionMode,
    #[serde(default)]
    pub dictionary: Vec<DictionaryEntry>,
    #[serde(default)]
    pub dismissed_suggestions: Vec<DictionarySuggestion>,
    #[serde(default)]
    pub speech_model_setup_attempted: bool,
    #[serde(default)]
    pub scribe_setup_dismissed: bool,
    #[serde(default)]
    pub onboarding_completed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let (dictation_hotkey, scribe_hotkey) = (
            HotkeyConfig {
                modifiers: vec!["Meta".into(), "Shift".into()],
                key: "D".into(),
                behavior: HotkeyBehavior::Hold,
            },
            HotkeyConfig {
                modifiers: vec!["Meta".into(), "Shift".into()],
                key: "S".into(),
                behavior: HotkeyBehavior::Hold,
            },
        );
        #[cfg(not(target_os = "macos"))]
        let (dictation_hotkey, scribe_hotkey) = (
            HotkeyConfig {
                modifiers: vec!["Ctrl".into()],
                key: "Space".into(),
                behavior: HotkeyBehavior::Hold,
            },
            HotkeyConfig {
                modifiers: vec!["Ctrl".into(), "Shift".into()],
                key: "Space".into(),
                behavior: HotkeyBehavior::Hold,
            },
        );
        Self {
            dictation_hotkey,
            scribe_hotkey,
            audio_input_device: None,
            whisper_model: "base.en".into(),
            backend: ComputeBackend::Auto,
            language: "en".into(),
            transcription_provider: TranscriptionProvider::Local,
            default_register: Register::Generic,
            cleanup_provider: CleanupProvider::Auto,
            cleanup_model: String::new(),
            cleanup_base_url: "http://127.0.0.1:11434".into(),
            trailing_buffer_ms: 1_500,
            launch_at_startup: false,
            keep_recovery_audio: true,
            injection_mode: InjectionMode::Clipboard,
            dictionary: Vec::new(),
            dismissed_suggestions: Vec::new(),
            speech_model_setup_attempted: false,
            scribe_setup_dismissed: false,
            onboarding_completed: false,
        }
    }
}

impl AppSettings {
    pub fn cap_dismissed_suggestions(&mut self) {
        let excess = self
            .dismissed_suggestions
            .len()
            .saturating_sub(MAX_DISMISSED_SUGGESTIONS);
        if excess > 0 {
            self.dismissed_suggestions.drain(..excess);
        }
    }

    /// Prevent a settings file copied from another operating system from
    /// selecting a backend that cannot exist on this machine.
    pub fn normalize_backend_for_platform(&mut self) -> bool {
        #[cfg(windows)]
        let unsupported = self.backend == ComputeBackend::Metal;
        #[cfg(target_os = "macos")]
        let unsupported = self.backend == ComputeBackend::Cuda;
        #[cfg(not(any(windows, target_os = "macos")))]
        let unsupported = matches!(self.backend, ComputeBackend::Cuda | ComputeBackend::Metal);

        if unsupported {
            self.backend = ComputeBackend::Auto;
        }
        unsupported
    }

    /// Migrate only the untouched Windows defaults on macOS. Exact matching
    /// protects every shortcut the user has deliberately customised.
    pub fn normalize_hotkeys_for_platform(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            if has_legacy_windows_hotkeys(self) {
                self.dictation_hotkey.modifiers = vec!["Meta".into(), "Shift".into()];
                self.dictation_hotkey.key = "D".into();
                self.scribe_hotkey.modifiers = vec!["Meta".into(), "Shift".into()];
                self.scribe_hotkey.key = "S".into();
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ComputeBackend {
    Auto,
    Cpu,
    Cuda,
    Metal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupProvider {
    Auto,
    Ollama,
    OpenaiCompatible,
    Gemini,
    Disabled,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptionProvider {
    #[default]
    Local,
    Groq,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InjectionMode {
    Clipboard,
    Keystrokes,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dictation,
    Scribe,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub kind: String,
    pub base_url: String,
    pub available: bool,
    pub models: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_entries_survive_settings_serialization() {
        let mut settings = AppSettings::default();
        settings.dictionary.push(DictionaryEntry {
            id: "entry-1".into(),
            spoken: "my email".into(),
            replacement: "aarav@example.com".into(),
            kind: DictionaryKind::Snippet,
        });

        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.dictionary, settings.dictionary);
    }

    #[test]
    fn older_settings_fields_receive_safe_defaults() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("dictionary");
        value
            .as_object_mut()
            .unwrap()
            .remove("dismissedSuggestions");
        value.as_object_mut().unwrap().remove("defaultRegister");
        value
            .as_object_mut()
            .unwrap()
            .remove("transcriptionProvider");
        value
            .as_object_mut()
            .unwrap()
            .remove("speechModelSetupAttempted");
        value
            .as_object_mut()
            .unwrap()
            .remove("scribeSetupDismissed");
        value.as_object_mut().unwrap().remove("onboardingCompleted");
        let restored: AppSettings = serde_json::from_value(value).unwrap();
        assert!(restored.dictionary.is_empty());
        assert!(restored.dismissed_suggestions.is_empty());
        assert_eq!(restored.default_register, Register::Generic);
        assert_eq!(
            restored.transcription_provider,
            TranscriptionProvider::Local
        );
        assert!(!restored.speech_model_setup_attempted);
        assert!(!restored.scribe_setup_dismissed);
        assert!(!restored.onboarding_completed);
    }

    #[test]
    fn legacy_windows_hotkey_detection_never_matches_custom_shortcuts() {
        let defaults = AppSettings::default();
        #[cfg(not(target_os = "macos"))]
        assert!(has_legacy_windows_hotkeys(&defaults));

        let mut custom = defaults;
        custom.dictation_hotkey.key = "D".into();
        assert!(!has_legacy_windows_hotkeys(&custom));
    }

    #[test]
    fn default_register_survives_settings_serialization() {
        let settings = AppSettings {
            default_register: Register::Email,
            ..AppSettings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.default_register, Register::Email);
    }

    #[test]
    fn dismissed_suggestions_survive_settings_serialization() {
        let mut settings = AppSettings::default();
        settings.dismissed_suggestions.push(DictionarySuggestion {
            spoken: "Tori".into(),
            replacement: "Tauri".into(),
        });

        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.dismissed_suggestions,
            settings.dismissed_suggestions
        );
    }

    #[test]
    fn dismissed_suggestion_cap_keeps_the_newest_pairs() {
        let mut settings = AppSettings {
            dismissed_suggestions: (0..MAX_DISMISSED_SUGGESTIONS + 2)
                .map(|index| DictionarySuggestion {
                    spoken: format!("spoken-{index}"),
                    replacement: format!("replacement-{index}"),
                })
                .collect(),
            ..AppSettings::default()
        };

        settings.cap_dismissed_suggestions();

        assert_eq!(
            settings.dismissed_suggestions.len(),
            MAX_DISMISSED_SUGGESTIONS
        );
        assert_eq!(settings.dismissed_suggestions[0].spoken, "spoken-2");
        assert_eq!(
            settings.dismissed_suggestions.last().unwrap().spoken,
            format!("spoken-{}", MAX_DISMISSED_SUGGESTIONS + 1)
        );
    }
}
