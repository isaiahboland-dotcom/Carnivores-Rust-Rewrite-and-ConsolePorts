//! The trophy room's keys and prompts, trophy plaques, and the hunter's
//! kill record.

use super::*;

/// What the trophy room asks before doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ask {
    Remove,
    Lock,
}

impl HuntView {
    /// The trophy room: `[` / `]` move the trophy shown to the next free
    /// stand that way, R removes it, L locks or unlocks it (both asked
    /// first: Y / N).
    pub(super) fn trophy_keys(&mut self, input: &Input) {
        if let Some(a) = self.ask {
            if input.pressed(KeyCode::KeyY) {
                match a {
                    Ask::Remove => self.hunt.remove_current_trophy(),
                    Ask::Lock => {
                        let _ = self.hunt.toggle_lock_current();
                    }
                }
                self.ask = None;
            } else if input.pressed(KeyCode::KeyN) || input.pressed(KeyCode::Escape) {
                self.ask = None;
            }
            return;
        }
        let Some(slot) = self.hunt.trophy_body else {
            return;
        };
        let locked = self.hunt.room.bodies.get(slot).is_some_and(|b| b.locked());
        if input.pressed(KeyCode::KeyR) {
            if locked {
                self.message = Some(("Locked: L to unlock it first".into(), 3.0));
            } else {
                self.ask = Some(Ask::Remove);
            }
        }
        if input.pressed(KeyCode::KeyL) {
            self.ask = Some(Ask::Lock);
        }
        for (key, dir) in [(KeyCode::BracketLeft, -1), (KeyCode::BracketRight, 1)] {
            if input.pressed(key) && !self.hunt.move_current_trophy(self.view.area(), dir) {
                self.message = Some(("No free stand that way".into(), 2.0));
            }
        }
    }

    /// This hunt's kills into the hunter's record (`trophy0N.rec`).
    pub(super) fn record_kills(&mut self) {
        use c2_core::record::{Kill, Mode};
        let kills = std::mem::take(&mut self.hunt.kills);
        let wave = self.hunt.wave();
        if kills.is_empty() && wave == 0 {
            return;
        }
        let mode = if self.hunt.survival.is_some() {
            Mode::Survival
        } else {
            Mode::Hunt
        };
        let reg = self.setup.reg;
        let mut r = crate::save::load_record(reg);
        let area = crate::save::area_title(&self.setup.game_dir, self.hunt.area_no);
        let (y, mo, d, _, _) = self.hunt.clock;
        for k in kills {
            r.kills.push(Kill {
                mode,
                animal: self
                    .hunt
                    .herd
                    .species
                    .get(k.ctype)
                    .map_or_else(String::new, |s| s.info.name.clone()),
                score: k.score,
                range: k.range,
                weapon: self
                    .weapon_infos
                    .get(k.weapon)
                    .map_or_else(String::new, |w| w.name.clone()),
                area: area.clone(),
                date: (y << 20) + (mo << 10) + d,
            });
        }
        r.best_wave = r.best_wave.max(wave);
        if let Err(e) = crate::save::store_record(reg, &r) {
            tracing::error!("{e:#}");
        }
    }

    /// A trophy's plaque: its lines of label and value.
    pub(super) fn plaque_lines(&self, slot: usize) -> Vec<Vec<(&'static str, String)>> {
        let Some(b) = self.hunt.room.bodies.get(slot) else {
            return Vec::new();
        };
        let info = self
            .hunt
            .herd
            .species
            .get(b.ctype.max(0) as usize)
            .map(|s| &s.info);
        let (name, mass, length) = info.map_or((String::new(), 0.0, 0.0), |i| {
            (i.name.clone(), i.mass, i.length)
        });
        let weapon = self
            .weapon_infos
            .get(b.weapon.max(0) as usize)
            .map_or("", |w| w.name.as_str());
        let us = self.hunt.room.options.sys != 0;
        let (sc, date, time) = (b.scale, b.date, b.time);
        let (weight, len, range) = if us {
            (
                format!("{:.2}t", mass * sc * sc / 0.907),
                format!("{:.2}ft", length * sc / 0.3),
                format!("{:.1}ft", b.range / 0.3),
            )
        } else {
            (
                format!("{:.2}T", mass * sc * sc),
                format!("{:.2}m", length * sc),
                format!("{:.1}m", b.range),
            )
        };
        let (d, m, y) = (date & 255, (date >> 10) & 255, date >> 20);
        let date = if us {
            format!("{m}.{d}.{y}")
        } else {
            format!("{d}.{m}.{y}")
        };
        let time = format!("{}:{:02}", (time >> 10) & 255, time & 255);
        let lock = if b.locked() { "  (locked)" } else { "" };
        let area = crate::save::area_title(&self.setup.game_dir, b.area());
        vec![
            vec![("Name: ", name + lock)],
            vec![("Weight: ", weight + " "), ("Length: ", len)],
            vec![
                ("Weapon: ", format!("{weapon}    ")),
                ("Score: ", b.score.to_string()),
            ],
            vec![("Range: ", range + "  "), ("Area: ", area)],
            vec![("Date: ", date + "   "), ("Time: ", time)],
        ]
    }

