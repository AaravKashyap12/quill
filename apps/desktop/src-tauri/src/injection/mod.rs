use anyhow::{anyhow, Result};
use arboard::Clipboard;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// The editor Quill was started from. On Windows this captures both the
/// foreground top-level window and, when available, its focused control.
/// Keeping it with the recording session prevents a later app switch from
/// redirecting committed text to the newly focused application.
#[derive(Clone, Debug)]
pub struct InsertionTarget {
    #[cfg(windows)]
    inner: windows::InsertionTarget,
    #[cfg(target_os = "macos")]
    inner: macos::InsertionTarget,
}

/// A privacy-bounded snapshot of text around the caret. The platform layer
/// returns text only from the focused editable control and rejects password
/// fields. Callers must keep this in memory and must never trace its contents.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditorTextContext {
    pub selected_text: Option<String>,
    pub surrounding_before: Option<String>,
    pub surrounding_after: Option<String>,
}

impl InsertionTarget {
    #[cfg(windows)]
    pub(crate) fn top_level(&self) -> isize {
        self.inner.top_level()
    }

    #[cfg(windows)]
    pub(crate) fn process_id(&self) -> u32 {
        self.inner.process_id()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn bundle_identifier(&self) -> Option<String> {
        macos::bundle_identifier(&self.inner)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetInjection {
    /// Text reached the captured target.
    Inserted,
    /// The target is still valid, but cannot safely receive text right now.
    /// The caller must retain the text and retry only when it is safe.
    Queued,
    /// The captured target was closed or otherwise became invalid.
    Unavailable,
}

/// Captures the intended destination before Quill shows its overlay or starts
/// audio. On Windows this is deliberately independent of the global keyboard
/// stream that `SendInput` uses later.
pub fn capture_target() -> Result<InsertionTarget> {
    #[cfg(windows)]
    {
        Ok(InsertionTarget {
            inner: windows::capture_target()?,
        })
    }

    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        return Ok(InsertionTarget {
            inner: macos::capture_target()?,
        });

        #[cfg(not(target_os = "macos"))]
        Ok(InsertionTarget {})
    }
}

/// Capture selected and nearby text without using the clipboard. Unsupported
/// controls return `Ok(None)` so ordinary Scribe remains available.
pub fn capture_editor_context(
    target: &InsertionTarget,
    max_context_chars: i32,
) -> Result<Option<EditorTextContext>> {
    #[cfg(windows)]
    {
        windows::capture_editor_context(&target.inner, max_context_chars)
    }

    #[cfg(target_os = "macos")]
    {
        return macos::capture_editor_context(&target.inner, max_context_chars);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (target, max_context_chars);
        Ok(None)
    }
}

/// Rewrite mode is fail-closed: after review, Quill verifies that the target
/// selection still contains the exact text captured at activation.
pub fn selected_text_matches(target: &InsertionTarget, expected: &str) -> Result<bool> {
    #[cfg(windows)]
    {
        windows::selected_text_matches(&target.inner, expected)
    }

    #[cfg(target_os = "macos")]
    {
        return macos::selected_text_matches(&target.inner, expected);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (target, expected);
        Ok(false)
    }
}

#[cfg(target_os = "macos")]
pub fn request_accessibility_permission() -> bool {
    macos::request_accessibility_permission()
}

/// Whether the captured top-level window is the foreground application.
/// Queued fallback text is only retried in this state, so it can never leak
/// into a different application merely because the user changed windows.
pub fn target_is_foreground(target: &InsertionTarget) -> bool {
    #[cfg(windows)]
    {
        windows::target_is_foreground(&target.inner)
    }

    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        return macos::target_is_foreground(&target.inner);

        #[cfg(not(target_os = "macos"))]
        {
            let _ = target;
            true
        }
    }
}

/// Whether the captured target still refers to the same live Windows
/// application. This protects queued text from being sent to a reused window
/// handle after the original editor was closed.
pub fn target_is_available(target: &InsertionTarget) -> bool {
    #[cfg(windows)]
    {
        windows::target_is_available(&target.inner)
    }

    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        return macos::target_is_available(&target.inner);

        #[cfg(not(target_os = "macos"))]
        {
            let _ = target;
            true
        }
    }
}

/// Sends text to a session's captured target. Windows first attempts a
/// bounded `WM_PASTE` delivery to that specific control when it is in the
/// background; if that target cannot accept the message, no foreground input
/// is sent and the caller receives `Queued` instead.
pub fn inject_text_to_target(
    target: &InsertionTarget,
    text: &str,
    use_clipboard: bool,
) -> Result<TargetInjection> {
    if text.is_empty() {
        return Ok(TargetInjection::Inserted);
    }

    #[cfg(windows)]
    {
        windows::inject_text_to_target(&target.inner, text, use_clipboard)
    }

    #[cfg(target_os = "macos")]
    {
        if !macos::target_is_available(&target.inner) {
            return Ok(TargetInjection::Unavailable);
        }
        if !macos::target_is_foreground(&target.inner) {
            return Ok(TargetInjection::Queued);
        }
        inject_text(text, use_clipboard)?;
        Ok(TargetInjection::Inserted)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = target;
        inject_text(text, use_clipboard)?;
        Ok(TargetInjection::Inserted)
    }
}

/// Completes an explicitly confirmed review by restoring the original editor
/// and inserting there. The foreground switch is intentional here: the user
/// pressed Done and expects to return to the app where Scribe started.
pub fn inject_review_text(
    target: &InsertionTarget,
    text: &str,
    use_clipboard: bool,
) -> Result<TargetInjection> {
    if text.is_empty() {
        return Ok(TargetInjection::Inserted);
    }

    #[cfg(windows)]
    {
        windows::inject_review_text(&target.inner, text, use_clipboard)
    }

    #[cfg(target_os = "macos")]
    {
        macos::activate_target(&target.inner)?;
        inject_text_to_target(target, text, use_clipboard)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        inject_text_to_target(target, text, use_clipboard)
    }
}

#[allow(dead_code)]
pub fn inject_text(text: &str, use_clipboard: bool) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    if use_clipboard {
        Clipboard::new()?.set_text(text.to_owned())?;
        #[cfg(windows)]
        return windows::paste();
        #[cfg(target_os = "macos")]
        return macos::paste();
    }

    #[cfg(windows)]
    return windows::type_unicode(text);
    #[cfg(target_os = "macos")]
    return Err(anyhow!(
        "direct Unicode injection is not enabled on macOS; use clipboard mode"
    ));
    #[allow(unreachable_code)]
    Err(anyhow!("text injection is unsupported on this platform"))
}
