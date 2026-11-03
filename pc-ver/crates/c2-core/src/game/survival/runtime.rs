//! Survival state, hunt setup and wave progression.

#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};

use super::navigation::{Fence, Flow};
use super::options::{SurvivalOptions, TREX_WAVE};
use crate::area::Area;
use crate::game::{GameEvent, Hunt, MAX_HEALTH};

#[derive(Debug, Clone, Default)]
pub struct Survival {
    pub opts: SurvivalOptions,
    /// The wave under way (0: the first is coming).
    pub wave: u32,
    /// Until the next wave comes (ms).
    pub rest_ms: i32,
    /// Ways to the hunter, for wave animals far off.
    pub(super) flow: Option<Flow>,
    /// The ground wave animals keep to (a survival with a spawn box).
    pub(super) fence: Option<Fence>,
}

impl Survival {
    pub fn new(opts: SurvivalOptions) -> Self {
        let rest_ms = opts.mobile.as_ref().map_or(3000, |m| m.first_ms);
        Self {
            opts,
            wave: 0,
            rest_ms,
            flow: None,
            fence: None,
        }
    }

    /// The animals of wave `n`: two more each wave, up to the cap.
    fn size(&self, n: u32) -> usize {
        (2 + n as usize).min(self.opts.max_wave.max(1))
    }
}

impl Hunt {
    /// Turns this hunt into a survival: all ordinary map wildlife goes;
    /// only survival waves may populate the map. A mobile survival puts the
    /// hunter at its spot.
    pub fn start_survival(&mut self, area: &Area, opts: SurvivalOptions) {
        while let Some(i) = self.herd.chars.iter().position(|c| c.wave == 0) {
            self.remove_character(i);
        }
        let mut fence = None;
        if let Some(m) = opts.mobile.as_ref().filter(|m| !m.stand_ins) {
            let cell = |v: f32| (v / 256.0).clamp(1.0, 1022.0) as u16;
            let at = (cell(m.hunter[0]), cell(m.hunter[1]));
            self.player = crate::player::Player::new(area, at);
            if let Some(a) = m.hunter_alpha {
                self.player.alpha = a;
            }
            fence = Some(Fence::new(
                area,
                (i32::from(at.0), i32::from(at.1)),
                m.spawn,
            ));
        }
        let mut s = Survival::new(opts);
        s.fence = fence;
        self.survival = Some(s);
    }

    /// The wave under way (0: none yet, or not a survival).
    pub fn wave(&self) -> u32 {
        self.survival.as_ref().map_or(0, |s| s.wave)
    }

    /// One survival step: keeps the wave after the hunter, counts it down,
    /// sends the next one.
    pub(in crate::game) fn survival_step(
        &mut self,
        area: &Area,
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        let Some(s) = self.survival.as_mut() else {
            return;
        };
        if self.death.is_some() {
            return;
        }
        if s.opts.unlimited {
            self.weapons.resupply();
        }
        // The ways to the hunter, again when he has moved a few cells.
        let (hx, hz) = (self.player.x, self.player.z);
        let cell = ((hx / 256.0) as i32, (hz / 256.0) as i32);
        let stale = s
            .flow
            .as_ref()
            .is_none_or(|f| (f.from.0 - cell.0).abs().max((f.from.1 - cell.1).abs()) >= 3);
        let waves = self.herd.chars.iter().any(|c| c.wave > 0 && c.health > 0);
        if stale && waves {
            s.flow = Some(Flow::new(area, cell, s.fence.as_ref()));
        }
        // Fenced: a wave animal that stepped off the ground goes back.
        if let Some(f) = s.fence.as_ref() {
            for c in self
                .herd
                .chars
                .iter_mut()
                .filter(|c| c.wave > 0 && c.health > 0)
            {
                if f.allows_pos(c.pos[0], c.pos[2]) {
                    c.fenced_at = Some((c.pos[0], c.pos[2]));
                } else if let Some((x, z)) = c.fenced_at {
                    c.pos = [x, area.land_height(x, z), z];
                    // On its way, not back at the wall.
                    (c.tgx, c.tgz) = c.lure.unwrap_or((hx, hz));
                }
            }
        }
        let flow = s.flow.take();
        for c in self.herd.chars.iter_mut().filter(|c| c.wave > 0) {
            c.lure = flow
                .as_ref()
                .filter(|_| c.health > 0)
                .and_then(|f| f.lure((c.pos[0] / 256.0) as i32, (c.pos[2] / 256.0) as i32));
        }
        if let Some(s) = self.survival.as_mut() {
            s.flow = flow;
        }
        // The wave keeps coming (a hunter that lost the hunter's track
        // takes it up again).
        let mut alive = 0;
        for c in self
            .herd
            .chars
            .iter_mut()
            .filter(|c| c.wave > 0 && c.health > 0)
        {
            alive += 1;
            if c.state == 0 {
                c.state = 2;
                c.afraid_time = 60_000;
                c.aware_hunter = true;
                (c.tgx, c.tgz) = c.lure.unwrap_or((hx, hz));
            }
        }
        if alive > 0 {
            return;
        }
        let Some(s) = self.survival.as_mut() else {
            return;
        };
        if s.rest_ms > 0 {
            s.rest_ms -= dt;
            return;
        }
        s.wave += 1;
        s.rest_ms = s.opts.mobile.as_ref().map_or(4000, |m| m.rest_ms);
        let n = s.wave;
        // The fallen of the last wave go.
        while let Some(i) = self
            .herd
            .chars
            .iter()
            .position(|c| c.wave > 0 && c.health == 0)
        {
            self.remove_character(i);
        }
        // Who comes: the mobile wave; or the roster, a T-Rex from wave 15,
        // hardcore extras.
        let s = self.survival.clone().unwrap_or_default();
        if let Some(m) = &s.opts.mobile {
            for &ctype in m.wave(n).iter().take(s.opts.max_wave.max(1)) {
                if m.stand_ins {
                    self.spawn_wave_animal(area, ctype, n);
                } else {
                    self.spawn_in_box(area, ctype, n, m.spawn);
                }
            }
            if !m.wave(n).is_empty() && !self.herd.chars.iter().any(|c| c.wave == n) {
                // A blocked box must not move the wave elsewhere or silently
                // skip it. Retry this wave after the rest (the hunter can move).
                if let Some(s) = self.survival.as_mut() {
                    s.wave -= 1;
                }
                out.push(GameEvent::Message(
                    "Survival spawn blocked; retrying".into(),
                ));
                return;
            }
            self.health = MAX_HEALTH;
            self.weapons.resupply();
            out.push(GameEvent::Message(format!("Wave {n}")));
            return;
        }
        let mut kinds = Vec::new();
        if let (Some(t), true) = (s.opts.trex, n >= TREX_WAVE) {
            kinds.push(t);
        }
        while kinds.len() < s.size(n) && !s.opts.roster.is_empty() {
            let i = self.rng.r_rand(s.opts.roster.len() as i32) as usize;
            kinds.push(s.opts.roster[i % s.opts.roster.len()]);
        }
        if !s.opts.hardcore.is_empty() {
            for _ in 0..1 + self.rng.r_rand(3) {
                let i = self.rng.r_rand(s.opts.hardcore.len() as i32) as usize;
                kinds.push(s.opts.hardcore[i % s.opts.hardcore.len()]);
            }
        }
        for ctype in kinds {
            self.spawn_wave_animal(area, ctype, n);
        }
        // A rest's worth: health and ammunition back.
        self.health = MAX_HEALTH;
        self.weapons.resupply();
        out.push(GameEvent::Message(format!("Wave {n}")));
    }
}
