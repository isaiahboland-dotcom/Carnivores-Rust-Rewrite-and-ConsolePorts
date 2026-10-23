//! Survival (README M13), after the mobile games' mode: waves of hunting
//! animals come for the hunter, each wave bigger; between waves a short
//! rest (health and ammunition back). It ends when the hunter dies. Kills
//! give no trophy and no score; the frontends record them (and the wave
//! reached) in the hunter's record.
//!
//! - Hardcore: each wave also brings 1-3 animals from a pool (the frontends
//!   give the modded ones).
//! - Custom: unlimited ammunition; the T-Rex may come from wave 15.

use super::*;

/// How a survival is set up.
#[derive(Debug, Clone, Default)]
pub struct SurvivalOptions {
    /// The animals of the waves (character types; hunters only).
    pub roster: Vec<usize>,
    /// Hardcore: 1-3 of these join each wave (empty: off).
    pub hardcore: Vec<usize>,
    /// Custom: the weapons never run out.
    pub unlimited: bool,
    /// The T-Rex (its type), from wave 15.
    pub trex: Option<usize>,
    /// At most this many animals in a wave (the 3DS: fewer).
    pub max_wave: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Survival {
    pub opts: SurvivalOptions,
    /// The wave under way (0: the first is coming).
    pub wave: u32,
    /// Until the next wave comes (ms).
    pub rest_ms: i32,
}

/// Which survival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurvivalKind {
    #[default]
    Normal,
    /// 1-3 modded animals join each wave.
    Hardcore,
    /// Unlimited ammunition; the T-Rex from wave 15.
    Custom,
}

impl SurvivalKind {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Hardcore, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Survival",
            Self::Hardcore => "Survival: hardcore",
            Self::Custom => "Survival: custom",
        }
    }
}

/// The original game's hunting animals (by name): the others are a mod's.
const ORIGINAL: [&str; 6] = [
    "allosaurus",
    "velociraptor",
    "spinosaurus",
    "ceratosaurus",
    "t-rex",
    "tyrannosaurus",
];

/// Whether a species is the T-Rex.
fn trex(sp: &crate::dino::Species) -> bool {
    is_trex(&sp.info.name, sp.info.ai, sp.mee.is_some())
}

/// Whether an animal is the T-Rex: the original's AI, or by name.
pub fn is_trex(name: &str, ai_id: i32, dialect: bool) -> bool {
    let n = name.to_ascii_lowercase();
    (!dialect && ai_id == ai::TREX) || n.contains("t-rex") || n.contains("tyrannosaur")
}

/// Whether an animal is a mod's (not one of the original's hunters), by name.
pub fn modded(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !ORIGINAL.iter().any(|o| n.contains(o))
}

/// The first wave a T-Rex may come in.
pub const TREX_WAVE: u32 = 15;

/// Whether a species hunts the hunter (a wave animal).
pub fn hunts(sp: &crate::dino::Species) -> bool {
    hunts_ai(sp.info.ai, sp.mee.is_some())
}

/// [`hunts`] by AI: the original's carnivore AIs, or a dialect AI marked
/// carnivore.
pub fn hunts_ai(ai_id: i32, dialect: bool) -> bool {
    if dialect {
        crate::dino::mee::ai_info(ai_id).carnivore
    } else {
        matches!(
            ai_id,
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX
        )
    }
}

