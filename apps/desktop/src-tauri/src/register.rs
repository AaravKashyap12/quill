use crate::injection::InsertionTarget;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Register {
    Email,
    Chat,
    Prompt,
    Notes,
    Generic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetApp {
    Gmail,
    Outlook,
    Slack,
    Discord,
    Teams,
    Whatsapp,
    Chatgpt,
    Claude,
    Gemini,
    Copilot,
    Perplexity,
    Notion,
    Obsidian,
    Word,
    Notepad,
    Code,
    Cursor,
    Terminal,
    Generic,
}

impl TargetApp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::Outlook => "Outlook",
            Self::Slack => "Slack",
            Self::Discord => "Discord",
            Self::Teams => "Teams",
            Self::Whatsapp => "WhatsApp",
            Self::Chatgpt => "ChatGPT",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Copilot => "Copilot",
            Self::Perplexity => "Perplexity",
            Self::Notion => "Notion",
            Self::Obsidian => "Obsidian",
            Self::Word => "Word",
            Self::Notepad => "Notepad",
            Self::Code => "Visual Studio Code",
            Self::Cursor => "Cursor",
            Self::Terminal => "Terminal",
            Self::Generic => "Current app",
        }
    }

    pub fn register(self) -> Register {
        match self {
            Self::Gmail | Self::Outlook => Register::Email,
            Self::Slack | Self::Discord | Self::Teams | Self::Whatsapp => Register::Chat,
            Self::Chatgpt
            | Self::Claude
            | Self::Gemini
            | Self::Copilot
            | Self::Perplexity
            | Self::Code
            | Self::Cursor
            | Self::Terminal => Register::Prompt,
            Self::Notion | Self::Obsidian | Self::Word | Self::Notepad => Register::Notes,
            Self::Generic => Register::Generic,
        }
    }
}

#[cfg(any(windows, test))]
const BROWSERS: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "arc", "opera", "vivaldi",
];

/// Resolve the writing register from the application that owned the caret
/// when Scribe started. The raw process path and window title are deliberately
/// short-lived: classification returns only this enum, and neither signal is
/// logged or recorded in metrics because titles can contain private subjects
/// and document names.
pub fn resolve_target_app(target: &InsertionTarget) -> TargetApp {
    #[cfg(windows)]
    {
        classify_target(
            window_title(target.top_level()).as_deref(),
            process_name(target.process_id()).as_deref(),
        )
    }

    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        return classify_bundle_identifier(target.bundle_identifier().as_deref());

        #[cfg(not(target_os = "macos"))]
        {
            let _ = target;
            TargetApp::Generic
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn classify_bundle_identifier(bundle_identifier: Option<&str>) -> TargetApp {
    let identifier = bundle_identifier.unwrap_or_default().to_lowercase();
    if identifier.contains("microsoft.outlook") || identifier == "com.apple.mail" {
        TargetApp::Outlook
    } else if identifier.contains("slack") {
        TargetApp::Slack
    } else if identifier.contains("discord") {
        TargetApp::Discord
    } else if identifier.contains("teams") {
        TargetApp::Teams
    } else if identifier.contains("whatsapp") {
        TargetApp::Whatsapp
    } else if identifier.contains("notion") {
        TargetApp::Notion
    } else if identifier.contains("obsidian") {
        TargetApp::Obsidian
    } else if identifier.contains("microsoft.word") {
        TargetApp::Word
    } else if identifier.contains("visual-studio-code") || identifier.contains("vscode") {
        TargetApp::Code
    } else if identifier.contains("cursor") {
        TargetApp::Cursor
    } else if identifier.contains("terminal") || identifier.contains("iterm") {
        TargetApp::Terminal
    } else {
        TargetApp::Generic
    }
}

#[cfg(test)]
fn classify(title: Option<&str>, process: Option<&str>) -> Register {
    classify_target(title, process).register()
}

#[cfg(any(windows, test))]
fn classify_target(title: Option<&str>, process: Option<&str>) -> TargetApp {
    let process = process.map(str::to_lowercase);
    let is_browser = process
        .as_deref()
        .is_some_and(|process| BROWSERS.contains(&process));

    if !is_browser {
        // A recognized native application's process identifies the writing
        // context. Its title is user content (for example, a filename), so it
        // must not be allowed to override the process classification.
        match process.as_deref() {
            Some("outlook" | "thunderbird") => return TargetApp::Outlook,
            Some("slack") => return TargetApp::Slack,
            Some("discord") => return TargetApp::Discord,
            Some("teams") => return TargetApp::Teams,
            Some("code") => return TargetApp::Code,
            Some("cursor") => return TargetApp::Cursor,
            Some("windowsterminal" | "powershell" | "cmd" | "alacritty") => {
                return TargetApp::Terminal
            }
            Some("notepad") => return TargetApp::Notepad,
            Some("winword") => return TargetApp::Word,
            Some("obsidian") => return TargetApp::Obsidian,
            Some("notion") => return TargetApp::Notion,
            _ => {}
        }
    }

    // Browser and unrecognized processes have no authoritative process
    // mapping, so their title is the only useful signal available.
    if let Some(title) = title {
        let title = title.to_lowercase();
        if title.contains("claude") {
            return TargetApp::Claude;
        }
        if title.contains("chatgpt") {
            return TargetApp::Chatgpt;
        }
        if title.contains("gemini") {
            return TargetApp::Gemini;
        }
        if title.contains("copilot") {
            return TargetApp::Copilot;
        }
        if title.contains("perplexity") {
            return TargetApp::Perplexity;
        }
        if title.contains("gmail") {
            return TargetApp::Gmail;
        }
        if contains_any(&title, &["outlook", "mail", "superhuman"]) {
            return TargetApp::Outlook;
        }
        if title.contains("slack") {
            return TargetApp::Slack;
        }
        if title.contains("discord") {
            return TargetApp::Discord;
        }
        if title.contains("teams") {
            return TargetApp::Teams;
        }
        if title.contains("whatsapp") {
            return TargetApp::Whatsapp;
        }
        if title.contains("notion") {
            return TargetApp::Notion;
        }
    }

    TargetApp::Generic
}

#[cfg(any(windows, test))]
fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(windows)]
fn process_name(process_id: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }

    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let queried =
        unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } != 0;
    unsafe {
        CloseHandle(process);
    }
    if !queried || length == 0 {
        return None;
    }

    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    std::path::Path::new(&path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
}

