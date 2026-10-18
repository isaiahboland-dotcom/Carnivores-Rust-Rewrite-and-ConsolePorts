//! The save file's key bindings are Windows virtual-key codes (`KeyMap`,
//! read with `KeyboardState[vk]`); 1 and 2 are the mouse buttons.

use winit::keyboard::KeyCode;

use crate::input::Input;

pub const VK_LBUTTON: i32 = 1;
pub const VK_RBUTTON: i32 = 2;

/// Keyboard VK codes and their keys. Shift/Ctrl/Alt (16/17/18) match either side.
const TABLE: &[(i32, KeyCode, &str)] = &[
    (8, KeyCode::Backspace, "Backspace"),
    (9, KeyCode::Tab, "Tab"),
    (13, KeyCode::Enter, "Enter"),
    (16, KeyCode::ShiftLeft, "Shift"),
    (16, KeyCode::ShiftRight, "Shift"),
    (17, KeyCode::ControlLeft, "Ctrl"),
    (17, KeyCode::ControlRight, "Ctrl"),
    (18, KeyCode::AltLeft, "Alt"),
    (18, KeyCode::AltRight, "Alt"),
    (20, KeyCode::CapsLock, "Caps Lock"),
    (32, KeyCode::Space, "Space"),
    (33, KeyCode::PageUp, "Page Up"),
    (34, KeyCode::PageDown, "Page Down"),
    (35, KeyCode::End, "End"),
    (36, KeyCode::Home, "Home"),
    (37, KeyCode::ArrowLeft, "Left"),
    (38, KeyCode::ArrowUp, "Up"),
    (39, KeyCode::ArrowRight, "Right"),
    (40, KeyCode::ArrowDown, "Down"),
    (45, KeyCode::Insert, "Insert"),
    (46, KeyCode::Delete, "Delete"),
    (48, KeyCode::Digit0, "0"),
    (49, KeyCode::Digit1, "1"),
    (50, KeyCode::Digit2, "2"),
    (51, KeyCode::Digit3, "3"),
    (52, KeyCode::Digit4, "4"),
    (53, KeyCode::Digit5, "5"),
    (54, KeyCode::Digit6, "6"),
    (55, KeyCode::Digit7, "7"),
    (56, KeyCode::Digit8, "8"),
    (57, KeyCode::Digit9, "9"),
    (65, KeyCode::KeyA, "A"),
    (66, KeyCode::KeyB, "B"),
    (67, KeyCode::KeyC, "C"),
    (68, KeyCode::KeyD, "D"),
    (69, KeyCode::KeyE, "E"),
    (70, KeyCode::KeyF, "F"),
    (71, KeyCode::KeyG, "G"),
    (72, KeyCode::KeyH, "H"),
    (73, KeyCode::KeyI, "I"),
    (74, KeyCode::KeyJ, "J"),
    (75, KeyCode::KeyK, "K"),
    (76, KeyCode::KeyL, "L"),
    (77, KeyCode::KeyM, "M"),
    (78, KeyCode::KeyN, "N"),
    (79, KeyCode::KeyO, "O"),
    (80, KeyCode::KeyP, "P"),
    (81, KeyCode::KeyQ, "Q"),
    (82, KeyCode::KeyR, "R"),
    (83, KeyCode::KeyS, "S"),
    (84, KeyCode::KeyT, "T"),
    (85, KeyCode::KeyU, "U"),
    (86, KeyCode::KeyV, "V"),
    (87, KeyCode::KeyW, "W"),
    (88, KeyCode::KeyX, "X"),
    (89, KeyCode::KeyY, "Y"),
    (90, KeyCode::KeyZ, "Z"),
    (96, KeyCode::Numpad0, "Num 0"),
    (97, KeyCode::Numpad1, "Num 1"),
    (98, KeyCode::Numpad2, "Num 2"),
    (99, KeyCode::Numpad3, "Num 3"),
    (100, KeyCode::Numpad4, "Num 4"),
    (101, KeyCode::Numpad5, "Num 5"),
    (102, KeyCode::Numpad6, "Num 6"),
    (103, KeyCode::Numpad7, "Num 7"),
    (104, KeyCode::Numpad8, "Num 8"),
    (105, KeyCode::Numpad9, "Num 9"),
    (106, KeyCode::NumpadMultiply, "Num *"),
    (107, KeyCode::NumpadAdd, "Num +"),
    (109, KeyCode::NumpadSubtract, "Num -"),
    (110, KeyCode::NumpadDecimal, "Num ."),
    (111, KeyCode::NumpadDivide, "Num /"),
    (186, KeyCode::Semicolon, ";"),
    (187, KeyCode::Equal, "="),
    (188, KeyCode::Comma, ","),
    (189, KeyCode::Minus, "-"),
    (190, KeyCode::Period, "."),
    (191, KeyCode::Slash, "/"),
    (192, KeyCode::Backquote, "`"),
    (219, KeyCode::BracketLeft, "["),
    (220, KeyCode::Backslash, "\\"),
    (221, KeyCode::BracketRight, "]"),
    (222, KeyCode::Quote, "'"),
];

/// Is the key (or mouse button) bound to `vk` held?
pub fn held(input: &Input, vk: i32) -> bool {
    match vk {
        VK_LBUTTON => input.grabbed && input.mouse_down,
        VK_RBUTTON => input.grabbed && input.right_down,
        _ => TABLE.iter().any(|&(v, k, _)| v == vk && input.held(k)),
    }
}

/// Did the key (or mouse button) bound to `vk` go down this frame?
pub fn pressed(input: &Input, vk: i32) -> bool {
    match vk {
        VK_LBUTTON => input.grabbed && input.clicked,
        VK_RBUTTON => input.grabbed && input.right_clicked,
        _ => TABLE.iter().any(|&(v, k, _)| v == vk && input.pressed(k)),
    }
}

/// The VK code of a key, if it can be bound.
pub fn vk_of(k: KeyCode) -> Option<i32> {
    TABLE.iter().find(|&&(_, c, _)| c == k).map(|&(v, _, _)| v)
}

/// Display name of a binding (`MapVKKey` + `GetKeyNameText`).
pub fn name(vk: i32) -> String {
    match vk {
        VK_LBUTTON => "Mouse1".into(),
        VK_RBUTTON => "Mouse2".into(),
        _ => TABLE
            .iter()
            .find(|&&(v, _, _)| v == vk)
            .map_or_else(|| format!("#{vk}"), |&(_, _, n)| n.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_round_trip() {
        for &(vk, k, _) in TABLE {
            assert_eq!(TABLE.iter().find(|e| e.0 == vk).map(|e| e.0), vk_of(k));
        }
        assert_eq!(name(87), "W");
        assert_eq!(name(VK_LBUTTON), "Mouse1");
    }
}
