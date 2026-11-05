//! Multi ammo's question (a setting; not in the original): picking the
//! tranquilizer asks **Half tranq** (each weapon's rounds split between
//! lethal ones and darts, swapped in the hunt) or **Full tranq** (darts
//! only, as the original's tranquilizer). The answer is kept in
//! `c2rs.cfg` (`half_tranq`).

use winit::keyboard::KeyCode;

use super::{inside, Menu, Rect, DIM, HOVER, PICKED, SAND, SHADOW, TEXT};
use crate::hud::Sprite;
use crate::input::Input;

/// The panel and its two buttons (Half, Full), canvas units.
const PANEL: Rect = (220.0, 210.0, 360.0, 170.0);
const BUTTONS: [Rect; 2] = [(245.0, 300.0, 145.0, 50.0), (410.0, 300.0, 145.0, 50.0)];

impl Menu {
    /// The tranquilizer on or off; turned on with multi ammo, it asks
    /// which.
    pub(super) fn toggle_tranq(&mut self) {
        let on = self.tranq();
        if let Some(s) = self.me_mut() {
            s.options.tranq = i32::from(!on);
        }
        self.tranq_ask = !on && self.pc.multi_ammo != 0;
    }

    /// The question's input: a button or H / F (1 / 2); Escape, or a click
    /// outside, takes the tranquilizer back off.
    pub(super) fn update_tranq_ask(&mut self, input: &Input) {
        let pick = if input.pressed(KeyCode::KeyH) || input.pressed(KeyCode::Digit1) {
            Some(true)
        } else if input.pressed(KeyCode::KeyF) || input.pressed(KeyCode::Digit2) {
            Some(false)
        } else if input.clicked {
            match BUTTONS.iter().position(|&b| inside(self.cursor, b)) {
                Some(i) => Some(i == 0),
                None if !inside(self.cursor, PANEL) => {
                    self.cancel_tranq_ask();
                    return;
                }
                None => None,
            }
        } else {
            None
        };
        if input.pressed(KeyCode::Escape) {
            self.cancel_tranq_ask();
            return;
        }
        if let Some(half) = pick {
            self.pc.half_tranq = i32::from(half);
            self.pc.store();
            self.tranq_ask = false;
        }
    }

    fn cancel_tranq_ask(&mut self) {
        if let Some(s) = self.me_mut() {
            s.options.tranq = 0;
        }
        self.tranq_ask = false;
    }

    pub(super) fn draw_tranq_ask(&self, out: &mut Vec<Sprite>) {
        self.fill(out, (0.0, 0.0, 800.0, 600.0), [0.0, 0.0, 0.0, 0.5]);
        self.fill(out, PANEL, [0.05, 0.07, 0.04, 0.95]);
        let title = "Tranquilizer ammo";
        let x = PANEL.0 + (PANEL.2 - self.text_width(16.0, title)) / 2.0;
        self.text(out, x, PANEL.1 + 18.0, 16.0, SAND, title);
        let note = "Half: swap lethal / darts in the hunt";
        let x = PANEL.0 + (PANEL.2 - self.text_width(12.0, note)) / 2.0;
        self.text(out, x, PANEL.1 + 52.0, 12.0, TEXT, note);
        let last = self.pc.half_tranq != 0;
        for (i, (b, label)) in BUTTONS.iter().zip(["Half tranq", "Full tranq"]).enumerate() {
            let hover = inside(self.cursor, *b);
            let border = if hover {
                HOVER
            } else if (i == 0) == last {
                PICKED
            } else {
                DIM
            };
            self.fill(out, *b, border);
            self.fill(out, (b.0 + 2.0, b.1 + 2.0, b.2 - 4.0, b.3 - 4.0), SHADOW);
            let x = b.0 + (b.2 - self.text_width(14.0, label)) / 2.0;
            self.text(out, x, b.1 + b.3 / 2.0 - 7.0, 14.0, border, label);
        }
        let keys = "H / F, Esc: no tranquilizer";
        let x = PANEL.0 + (PANEL.2 - self.text_width(11.0, keys)) / 2.0;
        self.text(out, x, PANEL.1 + PANEL.3 - 20.0, 11.0, DIM, keys);
    }
}
