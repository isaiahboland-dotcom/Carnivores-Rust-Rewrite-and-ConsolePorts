//! Night vision and the flashlight in dynamic-time hunts (both bought on the
//! gear page). N and L switch them on PC; the pad's right stick click steps
//! through off, flashlight, night vision (only the ones bought). On the
//! phone's hunt screen (an APK-built game) each has a round yellow button,
//! "nv" or "light", that a tap or a click switches. One light at a time:
//! turning one on turns the other off, and its button greys.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Light {
    Vision,
    Torch,
}

/// The lights on: (night vision, flashlight).
pub(super) type Lit = (bool, bool);

/// `which` switched; the other goes off when it comes on.
pub(super) fn toggle((nv, torch): Lit, which: Light) -> Lit {
    match which {
        Light::Vision => (!nv, torch && nv),
        Light::Torch => (nv && torch, !torch),
    }
}

/// The next of off, flashlight, night vision, skipping lights not `owned`.
pub(super) fn cycle(lit: Lit, owned: Lit) -> Lit {
    let mut states = vec![(false, false)];
    if owned.1 {
        states.push((false, true));
    }
    if owned.0 {
        states.push((true, false));
    }
    let i = states.iter().position(|&s| s == lit).unwrap_or(0);
    states[(i + 1) % states.len()]
}

/// The round buttons' places (centre x, y and diameter in pixels): down
/// the right edge, under the ammunition count.
pub(super) fn button_places(screen: (u32, u32), lights: &[Light]) -> Vec<(Light, (f32, f32, f32))> {
    let (sw, sh) = (screen.0 as f32, screen.1 as f32);
    let d = (sh * 0.085).max(36.0);
    let x = sw - 12.0 - d / 2.0;
    let y0 = 12.0 + sh * 0.06 + d / 2.0;
    lights
        .iter()
        .enumerate()
        .map(|(i, &l)| (l, (x, y0 + i as f32 * d * 1.25, d)))
        .collect()
}

fn on_button((px, py): (f32, f32), (x, y, d): (f32, f32, f32)) -> bool {
    (px - x).hypot(py - y) <= d / 2.0
}

impl HuntView {
    fn lit(&self) -> Lit {
        (self.view.night_vision == Some(true), self.view.flashlight)
    }

    fn set_lit(&mut self, (nv, torch): Lit) {
        self.view.night_vision = Some(nv);
        self.view.flashlight = torch;
    }

    /// The lights bought for this hunt, in button order.
    fn owned_lights(&self) -> Vec<Light> {
        let mut v = Vec::new();
        if self.setup.night_vision {
            v.push(Light::Vision);
        }
        if self.setup.flashlight {
            v.push(Light::Torch);
        }
        v
    }

    fn lights_live(&self) -> bool {
        self.view.area.cycle.is_some()
            && !self.hunt.show_exit()
            && !self.hunt.paused
            && self.hunt.death.is_none()
    }

    /// The round buttons, when this hunt shows them.
    fn light_buttons(&self) -> Vec<(Light, (f32, f32, f32))> {
        let h = &self.hunt;
        let quiet = h.paused || h.show_exit() || h.map_mode || h.opts.trophy;
        if self.kit.is_none() || quiet || self.picker.is_some() {
            return Vec::new();
        }
        button_places(self.screen, &self.owned_lights())
    }

    /// Keys, the pad and the buttons. True when a tap or click landed on a
    /// button (it doesn't fire the weapon too).
    pub(super) fn update_lights(&mut self, input: &Input) -> bool {
        if !self.lights_live() {
            return false;
        }
        let owned = (self.setup.night_vision, self.setup.flashlight);
        let mut lit = self.lit();
        if owned.0 && input.pressed(KeyCode::KeyN) {
            lit = toggle(lit, Light::Vision);
        }
        if owned.1 && input.pressed(KeyCode::KeyL) {
            lit = toggle(lit, Light::Torch);
        }
        if (owned.0 || owned.1) && input.pad.pressed(c2_pad::Action::Recenter) {
            lit = cycle(lit, owned);
        }
        // A free pointer's click, or a touch anywhere.
        let click = input.clicked.then_some(input.cursor).flatten();
        let points = input
            .taps
            .iter()
            .copied()
            .chain(click.filter(|_| !input.grabbed));
        let mut hit = false;
        for p in points {
            if let Some((l, _)) = self
                .light_buttons()
                .into_iter()
                .find(|&(_, b)| on_button(p, b))
            {
                lit = toggle(lit, l);
                hit = true;
            }
        }
        if lit != self.lit() {
            self.set_lit(lit);
        }
        hit
    }

