use super::HotkeyState;
use crate::model::{HotkeyBehavior, HotkeyConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type CGKeyCode = u16;
type CGEventSourceStateID = i32;
const COMBINED_SESSION_STATE: CGEventSourceStateID = 0;
const KEY_SPACE: CGKeyCode = 49;
const KEY_FN: CGKeyCode = 63;
const KEY_CONTROL: CGKeyCode = 59;
const KEY_SHIFT: CGKeyCode = 56;
const KEY_OPTION: CGKeyCode = 58;
const KEY_COMMAND: CGKeyCode = 55;
const KEY_RIGHT_CONTROL: CGKeyCode = 62;
const KEY_RIGHT_SHIFT: CGKeyCode = 60;
const KEY_RIGHT_OPTION: CGKeyCode = 61;
const KEY_RIGHT_COMMAND: CGKeyCode = 54;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventSourceKeyState(state_id: CGEventSourceStateID, key: CGKeyCode) -> bool;
    fn CGEventSourceFlagsState(state_id: CGEventSourceStateID) -> u64;
    fn CGPreflightListenEventAccess() -> bool;
}

fn key_code(key: &str) -> Option<CGKeyCode> {
    match key {
        "Space" => Some(KEY_SPACE),
        "Fn" => Some(KEY_FN),
        "AltRight" => Some(KEY_RIGHT_OPTION),
        "MetaRight" => Some(KEY_RIGHT_COMMAND),
        // Letter key codes are deliberately explicit because macOS key codes
        // represent physical positions rather than Unicode characters.
        "A" => Some(0),
        "S" => Some(1),
        "D" => Some(2),
        "F" => Some(3),
        "H" => Some(4),
        "G" => Some(5),
        "Z" => Some(6),
        "X" => Some(7),
        "C" => Some(8),
        "V" => Some(9),
        "B" => Some(11),
        "Q" => Some(12),
        "W" => Some(13),
        "E" => Some(14),
        "R" => Some(15),
        "Y" => Some(16),
        "T" => Some(17),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "6" => Some(22),
        "5" => Some(23),
        "9" => Some(25),
        "7" => Some(26),
        "8" => Some(28),
        "0" => Some(29),
        "O" => Some(31),
        "U" => Some(32),
        "I" => Some(34),
        "P" => Some(35),
        "L" => Some(37),
        "J" => Some(38),
        "K" => Some(40),
        "N" => Some(45),
        "M" => Some(46),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySample {
    key_down: bool,
    modifiers_down: bool,
}

impl KeySample {
    pub fn down(self) -> bool {
        self.key_down && self.modifiers_down
    }
}

fn sample_with(hotkey: &HotkeyConfig, down: impl Fn(CGKeyCode) -> bool) -> KeySample {
    let modifiers_down = hotkey
        .modifiers
        .iter()
        .all(|modifier| match modifier.as_str() {
            "Ctrl" => down(KEY_CONTROL) || down(KEY_RIGHT_CONTROL),
            "Shift" => down(KEY_SHIFT) || down(KEY_RIGHT_SHIFT),
            "Alt" => down(KEY_OPTION) || down(KEY_RIGHT_OPTION),
            "Meta" => down(KEY_COMMAND) || down(KEY_RIGHT_COMMAND),
            _ => false,
        });
    KeySample {
        modifiers_down,
        key_down: trigger_down(hotkey, down),
    }
}

fn modifier_only(config: &HotkeyConfig) -> bool {
    matches!(config.key.as_str(), "Fn" | "Alt" | "AltRight" | "MetaRight")
}

fn trigger_down(config: &HotkeyConfig, down: impl Fn(CGKeyCode) -> bool) -> bool {
    if config.key == "Alt" {
        down(KEY_OPTION) || down(KEY_RIGHT_OPTION)
    } else {
        key_code(&config.key).is_some_and(down)
    }
}

fn allowed_key(config: &HotkeyConfig, key: CGKeyCode) -> bool {
    if key_code(&config.key) == Some(key)
        || (config.key == "Alt" && matches!(key, KEY_OPTION | KEY_RIGHT_OPTION))
    {
        return true;
    }
    config
        .modifiers
        .iter()
        .any(|modifier| match modifier.as_str() {
            "Ctrl" => matches!(key, KEY_CONTROL | KEY_RIGHT_CONTROL),
            "Shift" => matches!(key, KEY_SHIFT | KEY_RIGHT_SHIFT),
            "Alt" => matches!(key, KEY_OPTION | KEY_RIGHT_OPTION),
            "Meta" => matches!(key, KEY_COMMAND | KEY_RIGHT_COMMAND),
            _ => false,
        })
}

// Device-side masks from Apple's IOLLEvent.h. CG key-state booleans can alias
// a physical right modifier to its left key code, so they cannot identify sides.
fn modifier_flag(key: CGKeyCode) -> Option<u64> {
    match key {
        KEY_CONTROL => Some(0x1),
        KEY_RIGHT_CONTROL => Some(0x2000),
        KEY_SHIFT => Some(0x2),
        KEY_RIGHT_SHIFT => Some(0x4),
        KEY_OPTION => Some(0x20),
        KEY_RIGHT_OPTION => Some(0x40),
        KEY_COMMAND => Some(0x8),
        KEY_RIGHT_COMMAND => Some(0x10),
        _ => None,
    }
}

// Normalized physical key-down booleans; no text or event history is retained.
struct KeySnapshot([bool; 128], bool);

impl KeySnapshot {
    fn read_with(
        configs: &(HotkeyConfig, HotkeyConfig),
        flags: u64,
        checking_release: bool,
        mut read: impl FnMut(CGKeyCode) -> bool,
    ) -> Self {
        let mut keys = std::array::from_fn(|key| {
            modifier_flag(key as CGKeyCode).is_some_and(|mask| flags & mask != 0)
        });
        let mut read = |key| match modifier_flag(key) {
            Some(mask) => flags & mask != 0,
            None => read(key),
        };
        for config in [&configs.0, &configs.1] {
            if let Some(key) = key_code(&config.key) {
                // Fn must use physical key 63: secondaryFn flags also mark navigation keys.
                keys[key as usize] = read(key);
            }
        }
        let checking_exclusivity = checking_release
            || [&configs.0, &configs.1].iter().any(|config| {
                modifier_only(config) && trigger_down(config, |key| keys[key as usize])
            });
        if checking_exclusivity {
            for (key, state) in keys.iter_mut().enumerate() {
                *state = read(key as CGKeyCode);
            }
        }
        Self(keys, checking_exclusivity)
    }

    fn conflicts(&self, config: &HotkeyConfig) -> Option<usize> {
        self.1.then(|| {
            self.0
                .iter()
                .enumerate()
                .filter(|(key, down)| **down && !allowed_key(config, *key as CGKeyCode))
                .count()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Transition {
    triggers: (KeySample, KeySample),
    active: (bool, bool),
    blocked: (bool, bool),
}

impl Transition {
    fn log(self, configs: &(HotkeyConfig, HotkeyConfig), keys: &KeySnapshot) {
        tracing::info!(
            dictation_trigger = self.triggers.0.key_down,
            dictation_modifiers = self.triggers.0.modifiers_down,
            scribe_trigger = self.triggers.1.key_down,
            scribe_modifiers = self.triggers.1.modifiers_down,
            dictation_active = self.active.0, scribe_active = self.active.1,
            dictation_blocked = self.blocked.0, scribe_blocked = self.blocked.1,
            dictation_conflicts = ?keys.conflicts(&configs.0),
            scribe_conflicts = ?keys.conflicts(&configs.1),
            full_scan = keys.1,
            normalized_ctrl = ?(keys.0[59], keys.0[62]),
            normalized_shift = ?(keys.0[56], keys.0[60]),
            normalized_alt = ?(keys.0[58], keys.0[61]),
            normalized_meta = ?(keys.0[55], keys.0[54]),
            // Nonmodifier raw key states are only read during the conflict scan.
            raw_caps_lock = ?keys.1.then_some(keys.0[57]),
            raw_function = ?keys.1.then_some(keys.0[63]),
            raw_num_lock = ?keys.1.then_some(keys.0[71]),
            "hotkey sampler transition"
        );
    }
}

#[derive(Default)]
struct PendingPresses(u8);

impl PendingPresses {
    fn record(&mut self, rising: bool) {
        if rising {
            // Saturating would lose parity; wrapping would lose 'any press'.
            self.0 = if self.0 == u8::MAX {
                u8::MAX - 1
            } else {
                self.0 + 1
            };
        }
    }

    fn drain(&mut self, down: bool) -> HotkeyState {
        let presses = std::mem::take(&mut self.0);
        HotkeyState {
            down,
            pressed: presses > 0,
            sampled_presses: Some(presses),
        }
    }
}

#[derive(Default)]
struct KeyTracker {
    started: Option<Duration>,
    blocked: bool,
    active: bool,
    tap_armed: bool,
}

#[derive(Default)]
struct Sample {
    down: bool,
    cancelled: bool,
    pressed: bool,
}

impl KeyTracker {
    fn update(&mut self, config: &HotkeyConfig, keys: &KeySnapshot, now: Duration) -> Sample {
        if !modifier_only(config) {
            self.active = sample_with(config, |key| keys.0[key as usize]).down();
            return Sample {
                down: self.active,
                cancelled: false,
                pressed: false,
            };
        }
        let sample = sample_with(config, |key| keys.0[key as usize]);
        let other = keys.conflicts(config).is_some_and(|count| count > 0);
        if config.behavior == HotkeyBehavior::TapToLock {
            // Commit on trigger release, after the whole chord can be checked.
            // This prevents Fn+Ctrl or Fn+another key from first toggling Fn.
            let pressed = !sample.key_down && self.tap_armed && !self.blocked && !other;
            if !sample.key_down {
                *self = Self::default();
            } else if other {
                self.blocked = true;
                self.tap_armed = false;
            } else if !self.blocked && sample.modifiers_down {
                self.tap_armed = true;
            }
            return Sample {
                pressed,
                ..Default::default()
            };
        }
        let cancelled = self.active && other;
        if !sample.key_down {
            *self = Self::default();
            return Sample {
                down: false,
                cancelled,
                pressed: false,
            };
        }
        if other {
            self.blocked = true;
        }
        // A competing chord latches until the trigger is released. In particular,
        // releasing Ctrl from Fn+Ctrl must never fall through to Fn Dictation.
        if self.blocked || !sample.modifiers_down {
            self.started = None;
            self.active = false;
        } else {
            self.active =
                now.saturating_sub(*self.started.get_or_insert(now)) >= Duration::from_millis(250);
        }
        Sample {
            down: self.active,
            cancelled,
            pressed: false,
        }
    }
}

// Bound memory even if the session consumer stalls indefinitely. Overflow cancels
// both recordings instead of guessing the order of start/stop requests.
const MAX_PENDING_CHANGES: usize = 1024;

#[derive(Default)]
struct PollingState {
    configs: Option<(HotkeyConfig, HotkeyConfig)>,
    trackers: (KeyTracker, KeyTracker),
    down: (bool, bool),
    cancelled: (bool, bool),
    presses: (PendingPresses, PendingPresses),
    last_transition: Option<Transition>,
    ordered: Vec<(HotkeyState, HotkeyState)>,
    overflowed: bool,
}

impl PollingState {
    fn read_keys(&self, flags: u64, read: impl FnMut(CGKeyCode) -> bool) -> Option<KeySnapshot> {
        self.configs.as_ref().map(|configs| {
            KeySnapshot::read_with(
                configs,
                flags,
                self.trackers.0.tap_armed || self.trackers.1.tap_armed,
                read,
            )
        })
    }

    fn configure(&mut self, dictation: &HotkeyConfig, scribe: &HotkeyConfig) {
        if self
            .configs
            .as_ref()
            .is_some_and(|pair| pair.0 == *dictation && pair.1 == *scribe)
        {
            return;
        }
        self.cancelled.0 |= self.down.0
            || self.configs.as_ref().is_some_and(|pair| {
                pair.0.behavior == HotkeyBehavior::TapToLock && pair.0 != *dictation
            });
        self.cancelled.1 |= self.down.1
            || self.configs.as_ref().is_some_and(|pair| {
                pair.1.behavior == HotkeyBehavior::TapToLock && pair.1 != *scribe
            });
        self.down = (false, false);
        self.presses = Default::default();
        self.ordered.clear();
        self.overflowed = false;
        self.trackers = Default::default();
        self.last_transition = None;
        tracing::info!(
            dictation_key = %dictation.key, dictation_behavior = ?dictation.behavior,
            scribe_key = %scribe.key, scribe_behavior = ?scribe.behavior,
            "hotkey sampler configuration reset; modifier-only threshold 250ms, poll 8ms"
        );
        self.configs = Some((dictation.clone(), scribe.clone()));
    }

    fn update(&mut self, keys: &KeySnapshot, now: Duration) -> Option<Transition> {
        if self.overflowed {
            if self.configs.as_ref().is_some_and(|pair| {
                [&pair.0, &pair.1]
                    .iter()
                    .any(|config| trigger_down(config, |key| keys.0[key as usize]))
            }) {
                return None;
            }
            self.overflowed = false;
            self.trackers = Default::default();
        }
        if let Some((dictation, scribe)) = &self.configs {
            let d = self.trackers.0.update(dictation, keys, now);
            let s = self.trackers.1.update(scribe, keys, now);
            let presses = (
                d.pressed || d.down && !self.down.0,
                s.pressed || s.down && !self.down.1,
            );
            if (dictation.behavior == HotkeyBehavior::TapToLock
                || scribe.behavior == HotkeyBehavior::TapToLock)
                && ((d.down, s.down) != self.down || presses.0 || presses.1)
            {
                if self.ordered.len() == MAX_PENDING_CHANGES {
                    self.ordered.clear();
                    self.presses = Default::default();
                    self.down = (false, false);
                    self.cancelled = (true, true);
                    self.overflowed = true;
                    self.trackers = (
                        KeyTracker {
                            blocked: true,
                            ..Default::default()
                        },
                        KeyTracker {
                            blocked: true,
                            ..Default::default()
                        },
                    );
                    tracing::warn!("shortcut event buffer full; cancelling recording");
                    return None;
                }
                let event = |down, pressed| HotkeyState {
                    down,
                    pressed,
                    sampled_presses: Some(u8::from(pressed)),
                };
                self.ordered
                    .push((event(d.down, presses.0), event(s.down, presses.1)));
            }
            self.presses.0.record(presses.0);
            self.presses.1.record(presses.1);
            self.down = (d.down, s.down);
            self.cancelled.0 |= d.cancelled;
            self.cancelled.1 |= s.cancelled;
            let transition = Transition {
                triggers: (
                    sample_with(dictation, |key| keys.0[key as usize]),
                    sample_with(scribe, |key| keys.0[key as usize]),
                ),
                active: self.down,
                blocked: (self.trackers.0.blocked, self.trackers.1.blocked),
            };
            if self.last_transition != Some(transition) {
                self.last_transition = Some(transition);
                return Some(transition);
            }
        }
        None
    }

    fn drain_ordered(&mut self) -> (Vec<(HotkeyState, HotkeyState)>, (bool, bool)) {
        if !self.configs.as_ref().is_some_and(|pair| {
            pair.0.behavior == HotkeyBehavior::TapToLock
                || pair.1.behavior == HotkeyBehavior::TapToLock
        }) {
            let (pair, cancelled) = self.drain();
            return (vec![pair], cancelled);
        }
        let mut events = std::mem::take(&mut self.ordered);
        self.presses = Default::default();
        // Synchronize the latest held state without introducing another edge.
        let state = |down| HotkeyState {
            down,
            sampled_presses: Some(0),
            ..Default::default()
        };
        events.push((state(self.down.0), state(self.down.1)));
        (events, std::mem::take(&mut self.cancelled))
    }

    fn drain(&mut self) -> ((HotkeyState, HotkeyState), (bool, bool)) {
        let cancelled = std::mem::take(&mut self.cancelled);
        (
            (
                self.presses.0.drain(self.down.0),
                self.presses.1.drain(self.down.1),
            ),
            cancelled,
        )
    }
}

pub struct HotkeyMonitor {
    state: Arc<Mutex<PollingState>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    cancelled: (bool, bool),
}

impl HotkeyMonitor {
    pub fn start() -> std::io::Result<Self> {
        let state = Arc::new(Mutex::new(PollingState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&state);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("quill-hotkeys".into())
            .spawn(move || {
                let start = Instant::now();
                tracing::info!(
                    input_monitoring = unsafe { CGPreflightListenEventAccess() },
                    "macOS shortcut input permission status"
                );
                while !worker_stop.load(Ordering::Relaxed) {
                    {
                        let mut state = worker_state.lock().unwrap();
                        if state.configs.is_some() {
                            let flags = unsafe { CGEventSourceFlagsState(COMBINED_SESSION_STATE) };
                            let snapshot = state
                                .read_keys(flags, |key| unsafe {
                                    CGEventSourceKeyState(COMBINED_SESSION_STATE, key)
                                })
                                .unwrap();
                            if let Some(transition) = state.update(&snapshot, start.elapsed()) {
                                transition.log(state.configs.as_ref().unwrap(), &snapshot);
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(8));
                }
            })?;
        Ok(Self {
            state,
            stop,
            worker: Some(worker),
            cancelled: (false, false),
        })
    }

    pub fn drain_pair(
        &mut self,
        dictation: &HotkeyConfig,
        scribe: &HotkeyConfig,
    ) -> Vec<(HotkeyState, HotkeyState)> {
        let mut state = self.state.lock().unwrap();
        state.configure(dictation, scribe);
        let (events, cancelled) = state.drain_ordered();
        self.cancelled.0 |= cancelled.0;
        self.cancelled.1 |= cancelled.1;
        events
    }

    /// Consume immediately after drain_pair, before finishing a released session.
    pub fn take_cancelled(&mut self) -> (bool, bool) {
        std::mem::take(&mut self.cancelled)
    }
}

impl Drop for HotkeyMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Injected samples exercise the actual producer/consumer contract without OS input.
#[cfg(test)]
pub(crate) struct TestSampler(PollingState);

#[cfg(test)]
impl TestSampler {
    pub(crate) fn new(dictation: &HotkeyConfig, scribe: &HotkeyConfig) -> Self {
        let mut state = PollingState::default();
        state.configure(dictation, scribe);
        Self(state)
    }

    pub(crate) fn update(&mut self, down: &[u16], millis: u64) {
        let mut keys = [false; 128];
        for &key in down {
            keys[key as usize] = true;
        }
        self.0
            .update(&KeySnapshot(keys, true), Duration::from_millis(millis));
    }

    pub(crate) fn drain_pairs(&mut self) -> Vec<(HotkeyState, HotkeyState)> {
        let (events, cancelled) = self.0.drain_ordered();
        self.0.cancelled = cancelled;
        events
    }

    pub(crate) fn configure(&mut self, dictation: &HotkeyConfig, scribe: &HotkeyConfig) {
        self.0.configure(dictation, scribe);
    }

    pub(crate) fn cancelled(&mut self) -> (bool, bool) {
        std::mem::take(&mut self.0.cancelled)
    }

    pub(crate) fn sample_physical(&mut self, flags: u64, down: &[u16], millis: u64) {
        let keys = self.0.read_keys(flags, |key| down.contains(&key)).unwrap();
        self.0.update(&keys, Duration::from_millis(millis));
    }

    pub(crate) fn drain(&mut self) -> (HotkeyState, HotkeyState) {
        self.0.drain().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HotkeyBehavior;

    #[test]
    fn either_side_of_each_modifier_can_hold_a_shortcut() {
        for (modifier, left, right) in [
            ("Ctrl", 59, 62),
            ("Shift", 56, 60),
            ("Alt", 58, 61),
            ("Meta", 55, 54),
        ] {
            let config = HotkeyConfig {
                modifiers: vec![modifier.into()],
                key: "D".into(),
                behavior: HotkeyBehavior::Hold,
            };
            for modifier_key in [left, right] {
                assert!(
                    sample_with(&config, |key| key == 2 || key == modifier_key).down(),
                    "{modifier} key {modifier_key}"
                );
            }
            assert!(!sample_with(&config, |key| key == 2).down());
            assert!(!sample_with(&config, |key| key == left).down());
        }
    }

    fn standalone(key: &str) -> HotkeyConfig {
        HotkeyConfig {
            modifiers: vec![],
            key: key.into(),
            behavior: HotkeyBehavior::Hold,
        }
    }

    fn keys(down: &[u16]) -> KeySnapshot {
        let mut keys = [false; 128];
        for &key in down {
            keys[key as usize] = true;
        }
        KeySnapshot(keys, true)
    }

    fn modifier_chord(key: &str, modifiers: &[&str]) -> HotkeyConfig {
        HotkeyConfig {
            modifiers: modifiers.iter().map(|name| (*name).into()).collect(),
            ..standalone(key)
        }
    }

    #[test]
    fn navigation_secondary_fn_flag_is_not_physical_fn() {
        let configs = (standalone("Fn"), modifier_chord("Fn", &["Ctrl"]));
        for navigation in [115, 116, 119, 121, 123, 124, 125, 126] {
            let mut state = PollingState::default();
            state.configure(&configs.0, &configs.1);
            let snapshot =
                KeySnapshot::read_with(&configs, 1 << 23, false, |key| key == navigation);
            state.update(&snapshot, Duration::ZERO);
            state.update(&snapshot, Duration::from_secs(1));
            let (pair, cancelled) = state.drain();
            assert!(!pair.0.down && !pair.1.down);
            assert_eq!(pair.0.press_count(false), 0);
            assert_eq!(cancelled, (false, false));
        }
        // Conversely, physical Fn works without trusting that aggregate flag.
        let snapshot = KeySnapshot::read_with(&configs, 0, false, |key| key == KEY_FN);
        let mut tracker = KeyTracker::default();
        tracker.update(&configs.0, &snapshot, Duration::ZERO);
        assert!(
            tracker
                .update(&configs.0, &snapshot, Duration::from_millis(250))
                .down
        );
    }

    #[test]
    fn fn_navigation_chord_never_becomes_dictation_when_navigation_is_released() {
        let configs = (standalone("Fn"), modifier_chord("Fn", &["Ctrl"]));
        for navigation in [115, 116, 119, 121, 123, 124, 125, 126] {
            let mut state = PollingState::default();
            state.configure(&configs.0, &configs.1);
            let chord = KeySnapshot::read_with(&configs, 1 << 23, false, |key| {
                key == KEY_FN || key == navigation
            });
            state.update(&chord, Duration::ZERO);
            state.update(&chord, Duration::from_millis(250));
            let fn_only = KeySnapshot::read_with(&configs, 1 << 23, false, |key| key == KEY_FN);
            state.update(&fn_only, Duration::from_secs(1));
            let (pair, cancelled) = state.drain();
            assert!(!pair.0.down && !pair.1.down);
            assert_eq!(pair.0.press_count(false), 0);
            assert_eq!(cancelled, (false, false));
        }
    }

    #[test]
    fn no_fn_preset_uses_normalized_physical_flags_and_exclusive_holds() {
        let configs = (
            modifier_chord("Alt", &["Ctrl"]),
            modifier_chord("Alt", &["Meta", "Ctrl"]),
        );
        for flags in [0x1 | 0x20, 0x2000 | 0x40] {
            let mut state = PollingState::default();
            state.configure(&configs.0, &configs.1);
            let clean = KeySnapshot::read_with(&configs, flags, false, |_| false);
            state.update(&clean, Duration::ZERO);
            state.update(&clean, Duration::from_millis(249));
            assert!(!state.drain().0 .0.down);
            state.update(&clean, Duration::from_millis(250));
            assert!(state.drain().0 .0.down);
            let typing = KeySnapshot::read_with(&configs, flags, false, |key| key == 2);
            state.update(&typing, Duration::from_millis(260));
            state.update(&clean, Duration::from_secs(1));
            let (pair, cancelled) = state.drain();
            assert!(!pair.0.down && !pair.1.down);
            assert_eq!(cancelled, (true, false));
        }
    }

    #[test]
    fn modifier_presets_prefer_scribe_in_either_press_order_and_never_fall_through() {
        for (dictation, scribe, trigger, ctrl, command) in [
            (
                standalone("Fn"),
                modifier_chord("Fn", &["Ctrl"]),
                63,
                59,
                None,
            ),
            (
                standalone("Fn"),
                modifier_chord("Fn", &["Ctrl"]),
                63,
                62,
                None,
            ),
            (
                modifier_chord("Alt", &["Ctrl"]),
                modifier_chord("Alt", &["Meta", "Ctrl"]),
                58,
                59,
                Some(55),
            ),
            (
                modifier_chord("Alt", &["Ctrl"]),
                modifier_chord("Alt", &["Meta", "Ctrl"]),
                61,
                62,
                Some(54),
            ),
        ] {
            let mut full = vec![trigger, ctrl];
            if let Some(command) = command {
                full.push(command);
            }
            for trigger_first in [false, true] {
                let mut state = PollingState::default();
                state.configure(&dictation, &scribe);
                let first = if trigger_first {
                    vec![trigger]
                } else {
                    full.iter().copied().filter(|key| *key != trigger).collect()
                };
                state.update(&keys(&first), Duration::ZERO);
                state.update(&keys(&full), Duration::from_millis(100));
                state.update(&keys(&full), Duration::from_millis(349));
                assert_eq!(state.down, (false, false));
                state.update(&keys(&full), Duration::from_millis(350));
                let (pair, cancelled) = state.drain();
                assert_eq!(pair.0.press_count(false), 0, "Dictation must not start");
                assert!(pair.1.down);
                assert_eq!(pair.1.press_count(false), 1);
                assert_eq!(cancelled, (false, false));
                // Release the distinguishing modifier while still holding the trigger.
                let remaining = if command.is_some() {
                    vec![trigger, ctrl]
                } else {
                    vec![trigger]
                };
                state.update(&keys(&remaining), Duration::from_millis(400));
                state.update(&keys(&remaining), Duration::from_millis(900));
                assert_eq!(state.down, (false, false));
                assert_eq!(state.drain().0 .0.press_count(false), 0);
                state.update(&keys(&[]), Duration::from_millis(910));
                state.update(&keys(&remaining), Duration::from_millis(920));
                state.update(&keys(&remaining), Duration::from_millis(1170));
                assert!(
                    state.drain().0 .0.down,
                    "fresh Dictation hold after release"
                );
            }
        }
    }

    #[test]
    fn fn_scribe_other_keys_cancel_and_remain_blocked_until_fn_release() {
        let configs = (standalone("Fn"), modifier_chord("Fn", &["Ctrl"]));
        for other in (0..128).filter(|key| ![63, 59, 62].contains(key)) {
            let mut state = PollingState::default();
            state.configure(&configs.0, &configs.1);
            state.update(&keys(&[63, 59]), Duration::ZERO);
            state.update(&keys(&[63, 59]), Duration::from_millis(250));
            assert!(state.drain().0 .1.down);
            state.update(&keys(&[63, 59, other]), Duration::from_millis(260));
            state.update(&keys(&[63, 59]), Duration::from_millis(1000));
            let (pair, cancelled) = state.drain();
            assert!(!pair.0.down && !pair.1.down);
            assert_eq!(cancelled, (false, true), "other key {other}");
            assert_eq!(state.drain().1, (false, false));
            state.update(&keys(&[]), Duration::from_millis(1010));
            state.update(&keys(&[63, 59]), Duration::from_millis(1020));
            state.update(&keys(&[63, 59]), Duration::from_millis(1269));
            assert!(!state.drain().0 .1.down);
            state.update(&keys(&[63, 59]), Duration::from_millis(1270));
            assert!(state.drain().0 .1.down);
        }
    }

    #[test]
    fn late_control_cancels_active_dictation_before_scribe_and_survives_drain() {
        let mut monitor = HotkeyMonitor {
            state: Arc::new(Mutex::new(PollingState::default())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            cancelled: (false, false),
        };
        let configs = (standalone("Fn"), modifier_chord("Fn", &["Ctrl"]));
        monitor.drain_pair(&configs.0, &configs.1);
        {
            let mut state = monitor.state.lock().unwrap();
            state.update(&keys(&[63]), Duration::ZERO);
            state.update(&keys(&[63]), Duration::from_millis(250));
        }
        assert!(monitor.drain_pair(&configs.0, &configs.1)[0].0.down);
        assert_eq!(monitor.take_cancelled(), (false, false));
        {
            let mut state = monitor.state.lock().unwrap();
            state.update(&keys(&[63, 59]), Duration::from_millis(300));
            state.update(&keys(&[63, 59]), Duration::from_millis(550));
        }
        let pair = monitor.drain_pair(&configs.0, &configs.1)[0];
        assert!(!pair.0.down && pair.1.down);
        // The session consumes this before processing release/transcription/upload.
        assert_eq!(monitor.take_cancelled(), (true, false));
        assert_eq!(monitor.take_cancelled(), (false, false));
    }

    #[test]
    fn modifier_only_tap_bindings_commit_only_on_exclusive_release() {
        for (mut config, down) in [
            (standalone("Fn"), vec![63]),
            (modifier_chord("Fn", &["Ctrl"]), vec![63, 59]),
            (modifier_chord("Alt", &["Ctrl"]), vec![58, 59]),
        ] {
            config.behavior = HotkeyBehavior::TapToLock;
            let mut sampler = TestSampler::new(&config, &standalone("MetaRight"));
            sampler.update(&down, 0);
            assert_eq!(sampler.drain().0.sampled_presses, Some(0));
            sampler.update(&[], 32);
            assert_eq!(sampler.drain().0.sampled_presses, Some(1));
            assert_eq!(sampler.drain().0.sampled_presses, Some(0));
        }
    }

    #[test]
    fn fn_tap_chords_choose_scribe_in_either_press_and_release_order() {
        let mut dictation = standalone("Fn");
        let mut scribe = modifier_chord("Fn", &["Ctrl"]);
        dictation.behavior = HotkeyBehavior::TapToLock;
        scribe.behavior = HotkeyBehavior::TapToLock;
        for first in [vec![63], vec![59]] {
            for remaining in [vec![63], vec![59]] {
                let mut sampler = TestSampler::new(&dictation, &scribe);
                sampler.update(&first, 0);
                sampler.update(&[59, 63], 16);
                sampler.update(&remaining, 32);
                sampler.update(&[], 48);
                let (d, s) = sampler.drain();
                assert_eq!((d.sampled_presses, s.sampled_presses), (Some(0), Some(1)));
            }
        }
    }

    #[test]
    fn fn_tap_conflicts_never_become_a_start_or_stop_press() {
        let mut config = standalone("Fn");
        config.behavior = HotkeyBehavior::TapToLock;
        for other in (0..128).filter(|key| *key != 63) {
            let mut sampler = TestSampler::new(&config, &standalone("MetaRight"));
            sampler.update(&[63], 0);
            sampler.update(&[63, other], 16);
            sampler.update(&[63], 32);
            sampler.update(&[], 48);
            assert_eq!(sampler.drain().0.sampled_presses, Some(0), "key {other}");
            sampler.update(&[63], 64);
            sampler.update(&[], 80);
            assert_eq!(sampler.drain().0.sampled_presses, Some(1));
        }
    }

    #[test]
    fn tap_release_samples_conflicts_even_after_the_trigger_is_up() {
        let mut config = standalone("Fn");
        config.behavior = HotkeyBehavior::TapToLock;
        let mut state = PollingState::default();
        state.configure(&config, &standalone("MetaRight"));
        let down = state.read_keys(0, |key| key == 63).unwrap();
        state.update(&down, Duration::ZERO);
        let release = state.read_keys(0, |key| key == 8).unwrap();
        state.update(&release, Duration::from_millis(16));
        assert_eq!(state.drain().0 .0.sampled_presses, Some(0));
    }

    #[test]
    fn tap_configuration_change_cancels_the_consumers_locked_session() {
        let mut config = standalone("Fn");
        config.behavior = HotkeyBehavior::TapToLock;
        let scribe = standalone("MetaRight");
        let mut state = PollingState::default();
        state.configure(&config, &scribe);
        state.update(&keys(&[63]), Duration::ZERO);
        state.update(&keys(&[]), Duration::from_millis(32));
        let _ = state.drain();
        config.behavior = HotkeyBehavior::Hold;
        state.configure(&config, &scribe);
        assert_eq!(state.drain().1, (true, false));
    }

    #[test]
    fn standalone_requires_250ms_and_only_the_right_side() {
        for (name, right, left) in [("AltRight", 61, 58), ("MetaRight", 54, 55)] {
            let mut tracker = KeyTracker::default();
            let config = standalone(name);
            assert!(!tracker.update(&config, &keys(&[left]), Duration::ZERO).down);
            assert!(
                !tracker
                    .update(&config, &keys(&[right]), Duration::from_millis(10))
                    .down
            );
            assert!(
                !tracker
                    .update(&config, &keys(&[right]), Duration::from_millis(259))
                    .down
            );
            assert!(
                tracker
                    .update(&config, &keys(&[right]), Duration::from_millis(260))
                    .down
            );
            assert!(
                !tracker
                    .update(&config, &keys(&[]), Duration::from_millis(270))
                    .down
            );
        }
    }

    #[test]
    fn every_other_key_blocks_activation_until_modifier_release() {
        for other in (0..128).filter(|key| *key != 54) {
            let mut tracker = KeyTracker::default();
            let config = standalone("MetaRight");
            tracker.update(&config, &keys(&[54]), Duration::ZERO);
            assert!(
                !tracker
                    .update(&config, &keys(&[54, other]), Duration::from_millis(100))
                    .down
            );
            assert!(
                !tracker
                    .update(&config, &keys(&[54]), Duration::from_millis(1000))
                    .down
            );
            tracker.update(&config, &keys(&[]), Duration::from_millis(1010));
            tracker.update(&config, &keys(&[54]), Duration::from_millis(1020));
            assert!(
                tracker
                    .update(&config, &keys(&[54]), Duration::from_millis(1270))
                    .down
            );
        }
    }

    #[test]
    fn contamination_after_activation_cancels_instead_of_finishing() {
        let mut tracker = KeyTracker::default();
        let config = standalone("MetaRight");
        tracker.update(&config, &keys(&[54]), Duration::ZERO);
        assert!(
            tracker
                .update(&config, &keys(&[54]), Duration::from_millis(250))
                .down
        );
        let sample = tracker.update(&config, &keys(&[54, 8]), Duration::from_millis(260));
        assert!(sample.cancelled);
        assert!(!sample.down);
        assert!(
            !tracker
                .update(&config, &keys(&[54]), Duration::from_millis(500))
                .down
        );
    }

    #[test]
    fn cancellation_survives_a_blocked_consumer_and_is_consumed_once() {
        let mut state = PollingState::default();
        state.configure(&standalone("AltRight"), &standalone("MetaRight"));
        state.update(&keys(&[54]), Duration::ZERO);
        state.update(&keys(&[54]), Duration::from_millis(250));
        assert!(state.drain().0 .1.down);
        // Audio initialization/session work is blocked while the sampler sees Cmd+C.
        state.update(&keys(&[54, 8]), Duration::from_millis(400));
        state.update(&keys(&[]), Duration::from_millis(420));
        state.update(&keys(&[]), Duration::from_millis(2409));
        let (down, cancelled) = state.drain();
        assert!(!down.1.down);
        assert_eq!(cancelled, (false, true));
        assert_eq!(state.drain().1, (false, false));
    }

    #[test]
    fn idle_default_sampling_needs_no_key_state_reads() {
        let configs = (standalone("AltRight"), standalone("MetaRight"));
        let mut calls = vec![];
        let _ = KeySnapshot::read_with(&configs, 0, false, |key| {
            calls.push(key);
            false
        });
        assert!(calls.is_empty());
        calls.clear();
        let snapshot = KeySnapshot::read_with(&configs, 0x10, false, |key| {
            calls.push(key);
            key == 54 || key == 8
        });
        assert!(snapshot.0[8]);
        assert!(calls.contains(&127));
        assert_eq!(calls.len(), 120);
        assert!(calls.iter().all(|key| modifier_flag(*key).is_none()));
    }

    #[test]
    fn physical_right_flags_override_aliased_left_key_state() {
        let configs = (standalone("AltRight"), standalone("MetaRight"));
        for (name, right, left, flag) in [("AltRight", 61, 58, 0x40), ("MetaRight", 54, 55, 0x10)] {
            // Physical evidence: right-side flags, but CGKeyState reports only left.
            let snapshot = KeySnapshot::read_with(&configs, flag, false, |key| key == left);
            let mut tracker = KeyTracker::default();
            let config = standalone(name);
            assert!(!tracker.update(&config, &snapshot, Duration::ZERO).down);
            assert!(
                !tracker
                    .update(&config, &snapshot, Duration::from_millis(249))
                    .down
            );
            assert!(
                tracker
                    .update(&config, &snapshot, Duration::from_millis(250))
                    .down
            );
            assert!(snapshot.0[right]);
            assert!(
                !snapshot.0[left as usize],
                "aggregate left state must not contaminate exclusivity"
            );
            assert_eq!(snapshot.conflicts(&config), Some(0));
        }
    }

    #[test]
    fn physical_left_only_and_both_sides_do_not_activate() {
        let configs = (standalone("AltRight"), standalone("MetaRight"));
        for (name, left_flag, right_flag) in [("AltRight", 0x20, 0x40), ("MetaRight", 0x8, 0x10)] {
            for flags in [left_flag, left_flag | right_flag] {
                // Even misleading raw right booleans must not override side flags.
                let snapshot =
                    KeySnapshot::read_with(&configs, flags, false, |key| key == 61 || key == 54);
                let mut tracker = KeyTracker::default();
                let config = standalone(name);
                tracker.update(&config, &snapshot, Duration::ZERO);
                assert!(
                    !tracker
                        .update(&config, &snapshot, Duration::from_secs(2))
                        .down
                );
                assert_eq!(
                    snapshot.conflicts(&config),
                    (flags == (left_flag | right_flag)).then_some(1)
                );
            }
        }
    }

    #[test]
    fn physical_modifier_snapshot_preserves_custom_either_side_chords() {
        for (modifier, left, right, left_flag, right_flag) in [
            ("Ctrl", 59, 62, 0x1, 0x2000),
            ("Shift", 56, 60, 0x2, 0x4),
            ("Alt", 58, 61, 0x20, 0x40),
            ("Meta", 55, 54, 0x8, 0x10),
        ] {
            let config = HotkeyConfig {
                modifiers: vec![modifier.into()],
                key: "D".into(),
                behavior: HotkeyBehavior::Hold,
            };
            let configs = (config.clone(), standalone("MetaRight"));
            for flags in [left_flag, right_flag, left_flag | right_flag] {
                let snapshot = KeySnapshot::read_with(&configs, flags, false, |key| key == 2);
                assert!(
                    sample_with(&config, |key| snapshot.0[key as usize]).down(),
                    "{modifier} flags {flags}"
                );
                assert_eq!(snapshot.0[left], flags & left_flag != 0);
                assert_eq!(snapshot.0[right], flags & right_flag != 0);
            }
            let snapshot = KeySnapshot::read_with(&configs, 0, false, |key| {
                key == 2 || key == left as u16 || key == right as u16
            });
            assert!(!sample_with(&config, |key| snapshot.0[key as usize]).down());
        }
    }

    #[test]
    fn normalized_scan_keeps_every_other_key_cancellation_and_latch() {
        let configs = (standalone("AltRight"), standalone("MetaRight"));
        for other in (0..128).filter(|key| *key != KEY_RIGHT_COMMAND) {
            let flags = 0x10 | modifier_flag(other).unwrap_or(0);
            let mut state = PollingState::default();
            state.configure(&configs.0, &configs.1);
            let clean = KeySnapshot::read_with(&configs, 0x10, false, |key| key == 55);
            state.update(&clean, Duration::ZERO);
            state.update(&clean, Duration::from_millis(250));
            assert!(state.drain().0 .1.down);
            let contaminated =
                KeySnapshot::read_with(&configs, flags, false, |key| key == 55 || key == other);
            state.update(&contaminated, Duration::from_millis(258));
            state.update(&clean, Duration::from_secs(2));
            let (pair, cancelled) = state.drain();
            assert!(!pair.1.down);
            assert_eq!(cancelled, (false, true));
            assert_eq!(state.drain().1, (false, false));
            state.update(
                &KeySnapshot::read_with(&configs, 0, false, |_| false),
                Duration::from_millis(2008),
            );
            state.update(&clean, Duration::from_millis(2016));
            state.update(&clean, Duration::from_millis(2266));
            assert!(state.drain().0 .1.down);
        }
    }

    #[test]
    fn completed_custom_stop_tap_survives_a_delayed_drain() {
        let config = HotkeyConfig {
            modifiers: vec!["Ctrl".into()],
            key: "Space".into(),
            behavior: HotkeyBehavior::TapToLock,
        };
        let mut state = PollingState::default();
        state.configure(&config, &standalone("MetaRight"));
        state.update(&keys(&[59, 49]), Duration::ZERO);
        state.drain();
        state.update(&keys(&[]), Duration::from_millis(8));
        state.drain();
        state.update(&keys(&[59, 49]), Duration::from_millis(16));
        state.update(&keys(&[]), Duration::from_millis(24));
        let (pair, _) = state.drain();
        assert!(!pair.0.down);
        assert!(pair.0.pressed, "the stop tap must survive ASR awaiting");
        assert_eq!(pair.0.press_count(true), 1);
        assert_eq!(pair.0.press_count(false), 1);
        assert!(!state.drain().0 .0.pressed);
    }

    #[test]
    fn delayed_presses_preserve_parity_even_past_counter_capacity() {
        let config = HotkeyConfig {
            modifiers: vec!["Ctrl".into()],
            key: "Space".into(),
            behavior: HotkeyBehavior::TapToLock,
        };
        for count in [1, 2, 3, 254, 255, 256, 257, 512] {
            let mut state = TestSampler::new(&config, &standalone("MetaRight"));
            for index in 0..count {
                state.update(&[59, 49], index * 16);
                state.update(&[], index * 16 + 8);
            }
            let presses = state.drain().0.press_count(false);
            assert!(presses > 0);
            assert_eq!(u64::from(presses % 2), count % 2, "{count} presses");
            assert_eq!(state.drain().0.press_count(false), 0);
        }
    }

    #[test]
    fn ordered_buffer_overflow_cancels_without_emitting_finish_events() {
        let mut config = standalone("Fn");
        config.behavior = HotkeyBehavior::TapToLock;
        let mut state = PollingState::default();
        state.configure(&config, &standalone("MetaRight"));
        for index in 0..=MAX_PENDING_CHANGES {
            state.update(&keys(&[63]), Duration::from_millis(index as u64 * 16));
            state.update(&keys(&[]), Duration::from_millis(index as u64 * 16 + 8));
            assert!(state.ordered.len() <= MAX_PENDING_CHANGES);
        }
        assert!(state.overflowed);
        let (events, cancelled) = state.drain_ordered();
        assert_eq!(cancelled, (true, true));
        assert!(events
            .iter()
            .all(|(d, s)| !d.down && !s.down && !d.pressed && !s.pressed));
        // Draining cannot re-arm a key that has not physically been released.
        state.update(&keys(&[63]), Duration::from_secs(100));
        assert!(state.drain_ordered().0.iter().all(|(d, _)| !d.pressed));
        state.update(&keys(&[]), Duration::from_secs(101));
        state.update(&keys(&[63]), Duration::from_secs(102));
        state.update(&keys(&[]), Duration::from_secs(103));
        assert_eq!(
            state
                .drain_ordered()
                .0
                .iter()
                .filter(|(d, _)| d.pressed)
                .count(),
            1
        );
    }

    #[test]
    fn held_final_sample_does_not_recount_a_drained_press() {
        let config = HotkeyConfig {
            modifiers: vec!["Ctrl".into()],
            key: "Space".into(),
            behavior: HotkeyBehavior::TapToLock,
        };
        let mut state = TestSampler::new(&config, &standalone("MetaRight"));
        state.update(&[59, 49], 0);
        assert_eq!(state.drain().0.press_count(false), 1);
        state.update(&[], 8);
        state.update(&[59, 49], 16);
        // The consumer still remembers 'down', but the sampler saw a new press.
        let next = state.drain().0;
        assert!(next.down);
        assert_eq!(next.press_count(true), 1);
        assert_eq!(state.drain().0.press_count(false), 0);
    }

    #[test]
    fn reconfiguration_discards_pending_old_binding_presses() {
        let mut state = PollingState::default();
        state.configure(&standalone("AltRight"), &standalone("MetaRight"));
        state.update(&keys(&[61]), Duration::ZERO);
        state.update(&keys(&[61]), Duration::from_millis(250));
        state.configure(&standalone("MetaRight"), &standalone("AltRight"));
        let (pair, cancelled) = state.drain();
        assert_eq!(pair.0.press_count(false), 0);
        assert_eq!(cancelled, (true, false));
        assert!(state.last_transition.is_none());
    }

    #[test]
    fn diagnostics_only_change_on_trigger_or_activation_transitions() {
        let mut state = PollingState::default();
        state.configure(&standalone("AltRight"), &standalone("MetaRight"));
        assert!(state.update(&keys(&[]), Duration::ZERO).is_some());
        assert!(state.update(&keys(&[]), Duration::from_millis(8)).is_none());
        let held = state
            .update(&keys(&[61]), Duration::from_millis(16))
            .unwrap();
        assert!(held.triggers.0.key_down);
        assert!(!held.active.0 && !held.blocked.0);
        assert!(state
            .update(&keys(&[61]), Duration::from_millis(265))
            .is_none());
        assert!(
            state
                .update(&keys(&[61]), Duration::from_millis(266))
                .unwrap()
                .active
                .0
        );
        let blocked = state
            .update(&keys(&[61, 57]), Duration::from_millis(274))
            .unwrap();
        assert!(blocked.blocked.0 && !blocked.active.0);
        assert!(state
            .update(&keys(&[61, 57, 8]), Duration::from_millis(282))
            .is_none());
        assert_eq!(
            keys(&[61, 57, 63, 71]).conflicts(&standalone("AltRight")),
            Some(3)
        );
        let idle = KeySnapshot([false; 128], false);
        assert_eq!(idle.conflicts(&standalone("AltRight")), None);
        assert!(state
            .update(&keys(&[]), Duration::from_millis(290))
            .is_some());
    }

    #[test]
    fn short_taps_and_clean_release_never_cancel() {
        let config = standalone("AltRight");
        let mut tracker = KeyTracker::default();
        assert!(!tracker.update(&config, &keys(&[61]), Duration::ZERO).down);
        let short = tracker.update(&config, &keys(&[]), Duration::from_millis(249));
        assert!(!short.down && !short.cancelled);
        tracker.update(&config, &keys(&[61]), Duration::from_millis(300));
        assert!(
            tracker
                .update(&config, &keys(&[61]), Duration::from_millis(550))
                .down
        );
        let released = tracker.update(&config, &keys(&[]), Duration::from_millis(560));
        assert!(!released.down && !released.cancelled);
    }
}
