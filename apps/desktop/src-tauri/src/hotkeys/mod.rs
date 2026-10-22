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
    /// Authoritative sampled rising edges since the last drain, including zero.
    /// Above 255 edges the count remains nonzero and preserves odd/even parity.
    #[cfg(target_os = "macos")]
    pub sampled_presses: Option<u8>,
}

impl HotkeyState {
    pub fn press_count(self, was_down: bool) -> u8 {
        #[cfg(target_os = "macos")]
        if let Some(presses) = self.sampled_presses {
            return presses;
        }
        u8::from((self.down || self.pressed) && !was_down)
    }
}

#[cfg(target_os = "macos")]
pub use macos::HotkeyMonitor;

#[cfg(all(test, target_os = "macos"))]
pub(crate) use macos::TestSampler;
