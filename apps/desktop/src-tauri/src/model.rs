use crate::register::{Register, TargetApp};
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StyleTone {
    Adaptive,
    Formal,
    Casual,
    Direct,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StyleLength {
    Brief,
    Balanced,
    Detailed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StylePolicy {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StyleStructure {
    Auto,
    Paragraphs,
    Bullets,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfile {
    pub target_app: TargetApp,
    pub tone: StyleTone,
    pub length: StyleLength,
    pub greeting: StylePolicy,
    pub sign_off: StylePolicy,
    pub contractions: StylePolicy,
    pub structure: StyleStructure,
    #[serde(default)]
    pub learned_samples: u16,
    #[serde(default)]
    pub total_words: u32,
    #[serde(default)]
    pub greeting_samples: u16,
    #[serde(default)]
    pub sign_off_samples: u16,
    #[serde(default)]
    pub contraction_samples: u16,
    #[serde(default)]
    pub bullet_samples: u16,
    #[serde(default)]
    pub paragraph_samples: u16,
    #[serde(default)]
    pub formal_samples: u16,
    #[serde(default)]
    pub casual_samples: u16,
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
    #[serde(default)]
    pub scribe_context_enabled: bool,
    #[serde(default)]
    pub style_profiles: Vec<StyleProfile>,
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
            scribe_context_enabled: false,
            style_profiles: Vec::new(),
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

    /// Learn only aggregate presentation traits from text the user explicitly
    /// accepted. The accepted text itself is never retained in settings.
    pub fn learn_style_profile(&mut self, target_app: TargetApp, accepted_text: &str) {
        let sample = StyleSample::from_text(accepted_text);
        if !self
            .style_profiles
            .iter()
            .any(|profile| profile.target_app == target_app)
        {
            self.style_profiles
                .push(StyleProfile::default_for(target_app));
        }
        let profile = self
            .style_profiles
            .iter_mut()
            .find(|profile| profile.target_app == target_app)
            .expect("a style profile exists for the target app");
        profile.learned_samples = profile.learned_samples.saturating_add(1);
        profile.total_words = profile.total_words.saturating_add(sample.word_count as u32);
        profile.greeting_samples = profile
            .greeting_samples
            .saturating_add(u16::from(sample.greeting));
        profile.sign_off_samples = profile
            .sign_off_samples
            .saturating_add(u16::from(sample.sign_off));
        profile.contraction_samples = profile
            .contraction_samples
            .saturating_add(u16::from(sample.contractions));
        profile.bullet_samples = profile
            .bullet_samples
            .saturating_add(u16::from(sample.bullets));
        profile.paragraph_samples = profile
            .paragraph_samples
            .saturating_add(u16::from(sample.paragraphs));
        profile.formal_samples = profile
            .formal_samples
            .saturating_add(u16::from(sample.formal));
        profile.casual_samples = profile
            .casual_samples
            .saturating_add(u16::from(sample.casual));
        profile.recompute_preferences();
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
#[allow(clippy::items_after_test_module)]
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
        value
            .as_object_mut()
            .unwrap()
            .remove("scribeContextEnabled");
        value.as_object_mut().unwrap().remove("styleProfiles");
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
        assert!(!restored.scribe_context_enabled);
        assert!(restored.style_profiles.is_empty());
    }

    #[test]
    fn style_profiles_survive_settings_serialization_without_raw_examples() {
        let mut settings = AppSettings::default();
        settings.style_profiles.push(StyleProfile {
            target_app: crate::register::TargetApp::Slack,
            tone: StyleTone::Casual,
            length: StyleLength::Brief,
            greeting: StylePolicy::Never,
            sign_off: StylePolicy::Never,
            contractions: StylePolicy::Always,
            structure: StyleStructure::Paragraphs,
            learned_samples: 3,
            ..StyleProfile::default_for(crate::register::TargetApp::Slack)
        });

        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.style_profiles, settings.style_profiles);
        assert!(!json.contains("rawDraft"));
        assert!(!json.contains("acceptedText"));
    }

    #[test]
    fn accepted_text_updates_only_aggregate_style_traits() {
        let mut settings = AppSettings::default();
        settings.learn_style_profile(
            TargetApp::Slack,
            "Hey team!\n\n- Ship the desktop build\n- Publish the notes",
        );

        let profile = settings.style_profiles.first().unwrap();
        assert_eq!(profile.target_app, TargetApp::Slack);
        assert_eq!(profile.learned_samples, 1);
        assert_eq!(profile.tone, StyleTone::Casual);
        assert_eq!(profile.structure, StyleStructure::Bullets);
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("Ship the desktop build"));
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

impl StyleProfile {
    fn default_for(target_app: TargetApp) -> Self {
        Self {
            target_app,
            tone: StyleTone::Adaptive,
            length: StyleLength::Balanced,
            greeting: StylePolicy::Auto,
            sign_off: StylePolicy::Auto,
            contractions: StylePolicy::Auto,
            structure: StyleStructure::Auto,
            learned_samples: 0,
            total_words: 0,
            greeting_samples: 0,
            sign_off_samples: 0,
            contraction_samples: 0,
            bullet_samples: 0,
            paragraph_samples: 0,
            formal_samples: 0,
            casual_samples: 0,
        }
    }

    fn recompute_preferences(&mut self) {
        let samples = self.learned_samples.max(1);
        let majority = |count: u16| count.saturating_mul(2) >= samples;
        self.greeting = if majority(self.greeting_samples) {
            StylePolicy::Always
        } else {
            StylePolicy::Never
        };
        self.sign_off = if majority(self.sign_off_samples) {
            StylePolicy::Always
        } else {
            StylePolicy::Never
        };
        self.contractions = if majority(self.contraction_samples) {
            StylePolicy::Always
        } else {
            StylePolicy::Never
        };
        self.structure = if majority(self.bullet_samples) {
            StyleStructure::Bullets
        } else if majority(self.paragraph_samples) {
            StyleStructure::Paragraphs
        } else {
            StyleStructure::Auto
        };
        let average_words = self.total_words / u32::from(samples);
        self.length = if average_words <= 24 {
            StyleLength::Brief
        } else if average_words >= 100 {
            StyleLength::Detailed
        } else {
            StyleLength::Balanced
        };
        self.tone = if self.formal_samples >= self.casual_samples && majority(self.formal_samples) {
            StyleTone::Formal
        } else if majority(self.casual_samples) {
            StyleTone::Casual
        } else {
            StyleTone::Direct
        };
    }
}

struct StyleSample {
    word_count: usize,
    greeting: bool,
    sign_off: bool,
    contractions: bool,
    bullets: bool,
    paragraphs: bool,
    formal: bool,
    casual: bool,
}

impl StyleSample {
    fn from_text(text: &str) -> Self {
        let lower = text.to_lowercase();
        let trimmed = lower.trim();
        let greeting = ["hi ", "hello ", "dear ", "hey "]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix));
        let sign_off = ["\nbest,", "\nthanks,", "\nregards,", "\ncheers,"]
            .iter()
            .any(|marker| lower.contains(marker));
        let contractions = ["'m", "'re", "'ve", "'ll", "n't", "'d"]
            .iter()
            .any(|marker| lower.contains(marker));
        let bullet_lines = text
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                line.starts_with("- ") || line.starts_with("• ") || line.starts_with("* ")
            })
            .count();
        let paragraphs = text
            .split("\n\n")
            .filter(|part| !part.trim().is_empty())
            .count()
            > 1;
        let formal = greeting && sign_off;
        let casual = contractions || trimmed.starts_with("hey ") || text.contains('!');
        Self {
            word_count: text.split_whitespace().count(),
            greeting,
            sign_off,
            contractions,
            bullets: bullet_lines >= 2,
            paragraphs,
            formal,
            casual,
        }
    }
}
