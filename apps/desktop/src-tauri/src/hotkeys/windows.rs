use super::HotkeyState;
use crate::model::HotkeyConfig;
use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    VK_SHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Debug, Clone, Copy)]
struct RawKeyEvent {
    key: u32,
    down: bool,
}

static EVENT_SENDER: OnceLock<Mutex<Option<Sender<RawKeyEvent>>>> = OnceLock::new();

fn event_sender() -> &'static Mutex<Option<Sender<RawKeyEvent>>> {
    EVENT_SENDER.get_or_init(|| Mutex::new(None))
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam as u32;
        let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if down || up {
            // Quill's own SendInput events (typing and clipboard paste) must
            // never look like physical shortcut changes.
            let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            if event.flags & LLKHF_INJECTED == 0 {
                if let Ok(sender) = event_sender().lock() {
                    if let Some(sender) = sender.as_ref() {
                        let _ = sender.send(RawKeyEvent {
                            key: event.vkCode,
                            down,
                        });
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

pub struct HotkeyMonitor {
    receiver: Receiver<RawKeyEvent>,
    pressed_keys: HashSet<u32>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HotkeyMonitor {
    pub fn start() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        *event_sender()
            .lock()
            .map_err(|_| anyhow!("global hotkey event channel is unavailable"))? = Some(event_tx);

        let thread = thread::Builder::new()
            .name("quill-hotkeys".into())
            .spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                let hook = unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), std::ptr::null_mut(), 0)
                };
                if hook.is_null() {
                    let _ = ready_tx.send(Err(std::io::Error::last_os_error().to_string()));
                    return;
                }
                if ready_tx.send(Ok(thread_id)).is_err() {
                    unsafe { UnhookWindowsHookEx(hook) };
                    return;
                }

                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {}
                unsafe { UnhookWindowsHookEx(hook) };
            })?;

        let thread_id = ready_rx
            .recv()
            .map_err(|_| anyhow!("global hotkey listener stopped during startup"))?
            .map_err(|reason| anyhow!("could not install the Windows hotkey listener: {reason}"))?;

        Ok(Self {
            receiver: event_rx,
            pressed_keys: HashSet::new(),
            thread_id,
            thread: Some(thread),
        })
    }

    pub fn drain_pair(
        &mut self,
        dictation: &HotkeyConfig,
        scribe: &HotkeyConfig,
    ) -> Vec<(HotkeyState, HotkeyState)> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        events
            .into_iter()
            .map(|event| {
                let repeated = if event.down {
                    !self.pressed_keys.insert(event.key)
                } else {
                    !self.pressed_keys.remove(&event.key)
                };
                (
                    chord_state(dictation, &self.pressed_keys, event.down && !repeated),
                    chord_state(scribe, &self.pressed_keys, event.down && !repeated),
                )
            })
            .collect()
    }
}

impl Drop for HotkeyMonitor {
    fn drop(&mut self) {
        if let Ok(mut sender) = event_sender().lock() {
            *sender = None;
        }
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn virtual_key(key: &str) -> u32 {
    match key {
        "Space" => VK_SPACE as u32,
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as u32,
        _ => 0,
    }
}

fn modifier_down(keys: &HashSet<u32>, generic: u16, left: u16, right: u16) -> bool {
    keys.contains(&(generic as u32))
        || keys.contains(&(left as u32))
        || keys.contains(&(right as u32))
}

fn chord_state(hotkey: &HotkeyConfig, keys: &HashSet<u32>, physical_press: bool) -> HotkeyState {
    let key = virtual_key(&hotkey.key);
    let key_down = key != 0 && keys.contains(&key);
    let modifiers = [
        (
            "Ctrl",
            modifier_down(keys, VK_CONTROL, VK_LCONTROL, VK_RCONTROL),
        ),
        ("Shift", modifier_down(keys, VK_SHIFT, VK_LSHIFT, VK_RSHIFT)),
        ("Alt", modifier_down(keys, VK_MENU, VK_LMENU, VK_RMENU)),
    ];
    let exact_modifiers = modifiers
        .iter()
        .all(|(name, down)| hotkey.modifiers.iter().any(|item| item == name) == *down);
    let down = key_down && exact_modifiers;
    HotkeyState {
        down,
        pressed: physical_press && down,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hotkey(modifiers: &[&str], key: &str) -> HotkeyConfig {
        HotkeyConfig {
            modifiers: modifiers.iter().map(|value| (*value).to_string()).collect(),
            key: key.to_string(),
            behavior: crate::model::HotkeyBehavior::Hold,
        }
    }

    #[test]
    fn exact_modifier_chord_activates_once() {
        let config = hotkey(&["Ctrl"], "Space");
        let keys = HashSet::from([VK_LCONTROL as u32, VK_SPACE as u32]);
        assert!(chord_state(&config, &keys, true).pressed);
        assert!(!chord_state(&config, &keys, false).pressed);
    }

    #[test]
    fn extra_modifier_rejects_chord() {
        let config = hotkey(&["Ctrl"], "Space");
        let keys = HashSet::from([VK_LCONTROL as u32, VK_LSHIFT as u32, VK_SPACE as u32]);
        assert!(!chord_state(&config, &keys, true).down);
    }
}