    /// `DrawTrophyText`: the trophy's record on its plaque.
    pub(super) fn plaque_text(&self, slot: usize, x0: f32, y0: f32, scale: f32) -> Vec<Sprite> {
        let lines = self.plaque_lines(slot);
        let (grey, yellow) = ([0.75, 0.75, 0.75, 1.0], [0.75, 0.75, 0.0, 1.0]);
        let px = 8.0 * scale;
        let mut out = Vec::new();
        for (i, parts) in lines.iter().enumerate() {
            let y = y0 + (18.0 + 16.0 * i as f32) * scale;
            let mut x = x0 + 16.0 * scale;
            for (label, value) in parts.iter() {
                for (t, c) in [(*label, grey), (value.as_str(), yellow)] {
                    out.extend(
                        self.hud
                            .label(x + scale, y + scale, px, [0.0, 0.0, 0.0, 1.0], t),
                    );
                    out.extend(self.hud.label(x, y, px, c, t));
                    x += self.hud.label_width(px, t);
                }
            }
        }
        out
    }

    /// The trophy room's "Esc to exit" banner, top centre.
    pub(super) fn draw_trophy_banner(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        if !self.hunt.opts.trophy {
            return;
        }
        if let Some((id, w)) = self
            .trophy_exit_img
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32)))
        {
            let x = (screen.0 as f32 - w * scale) / 2.0;
            out.push(Sprite::new(id, x, 2.0 * scale, scale));
        }
    }

    /// The plaque of the trophy shown: the game's picture with its record,
    /// or the kit's.
    pub(super) fn draw_plaque(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let Some(slot) = self.hunt.trophy_body else {
            return;
        };
        if let Some((id, w, h)) = self
            .plaque_img
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
        {
            let y = screen.1 as f32 - (h + 12.0) * scale;
            let x = if self.hunt.opts.trophy {
                screen.0 as f32 - (w + 16.0) * scale
            } else {
                (screen.0 as f32 - w * scale) / 2.0
            };
            out.push(Sprite::new(id, x, y, scale));
            out.extend(self.plaque_text(slot, x, y, scale));
        } else if let Some(k) = &self.kit {
            let lines = self.plaque_lines(slot);
            k.plaque(&self.hud, screen, self.hunt.opts.trophy, &lines, out);
        }
    }

    /// The trophy room's question, or its hint.
    pub(super) fn draw_trophy_prompt(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        if !self.hunt.opts.trophy {
            return;
        }
        let t = match self.ask {
            Some(Ask::Remove) => "Remove this trophy?  Y / N".to_string(),
            Some(Ask::Lock) => {
                let locked = self
                    .hunt
                    .trophy_body
                    .and_then(|s| self.hunt.room.bodies.get(s))
                    .is_some_and(|b| b.locked());
                if locked {
                    "Unlock this trophy?  Y / N".into()
                } else {
                    "Lock this trophy (it can't be removed)?  Y / N".into()
                }
            }
            None if self.hunt.trophy_body.is_some() => "[ ] move   R remove   L lock".to_string(),
            None => String::new(),
        };
        if t.is_empty() {
            return;
        }
        let px = 8.0 * scale;
        let w = self.hud.label_width(px, &t) + 16.0 * scale;
        let (x, y) = ((screen.0 as f32 - w) / 2.0, 40.0 * scale);
        out.extend(self.hud.rect(x, y, w, 20.0 * scale, [0.0, 0.0, 0.0, 0.7]));
        let c = if self.ask.is_some() {
            [0.95, 0.85, 0.2, 1.0]
        } else {
            [0.8, 0.8, 0.8, 1.0]
        };
        out.extend(self.hud.label(x + 8.0 * scale, y + 6.0 * scale, px, c, &t));
    }
}
