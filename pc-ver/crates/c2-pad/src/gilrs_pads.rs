//! The PC's pads through `gilrs` (with [`crate::MAPPINGS`]).

use gilrs::{Axis, Button as G, EventType, Gilrs, GilrsBuilder};

use crate::{dead, Button};

pub struct GilrsPads(Option<Gilrs>);

/// `gilrs`'s button as ours (its names are Xbox-style positions).
fn ours(b: G) -> Option<Button> {
    Some(match b {
        G::South => Button::South,
        G::East => Button::East,
        G::West => Button::West,
        G::North => Button::North,
        G::LeftTrigger => Button::L1,
        G::RightTrigger => Button::R1,
        G::LeftTrigger2 => Button::L2,
        G::RightTrigger2 => Button::R2,
        G::Select => Button::Select,
        G::Start => Button::Start,
        G::LeftThumb => Button::LeftThumb,
        G::RightThumb => Button::RightThumb,
        G::DPadUp => Button::DPadUp,
        G::DPadDown => Button::DPadDown,
        G::DPadLeft => Button::DPadLeft,
        G::DPadRight => Button::DPadRight,
        _ => return None,
    })
}

impl GilrsPads {
    pub fn new() -> Self {
        Self(
            GilrsBuilder::new()
                .add_mappings(crate::MAPPINGS)
                .build()
                .ok(),
        )
    }

    pub fn names(&self) -> Vec<String> {
        self.0
            .as_ref()
            .map(|g| g.gamepads().map(|(_, p)| p.name().to_string()).collect())
            .unwrap_or_default()
    }

    /// Button changes since the last call into `events`; the strongest
    /// sticks of every pad (walk, look), deadzoned.
    pub fn poll(&mut self, events: &mut Vec<(Button, bool)>) -> ([f32; 2], [f32; 2]) {
        let Some(g) = self.0.as_mut() else {
            return ([0.0; 2], [0.0; 2]);
        };
        while let Some(ev) = g.next_event() {
            let (b, down) = match ev.event {
                EventType::ButtonPressed(b, _) => (b, true),
                EventType::ButtonReleased(b, _) => (b, false),
                _ => continue,
            };
            if let Some(b) = ours(b) {
                events.push((b, down));
            }
        }
        let (mut walk, mut look) = ([0.0f32; 2], [0.0f32; 2]);
        for (_, pad) in g.gamepads() {
            let w = dead([pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)]);
            let l = dead([pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)]);
            for (sum, v) in [(&mut walk, w), (&mut look, l)] {
                if v[0].abs() + v[1].abs() > sum[0].abs() + sum[1].abs() {
                    *sum = v;
                }
            }
        }
        (walk, look)
    }
}
