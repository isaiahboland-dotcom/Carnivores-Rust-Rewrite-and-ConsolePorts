//! The hunt setup: area, animals, weapons and gear, picks and the hunt's start.

use super::*;

impl Menu {
    pub(super) fn picked_mask(&self, col: usize) -> Bits {
        let c = &self.choice;
        match col {
            0 => Bits::bit(c.area),
            1 => c.dinos,
            2 => Bits::from(c.weapons),
            _ => Bits::from(u128::from(c.equip)),
        }
    }

    /// Account left after paying for the choices.
    pub(super) fn remaining(&self) -> i32 {
        let score = self.me().map_or(0, |s| s.score);
        if self.choice.observer {
            return score;
        }
        if self.choice.survival.is_some() {
            return score
                - if self.pc.dynamic_time != 0 {
                    i32::from(self.choice.equip & 16 != 0) * 150
                        + i32::from(self.choice.equip & 32 != 0) * 15
                } else {
                    0
                };
        }
        let mut spent = 0;
        for (ci, c) in self.columns.iter().enumerate() {
            let mask = self.picked_mask(ci);
            for (i, p) in c.prices.iter().enumerate() {
                if mask.has(i) {
                    spent += p;
                }
            }
        }
        score - spent
    }

    pub(super) fn can_hunt(&self) -> bool {
        self.choice.area < self.areas.len()
            && (self.choice.observer
                || (!self.choice.dinos.is_empty() || self.choice.survival.is_some())
                    && self.choice.weapons != 0)
            && self.remaining() >= 0
            && self.within_picks()
    }

    /// The picks are within `--max-picks` (a survival brings its own
    /// animals and weapon, so it always is).
    pub(super) fn within_picks(&self) -> bool {
        let c = &self.choice;
        MAX_PICKS.get().is_none_or(|&cap| {
            c.survival.is_some()
                || (c.dinos.count() <= cap && c.weapons.count_ones() as usize <= cap)
        })
    }

    pub(super) fn row_at(&self, col: usize) -> Option<usize> {
        let c = &self.columns[col];
        let (x, y) = self.cursor?;
        if !inside(self.cursor, c.rect) {
            return None;
        }
        let _ = x;
        let r = ((y - c.rect.1 - 6.0) / ROW_H).floor();
        if r < 0.0 || r as usize >= c.visible() {
            return None;
        }
        c.order.get(r as usize + c.scroll).copied()
    }

    pub(super) fn update_setup(&mut self, input: &Input, region: u8) {
        let on_mode = inside(self.cursor, MODE_RECT) && self.choice.survival.is_some();
        let region = if on_mode { 0 } else { region };
        if input.clicked && on_mode {
            // Cycle only modes available for this install.
            let all = self.survival_modes();
            if let Some(k) = self.choice.survival {
                let i = all.iter().position(|&a| a == k).unwrap_or(0);
                self.choice.survival = Some(all[(i + 1) % all.len()]);
            }
            return;
        }
        if input.pressed(KeyCode::Escape) || (input.clicked && region == 7) {
            self.save_equipment();
            self.screen = Screen::Main;
            return;
        }
        // The wheel scrolls the list under the cursor.
        if input.scroll != 0.0 {
            let cursor = self.cursor;
            for c in &mut self.columns {
                if inside(cursor, c.rect) {
                    let max = c.names.len().saturating_sub(c.visible());
                    let step = if input.scroll > 0.0 { -1 } else { 1 };
                    c.scroll = c.scroll.saturating_add_signed(step).min(max);
                }
            }
        }
        // Hovering a list item shows it.
        let survival = self.choice.survival.is_some();
        for col in 0..self.columns.len() {
            if let Some(r) = self.row_at(col).filter(|_| !survival || col == 2) {
                self.choice.focus = Some((col, r));
            }
        }
        if !input.clicked {
            return;
        }
        // Survival launches directly from its mode chooser.
        if survival {
            if region == 8 && self.can_hunt() {
                self.launch_hunt();
            } else if let Some(r) = self.row_at(2) {
                self.pick(2, r);
            }
            return;
        }
        match region {
            1..=3 if crate::instance_rules::get().time_allowed(region as usize - 1) => {
                self.choice.time = region as usize - 1
            }
            4 => {
                self.pc.fog = i32::from(self.pc.fog == 0);
                self.pc.store();
            }
            5 => self.toggle_tranq(),
            6 => {
                self.toggle_observer();
                if self.choice.observer {
                    self.choice.survival = None;
                }
            }
            8 if self.can_hunt() => self.launch_hunt(),
            _ => {}
        }
        for col in 0..self.columns.len() {
            if let Some(r) = self.row_at(col) {
                self.pick(col, r);
            }
        }
    }

