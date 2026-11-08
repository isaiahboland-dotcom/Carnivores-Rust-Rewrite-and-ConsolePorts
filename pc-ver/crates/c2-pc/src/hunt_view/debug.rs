//! Debug and screenshot helpers (`c2-pc hunt --sim/--die/--kill/...`):
//! states set up without input before a frame is shot.

use super::*;

impl HuntView {
    /// Debug/screenshot helper: simulate `ms` with no input, then stand the
    /// hunter `dist` units from the first animal with AI `ai_id`, facing it;
    /// `after_ms` more pass with the camera fixed.
    pub fn spectate(&mut self, ms: u32, ai_id: i32, dist: f32, after_ms: u32) {
        let idle = HuntInput::default();
        for _ in 0..ms / TICK_MS {
            self.step(&idle);
        }
        // By its species' AI number (an extra one runs as another), else
        // the AI it runs.
        let herd = &self.hunt.herd;
        let species_ai =
            |ch: &c2_core::dino::Character| herd.species.get(ch.ctype).map(|s| s.info.ai);
        let Some(d) = herd
            .chars
            .iter()
            .find(|ch| species_ai(ch) == Some(ai_id))
            .or_else(|| herd.chars.iter().find(|ch| ch.ai == ai_id))
            .cloned()
        else {
            tracing::warn!("no character with ai {ai_id}");
            return;
        };
        // Beside it; in the trophy room, where its plaque shows (2.5 cells
        // in front of it).
        let (x, z) = if self.hunt.opts.trophy {
            (d.pos[0] + d.lookx * 640.0, d.pos[2] + d.lookz * 640.0)
        } else {
            (d.pos[0] + d.lookz * dist, d.pos[2] - d.lookx * dist)
        };
        let area = self.view.area();
        let p = &mut self.hunt.player;
        p.x = x;
        p.z = z;
        p.y = area.land_qh(x, z, f32::MAX);
        p.alpha = f32::atan2(d.pos[0] - x, -(d.pos[2] - z));
        p.beta = 0.1;
        let keep = (p.x, p.y, p.z, p.alpha, p.beta);
        for _ in 0..after_ms / TICK_MS {
            self.step(&idle);
            let p = &mut self.hunt.player;
            (p.x, p.y, p.z, p.alpha, p.beta) = keep;
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Debug/screenshot helper: bag the first animal with AI `ai_id`, run
    /// `ms`, then look at the ship from `dist` away.
    pub fn watch_ship(&mut self, ai_id: i32, ms: u32, dist: f32) {
        if let Some(i) = self
            .hunt
            .herd
            .chars
            .iter()
            .position(|c| c.ai == ai_id && c.health > 0)
        {
            self.hunt.debug_bag(self.view.area(), i, &mut self.events);
        }
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        let s = self.hunt.ship.pos;
        tracing::info!(
            "ship state {} at {:?} alpha {:.2}",
            self.hunt.ship.state,
            s,
            self.hunt.ship.alpha
        );
        let p = &mut self.hunt.player;
        p.x = s[0] + dist;
        p.z = s[2];
        p.y = s[1] - 400.0;
        p.alpha = -std::f32::consts::FRAC_PI_2;
        p.beta = 0.0;
        self.prev_cam = self.hunt.camera().0;
    }

    /// `--die AI,MS` (screenshots of the death camera): an animal of `ai_id`
    /// is put beside the hunter and set hunting; the hunt runs until it kills
    /// (at most 30 s), then `ms` more.
    pub fn die_by(&mut self, ai_id: i32, ms: u32, name: Option<&str>) {
        let (x, y, z) = (self.hunt.player.x, self.hunt.player.y, self.hunt.player.z);
        let alpha = self.hunt.player.alpha;
        let species = &self.hunt.herd.species;
        let named = |c: &c2_core::dino::Character| {
            name.is_none_or(|n| {
                species
                    .get(c.ctype)
                    .is_some_and(|s| s.info.name.eq_ignore_ascii_case(n))
            })
        };
        let Some(k) = self
            .hunt
            .herd
            .chars
            .iter()
            .position(|c| c.ai == ai_id && c.health > 0 && named(c))
        else {
            tracing::warn!("--die: no living animal of AI {ai_id} {name:?}");
            return;
        };
        // The others of that AI out of the way, so this one kills.
        for (i, c) in self.hunt.herd.chars.iter_mut().enumerate() {
            if i != k && c.ai == ai_id {
                c.pos[0] += 40_000.0;
            }
        }
        let c = &mut self.hunt.herd.chars[k];
        c.pos = [x + alpha.sin() * 600.0, y, z - alpha.cos() * 600.0];
        c.state = 1;
        c.afraid_time = 20_000;
        c.aware_hunter = true;
        let mut waited = 0;
        while self.hunt.death.is_none() && waited < 30_000 {
            self.step(&HuntInput::default());
            waited += TICK_MS;
        }
        let killer = self.hunt.herd.chars.get(k).map(|c| (c.ctype, c.scale));
        tracing::info!(
            "--die: dead after {waited} ms: {}",
            self.hunt.death.is_some()
        );
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        if let (Some(d), Some((ctype, scale))) = (self.hunt.death, killer) {
            let body = self.hunt.herd.chars.get(d.body);
            let name = self
                .hunt
                .herd
                .species
                .get(ctype)
                .map(|s| s.info.name.clone());
            tracing::info!(
                "--die: killer {name:?} scale {scale:.2} size {:?}; camera {:?} base {} lift {}; body {:?} phase {:?} ftime {:?}",
                self.hunt.herd.species.get(ctype).map(|s| s.size),
                d.pos,
                d.base,
                d.lift,
                body.map(|b| b.pos),
                body.map(|b| b.phase),
                body.map(|b| b.ftime),
            );
        }
        if let Some(b) = self
            .hunt
            .death
            .and_then(|d| self.hunt.herd.chars.get(d.body))
        {
            let sp = self.hunt.herd.species.get(b.ctype);
            tracing::info!(
                "--die: body species {} {:?} file {:?}, {:?} animations; watched {:?}",
                b.ctype,
                sp.map(|s| s.info.name.clone()),
                sp.map(|s| s.info.file.clone()),
                sp.map(|s| s.ani_time.len()),
                self.hunt.herd.demo_index,
            );
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Fires once, then runs `ms` (screenshots).
    pub fn fire_and_wait(&mut self, ms: u32) {
        self.step(&HuntInput {
            fire: true,
            ..Default::default()
        });
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Paused, asking to leave, or a message up (screenshots).
    pub fn show(&mut self, pause: bool, exit: bool, message: Option<&str>) {
        self.hunt.paused |= pause;
        self.hunt.exit_prompt |= exit;
        if let Some(m) = message {
            self.message = Some((m.to_string(), 2.0));
        }
    }

    /// A kill of the first huntable animal (screenshots): its notice, or
    /// its trophy's plaque in stand 0.
    pub fn show_kill(&mut self, plaque: bool) {
        let Some((ctype, s)) = self
            .hunt
            .herd
            .species
            .iter()
            .enumerate()
            .find(|(_, s)| s.info.ai >= 10)
        else {
            return;
        };
        if !plaque {
            self.kill = Some((s.info.name.clone(), 25, 84.0, false, 60.0, 0));
            return;
        }
        let (y, mo, d, h, mi) = self.hunt.clock;
        if let Some(b) = self.hunt.room.bodies.first_mut() {
            *b = c2_formats::sav::TrophyItem {
                ctype: ctype as i32,
                scale: 1.0,
                weapon: self.hunt.weapons.current as i32,
                score: 25,
                time: (h << 10) + mi,
                date: (y << 20) + (mo << 10) + d,
                range: 84.0,
                ..Default::default()
            };
            self.hunt.room.bodies[0].set_area(self.hunt.area_no);
            self.hunt.trophy_time = 60 * 1000;
            self.hunt.trophy_body = Some(0);
        }
    }

    /// The trophy room with every stand taken, the huntable animals in
    /// turn (screenshots: in memory, the save untouched unless left).
    pub fn fill_trophies(&mut self) {
        let kinds: Vec<usize> = self
            .hunt
            .herd
            .species
            .iter()
            .enumerate()
            .filter(|(_, s)| s.info.ai >= 10 && !s.ani_time.is_empty())
            .map(|(i, _)| i)
            .collect();
        if kinds.is_empty() || !self.hunt.opts.trophy {
            return;
        }
        for (i, b) in self.hunt.room.bodies.iter_mut().enumerate().take(24) {
            *b = c2_formats::sav::TrophyItem {
                ctype: kinds[i % kinds.len()] as i32,
                scale: 1.0,
                score: 10,
                ..Default::default()
            };
        }
        let room = self.hunt.room.clone();
        let area = self.view.area();
        self.hunt
            .herd
            .place_trophies(area, &room, &mut self.hunt.rng);
    }

    /// Opens the weapon picker, or changes the call (screenshots).
    pub fn open_picker(&mut self, call: bool) {
        if call {
            let input = HuntInput {
                change_call: true,
                ..Default::default()
            };
            self.step(&input);
        } else {
            self.open_pick(Pick::Weapon, false);
        }
    }

    /// Opens the map right away (screenshots).
    pub fn open_map(&mut self) {
        let input = HuntInput {
            toggle_map: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Raises the binoculars right away (screenshots).
    pub fn raise_binoculars(&mut self) {
        let input = HuntInput {
            toggle_binoculars: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Draws weapon `w` right away (screenshots).
    pub fn draw_weapon(&mut self, w: usize) {
        let input = HuntInput {
            select_weapon: Some(w),
            ..Default::default()
        };
        self.step(&input);
        for _ in 0..200 {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }
}
