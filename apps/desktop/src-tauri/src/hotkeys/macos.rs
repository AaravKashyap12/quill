use crate::model::HotkeyConfig;

type CGKeyCode = u16;
type CGEventSourceStateID = i32;
const COMBINED_SESSION_STATE: CGEventSourceStateID = 0;
const HID_SYSTEM_STATE: CGEventSourceStateID = 1;
const KEY_SPACE: CGKeyCode = 49;
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
}

fn key_code(key: &str) -> Option<CGKeyCode> {
    match key {
        "Space" => Some(KEY_SPACE),
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

#[derive(Debug, Clone, Copy)]
pub struct HotkeySample {
    pub session: KeySample,
    pub hid: KeySample,
}

pub fn sample(hotkey: &HotkeyConfig) -> HotkeySample {
    let read = |state| sample_with(hotkey, |key| unsafe { CGEventSourceKeyState(state, key) });
    HotkeySample {
        session: read(COMBINED_SESSION_STATE),
        hid: read(HID_SYSTEM_STATE),
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
        key_down: key_code(&hotkey.key).is_some_and(down),
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
}
