//! The bottom screen's buttons for settings not in the original:
//!
//! - Multi ammo (Half tranq): an ammo switch under the map, "Switch" with
//!   the kind loaded ("lethal" / "tranq") under it; the other buttons
//!   narrow beside it. The weapon cards show both counts.
//! - Advanced observer mode: Fly and Time (stop / go) in the weapon
//!   column, which an observer has no weapons for.
//!
//! ```text
//!  74       132   170   208  246
//!  +--------+                      192
//!  | Switch | [Ammo][Bino][ II ]   212
//!  | lethal |                      236
//!  +--------+
//! ```

use super::*;

pub(super) const SWITCH: [f32; 4] = [74.0, 192.0, 56.0, 46.0];
const SLIM: [[f32; 4]; 3] = [
    [132.0, 212.0, 36.0, 24.0],
    [170.0, 212.0, 36.0, 24.0],
    [208.0, 212.0, 38.0, 24.0],
];
pub(super) const FLY: [f32; 4] = [4.0, 4.0, LEFT_W - 8.0, 30.0];
pub(super) const TIME: [f32; 4] = [4.0, 40.0, LEFT_W - 8.0, 30.0];
const BLUE: Rgba = [120, 180, 255, 255];

/// The under-map buttons' places: ammo (refills), binoculars, pause; with
/// multi ammo, narrowed beside the switch.
pub(super) fn buttons(h: &Hunt) -> [[f32; 4]; 3] {
    if h.weapons.multi_ammo() {
        SLIM
    } else {
        [AMMO, BINOC, PAUSE]
    }
}

/// A tap on one of these buttons.
pub(super) fn tap(h: &Hunt, x: f32, y: f32) -> Option<Tap> {
    if h.weapons.multi_ammo() && inside(SWITCH, x, y) {
        return Some(Tap::AmmoSwitch);
    }
    if h.free_observer() {
        if inside(FLY, x, y) {
            return Some(Tap::Fly);
        }
        if inside(TIME, x, y) {
            return Some(Tap::TimeStop);
        }
    }
    None
}

/// A weapon card's count: "x14", or "L7 T7" with multi ammo.
pub(super) fn count(h: &Hunt, i: usize) -> String {
    let w = &h.weapons;
    if w.multi_ammo() {
        let (l, t) = w.kinds(i);
        format!("L{l} T{t}")
    } else {
        format!("x{}", w.rounds(i))
    }
}

/// The buttons' panels (before the canvas is flushed).
pub(super) fn panels(c: &mut Canvas, h: &Hunt) {
    if h.weapons.multi_ammo() {
        c.rect(
            SWITCH,
            if h.weapons.fires_darts(h.weapons.current) {
                HILITE
            } else {
                PANEL
            },
        );
    }
    if h.free_observer() {
        c.rect(FLY, if h.flying { HILITE } else { PANEL });
        c.rect(TIME, if h.time_stopped { HILITE } else { PANEL });
    }
}

/// Their words.
pub(super) fn labels(c: &mut Canvas, gpu: &mut Gpu, h: &Hunt) {
    if h.weapons.multi_ammo() {
        let cx = SWITCH[0] + SWITCH[2] / 2.0;
        c.text_c(gpu, cx, SWITCH[1] + 8.0, 1.0, WHITE, "Switch");
        let darts = h.weapons.fires_darts(h.weapons.current);
        let (t, col) = if darts {
            ("tranq", BLUE)
        } else {
            ("lethal", GOLD)
        };
        c.text_c(gpu, cx, SWITCH[1] + 26.0, 1.0, col, t);
    }
    if h.free_observer() {
        for (r, t) in [
            (FLY, if h.flying { "Walk" } else { "Fly" }),
            (TIME, if h.time_stopped { "Go" } else { "Stop" }),
        ] {
            c.text_c(gpu, r[0] + r[2] / 2.0, r[1] + 11.0, 1.0, WHITE, t);
        }
        c.text_c(gpu, LEFT_W / 2.0, TIME[1] + 38.0, 1.0, GREY, "time");
    }
}
