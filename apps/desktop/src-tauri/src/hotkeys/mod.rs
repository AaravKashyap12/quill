use crate::model::HotkeyConfig;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, Default)]
pub struct HotkeyState {
    pub down: bool,
    pub pressed: bool,
}

pub fn poll_pair(dictation: &HotkeyConfig, scribe: &HotkeyConfig) -> (HotkeyState, HotkeyState) {
    #[cfg(windows)]
    {
        return windows::poll_pair(dictation, scribe);
    }
    #[cfg(target_os = "macos")]
    {
        return (
            HotkeyState {
                down: macos::is_pressed(dictation),
                pressed: false,
            },
            HotkeyState {
                down: macos::is_pressed(scribe),
                pressed: false,
            },
        );
    }
    #[allow(unreachable_code)]
    (HotkeyState::default(), HotkeyState::default())
}