impl SurvivalOptions {
    /// The waves for a hunt: the hunting animals picked as targets (none
    /// picked: every hunting animal but the T-Rex); hardcore extras from the
    /// mods' hunting animals, those not in the waves first (the game's own
    /// if there are none).
    pub fn pick(hunt: &Hunt, kind: SurvivalKind, max_wave: usize) -> Self {
        let sp = &hunt.herd.species;
        let hunting: Vec<usize> = (0..sp.len()).filter(|&i| hunts(&sp[i])).collect();
        let target = |i: &usize| {
            hunt.is_target(&Character {
                ctype: *i,
                ai: sp[*i].info.ai,
                ..Default::default()
            })
        };
        let mut roster: Vec<usize> = hunting.iter().copied().filter(target).collect();
        if roster.is_empty() {
            roster = hunting.iter().copied().filter(|&i| !trex(&sp[i])).collect();
        }
        let hardcore = if kind == SurvivalKind::Hardcore {
            let mods = |i: &usize| modded(&sp[*i].info.name);
            let fresh: Vec<usize> = hunting
                .iter()
                .copied()
                .filter(|i| mods(i) && !roster.contains(i))
                .collect();
            let any: Vec<usize> = hunting.iter().copied().filter(mods).collect();
            [fresh, any, hunting.clone()]
                .into_iter()
                .find(|v| !v.is_empty())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let custom = kind == SurvivalKind::Custom;
        Self {
            roster,
            hardcore,
            unlimited: custom,
            trex: hunting.iter().copied().find(|&i| custom && trex(&sp[i])),
            max_wave,
        }
    }
}

impl Survival {
    pub fn new(opts: SurvivalOptions) -> Self {
        Self {
            opts,
            wave: 0,
            rest_ms: 3000,
        }
    }

    /// The animals of wave `n`: two more each wave, up to the cap.
    fn size(&self, n: u32) -> usize {
        (2 + n as usize).min(self.opts.max_wave.max(1))
    }
}

impl Hunt {
    /// Turns this hunt into a survival: the map's own hunting animals go
    /// (the plant-eaters stay for the scenery); the first wave comes soon.
    pub fn start_survival(&mut self, opts: SurvivalOptions) {
        while let Some(i) = self
            .herd
            .chars
            .iter()
            .position(|c| self.herd.species.get(c.ctype).is_some_and(hunts) && c.wave == 0)
        {
            self.remove_character(i);
        }
        self.survival = Some(Survival::new(opts));
    }

    /// The wave under way (0: none yet, or not a survival).
    pub fn wave(&self) -> u32 {
        self.survival.as_ref().map_or(0, |s| s.wave)
    }

    /// One survival step: keeps the wave after the hunter, counts it down,
    /// sends the next one.
    pub(super) fn survival_step(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        let Some(s) = self.survival.as_mut() else {
            return;
        };
        if self.death.is_some() {
            return;
        }
        if s.opts.unlimited {
            self.weapons.resupply();
        }
        // The wave keeps coming (a hunter that lost the hunter's track
        // takes it up again).
        let (hx, hz) = (self.player.x, self.player.z);
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
                (c.tgx, c.tgz) = (hx, hz);
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
        s.rest_ms = 4000;
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
        // Who comes: the roster, a T-Rex from wave 15, hardcore extras.
        let s = self.survival.clone().unwrap_or_default();
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

    /// One animal of wave `n` on land in a ring around the hunter, out of
    /// his sight, coming for him.
    fn spawn_wave_animal(&mut self, area: &Area, ctype: usize, n: u32) {
        let Some(sp) = self.herd.species.get(ctype) else {
            return;
        };
        let (hx, hz) = (self.player.x, self.player.z);
        let r0 = (self.opts.view_r as f32 * 0.7).clamp(20.0, 40.0) * 256.0;
        let size = 1024.0 * 256.0;
        for _ in 0..40 {
            let a = self.rng.r_rand(6283) as f32 / 1000.0;
            let r = r0 + self.rng.r_rand(2048) as f32;
            let (x, z) = (hx + a.cos() * r, hz + a.sin() * r);
            if !(4096.0..size - 4096.0).contains(&x) || !(4096.0..size - 4096.0).contains(&z) {
                continue;
            }
            if area.land_up_h(x, z) - area.land_height(x, z) > 32.0 {
                continue;
            }
            let mut p = [x, area.land_height(x, z), z];
            if crate::dino::check_place_collision_p(area, &mut p) {
                continue;
            }
            let mut c = Character {
                ctype,
                alpha: a + PI,
                pos: p,
                ..Default::default()
            };
            c.reset(sp, &mut self.rng, self.opts.opt_agres);
            c.wave = n;
            c.state = 2;
            c.afraid_time = 60_000;
            c.aware_hunter = true;
            (c.tgx, c.tgz) = (hx, hz);
            self.herd.chars.push(c);
            return;
        }
    }
}
