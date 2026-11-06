//! The phone's hunt buttons, for a game built from the player's APK, while
//! the last input was a touch (`Input::touch_mode`; a key or a pad hides
//! them, as controller-friendly phone games do). Drawn with the phone
//! game's own pictures (`HuntKit::touch`):
//!
//! - the move stick, bottom left (pushed all the way: run);
//! - a drag anywhere else looks around;
//! - fire, bottom right, and a small second fire button over the stick;
//! - one button per carried weapon: a tap takes it out, a tap on the one in
//!   hand puts it away;
//! - binoculars; the call (a tap calls, holding it changes the call);
//! - the compass opens the map (and closes it again); the "menu" tab is
//!   Escape.
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
    fire: Vec<u64>,
    /// The call button held: since when, and whether holding changed the call.
    call: Option<(u64, Instant, bool)>,
    /// The stick pushed all the way.
    pub(super) run: bool,
}

impl Touch {
    fn release(&mut self) {
        let on = self.on;
        *self = Self::default();
        self.on = on;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Control {
    Stick,
    Fire,
    Binoculars,
    Call,
    Compass,
    Tab,
    Weapon(usize),
    /// The light buttons (read by `lights`).
    Light,
}

/// Places in screen pixels: circles are (centre x, y, diameter).
struct Layout {
    stick: (f32, f32, f32),
    fire: (f32, f32, f32, f32),
    alt_fire: (f32, f32, f32),
    binoculars: (f32, f32, f32),
    call: (f32, f32, f32),
    compass: (f32, f32, f32, f32),
    tab: (f32, f32, f32, f32),
    weapons: Vec<(usize, (f32, f32, f32))>,
    lights: Vec<(f32, f32, f32)>,
}

/// The fire button's picture is 216 x 136.
const FIRE_ASPECT: f32 = 216.0 / 136.0;

fn inside((px, py): (f32, f32), (x, y, w, h): (f32, f32, f32, f32)) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

fn in_circle((px, py): (f32, f32), (x, y, d): (f32, f32, f32), slack: f32) -> bool {
    (px - x).hypot(py - y) <= d / 2.0 * slack
}

impl Layout {
    fn new(screen: (u32, u32), weapons: &[usize], lights: Vec<(f32, f32, f32)>) -> Self {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let m = sh * 0.035;
        let r = sh * 0.16;
        let stick = (m + r + sh * 0.04, sh - m - r - sh * 0.02, r * 2.0);
        let fh = sh * 0.22;
        let fw = fh * FIRE_ASPECT;
        let fire = (sw - m - fw, sh - m - fh, fw, fh);
        let alt_fire = (stick.0 + r * 1.3, stick.1 - r * 1.35, sh * 0.13);
        let d = sh * 0.12;
        let above = fire.1 - d * 0.7;
        let binoculars = (fire.0 + d * 0.5, above, d);
        let call = (fire.0 + fw - d * 0.5, above, d);
        // Weapons along the bottom, left of the fire button, four to a row,
        // in the weapons' order from the left.
        let wd = sh * 0.115;
        let n = weapons.len();
        let weapons = weapons
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let row_len = (n - i / 4 * 4).min(4);
                let from_right = (row_len - 1 - i % 4) as f32;
                let row = (i / 4) as f32;
                let x = fire.0 - m - wd / 2.0 - from_right * wd * 1.12;
                let y = sh - m - wd / 2.0 - row * wd * 1.12;
                (w, (x, y, wd))
            })
            .collect();
        Self {
            stick,
            fire,
            alt_fire,
            binoculars,
            call,
            compass: HuntKit::compass_rect(screen),
            tab: HuntKit::tab_rect(screen),
            weapons,
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
        if inside(p, self.fire) || in_circle(p, self.alt_fire, 1.15) {
            return Some(Control::Fire);
        }
        if in_circle(p, self.binoculars, 1.15) {
            return Some(Control::Binoculars);
        }
        if in_circle(p, self.call, 1.15) {
            return Some(Control::Call);
        }
        self.weapons
            .iter()
            .find(|&&(_, b)| in_circle(p, b, 1.08))
            .map(|&(w, _)| Control::Weapon(w))
    }
}

impl HuntView {
    /// Something over the hunt takes taps as clicks (its own buttons).
    fn touch_popup(&self) -> bool {
        let h = &self.hunt;
        h.paused || h.show_exit() || h.death.is_some() || self.picker.is_some()
    }

    /// The full set (fire, weapons, calls), not just walking and looking
    /// (the trophy room, observing).
    fn touch_full(&self) -> bool {
        !self.hunt.opts.trophy && !self.hunt.opts.observer
    }

    fn touch_layout(&self) -> Layout {
        let weapons = if self.touch_full() {
            self.carried()
        } else {
            Vec::new()
        };
        let lights = self.light_buttons().into_iter().map(|(_, b)| b).collect();
        Layout::new(self.screen, &weapons, lights)
    }

