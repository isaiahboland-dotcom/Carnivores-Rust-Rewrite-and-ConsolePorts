//! Gamepads as the game's actions, shared by `c2-pc` and `c2-vr` (docs/DEVELOPMENT.md §4
//! names `gilrs`). The layout is the Wii U plan's (`wiiu-ver/gameplan.md`),
//! so every console-style frontend plays alike. Face buttons go by position,
//! Nintendo style (the Wii U's and a Switch pad's labels; an Xbox pad's
//! labels differ, its positions don't):
//!
//! | Control | Action |
//! | --- | --- |
//! | left stick | move |
//! | right stick | look (VR: snap turn) |
//! | ZL / left trigger | draw or holster the weapon |
//! | ZR / right trigger | fire |
//! | L / left bumper | binoculars |
//! | R / right bumper | map |
//! | A (right) | jump |
//! | B (bottom) | toggle run |
//! | X (top) | use the call |
//! | Y (left) | next weapon |
//! | D-pad up | crouch |
//! | D-pad down | next call |
//! | + / Start | pause |
//! | − / Select | exit prompt (VR: quit) |
//! | right stick click | VR: recenter (standing height here, the menu in front) |

use gilrs::{Axis, Button, EventType, Gilrs, GilrsBuilder};

/// SDL-format mappings for pads `gilrs`'s database lacks (more can come from
/// `SDL_GAMECONTROLLERCONFIG`, which gilrs also reads). Without one, a pad's
/// raw codes are guessed at: the owner's PowerA Nano (Switch layout) came
/// through with its right stick as `LeftZ` / `RightZ` and nameless buttons.
/// Its line is the database's "PowerA Core Controller" (20d6:a711, the same
/// family) under the Nano's id (20d6:a718); its evdev events (2026-10-29)
/// match it: Y B A X L R ZL ZR - + as buttons 0-9, right stick on a2 / a3.
pub const MAPPINGS: &str = "\
03000000d620000018a7000011010000,PowerA NSW Nano Wired controller,a:b1,b:b2,back:b8,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,dpup:h0.1,guide:b12,leftshoulder:b4,leftstick:b10,lefttrigger:b6,leftx:a0,lefty:a1,rightshoulder:b5,rightstick:b11,righttrigger:b7,rightx:a2,righty:a3,start:b9,x:b0,y:b3,platform:Linux,
";

/// A game action a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    Draw,
    Fire,
    Binoculars,
    Map,
    Jump,
    Run,
    Call,
    NextWeapon,
    Crouch,
    NextCall,
    Pause,
    Exit,
    /// VR only.
    Recenter,
}

const ACTIONS: usize = 13;

/// The layout: which button does what.
fn action(b: Button) -> Option<Action> {
    Some(match b {
        Button::LeftTrigger2 => Action::Draw,
        Button::RightTrigger2 => Action::Fire,
        Button::LeftTrigger => Action::Binoculars,
        Button::RightTrigger => Action::Map,
        Button::East => Action::Jump,
        Button::South => Action::Run,
        Button::North => Action::Call,
        Button::West => Action::NextWeapon,
        Button::DPadUp => Action::Crouch,
        Button::DPadDown => Action::NextCall,
        Button::Start => Action::Pause,
        Button::Select => Action::Exit,
        Button::RightThumb => Action::Recenter,
        _ => return None,
    })
}

/// Sticks under this are still (worn pads drift).
const DEADZONE: f32 = 0.15;

/// `v` with a round deadzone, rescaled so just past it is near zero.
fn dead(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < DEADZONE {
        return [0.0, 0.0];
    }
    let k = ((len - DEADZONE) / (1.0 - DEADZONE)).min(1.0) / len;
    [v[0] * k, v[1] * k]
}

/// This frame's pad state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Left stick: x right, y forward (up), -1..1.
    pub walk: [f32; 2],
    /// Right stick: x right, y up, -1..1.
    pub look: [f32; 2],
    /// The D-pad held: x right, y up, -1 / 0 / 1 (also read as its actions:
    /// up crouch, down next call; the PC map moves its marker with it).
    pub dpad: [i8; 2],
    held: [bool; ACTIONS],
    pressed: [bool; ACTIONS],
}