    /// The hint bar's entries for the lights bought (dynamic-time hunts).
    pub(super) fn light_hints(&self, pad: bool) -> Vec<(String, &'static str)> {
        if self.view.area.cycle.is_none() {
            return Vec::new();
        }
        let owned = (self.setup.night_vision, self.setup.flashlight);
        match (pad, owned) {
            (_, (false, false)) => Vec::new(),
            (true, _) => vec![("R stick click".into(), "Lights")],
            (false, _) => [(owned.0, "N", "Night vision"), (owned.1, "L", "Flashlight")]
                .into_iter()
                .filter(|o| o.0)
                .map(|(_, k, a)| (k.to_string(), a))
                .collect(),
        }
    }

    /// The phone screen's round buttons, or the PC's "N: Night vision Off"
    /// lines.
    pub(super) fn draw_lights(&self, sprites: &mut Vec<Sprite>, scale: f32) {
        if self.view.area.cycle.is_none() || self.hunt.death.is_some() {
            return;
        }
        let (nv, torch) = self.lit();
        if let Some(k) = &self.kit {
            for (l, place) in self.light_buttons() {
                let (on, other) = match l {
                    Light::Vision => (nv, torch),
                    Light::Torch => (torch, nv),
                };
                let word = match l {
                    Light::Vision => "nv",
                    Light::Torch => "light",
                };
                k.light_button(&self.hud, place, word, on, other, sprites);
            }
            return;
        }
        // Under the survival's wave box (top left) when there is one.
        let mut y = if self.hunt.survival.is_some() {
            54.0
        } else {
            12.0
        } * scale;
        for (owned, key, name, on) in [
            (self.setup.night_vision, "N", "Night vision", nv),
            (self.setup.flashlight, "L", "Flashlight", torch),
        ] {
            if owned {
                let label = format!("{key}: {name} {}", if on { "On" } else { "Off" });
                sprites.extend(
                    self.hud
                        .label(12.0 * scale, y, 9.0 * scale, [1.0; 4], &label),
                );
                y += 14.0 * scale;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_light_at_a_time() {
        assert_eq!(toggle((false, false), Light::Vision), (true, false));
        assert_eq!(toggle((true, false), Light::Torch), (false, true));
        assert_eq!(toggle((false, true), Light::Vision), (true, false));
        assert_eq!(toggle((false, true), Light::Torch), (false, false));
        assert_eq!(toggle((true, false), Light::Vision), (false, false));
    }

    #[test]
    fn the_pad_steps_through_the_lights_bought() {
        let both = (true, true);
        assert_eq!(cycle((false, false), both), (false, true));
        assert_eq!(cycle((false, true), both), (true, false));
        assert_eq!(cycle((true, false), both), (false, false));
        assert_eq!(cycle((false, false), (true, false)), (true, false));
        assert_eq!(cycle((true, false), (true, false)), (false, false));
        assert_eq!(cycle((false, false), (false, true)), (false, true));
    }

    #[test]
    fn buttons_stack_down_the_right_edge() {
        let b = button_places((1280, 720), &[Light::Vision, Light::Torch]);
        let (_, (x0, y0, d)) = b[0];
        let (_, (x1, y1, _)) = b[1];
        assert!(x0 + d / 2.0 <= 1280.0 && x0 > 1100.0);
        assert_eq!(x0, x1);
        assert!(y1 - y0 > d, "they don't overlap");
        assert!(on_button((x0, y0), b[0].1) && !on_button((x0, y1), b[0].1));
    }
}