    /// Observer mode on or off (off: picks the account can't pay go).
    pub(super) fn toggle_observer(&mut self) {
        self.choice.observer = !self.choice.observer;
        if !self.choice.observer && self.remaining() < 0 {
            self.choice.dinos = Bits::NONE;
            self.choice.weapons = 0;
            self.choice.equip = 0;
        }
    }

    /// Picks (or drops) item `r` of list `col`, unless the account can't
    /// pay for it.
    pub(super) fn pick(&mut self, col: usize, r: usize) {
        // Picks are bit masks; animals go to the hunt as bits 10 and up.
        let max = match col {
            0 => self.areas.len(),
            1 => Bits::LEN - 10,
            2 => 128,
            _ => 32,
        };
        if r >= max || (col == 3 && r >= 4 && self.pc.dynamic_time == 0) {
            return;
        }
        let bit = 1u128.checked_shl(r as u32).unwrap_or(0);
        self.choice.full = None;
        // A full list takes no more (dropping a pick is always allowed).
        let adding = match col {
            1 => !self.choice.dinos.has(r),
            2 => self.choice.weapons & bit == 0,
            _ => false,
        };
        let count = match col {
            1 => self.choice.dinos.count(),
            _ => self.choice.weapons.count_ones() as usize,
        };
        if adding && MAX_PICKS.get().is_some_and(|&cap| count >= cap) {
            self.choice.focus = Some((col, r));
            self.choice.full = Some(col);
            return;
        }
        let price = self.columns[col].prices[r];
        let old = self.choice.clone();
        match col {
            0 => self.choice.area = r,
            1 => {
                if self.choice.dinos.has(r) {
                    self.choice.dinos.clear(r);
                } else {
                    self.choice.dinos.set(r);
                }
            }
            2 => self.choice.weapons ^= bit,
            _ => self.choice.equip ^= bit as u32,
        }
        // Can't spend more than the account holds.
        if self.remaining() < 0 && price > 0 {
            self.choice = Choice {
                focus: Some((col, r)),
                ..old
            };
        }
    }