impl Frame {
    pub fn held(&self, a: Action) -> bool {
        self.held[a as usize]
    }

    /// Went down this frame.
    pub fn pressed(&self, a: Action) -> bool {
        self.pressed[a as usize]
    }

    /// Anything pushed or pressed (to tell a pad is in use).
    pub fn active(&self) -> bool {
        self.held.iter().any(|&h| h)
            || self.walk != [0.0; 2]
            || self.look != [0.0; 2]
            || self.dpad != [0; 2]
    }
}

/// Every connected pad, read as one.
pub struct Pads {
    gilrs: Option<Gilrs>,
    held: [bool; ACTIONS],
    /// D-pad left, right, down, up held.
    dpad: [bool; 4],
}

impl Default for Pads {
    fn default() -> Self {
        Self::new()
    }
}

impl Pads {
    /// No pad support (no input devices, no udev) means no pads, not an
    /// error: the game plays on without them.
    pub fn new() -> Self {
        let gilrs = GilrsBuilder::new().add_mappings(MAPPINGS).build().ok();
        Self {
            gilrs,
            held: [false; ACTIONS],
            dpad: [false; 4],
        }
    }

    /// The connected pads' names.
    pub fn names(&self) -> Vec<String> {
        self.gilrs
            .as_ref()
            .map(|g| g.gamepads().map(|(_, p)| p.name().to_string()).collect())
            .unwrap_or_default()
    }

    /// Reads the pads' events since the last call.
    pub fn poll(&mut self) -> Frame {
        let mut f = Frame::default();
        let Some(g) = self.gilrs.as_mut() else {
            return f;
        };
        while let Some(ev) = g.next_event() {
            let (b, down) = match ev.event {
                EventType::ButtonPressed(b, _) => (b, true),
                EventType::ButtonReleased(b, _) => (b, false),
                _ => continue,
            };
            let d = match b {
                Button::DPadLeft => Some(0),
                Button::DPadRight => Some(1),
                Button::DPadDown => Some(2),
                Button::DPadUp => Some(3),
                _ => None,
            };
            if let Some(d) = d {
                self.dpad[d] = down;
            }
            if let Some(a) = action(b) {
                if down && !self.held[a as usize] {
                    f.pressed[a as usize] = true;
                }
                self.held[a as usize] = down;
            }
        }
        f.held = self.held;
        let axis = |neg: bool, pos: bool| i8::from(pos) - i8::from(neg);
        f.dpad = [
            axis(self.dpad[0], self.dpad[1]),
            axis(self.dpad[2], self.dpad[3]),
        ];
        for (_, pad) in g.gamepads() {
            let walk = dead([pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)]);
            let look = dead([pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)]);
            for (sum, v) in [(&mut f.walk, walk), (&mut f.look, look)] {
                if v[0].abs() + v[1].abs() > sum[0].abs() + sum[1].abs() {
                    *sum = v;
                }
            }
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_is_round_and_rescaled() {
        assert_eq!(dead([0.1, 0.1]), [0.0, 0.0]);
        let [x, y] = dead([1.0, 0.0]);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let [x, _] = dead([0.2, 0.0]);
        assert!(x > 0.0 && x < 0.1);
    }

    #[test]
    fn every_action_has_a_button() {
        let buttons = [
            Button::LeftTrigger2,
            Button::RightTrigger2,
            Button::LeftTrigger,
            Button::RightTrigger,
            Button::East,
            Button::South,
            Button::North,
            Button::West,
            Button::DPadUp,
            Button::DPadDown,
            Button::Start,
            Button::Select,
            Button::RightThumb,
        ];
        let mut seen = [false; ACTIONS];
        for b in buttons {
            if let Some(a) = action(b) {
                seen[a as usize] = true;
            }
        }
        assert!(seen.iter().all(|&s| s));
    }
}