#[cfg(windows)]
fn window_title(top_level: isize) -> Option<String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let window = top_level as HWND;
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return None;
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_titles_select_the_register() {
        assert_eq!(
            classify(Some("Draft - ChatGPT"), Some("chrome")),
            Register::Prompt
        );
        assert_eq!(
            classify(Some("Inbox - Gmail"), Some("msedge")),
            Register::Email
        );
        assert_eq!(
            classify(Some("Project - Slack"), Some("firefox")),
            Register::Chat
        );
    }

    #[test]
    fn native_processes_ignore_content_keywords_in_titles() {
        assert_eq!(
            classify(Some("mailer.ts - quill - Visual Studio Code"), Some("code")),
            Register::Prompt
        );
        assert_eq!(
            classify(
                Some("teams-api.ts - quill - Visual Studio Code"),
                Some("code")
            ),
            Register::Prompt
        );
        assert_eq!(
            classify(
                Some("npm run mail - Windows Terminal"),
                Some("windowsterminal")
            ),
            Register::Prompt
        );
        assert_eq!(
            classify(Some("Email drafts - Obsidian"), Some("obsidian")),
            Register::Notes
        );
        assert_eq!(
            classify(Some("Project - Slack"), Some("code")),
            Register::Prompt
        );
    }

    #[test]
    fn unknown_processes_can_still_use_title_detection() {
        assert_eq!(
            classify(Some("Claude"), Some("claude-electron")),
            Register::Prompt
        );
    }

    #[test]
    fn process_classification_covers_native_targets() {
        for process in ["outlook", "thunderbird"] {
            assert_eq!(classify(None, Some(process)), Register::Email);
        }
        for process in ["slack", "discord", "teams"] {
            assert_eq!(classify(None, Some(process)), Register::Chat);
        }
        for process in [
            "code",
            "cursor",
            "windowsterminal",
            "powershell",
            "cmd",
            "alacritty",
        ] {
            assert_eq!(classify(None, Some(process)), Register::Prompt);
        }
        for process in ["notepad", "winword", "obsidian", "notion"] {
            assert_eq!(classify(None, Some(process)), Register::Notes);
        }
        assert_eq!(classify(None, Some("chrome")), Register::Generic);
        assert_eq!(classify(None, None), Register::Generic);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            classify(Some("AARAV - CLAUDE"), Some("CHROME")),
            Register::Prompt
        );
        assert_eq!(classify(None, Some("PowerShell")), Register::Prompt);
    }

    #[test]
    fn macos_bundle_identifiers_map_without_reading_document_titles() {
        assert_eq!(
            classify_bundle_identifier(Some("com.tinyspeck.slackmacgap")),
            TargetApp::Slack
        );
        assert_eq!(
            classify_bundle_identifier(Some("com.microsoft.VSCode")),
            TargetApp::Code
        );
        assert_eq!(
            classify_bundle_identifier(Some("com.apple.mail")),
            TargetApp::Outlook
        );
    }

    #[test]
    fn register_serializes_to_the_frontend_contract() {
        assert_eq!(
            serde_json::to_string(&Register::Email).unwrap(),
            "\"email\""
        );
        assert_eq!(serde_json::to_string(&Register::Chat).unwrap(), "\"chat\"");
        assert_eq!(
            serde_json::to_string(&Register::Prompt).unwrap(),
            "\"prompt\""
        );
        assert_eq!(
            serde_json::to_string(&Register::Notes).unwrap(),
            "\"notes\""
        );
        assert_eq!(
            serde_json::to_string(&Register::Generic).unwrap(),
            "\"generic\""
        );
    }
}