    /// Starts the hunt the choices describe.
    pub(super) fn launch_hunt(&mut self) {
        debug_assert!(self.within_picks(), "hunt started over --max-picks");
        if self.choice.survival.is_none() {
            self.save_equipment();
        }
        let mut c = self.choice.clone();
        // Mobile survival: Mount Ravan, daylight, shotgun, no equipment.
        if c.survival.is_some() {
            c.area = self
                .areas
                .iter()
                .position(|p| {
                    p.file_stem()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("AREA5"))
                })
                .unwrap_or(c.area);
            c.time = 1;
            c.dinos = Bits::ALL;
            c.equip = 0;
        }
        if c.survival.is_none() {
            c.time = crate::instance_rules::get().allowed_time(c.time);
        }
        let time = [TimeOfDay::Dusk, TimeOfDay::Day, TimeOfDay::Night][c.time];
        // An observer's picked animals are placed and shown on the radar;
        // with none picked, the original's random mix.
        let targets = c.dinos.up(10);
        let weapons = if c.observer { 0 } else { c.weapons };
        let map = self.areas[c.area].clone();
        let double = !c.observer && c.equip & 8 != 0;
        self.start(map, time, targets, weapons, double, c.observer);
        if let Some(l) = &mut self.launch {
            l.survival = c.survival;
        }
    }

    /// APK-independent modes, with Classic hidden for standalone rosters
    /// and Hardcore only when its setting is on.
    pub(super) fn survival_modes(&self) -> Vec<SurvivalKind> {
        let mut v = Vec::new();
        if self.classic_survival {
            v.push(SurvivalKind::Classic);
        }
        v.push(SurvivalKind::Modded);
        if self.pc.hardcore_survival != 0 {
            v.push(SurvivalKind::Hardcore);
        }
        v
    }

    /// The `i`-th of `n` buttons of the survival chooser.
    pub(super) fn pick_rect(i: usize, n: usize) -> Rect {
        let h = 44.0;
        let top = 300.0 - (n as f32 * h) / 2.0;
        (300.0, top + i as f32 * h, 424.0, 36.0)
    }

    pub(super) fn start_survival(&mut self, kind: SurvivalKind) {
        self.choice.survival = Some(kind);
        self.choice.observer = false;
        self.choice.focus = None;
        self.choice.weapons = self.columns[2]
            .names
            .iter()
            .position(|n| n.trim().eq_ignore_ascii_case("shotgun"))
            .map_or(0, |i| 1u128 << i);
        self.choice.equip = 0;
        self.launch_hunt();
    }

    /// Keeps the accessory choice in the save, as the original does.
    pub(super) fn save_equipment(&mut self) {
        let e = self.choice.equip;
        if let Some(s) = self.me_mut() {
            let on = |bit: u32| i32::from(e & bit != 0);
            s.options.camo = on(1);
            s.options.radar = on(2);
            s.options.scent = on(4);
        }
        self.store_me();
    }

    pub(super) fn start(
        &mut self,
        map: PathBuf,
        time: TimeOfDay,
        targets: Bits,
        weapons: u128,
        double_ammo: bool,
        observer: bool,
    ) {
        let Some(reg) = self.me().map(|s| s.reg_number) else {
            return;
        };
        let (night_vision, flashlight) =
            crate::hunt_view::lights_bought(self.pc.dynamic_time != 0, self.choice.equip);
        self.launch = Some(HuntSetup {
            game_dir: self.game_dir.clone(),
            map,
            time,
            seed: None,
            targets,
            weapons,
            double_ammo,
            observer,
            survival: None,
            reg,
            from_menu: true,
            pc: self.pc,
            night_vision,
            flashlight,
        });
        self.shown_loading = false;
        self.screen = Screen::Loading;
    }

    pub(super) fn draw_setup(&self, out: &mut Vec<Sprite>, region: u8) {
        let on_mode = inside(self.cursor, MODE_RECT) && self.choice.survival.is_some();
        // A time of day the instance doesn't offer doesn't light up.
        let barred = (1..=3).contains(&region)
            && !crate::instance_rules::get().time_allowed(region as usize - 1);
        let region = if on_mode || barred { 0 } else { region };
        let c = &self.choice;
        let mut lit = vec![region, c.time as u8 + 1];
        if self.tranq() {
            lit.push(5);
        }
        if c.observer {
            lit.push(6);
        }
        self.board(out, &self.setup, &lit);
        // The original's empty box beside the times of day: the fog toggle.
        let on = self.pc.fog != 0;
        let fc = if on || region == 4 { PICKED } else { DIM };
        self.text(
            out,
            301.0 - self.text_width(14.0, "FOG") / 2.0,
            304.0,
            14.0,
            fc,
            "FOG",
        );
        let score = self.me().map_or(0, |s| s.score);
        let left = self.remaining();
        for (x, v) in [(332.0, score), (408.0, left)] {
            let t = v.to_string();
            let col = if v < 0 { [1.0, 0.3, 0.2, 1.0] } else { TEXT };
            self.text(
                out,
                x + (62.0 - self.text_width(14.0, &t)) / 2.0,
                42.0,
                14.0,
                col,
                &t,
            );
        }
        let us = self.me().is_some_and(|s| s.options.sys != 0);
        for (ci, col) in self.columns.iter().enumerate() {
            // A survival: only the guns; the rest is random.
            if c.survival.is_some() && ci != 2 {
                let note = ["Mount Ravan", "Wave animals", "Shotgun only", "-"][ci.min(3)];
                let note = if ci == 1 { "Random, dangerous" } else { note };
                self.text(out, col.rect.0 + 6.0, col.rect.1 + 6.0, 12.0, DIM, note);
                continue;
            }
            let mask = self.picked_mask(ci);
            let hover_row = self.row_at(ci);
            let shown = col.scroll..col.scroll + col.visible();
            for (row, &i) in col.order.iter().enumerate() {
                if !shown.contains(&row) {
                    continue;
                }
                let name = &col.names[i];
                let y = col.rect.1 + 6.0 + (row - col.scroll) as f32 * ROW_H;
                let picked = mask.has(i);
                let price = col.prices[i];
                let affordable = c.observer || picked || left - price >= 0;
                let color = if picked {
                    PICKED
                } else if !affordable {
                    DIM
                } else if hover_row == Some(i) {
                    HOVER
                } else {
                    TEXT
                };
                let p = price.to_string();
                let w = self.text_width(12.0, &p);
                // Long names give way to the price.
                let room = col.rect.2 - 22.0 - w;
                let mut name = name.clone();
                while self.text_width(12.0, &name) > room && name.pop().is_some() {}
                self.text(out, col.rect.0 + 6.0, y, 12.0, color, &name);
                self.text(out, col.rect.0 + col.rect.2 - 8.0 - w, y, 12.0, color, &p);
            }
            // More rows above / below: scroll with the wheel.
            let mid = col.rect.0 + col.rect.2 / 2.0 - 4.0;
            if col.scroll > 0 {
                self.text(out, mid, col.rect.1 - 6.0, 12.0, TEXT, "^");
            }
            if col.scroll + col.visible() < col.names.len() {
                self.text(out, mid, col.rect.1 + col.rect.3 - 8.0, 12.0, TEXT, "v");
            }
        }
        // Our survival switch (a survival only).
        let mode = c.survival.map_or("", |k| match k {
            SurvivalKind::Classic => "CLASSIC SURVIVAL",
            SurvivalKind::Modded => "MODDED SURVIVAL",
            SurvivalKind::Hardcore => "HARDCORE SURVIVAL",
        });
        let mc = if on_mode {
            HOVER
        } else if c.survival.is_some() {
            PICKED
        } else {
            DIM
        };
        let (mx, my, mw, _) = MODE_RECT;
        let mx = mx + (mw - self.text_width(14.0, mode)) / 2.0;
        self.text(out, mx, my + 8.0, 14.0, mc, mode);
        let mode_text: Vec<String> = [
            "Survival (click to change the kind).",
            "",
            "Waves of dangerous animals, picked",
            "at random, come for you, more each",
            "wave, in Mount Ravan's ravine. Health and",
            "ammunition come back between waves.",
            "Free; no trophies or score.",
            "Budget: twice cheapest attacker; +10% per round.",
            "Classic: base-game animals only.",
            "Modded: all eligible installed animals.",
        ]
        .map(String::from)
        .to_vec();
        // The view and info panels: the focused item, or a hovered toggle.
        let (pic, text): (Option<usize>, &[String]) = match region {
            _ if on_mode || (c.survival.is_some() && c.focus.is_none()) => (None, &mode_text),
            1..=3 => (None, &self.day_texts[region as usize - 1]),
            4 => (None, &self.fog_text),
            5 => (None, &self.tranq_text),
            6 => (None, &self.observe_text),
            _ => match c.focus {
                Some((ci, r)) => {
                    let col = &self.columns[ci];
                    let t = if us { &col.texts_us } else { &col.texts };
                    (
                        col.pics.get(r).copied().flatten(),
                        t.get(r).map_or(&[][..], |v| &v[..]),
                    )
                }
                None => (None, &self.day_texts[c.time]),
            },
        };
        self.image_native(out, pic, 38.0, 74.0);
        for (i, l) in text.iter().enumerate().take(11) {
            self.text(out, 416.0, 80.0 + i as f32 * 17.0, 12.0, TEXT, l);
        }
        let cap = MAX_PICKS.get().copied().unwrap_or(0);
        if !self.can_hunt() && region == 8 {
            let why = if !self.within_picks() {
                format!("At most {cap} animals and {cap} weapons")
            } else if self.choice.dinos.is_empty() && self.choice.survival.is_none() {
                "Choose dinosaurs to hunt".into()
            } else {
                "Choose a weapon".into()
            };
            self.text(out, 420.0, 566.0, 12.0, HOVER, &why);
        } else if let Some(col) = c.full {
            let what = if col == 1 { "animals" } else { "weapons" };
            let note = format!("Limit reached: {cap} {what} per hunt");
            self.text(out, 420.0, 566.0, 12.0, HOVER, &note);
        }
    }
}
