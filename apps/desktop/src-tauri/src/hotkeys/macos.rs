use crate::model::HotkeyConfig;

type CGKeyCode = u16;
type CGEventSourceStateID = i32;
const COMBINED_SESSION_STATE: CGEventSourceStateID = 0;
const KEY_SPACE: CGKeyCode = 49;
const KEY_CONTROL: CGKeyCode = 59;
const KEY_SHIFT: CGKeyCode = 56;
const KEY_OPTION: CGKeyCode = 58;
const KEY_COMMAND: CGKeyCode = 55;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventSourceKeyState(state_id: CGEventSourceStateID, key: CGKeyCode) -> bool;
}

fn down(key: CGKeyCode) -> bool {
    unsafe { CGEventSourceKeyState(COMBINED_SESSION_STATE, key) }
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

pub fn is_pressed(hotkey: &HotkeyConfig) -> bool {
    let modifiers_down = hotkey
        .modifiers
        .iter()
        .all(|modifier| match modifier.as_str() {
            "Ctrl" => down(KEY_CONTROL),
            "Shift" => down(KEY_SHIFT),
            "Alt" => down(KEY_OPTION),
            "Meta" => down(KEY_COMMAND),
            _ => false,
        });
    modifiers_down && key_code(&hotkey.key).is_some_and(down)
}
