//! The pause and exit prompts, and the hint bars: under a prompt, the open
//! map's, or the controls' (a PC setting).

use super::*;

impl HuntView {
    /// The pause and exit prompts and the one hint bar that fits.
    pub(super) fn draw_prompts(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let pause_below = self.draw_pause(screen, scale, out);
        if self.hunt.show_exit() {
            self.draw_exit(screen, scale, out);
        } else if self.hunt.paused {
            let below = pause_below.unwrap_or_else(|| {
                self.pause_img
                    .and_then(|i| self.hud.image(i))
                    .map_or(screen.1 as f32 / 2.0, |im| {
                        (screen.1 as f32 + im.h as f32 * scale) / 2.0
                    })
            });
            let hint: &[(&str, &str)] = if self.pad_last {
                &[("+", "Resume"), ("-", "Exit menu")]
            } else {
                &[("P", "Resume"), ("Esc", "Exit menu")]
            };
            self.hint_bar(out, screen, scale, below + 8.0 * scale, true, hint);
        } else if self.hunt.map_mode {
            let y = screen.1 as f32 - 28.0 * scale;
            self.hint_bar(out, screen, scale, y, true, self.map_hints());
        } else if self.setup.pc.hints != 0 && self.hunt.death.is_none() && !self.touch.on {
            self.draw_control_hints(screen, scale, out);
        }
    }

    /// Paused: `PAUSE.TGA`, or the kit's popup (not under the exit
    /// prompt), whose bottom edge it returns.
    fn draw_pause(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) -> Option<f32> {
        if !self.hunt.paused {
            return None;
        }
        if let Some((id, w, h)) = self
            .pause_img
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
        {
            let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
            let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
            out.push(Sprite::new(id, x, y, scale));
            return None;
        }
        self.kit
            .as_ref()
            .filter(|_| !self.hunt.show_exit())
            .map(|k| k.popup(&self.hud, screen, "Paused", out))
    }

    /// Asking to leave: `EXIT.TGA` and its hints, or the kit's menu.
    fn draw_exit(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let mut below = screen.1 as f32 / 2.0;
        if let Some((id, w, h)) = self
            .exit_img
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
        {
            let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
            let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
            out.push(Sprite::new(id, x, y, scale));
            below = y + h as f32 * scale;
        } else if let Some(k) = &self.kit {
            // The phone's menu: hexagons along the top, each with its
            // key.
            let keys = if self.pad_last {
                ["Y", "B", "A"]
            } else {
                ["R", "N", "Y"]
            };
            let items = [
                ("RESTART", keys[0]),
                ("RESUME", keys[1]),
                ("ESCAPE!", keys[2]),
            ];
            k.menu(&self.hud, screen, &items, out);
            below = f32::NAN;
        }
        let hint: &[(&str, &str)] = if self.pad_last {
            &[("A", "Leave"), ("Y", "Restart"), ("B", "Resume")]
        } else {
            &[("Y", "Leave"), ("R", "Restart"), ("N", "Resume")]
        };
        if !below.is_nan() {
            self.hint_bar(out, screen, scale, below + 8.0 * scale, true, hint);
        }
    }

    /// The open map's hints: pad or keys, zoomable or not, teleporting or
    /// not.
    fn map_hints(&self) -> &'static [(&'static str, &'static str)] {
        let zoom = self.setup.pc.map_zoom != 0;
        let teleport = self.hunt.can_teleport();
        if self.pad_last && !teleport {
            if zoom {
                &[("R-stick", "Pointer"), ("R", "Close")]
            } else {
                &[("R", "Close")]
            }
        } else if self.pad_last {
            &[
                ("D-pad / R-stick", "Move"),
                ("ZR", "Mark"),
                ("A", "Go"),
                ("R", "Close"),
            ]
        } else if !zoom {
            if teleport {
                &[
                    ("Mouse", "Move"),
                    ("Click", "Mark"),
                    ("Enter", "Go"),
                    ("Tab", "Close"),
                ]
            } else {
                &[("Tab", "Close")]
            }
        } else if !teleport {
            &[
                ("Wheel", "Zoom"),
                ("Right-drag", "Pan"),
                ("Home", "Full map"),
                ("Tab", "Close"),
            ]
        } else {
            &[
                ("Wheel", "Zoom"),
                ("Right-drag", "Pan"),
                ("Click", "Mark"),
                ("Enter", "Go"),
                ("Tab", "Close"),
            ]
        }
    }

    /// The controls' hints (bottom right), with the keys as bound or the
    /// pad's buttons.
    fn draw_control_hints(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let k = &self.hunt.room.options.keys;
        let pick = format!("1-6 / {}", keys_name(self.pick_key()));
        let keyboard = [
            ("Tab".to_string(), "Map"),
            (keys::name(k.binoculars), "Binoculars"),
            (pick, "Next gun (hold: choose)"),
            (keys::name(k.change_call), "Next call (hold: choose)"),
            (keys::name(k.jump), "Jump"),
            (keys::name(k.crouch), "Crouch"),
        ];
        let pad = [
            ("R".to_string(), "Map"),
            ("L".to_string(), "Binoculars"),
            ("Y".to_string(), "Switch gun"),
            ("D-pad down".to_string(), "Next call"),
            ("A".to_string(), "Jump"),
            ("D-pad up".to_string(), "Crouch"),
        ];
        let mut rows: Vec<(String, &str)> = if self.pad_last {
            pad.to_vec()
        } else {
            keyboard.to_vec()
        };
        rows.extend(self.light_hints(self.pad_last));
        rows.extend(self.extra_hints(self.pad_last));
        let rows: Vec<(&str, &str)> = rows.iter().map(|(b, a)| (b.as_str(), *a)).collect();
        let y = screen.1 as f32 - 28.0 * scale;
        self.hint_bar(out, screen, scale, y, false, &rows);
    }

    /// A strip of `(button, action)` hints: centred at `y`, or (not
    /// `centre`) at the right edge; buttons gold, actions pale.
    pub(super) fn hint_bar(
        &self,
        out: &mut Vec<Sprite>,
        screen: (u32, u32),
        scale: f32,
        y: f32,
        centre: bool,
        hints: &[(&str, &str)],
    ) {
        let px = 8.0 * scale;
        let gap = 14.0 * scale;
        let space = self.hud.label_width(px, " ");
        let width: f32 = hints
            .iter()
            .map(|(b, a)| self.hud.label_width(px, b) + space + self.hud.label_width(px, a))
            .sum::<f32>()
            + gap * hints.len().saturating_sub(1) as f32;
        let pad = 8.0 * scale;
        let x0 = if centre {
            (screen.0 as f32 - width) / 2.0
        } else {
            screen.0 as f32 - width - pad - 8.0 * scale
        };
        out.extend(self.hud.rect(
            x0 - pad,
            y - 6.0 * scale,
            width + 2.0 * pad,
            20.0 * scale,
            [0.0, 0.0, 0.0, 0.6],
        ));
        let mut x = x0;
        for (b, a) in hints {
            out.extend(self.hud.label(x, y, px, [0.94, 0.78, 0.27, 1.0], b));
            x += self.hud.label_width(px, b) + space;
            out.extend(self.hud.label(x, y, px, [0.85, 0.85, 0.8, 1.0], a));
            x += self.hud.label_width(px, a) + gap;
        }
    }
}
