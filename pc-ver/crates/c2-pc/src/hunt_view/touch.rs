//! The phone's hunt buttons, for a game built from the player's APK, while
//! the last input was a touch (`Input::touch_mode`; a key or a pad hides
//! them, as controller-friendly phone games do). The mobile game's layout
//! and pictures (`HuntKit::touch`), from the owner's screenshots of it:
//!
//! - the compass, top left, opens the map (and closes it again);
//! - under it the binoculars, or fire while a weapon is in hand;
//! - the move stick, bottom left (pushed all the way: run); a drag
//!   anywhere else looks around;
//! - the call, top right: a tap calls, holding opens the call picker;
//! - under it the weapon in use: a tap takes it out or puts it away,
//!   holding opens the weapon picker (slide to a hexagon and let go);
//! - the "menu" tab is the hunt menu (Restart, Resume, Escape!), whose
//!   hexagons take taps; a tap ends a pause.
//!
//! The buttons become pad actions (`c2_pad::Frame`), so the hunt reads them
//! as it reads a pad, and the first finger stops acting as the mouse (a tap
//! no longer fires).
use std::time::Instant;

use super::*;

/// The touch controls' state between frames.
#[derive(Default)]
pub(super) struct Touch {
    /// Showing (an APK game, the last input a touch, nothing over the hunt).
    pub(super) on: bool,
    stick: Option<u64>,
    /// The knob's offset from the stick's centre, in pixels.
    knob: (f32, f32),
    look: Option<(u64, (f32, f32))>,
    fire: Option<u64>,
    /// The weapon or call button held: which, since when.
    hold: Option<(u64, Pick, Instant)>,
    /// The finger holding a picker open, where it is, and whether it left.
    picking: Option<(u64, (f32, f32))>,
    pick_lifted: Option<(f32, f32)>,
    /// A tap with the picker open (not by holding).
    pick_tap: Option<(f32, f32)>,
    /// The stick pushed all the way.
    pub(super) run: bool,
    /// The last call by touch (the ring spreading from its button).
    called: Option<Instant>,
}

