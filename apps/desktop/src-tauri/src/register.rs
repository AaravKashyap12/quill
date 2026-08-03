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

const BROWSERS: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "arc", "opera", "vivaldi",
];

/// Resolve the writing register from the application that owned the caret
/// when Scribe started. The raw process path and window title are deliberately
/// short-lived: classification returns only this enum, and neither signal is
/// logged or recorded in metrics because titles can contain private subjects
/// and document names.
pub fn resolve(target: &InsertionTarget) -> Register {
    #[cfg(windows)]
    {
        classify(
            window_title(target.top_level()).as_deref(),
            process_name(target.process_id()).as_deref(),
        )
    }

    #[cfg(not(windows))]
    {
        let _ = target;
        Register::Generic
    }
}

fn classify(title: Option<&str>, process: Option<&str>) -> Register {
    let process = process.map(str::to_lowercase);
    let is_browser = process
        .as_deref()
        .is_some_and(|process| BROWSERS.contains(&process));

    if !is_browser {
        // A recognized native application's process identifies the writing
        // context. Its title is user content (for example, a filename), so it
        // must not be allowed to override the process classification.
        match process.as_deref() {
            Some("outlook" | "thunderbird") => return Register::Email,
            Some("slack" | "discord" | "teams") => return Register::Chat,
            Some("code" | "cursor" | "windowsterminal" | "powershell" | "cmd" | "alacritty") => {
                return Register::Prompt
            }
            Some("notepad" | "winword" | "obsidian" | "notion") => {
                return Register::Notes;
            }
            _ => {}
        }
    }

    // Browser and unrecognized processes have no authoritative process
    // mapping, so their title is the only useful signal available.
    if let Some(title) = title {
        let title = title.to_lowercase();
        if contains_any(
            &title,
            &["claude", "chatgpt", "gemini", "copilot", "perplexity"],
        ) {
            return Register::Prompt;
        }
        if contains_any(&title, &["gmail", "outlook", "mail", "superhuman"]) {
            return Register::Email;
        }
        if contains_any(&title, &["slack", "discord", "teams", "whatsapp"]) {
            return Register::Chat;
        }
    }

    Register::Generic
}

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
