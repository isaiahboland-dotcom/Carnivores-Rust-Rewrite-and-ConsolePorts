//! The main menu, its statistics panel and the phone's MORE page.

use super::*;

impl Menu {
    pub(super) fn update_main(&mut self, input: &Input, region: u8) {
        if input.pressed(KeyCode::Escape) {
            if self.survival_pick {
                self.survival_pick = false;
            } else if self.more {
                (self.more, self.stats) = (false, false);
            } else {
                self.screen = Screen::Quit;
            }
            return;
        }
        if !input.clicked {
            return;
        }
        if self.survival_pick {
            // Which survival: a button each, elsewhere closes.
            let modes = self.survival_modes();
            let hit =
                (0..modes.len()).find(|&i| inside(self.cursor, Self::pick_rect(i, modes.len())));
            self.survival_pick = false;
            if let Some(i) = hit {
                self.start_survival(modes[i]);
            }
            return;
        }
        let survive = if self.kit.is_some() {
            region == 7
        } else {
            inside(self.cursor, SURVIVAL_BUTTON)
        };
        if survive {
            self.survival_pick = !self.survival_modes().is_empty();
            return;
        }
        // The name field: change hunter.
        if (self.kit.is_none() || self.more) && inside(self.cursor, (84.0, 4.0, 272.0, 34.0)) {
            self.name = self.me().map(|p| p.player_name()).unwrap_or_default();
            self.picked = self.player;
            self.screen = Screen::Register;
            return;
        }
        match region {
            1 => {
                self.choice.survival = None;
                self.screen = Screen::Setup;
            }
            2 => self.screen = Screen::Options,
            3 => {
                if let Some(trophy) =
                    find_file(&self.huntdat, "AREAS").and_then(|a| find_file(&a, "TROPHY.MAP"))
                {
                    self.start(trophy, TimeOfDay::Day, Bits::NONE, 0, false, false);
                }
            }
            4 => self.screen = Screen::Credits,
            5 => self.screen = Screen::Quit,
            8 => self.more = true,
            9 => (self.more, self.stats) = (false, false),
            6 => {
                self.stats = !self.stats;
                if let (true, Some(p)) = (self.stats, self.me()) {
                    self.record = crate::save::load_record(p.reg_number);
                }
            }
            _ => {}
        }
    }

    pub(super) fn main_board(&self) -> &Board {
        if self.stats {
            &self.stats_board
        } else if self.more {
            &self.more_board
        } else {
            &self.main
        }
    }

    pub(super) fn draw_main(&self, out: &mut Vec<Sprite>, region: u8) {
        let b = self.main_board();
        let mut lit = vec![region];
        if self.stats {
            lit.push(6);
        }
        self.board(out, b, &lit);
        let Some(p) = self.me() else { return };
        let kit = self.kit.is_some();
        if !kit || self.more {
            self.text(out, 92.0, 14.0, 16.0, SAND, &p.player_name());
            self.text(out, 474.0, 14.0, 16.0, SAND, &p.score.to_string());
        }
        // Survival: choose Classic/Modded, then launch immediately.
        let modes = self.survival_modes();
        let mut buttons = Vec::new();
        if self.survival_pick {
            self.fill(out, (0.0, 0.0, 1024.0, 768.0), [0.0, 0.0, 0.0, 0.5]);
            for (i, k) in modes.iter().enumerate() {
                let label = match k {
                    SurvivalKind::Classic => "CLASSIC",
                    SurvivalKind::Modded => "MODDED",
                    SurvivalKind::Hardcore => "HARDCORE",
                };
                buttons.push((Self::pick_rect(i, modes.len()), label));
            }
        } else if !modes.is_empty() && !kit {
            buttons.push((SURVIVAL_BUTTON, "SURVIVAL"));
        }
        for (r, label) in buttons {
            self.fill(out, r, [0.0, 0.0, 0.0, 0.6]);
            let col = if inside(self.cursor, r) { HOVER } else { SAND };
            let x = r.0 + (r.2 - self.text_width(16.0, label)) / 2.0;
            self.text(out, x, r.1 + 7.0, 16.0, col, label);
        }
        if !self.stats {
            return;
        }
        // The panel: the PC's at the right edge, the phone's wider.
        let (sx, sw) = if kit { (548.0, 252.0) } else { (612.0, 186.0) };
        let us = p.options.sys != 0;
        let rank = ["Beginner", "Advanced", "Expert"]
            .get(p.rank.clamp(0, 2) as usize)
            .copied()
            .unwrap_or("");
        for (y0, st, extra) in [
            (74.0, &p.last, None),
            (204.0, &p.total, Some(format!("Rank: {rank}"))),
        ] {
            // `path` is in half metres (distance / 128 units).
            let metres = st.path * 2.0;
            let path = if us {
                format!("{:.2} mi", metres / 1609.3)
            } else {
                format!("{:.2} km", metres / 1000.0)
            };
            let t = st.time.max(0.0) as i32;
            let lines = [
                format!("Path: {path}"),
                format!("Time: {}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60),
                format!("Shots: {}", st.shots_made),
                format!("Success: {}", st.success),
            ];
            for (i, l) in lines.iter().chain(extra.iter()).enumerate() {
                self.text(out, sx, y0 + i as f32 * 18.0, 12.0, SAND, l);
            }
        }
        // Records (ours: `trophy0N.rec`).
        use c2_core::record::Mode;
        let r = &self.record;
        let dist = |m: f32| {
            if us {
                format!("{:.0} ft", m / 0.3)
            } else {
                format!("{m:.0} m")
            }
        };
        let short = |s: &str| s.chars().take(12).collect::<String>();
        let mut lines = vec![("Records".to_string(), PICKED)];
        if let Some(k) = r.longest() {
            lines.push((format!("Longest: {}", dist(k.range)), SAND));
            lines.push((
                format!("  {} ({})", short(&k.animal), short(&k.weapon)),
                TEXT,
            ));
        }
        if let Some(k) = r.best() {
            lines.push((format!("Best kill: {}", k.score), SAND));
            lines.push((format!("  {}", short(&k.animal)), TEXT));
        }
        lines.push((format!("Kills: {} hunt", r.count(Mode::Hunt)), SAND));
        lines.push((format!("  {} survival", r.count(Mode::Survival)), SAND));
        lines.push((format!("Best wave: {}", r.best_wave), SAND));
        let h = lines.len() as f32 * 18.0 + 10.0;
        self.fill(out, (sx - 8.0, 322.0, sw, h), [0.0, 0.0, 0.0, 0.65]);
        for (i, (l, c)) in lines.iter().enumerate() {
            self.text(out, sx, 330.0 + i as f32 * 18.0, 12.0, *c, l);
        }
    }
}