impl Touch {
    fn release(&mut self) {
        *self = Self {
            on: self.on,
            picking: self.picking,
            called: self.called,
            ..Self::default()
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Control {
    Stick,
    /// Binoculars, or fire with a weapon in hand.
    Left,
    Call,
    Weapon,
    Compass,
    Tab,
    /// The light buttons (read by `lights`).
    Light,
}

/// Places in screen pixels: circles are (centre x, y, diameter).
struct Layout {
    stick: (f32, f32, f32),
    left: (f32, f32, f32),
    call: (f32, f32, f32),
    weapon: (f32, f32, f32),
    compass: (f32, f32, f32, f32),
    tab: (f32, f32, f32, f32),
    lights: Vec<(f32, f32, f32)>,
}

fn inside((px, py): (f32, f32), (x, y, w, h): (f32, f32, f32, f32)) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

fn in_circle((px, py): (f32, f32), (x, y, d): (f32, f32, f32), slack: f32) -> bool {
    (px - x).hypot(py - y) <= d / 2.0 * slack
}

/// Seconds the call's ring takes to spread and fade.
const RIPPLE: f32 = 0.7;

/// The round buttons' size, as a share of the screen's height.
const BUTTON: f32 = 0.14;

/// Where the light buttons go while the touch buttons show: down the right
/// edge, under the weapon button.
pub(super) fn light_top(screen: (u32, u32)) -> f32 {
    let sh = screen.1 as f32;
    sh * 0.35 + sh * BUTTON * 0.75
}

impl Layout {
    fn new(screen: (u32, u32), lights: Vec<(f32, f32, f32)>) -> Self {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let d = sh * BUTTON;
        let r = sh * 0.16;
        let right = sw - sh * 0.035 - d / 2.0;
        Self {
            stick: (sh * 0.22, sh * 0.70, r * 2.0),
            left: (sh * 0.035 + d / 2.0 + sh * 0.02, sh * 0.33, d),
            call: (right, sh * 0.145, d),
            weapon: (right, sh * 0.35, d),
            compass: HuntKit::compass_rect(screen),
            tab: HuntKit::tab_rect(screen),
            lights,
        }
    }

    /// The control under `p`; in the map only the compass and the tab.
    fn hit(&self, p: (f32, f32), map: bool, full: bool) -> Option<Control> {
        if inside(p, self.compass) {
            return Some(Control::Compass);
        }
        if inside(p, self.tab) {
            return Some(Control::Tab);
        }
        if map {
            return None;
        }
        if self.lights.iter().any(|&b| in_circle(p, b, 1.1)) {
            return Some(Control::Light);
        }
        if in_circle(p, self.stick, 1.3) {
            return Some(Control::Stick);
        }
        if !full {
            return None;
        }
        [
            (self.left, Control::Left),
            (self.call, Control::Call),
            (self.weapon, Control::Weapon),
        ]
        .into_iter()
        .find(|&(b, _)| in_circle(p, b, 1.2))
        .map(|(_, c)| c)
    }
}

impl HuntView {
    /// The full set (weapons, calls), not just walking and looking (the
    /// trophy room, observing).
    fn touch_full(&self) -> bool {
        !self.hunt.opts.trophy && !self.hunt.opts.observer
    }

    fn touch_layout(&self) -> Layout {
        let lights = self.light_buttons().into_iter().map(|(_, b)| b).collect();
        Layout::new(self.screen, lights)
    }

    /// A weapon in hand (the left button fires).
    fn touch_armed(&self) -> bool {
        self.hunt.weapons.pose().is_some() && !self.hunt.bin_mode
    }

    /// The touches as pad actions, before the hunt reads its input.
    pub(super) fn touch_input(&mut self, input: &mut Input) {
        let live = self.kit.is_some() && input.touch_mode;
        if !live {
            self.touch.on = false;
            self.touch.release();
            self.touch.picking = None;
            return;
        }
        if self.picker.is_some() {
            self.touch.on = false;
            self.touch_picking(input);
            return;
        }
        self.touch.picking = None;
        let h = &self.hunt;
        if h.show_exit() || h.paused || h.death.is_some() {
            self.touch.on = false;
            self.touch.release();
            self.touch_menu(input);
            return;
        }
        self.touch.on = true;
        use c2_pad::Action as A;
        let lay = self.touch_layout();
        let map = self.hunt.map_mode;
        let full = self.touch_full();
        let mut pad = c2_pad::Frame::default();
        let mut used = false;
        for f in input.fingers.iter().filter(|f| f.new) {
            let hit = lay.hit(f.at, map, full);
            used |= hit.is_some();
            match hit {
                Some(Control::Stick) => self.touch.stick = Some(f.id),
                Some(Control::Left) if self.touch_armed() => self.touch.fire = Some(f.id),
                Some(Control::Left) => pad.press(A::Binoculars),
                Some(Control::Call) => self.touch.hold = Some((f.id, Pick::Call, Instant::now())),
                Some(Control::Weapon) => {
                    self.touch.hold = Some((f.id, Pick::Weapon, Instant::now()));
                }
                Some(Control::Compass) => pad.press(A::Map),
                Some(Control::Tab) => pad.press(A::Exit),
                Some(Control::Light) => {}
                None if self.touch.look.is_none() && !map => {
                    self.touch.look = Some((f.id, f.at));
                }
                None => {}
            }
        }
        for f in &input.lifted {
            if self.touch.stick == Some(f.id) {
                self.touch.stick = None;
            }
            if self.touch.look.is_some_and(|(id, _)| id == f.id) {
                self.touch.look = None;
            }
            if self.touch.fire == Some(f.id) {
                self.touch.fire = None;
            }
            // Let go before the picker opened: a tap.
            if let Some((id, kind, _)) = self.touch.hold {
                if id == f.id {
                    self.touch.hold = None;
                    match kind {
                        Pick::Call => {
                            pad.press(A::Call);
                            self.touch.called = Some(Instant::now());
                        }
                        Pick::Weapon => self.pending.toggle_weapon = true,
                    }
                }
            }
        }
        // Held long enough: the picker opens, the finger holding it.
        if let Some((id, kind, since)) = self.touch.hold {
            if since.elapsed().as_secs_f32() >= HOLD_TO_PICK {
                self.touch.hold = None;
                let at = input.fingers.iter().find(|f| f.id == id).map(|f| f.at);
                self.open_pick(kind, true);
                if self.picker.is_some() {
                    self.touch.picking = at.map(|at| (id, at));
                }
            }
        }
        // In the map the pointer pans and picks as before.
        if map {
            if used {
                input.clicked = false;
            }
            input.pad.merge(&pad);
            return;
        }
        let finger = |id: Option<u64>| id.and_then(|id| input.fingers.iter().find(|f| f.id == id));
        let (sx, sy, sd) = lay.stick;
        let r = sd / 2.0;
        self.touch.knob = (0.0, 0.0);
        self.touch.run = false;
        if let Some(f) = finger(self.touch.stick) {
            let (dx, dy) = (f.at.0 - sx, f.at.1 - sy);
            let len = dx.hypot(dy);
            let k = if len > r { r / len } else { 1.0 };
            self.touch.knob = (dx * k, dy * k);
            pad.walk = [dx * k / r, -dy * k / r];
            self.touch.run = len >= r * 0.92;
        }
        let mut look = (0.0, 0.0);
        if let Some((id, last)) = self.touch.look {
            if let Some(f) = input.fingers.iter().find(|f| f.id == id) {
                // A full-width swipe turns about half way round.
                let k = 1920.0 / self.screen.0.max(1) as f32;
                look = ((f.at.0 - last.0) * k, (f.at.1 - last.1) * k);
                self.touch.look = Some((id, f.at));
            }
        }
        pad.hold(A::Fire, self.touch.fire.is_some());
        // The first finger no longer acts as the mouse here.
        input.clicked = false;
        input.mouse_down = false;
        input.dragging = false;
        input.mouse_delta = look;
        input.pad.merge(&pad);
    }

    /// The picker open: the finger holding it slides over its hexagons, or
    /// (opened by a key) a tap picks one.
    fn touch_picking(&mut self, input: &mut Input) {
        self.touch.pick_tap = input.fingers.iter().find(|f| f.new).map(|f| f.at);
        if let Some((id, _)) = self.touch.picking {
            if let Some(f) = input.fingers.iter().find(|f| f.id == id) {
                self.touch.picking = Some((id, f.at));
            } else if let Some(f) = input.lifted.iter().find(|f| f.id == id) {
                self.touch.picking = None;
                self.touch.pick_lifted = Some(f.at);
            }
        }
        input.clicked = false;
    }

    /// The finger holding the picker open (it picks when let go).
    pub(super) fn touch_holds_picker(&self) -> bool {
        self.touch.picking.is_some()
    }

    /// The picker's touch this frame, for `n` hexagons: the row under the
    /// finger, and whether to pick it now; `Err` closes it unpicked (let go
    /// off the hexagons).
    pub(super) fn touch_pick(&mut self, n: usize) -> Result<(Option<usize>, bool), ()> {
        let places = HuntKit::hex_places(self.screen, n);
        let under = |p: (f32, f32)| places.iter().position(|&r| inside(p, r));
        if let Some(at) = self.touch.pick_lifted.take() {
            return under(at).map(|r| (Some(r), true)).ok_or(());
        }
        if let Some(at) = self.touch.pick_tap.take() {
            return under(at).map(|r| (Some(r), true)).ok_or(());
        }
        Ok((self.touch.picking.and_then(|(_, at)| under(at)), false))
    }

    /// The hunt menu's hexagons and the pause, by touch.
    fn touch_menu(&mut self, input: &mut Input) {
        let Some(tap) = input.fingers.iter().find(|f| f.new).map(|f| f.at) else {
            return;
        };
        input.clicked = false;
        if self.hunt.show_exit() {
            let places = HuntKit::menu_places(self.screen, 3);
            match places.iter().position(|&r| inside(tap, r)) {
                Some(0) => self.restart(),
                Some(1) => self.hunt.exit_prompt = false,
                Some(2) => self.hunt.confirm_exit(),
                _ => {}
            }
        } else if self.hunt.paused {
            self.hunt.toggle_pause();
        }
    }

    /// The buttons over the hunt.
    pub(super) fn draw_touch(&self, out: &mut Vec<Sprite>) {
        let Some(k) = &self.kit else { return };
        if !self.touch.on {
            return;
        }
        let lay = self.touch_layout();
        let t = &k.touch;
        let round = |id: Option<usize>,
                     (x, y, d): (f32, f32, f32),
                     a: f32,
                     label: &str,
                     out: &mut Vec<Sprite>| {
            if let Some(id) = id {
                out.push(Sprite {
                    size: Some([d, d]),
                    color: [1.0, 1.0, 1.0, a],
                    ..Sprite::new(id, x - d / 2.0, y - d / 2.0, 1.0)
                });
            } else {
                k.disc_at((x, y, d), [0.95, 0.86, 0.25, a * 0.8], out);
                k.centred_text((x, y), (d * 0.3).round(), [0.1, 0.1, 0.1, a], label, out);
            }
        };
        if self.hunt.map_mode {
            // The way back out of the map.
            let (x, y, w, h) = lay.compass;
            round(None, (x + w / 2.0, y + h / 2.0, h * 0.9), 0.9, "map", out);
            return;
        }
        let (sx, sy, sd) = lay.stick;
        let a = if self.touch.stick.is_some() {
            0.95
        } else {
            0.7
        };
        round(t.stick, (sx, sy, sd), a, "", out);
        let (kx, ky) = self.touch.knob;
        round(t.knob, (sx + kx, sy + ky, sd * 0.43), a, "", out);
        if !self.touch_full() {
            return;
        }
        if self.touch_armed() {
            let a = if self.touch.fire.is_some() { 1.0 } else { 0.85 };
            round(t.fire, lay.left, a, "fire", out);
        } else {
            let a = if self.hunt.bin_mode { 1.0 } else { 0.85 };
            round(t.binoculars, lay.left, a, "bino", out);
        }
        let held = |kind| matches!(self.touch.hold, Some((_, k, _)) if k == kind);
        let a = if held(Pick::Call) { 1.0 } else { 0.85 };
        // The phone's call: a gold ring spreads from the button and fades.
        if let Some(t0) = self.touch.called {
            let f = t0.elapsed().as_secs_f32() / RIPPLE;
            if let (Some(id), true) = (t.call_circle, f < 1.0) {
                let (x, y, d) = lay.call;
                let d = d * (1.0 + 1.6 * f);
                out.push(Sprite {
                    size: Some([d, d]),
                    color: [1.0, 1.0, 1.0, 1.0 - f],
                    ..Sprite::new(id, x - d / 2.0, y - d / 2.0, 1.0)
                });
            }
        }
        round(t.call, lay.call, a, "call", out);
        let ws = &self.hunt.weapons;
        let w = ws.target;
        let a = if held(Pick::Weapon) || ws.pose().is_some() {
            1.0
        } else {
            0.7
        };
        let id = t.weapons.get(w).copied().flatten();
        round(id, lay.weapon, a, &(w + 1).to_string(), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_do_not_overlap_and_the_middle_looks() {
        let lay = Layout::new((1920, 1080), Vec::new());
        assert_eq!(lay.hit((960.0, 540.0), false, true), None);
        for (b, c) in [
            (lay.stick, Control::Stick),
            (lay.left, Control::Left),
            (lay.call, Control::Call),
            (lay.weapon, Control::Weapon),
        ] {
            assert_eq!(lay.hit((b.0, b.1), false, true), Some(c));
        }
        // The light buttons start under the weapon button.
        assert!(light_top((1920, 1080)) > lay.weapon.1 + lay.weapon.2 / 2.0);
    }

    #[test]
    fn the_map_takes_only_the_compass_and_the_tab() {
        let lay = Layout::new((1920, 1080), Vec::new());
        assert_eq!(lay.hit((lay.stick.0, lay.stick.1), true, true), None);
        let (x, y, w, h) = lay.compass;
        assert_eq!(
            lay.hit((x + w / 2.0, y + h / 2.0), true, true),
            Some(Control::Compass)
        );
    }
}
