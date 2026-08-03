use super::HotkeyState;
use crate::model::HotkeyConfig;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT, VK_SPACE,
};

#[derive(Debug, Clone, Copy, Default)]
struct KeyState {
    down: bool,
    pressed: bool,
}

fn sample(key: i32) -> KeyState {
    // Both bits come from polling GetAsyncKeyState; no keyboard hook is used.
    let state = unsafe { GetAsyncKeyState(key) as u16 };
    KeyState {
        down: state & 0x8000 != 0,
        pressed: state & 0x0001 != 0,
    }
}

fn virtual_key(key: &str) -> i32 {
    match key {
        "Space" => VK_SPACE as i32,
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as i32,
        _ => 0,
    }
}

pub fn poll_pair(dictation: &HotkeyConfig, scribe: &HotkeyConfig) -> (HotkeyState, HotkeyState) {
    let ctrl = sample(VK_CONTROL as i32);
    let shift = sample(VK_SHIFT as i32);
    let alt = sample(VK_MENU as i32);
    let dictation_key = virtual_key(&dictation.key);
    let scribe_key = virtual_key(&scribe.key);
    let dictation_sample = if dictation_key != 0 {
        sample(dictation_key)
    } else {
        KeyState::default()
    };
    let scribe_sample = if scribe_key == dictation_key {
        dictation_sample
    } else {
        if scribe_key != 0 {
            sample(scribe_key)
        } else {
            KeyState::default()
        }
    };
    (
        chord_state(dictation, dictation_sample, ctrl, shift, alt),
        chord_state(scribe, scribe_sample, ctrl, shift, alt),
    )
}

fn chord_state(
    hotkey: &HotkeyConfig,
    key: KeyState,
    ctrl: KeyState,
    shift: KeyState,
    alt: KeyState,
) -> HotkeyState {
    let modifiers = [("Ctrl", ctrl), ("Shift", shift), ("Alt", alt)];
    let exact_down = modifiers
        .iter()
        .all(|(name, state)| hotkey.modifiers.iter().any(|item| item == name) == state.down);
    let exact_pressed = modifiers.iter().all(|(name, state)| {
        let required = hotkey.modifiers.iter().any(|item| item == name);
        if required {
            state.down || state.pressed
        } else {
            !state.down && !state.pressed
        }
    });
    HotkeyState {
        down: key.down && exact_down,
        pressed: key.pressed && exact_pressed,
    }
}
