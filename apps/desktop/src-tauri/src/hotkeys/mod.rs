#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::HotkeyMonitor;

#[derive(Debug, Clone, Copy, Default)]
pub struct HotkeyState {
    pub down: bool,
    pub pressed: bool,
}

#[cfg(target_os = "macos")]
pub struct HotkeyMonitor;

#[cfg(target_os = "macos")]
impl HotkeyMonitor {
    pub fn start() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn drain_pair(
        &mut self,
        dictation: &crate::model::HotkeyConfig,
        scribe: &crate::model::HotkeyConfig,
    ) -> Vec<(HotkeyState, HotkeyState)> {
        vec![(
            HotkeyState {
                down: macos::is_pressed(dictation),
                pressed: false,
            },
            HotkeyState {
                down: macos::is_pressed(scribe),
                pressed: false,
            },
        )]
    }
}
