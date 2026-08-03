use super::TargetInjection;
use anyhow::{anyhow, Result};
use arboard::Clipboard;
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_CONTROL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, IsWindow, SendMessageTimeoutW,
    SetForegroundWindow, GUITHREADINFO, SMTO_ABORTIFHUNG, WM_PASTE,
};

/// A stable snapshot of the app/control that had the text caret when Quill's
/// hotkey was pressed. `SendInput` cannot target a background app, so this is
/// also used to decide when it is unsafe to emit simulated keystrokes.
#[derive(Clone, Debug)]
pub struct InsertionTarget {
    top_level: isize,
    focused_control: isize,
    process_id: u32,
    thread_id: u32,
}

impl InsertionTarget {
    pub(super) fn top_level(&self) -> isize {
        self.top_level
    }

    pub(super) fn process_id(&self) -> u32 {
        self.process_id
    }
}

fn hwnd(value: isize) -> HWND {
    value as HWND
}

pub fn capture_target() -> Result<InsertionTarget> {
    let top_level = unsafe { GetForegroundWindow() };
    if top_level.is_null() {
        return Err(anyhow!(
            "no foreground window was available to receive dictation"
        ));
    }

    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(top_level, &mut process_id) };
    if thread_id == 0 || process_id == 0 {
        return Err(anyhow!("could not identify the foreground text target"));
    }

    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let focused_control =
        if unsafe { GetGUIThreadInfo(thread_id, &mut info) } != 0 && !info.hwndFocus.is_null() {
            info.hwndFocus
        } else {
            top_level
        };

    tracing::info!(
        top_level = ?top_level,
        focused_control = ?focused_control,
        process_id,
        thread_id,
        "captured dictation insertion target"
    );
    Ok(InsertionTarget {
        top_level: top_level as isize,
        focused_control: focused_control as isize,
        process_id,
        thread_id,
    })
}

pub fn target_is_foreground(target: &InsertionTarget) -> bool {
    target_is_available(target) && unsafe { GetForegroundWindow() == hwnd(target.top_level) }
}

pub fn target_is_available(target: &InsertionTarget) -> bool {
    if unsafe { IsWindow(hwnd(target.top_level)) } == 0 {
        return false;
    }
    let mut current_process_id = 0;
    let current_thread_id =
        unsafe { GetWindowThreadProcessId(hwnd(target.top_level), &mut current_process_id) };
    current_process_id == target.process_id && current_thread_id == target.thread_id
}

fn target_handle(target: &InsertionTarget) -> HWND {
    if unsafe { IsWindow(hwnd(target.focused_control)) } != 0 {
        hwnd(target.focused_control)
    } else {
        hwnd(target.top_level)
    }
}

/// `WM_PASTE` is the only background insertion attempt we make. It addresses
/// compatible Win32/Electron controls directly and has a short timeout; it
/// never changes focus. A timeout or rejected control is treated as queued
/// text rather than falling through to global `SendInput`.
fn paste_to_target(target: &InsertionTarget) -> bool {
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            target_handle(target),
            WM_PASTE,
            0 as WPARAM,
            0 as LPARAM,
            SMTO_ABORTIFHUNG,
            250,
            &mut result,
        ) != 0
    }
}

pub fn inject_text_to_target(
    target: &InsertionTarget,
    text: &str,
    use_clipboard: bool,
) -> Result<TargetInjection> {
    if !target_is_available(target) {
        return Ok(TargetInjection::Unavailable);
    }

    if use_clipboard {
        Clipboard::new()?.set_text(text.to_owned())?;
        if !target_is_foreground(target) {
            if paste_to_target(target) {
                tracing::info!("inserted text into the pinned background target");
                return Ok(TargetInjection::Inserted);
            }
            return Ok(TargetInjection::Queued);
        }
        // A focused target uses the normal keyboard paste path, which has the
        // broadest compatibility with browser/Electron editing surfaces.
        paste()?;
        return Ok(TargetInjection::Inserted);
    }

    // Direct Unicode uses the global input stream. It is safe only while the
    // same top-level target still owns the foreground.
    if !target_is_foreground(target) {
        return Ok(TargetInjection::Queued);
    }
    type_unicode(text)?;
    Ok(TargetInjection::Inserted)
}

pub fn inject_review_text(
    target: &InsertionTarget,
    text: &str,
    use_clipboard: bool,
) -> Result<TargetInjection> {
    if !target_is_available(target) {
        return Ok(TargetInjection::Unavailable);
    }

    // A Done click is an explicit hand-off back to the original editor, so a
    // foreground activation is appropriate here (unlike live Dictation).
    unsafe {
        SetForegroundWindow(hwnd(target.top_level));
    }
    std::thread::sleep(std::time::Duration::from_millis(55));
    inject_text_to_target(target, text, use_clipboard)
}

fn send(inputs: &mut [INPUT]) -> Result<()> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err(anyhow!(
            "SendInput inserted {sent} of {} events",
            inputs.len()
        ));
    }
    Ok(())
}

fn keyboard_input(code: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: if flags & KEYEVENTF_UNICODE == 0 {
                    code
                } else {
                    0
                },
                wScan: if flags & KEYEVENTF_UNICODE != 0 {
                    code
                } else {
                    0
                },
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn type_unicode(text: &str) -> Result<()> {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
    for unit in text.encode_utf16() {
        inputs.push(keyboard_input(unit, KEYEVENTF_UNICODE));
        inputs.push(keyboard_input(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    send(&mut inputs)
}

pub fn paste() -> Result<()> {
    const KEY_V: u16 = b'V' as u16;
    let mut inputs = [
        keyboard_input(VK_CONTROL, 0),
        keyboard_input(KEY_V, 0),
        keyboard_input(KEY_V, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    send(&mut inputs)
}
