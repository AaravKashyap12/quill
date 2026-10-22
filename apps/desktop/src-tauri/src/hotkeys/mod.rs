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
pub struct HotkeyMonitor {
    last_down: (bool, bool),
}

#[cfg(target_os = "macos")]
impl HotkeyMonitor {
    pub fn start() -> anyhow::Result<Self> {
        Ok(Self {
            last_down: (false, false),
        })
    }

    pub fn drain_pair(
        &mut self,
        dictation: &crate::model::HotkeyConfig,
        scribe: &crate::model::HotkeyConfig,
    ) -> Vec<(HotkeyState, HotkeyState)> {
        let dictation_sample = macos::sample(dictation);
        let scribe_sample = macos::sample(scribe);
        let down = (
            dictation_sample.session.down(),
            scribe_sample.session.down(),
        );
        if down != self.last_down {
            tracing::info!(
                dictation_session = ?dictation_sample.session,
                dictation_hid = ?dictation_sample.hid,
                scribe_session = ?scribe_sample.session,
                scribe_hid = ?scribe_sample.hid,
                "macOS shortcut state changed"
            );
            self.last_down = down;
        }
        vec![(
            HotkeyState {
                down: down.0,
                pressed: false,
            },
            HotkeyState {
                down: down.1,
                pressed: false,
            },
        )]
    }
}
