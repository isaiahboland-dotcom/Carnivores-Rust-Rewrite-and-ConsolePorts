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
//! | right stick click | VR: recenter (standing height here, the menu in front); PC hunt: next light (off, flashlight, night vision) |

//!
//! `gilrs` reads the pads on PC; it has no Android backend, so the Android
//! app feeds its pads' buttons and sticks in itself ([`Pads::button`],
//! [`Pads::sticks`]).

#[cfg(not(target_os = "android"))]
mod gilrs_pads;

/// A pad button, by position (Nintendo-style names are the table's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Face buttons: bottom, right, left, top.
    South,
    East,
    West,
    North,
    /// Bumpers and triggers.
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

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
    /// The Modder's Edition's weapons: reload, rack, the firing mode.
    Reload,
    Rack,
    FireMode,
}

const ACTIONS: usize = 16;

/// The layout: which button does what.
fn action(b: Button) -> Option<Action> {
    Some(match b {
        Button::L2 => Action::Draw,
        Button::R2 => Action::Fire,
        Button::L1 => Action::Binoculars,
        Button::R1 => Action::Map,
        Button::East => Action::Jump,
        Button::South => Action::Run,
        Button::North => Action::Call,
        Button::West => Action::NextWeapon,
        Button::DPadUp => Action::Crouch,
        Button::DPadDown => Action::NextCall,
        Button::Start => Action::Pause,
        Button::Select => Action::Exit,
        Button::RightThumb => Action::Recenter,
        Button::DPadLeft => Action::Reload,
        Button::DPadRight => Action::Rack,
        Button::LeftThumb => Action::FireMode,
    })
}

/// Sticks under this are still (worn pads drift).
const DEADZONE: f32 = 0.15;

/// `v` with a round deadzone, rescaled so just past it is near zero.
pub fn dead(v: [f32; 2]) -> [f32; 2] {
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
    /// `a` held, and pressed if it wasn't (on-screen buttons acting as a pad).
    pub fn hold(&mut self, a: Action, down: bool) {
        let i = a as usize;
        self.pressed[i] |= down && !self.held[i];
        self.held[i] |= down;
    }

    /// `a` pressed this frame (a tap), held for the frame too.
    pub fn press(&mut self, a: Action) {
        self.pressed[a as usize] = true;
        self.held[a as usize] = true;
    }

    /// `other`'s buttons too, and its sticks where they are pushed further.
    pub fn merge(&mut self, other: &Frame) {
        for i in 0..ACTIONS {
            self.held[i] |= other.held[i];
            self.pressed[i] |= other.pressed[i];
        }
        for (mine, theirs) in [(&mut self.walk, other.walk), (&mut self.look, other.look)] {
            if theirs[0].abs() + theirs[1].abs() > mine[0].abs() + mine[1].abs() {
                *mine = theirs;
            }
        }
    }

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
    #[cfg(not(target_os = "android"))]
    gilrs: gilrs_pads::GilrsPads,
    held: [bool; ACTIONS],
    /// Went down since the last poll.
    pressed: [bool; ACTIONS],
    /// D-pad left, right, down, up held.
    dpad: [bool; 4],
    /// Sticks fed in by the frontend (Android), before the deadzone.
    walk: [f32; 2],
    look: [f32; 2],
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
        Self {
            #[cfg(not(target_os = "android"))]
            gilrs: gilrs_pads::GilrsPads::new(),
            held: [false; ACTIONS],
            pressed: [false; ACTIONS],
            dpad: [false; 4],
            walk: [0.0; 2],
            look: [0.0; 2],
        }
    }

    /// The connected pads' names.
    pub fn names(&self) -> Vec<String> {
        #[cfg(not(target_os = "android"))]
        return self.gilrs.names();
        #[cfg(target_os = "android")]
        Vec::new()
    }

    /// A button went down or up.
    pub fn button(&mut self, b: Button, down: bool) {
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
            let i = a as usize;
            self.pressed[i] |= down && !self.held[i];
            self.held[i] = down;
        }
    }

    /// The sticks now (x right, y up, -1..1), from the frontend.
    pub fn sticks(&mut self, walk: [f32; 2], look: [f32; 2]) {
        self.walk = walk;
        self.look = look;
    }

    /// This frame's state: the buttons since the last call, the sticks now.
    pub fn poll(&mut self) -> Frame {
        #[cfg(not(target_os = "android"))]
        let (walk, look) = {
            let mut events = Vec::new();
            let sticks = self.gilrs.poll(&mut events);
            for (b, down) in events {
                self.button(b, down);
            }
            sticks
        };
        #[cfg(target_os = "android")]
        let (walk, look) = ([0.0; 2], [0.0; 2]);
        let mut f = Frame {
            held: self.held,
            pressed: std::mem::take(&mut self.pressed),
            ..Frame::default()
        };
        let axis = |neg: bool, pos: bool| i8::from(pos) - i8::from(neg);
        f.dpad = [
            axis(self.dpad[0], self.dpad[1]),
            axis(self.dpad[2], self.dpad[3]),
        ];
        let stronger = |a: [f32; 2], b: [f32; 2]| {
            if b[0].abs() + b[1].abs() > a[0].abs() + a[1].abs() {
                b
            } else {
                a
            }
        };
        f.walk = stronger(walk, dead(self.walk));
        f.look = stronger(look, dead(self.look));
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
    fn fed_buttons_press_once_and_hold() {
        let mut p = Pads::new();
        p.button(Button::R2, true);
        let f = p.poll();
        assert!(f.pressed(Action::Fire) && f.held(Action::Fire));
        let f = p.poll();
        assert!(!f.pressed(Action::Fire) && f.held(Action::Fire));
        p.button(Button::R2, false);
        p.button(Button::DPadUp, true);
        let f = p.poll();
        assert!(!f.held(Action::Fire) && f.dpad == [0, 1] && f.held(Action::Crouch));
    }

    #[test]
    fn every_action_has_a_button() {
        let buttons = [
            Button::L2,
            Button::R2,
            Button::L1,
            Button::R1,
            Button::East,
            Button::South,
            Button::North,
            Button::West,
            Button::DPadUp,
            Button::DPadDown,
            Button::Start,
            Button::Select,
            Button::RightThumb,
            Button::DPadLeft,
            Button::DPadRight,
            Button::LeftThumb,
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