    /// The touches as pad actions, before the hunt reads its input.
    pub(super) fn touch_input(&mut self, input: &mut Input) {
        self.touch.on = self.kit.is_some() && input.touch_mode && !self.touch_popup();
        if !self.touch.on {
            self.touch.release();
            return;
        }
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
                Some(Control::Fire) => self.touch.fire.push(f.id),
                Some(Control::Binoculars) => pad.press(A::Binoculars),
                Some(Control::Call) => self.touch.call = Some((f.id, Instant::now(), false)),
                Some(Control::Compass) => pad.press(A::Map),
                Some(Control::Tab) => pad.press(A::Exit),
                Some(Control::Weapon(w)) => {
                    let ws = &self.hunt.weapons;
                    if ws.current == w && ws.pose().is_some() {
                        self.pending.toggle_weapon = true;
                    } else {
                        self.pending.select_weapon = Some(w);
                    }
                }
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
            self.touch.fire.retain(|&id| id != f.id);
            if let Some((id, _, changed)) = self.touch.call {
                if id == f.id {
                    if !changed {
                        pad.press(A::Call);
                    }
                    self.touch.call = None;
                }
            }
        }
        if let Some((_, since, changed)) = &mut self.touch.call {
            if !*changed && since.elapsed().as_secs_f32() >= HOLD_TO_PICK {
                *changed = true;
                self.pending.change_call = true;
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
        pad.hold(A::Fire, !self.touch.fire.is_empty());
        // The first finger no longer acts as the mouse here.
        input.clicked = false;
        input.mouse_down = false;
        input.dragging = false;
        input.mouse_delta = look;
        input.pad.merge(&pad);
    }

    /// The buttons over the hunt.
    pub(super) fn draw_touch(&self, out: &mut Vec<Sprite>) {
        let Some(k) = &self.kit else { return };
        if !self.touch.on {
            return;
        }
        let lay = self.touch_layout();
        let t = &k.touch;
        let pic = |id: Option<usize>,
                   (x, y, w, h): (f32, f32, f32, f32),
                   a: f32,
                   out: &mut Vec<Sprite>| {
            if let Some(id) = id {
                out.push(Sprite {
                    size: Some([w, h]),
                    color: [1.0, 1.0, 1.0, a],
                    ..Sprite::new(id, x, y, 1.0)
                });
                true
            } else {
                false
            }
        };
        let round = |id, (x, y, d): (f32, f32, f32), a, label: &str, out: &mut Vec<Sprite>| {
            if !pic(id, (x - d / 2.0, y - d / 2.0, d, d), a, out) {
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
        let held = self.touch.stick.is_some();
        let a = if held { 0.95 } else { 0.7 };
        round(t.stick, (sx, sy, sd), a, "", out);
        let kd = sd * 0.43;
        round(
            t.knob,
            (sx + self.touch.knob.0, sy + self.touch.knob.1, kd),
            a,
            "",
            out,
        );
        if !self.touch_full() {
            return;
        }
        let firing = !self.touch.fire.is_empty();
        let a = if firing { 1.0 } else { 0.8 };
        if !pic(t.fire, lay.fire, a, out) {
            let (x, y, w, h) = lay.fire;
            round(None, (x + w / 2.0, y + h / 2.0, h), a, "fire", out);
        }
        round(t.alt_fire, lay.alt_fire, a, "fire", out);
        let bino = self.hunt.bin_mode;
        round(
            t.binoculars,
            lay.binoculars,
            if bino { 1.0 } else { 0.8 },
            "bino",
            out,
        );
        let calling = self.touch.call.is_some();
        round(
            t.call,
            lay.call,
            if calling { 1.0 } else { 0.8 },
            "call",
            out,
        );
        let ws = &self.hunt.weapons;
        for &(w, place) in &lay.weapons {
            let in_hand = ws.current == w && ws.pose().is_some();
            let label = (w + 1).to_string();
            let id = t.weapons.get(w).copied().flatten();
            round(id, place, if in_hand { 1.0 } else { 0.55 }, &label, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_do_not_overlap_and_the_middle_looks() {
        let lay = Layout::new((1920, 1080), &[0, 1, 2, 3, 4, 5, 6, 7], Vec::new());
        assert_eq!(lay.hit((960.0, 540.0), false, true), None);
        let (sx, sy, _) = lay.stick;
        assert_eq!(lay.hit((sx, sy), false, true), Some(Control::Stick));
        let (fx, fy, fw, fh) = lay.fire;
        assert_eq!(
            lay.hit((fx + fw / 2.0, fy + fh / 2.0), false, true),
            Some(Control::Fire)
        );
        for &(w, (x, y, _)) in &lay.weapons {
            assert_eq!(lay.hit((x, y), false, true), Some(Control::Weapon(w)));
        }
        let (bx, by, _) = lay.binoculars;
        assert_eq!(lay.hit((bx, by), false, true), Some(Control::Binoculars));
        let (cx, cy, _) = lay.call;
        assert_eq!(lay.hit((cx, cy), false, true), Some(Control::Call));
        // The leftmost weapon stays clear of the stick.
        let left = lay.weapons.iter().map(|&(_, (x, _, d))| x - d / 2.0);
        let left = left.fold(f32::MAX, f32::min);
        assert!(left > sx + lay.stick.2 / 2.0 * 1.3);
    }

    #[test]
    fn the_map_takes_only_the_compass_and_the_tab() {
        let lay = Layout::new((1920, 1080), &[0], Vec::new());
        let (sx, sy, _) = lay.stick;
        assert_eq!(lay.hit((sx, sy), true, true), None);
        let (x, y, w, h) = lay.compass;
        assert_eq!(
            lay.hit((x + w / 2.0, y + h / 2.0), true, true),
            Some(Control::Compass)
        );
    }
}
