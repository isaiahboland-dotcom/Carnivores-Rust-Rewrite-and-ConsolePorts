//! Survival (README M13), after the mobile games' mode: waves of dangerous
//! animals, picked at random, come for the hunter (always: they never
//! back off), each wave bigger; between waves a short
//! rest (health and ammunition back). It ends when the hunter dies. Kills
//! give no trophy and no score; the frontends record them (and the wave
//! reached) in the hunter's record.
//!
//! - Hardcore: each wave also brings 1-3 animals from a pool (the frontends
//!   give the modded ones).
//! - Custom: unlimited ammunition; the T-Rex may come from wave 15.
//! - Mobile: the mobile game's own Survive mode, as read from the player's
//!   APK (`c2_formats::mobile_arcade`): its waves, in turn, from a box on
//!   Mount Ravan; the hunter at its spot.

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
    /// The mobile game's waves instead.
    pub mobile: Option<MobileWaves>,
}

/// The mobile game's Survive mode for this hunt's species.
#[derive(Debug, Clone, Default)]
pub struct MobileWaves {
    /// Character types of each wave in turn, then of every later one.
    pub waves: Vec<Vec<usize>>,
    pub after: Vec<usize>,
    /// Where they come: centre `x`, `z`, half sizes.
    pub spawn: [f32; 4],
    pub hunter: [f32; 2],
    pub first_ms: i32,
    pub rest_ms: i32,
    /// Ice Age stand-ins, away from Mount Ravan: the hunter keeps his start
    /// and the waves come round him as in our survival (no box).
    pub stand_ins: bool,
    /// Which way the hunter faces at his spot (rad; the dialect's
    /// `survivalPlayerA`), if set.
    pub hunter_alpha: Option<f32>,
}

impl MobileWaves {
    /// `s`'s animals found among `species` by name, or their stand-ins
    /// (`Survive::for_game`); `None` if one isn't.
    pub fn new(
        s: &c2_formats::mobile_arcade::Survive,
        species: &[crate::dino::Species],
    ) -> Option<Self> {
        let names: Vec<String> = species.iter().map(|sp| sp.info.name.clone()).collect();
        let s = &s.for_game(&names)?;
        let norm = |n: &str| -> String {
            n.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect()
        };
        let find = |n: &String| species.iter().position(|sp| norm(&sp.info.name) == norm(n));
        let all = |w: &Vec<String>| w.iter().map(find).collect::<Option<Vec<_>>>();
        Some(Self {
            waves: s.waves.iter().map(all).collect::<Option<_>>()?,
            after: all(&s.after)?,
            spawn: s.spawn,
            hunter: s.hunter,
            first_ms: s.first_ms,
            rest_ms: s.rest_ms,
            stand_ins: s.stand_ins,
            hunter_alpha: None,
        })
    }

    /// The animals of wave `n` (from 1).
    fn wave(&self, n: u32) -> &[usize] {
        self.waves.get(n as usize - 1).unwrap_or(&self.after)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Survival {
    pub opts: SurvivalOptions,
    /// The wave under way (0: the first is coming).
    pub wave: u32,
    /// Until the next wave comes (ms).
    pub rest_ms: i32,
    /// Ways to the hunter, for wave animals far off.
    flow: Option<Flow>,
}

/// Cells of the window a [`Flow`] covers (a side).
const FLOW_W: i32 = 192;
/// How far above or below the hunter a wave animal may start (map units:
/// a few cells of steep slope), when a spot like that is found.
const SPAWN_RISE: f32 = 1536.0;
/// Closer than this (cells), wave animals go straight for the hunter.
const LURE_NEAR: u16 = 12;
/// How far along the way a lure is (cells).
const LURE_AHEAD: usize = 8;
const AROUND: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Steps (cells) from each cell of a window to the hunter's, over ground an
/// animal can walk (no water, no steeper than `CheckPlaceCollision`
/// allows): the AIs only steer around what is just ahead, so a ridge
/// between a wave and the hunter would hold it up for good.
#[derive(Debug, Clone, Default)]
struct Flow {
    /// The window's first cell, and the hunter's cell it was made for.
    x0: i32,
    z0: i32,
    from: (i32, i32),
    dist: Vec<u16>,
}

impl Flow {
    fn new(area: &Area, (hx, hz): (i32, i32)) -> Self {
        let (x0, z0) = (hx - FLOW_W / 2, hz - FLOW_W / 2);
        let n = (FLOW_W * FLOW_W) as usize;
        let inside = |x: i32, z: i32| (4..1020).contains(&x) && (4..1020).contains(&z);
        let at = |x: i32, z: i32| ((z - z0) * FLOW_W + (x - x0)) as usize;
        let mut h = vec![f32::NAN; n];
        for z in z0..z0 + FLOW_W {
            for x in x0..x0 + FLOW_W {
                if !inside(x, z) {
                    continue;
                }
                let wet = area.map.flags[crate::area::idx(x as usize, z as usize)]
                    & crate::area::cell_flags::WATER
                    != 0;
                if !wet {
                    h[at(x, z)] =
                        area.land_height(x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0);
                }
            }
        }
        let mut dist = vec![u16::MAX; n];
        let mut queue = std::collections::VecDeque::new();
        if inside(hx, hz) {
            dist[at(hx, hz)] = 0;
            queue.push_back((hx, hz));
        }
        while let Some((x, z)) = queue.pop_front() {
            let (d, hh) = (dist[at(x, z)], h[at(x, z)]);
            for (dx, dz) in AROUND {
                let (nx, nz) = (x + dx, z + dz);
                if nx < x0 || nz < z0 || nx >= x0 + FLOW_W || nz >= z0 + FLOW_W {
                    continue;
                }
                let j = at(nx, nz);
                // 100 up per 64 across, as `slope_too_steep`; the start
                // (the hunter's cell) may be anything.
                let run = if dx != 0 && dz != 0 { 362.0 } else { 256.0 };
                let steep = d > 0 && (h[j] - hh).abs() > run * 100.0 / 64.0;
                if dist[j] == u16::MAX && !h[j].is_nan() && !steep {
                    dist[j] = d + 1;
                    queue.push_back((nx, nz));
                }
            }
        }
        Self {
            x0,
            z0,
            from: (hx, hz),
            dist,
        }
    }

    /// Whether cell (`x`, `z`) is in the window (else [`Flow::get`] knows
    /// nothing of it).
    fn covers(&self, x: i32, z: i32) -> bool {
        (0..FLOW_W).contains(&(x - self.x0)) && (0..FLOW_W).contains(&(z - self.z0))
    }

    fn get(&self, x: i32, z: i32) -> u16 {
        let (i, j) = (x - self.x0, z - self.z0);
        if (0..FLOW_W).contains(&i) && (0..FLOW_W).contains(&j) {
            self.dist[(j * FLOW_W + i) as usize]
        } else {
            u16::MAX
        }
    }

    /// Where an animal at cell (`x`, `z`) should head: [`LURE_AHEAD`] cells
    /// along the way, or `None` (near, or no way known: straight on).
    fn lure(&self, mut x: i32, mut z: i32) -> Option<(f32, f32)> {
        let d = self.get(x, z);
        if d == u16::MAX || d <= LURE_NEAR {
            return None;
        }
        for _ in 0..LURE_AHEAD {
            let here = self.get(x, z);
            let next = AROUND
                .into_iter()
                .map(|(dx, dz)| (x + dx, z + dz))
                .min_by_key(|&(nx, nz)| self.get(nx, nz))?;
            if self.get(next.0, next.1) >= here {
                break;
            }
            (x, z) = next;
        }
        Some((x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0))
    }
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
    /// The mobile game's Survive mode (from the player's APK).
    Mobile,
    /// The Modder's Edition's own (`-survival`), from the game's data.
    Mee,
}

impl SurvivalKind {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Hardcore, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Survival",
            Self::Hardcore => "Survival: hardcore",
            Self::Custom => "Survival: custom",
            Self::Mobile => "Classic survival",
            Self::Mee => "Modded survival",
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

/// Whether a species is dangerous to the hunter (a survival animal): the
/// original's hunters and its Chasmosaurus (which charges); in a dialect
/// game, a carnivore AI or any huntable with an attack (`aggress`).
pub fn dangerous(sp: &crate::dino::Species) -> bool {
    dangerous_ai(sp.info.ai, sp.mee.as_ref().map(|m| m.aggress))
}

/// [`dangerous`] by AI and, in a dialect game, `aggress` (`None`: the
/// original's rules).
pub fn dangerous_ai(ai_id: i32, aggress: Option<i32>) -> bool {
    use crate::dino::mee;
    match aggress {
        Some(a) => {
            (mee::is_huntable(ai_id) || ai_id == ai::TREX)
                && (mee::ai_info(ai_id).carnivore || a > 0)
        }
        None => matches!(
            ai_id,
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX | ai::CHASM
        ),
    }
}

impl SurvivalOptions {
    /// The waves for a hunt: the dangerous animals among the targets (the
    /// frontends make every loaded animal one), at random each wave, but the
    /// T-Rex (none: every dangerous animal but it); hardcore extras from the
    /// mods' dangerous animals, those not in the waves first (the game's own
    /// if there are none).
    pub fn pick(hunt: &Hunt, kind: SurvivalKind, max_wave: usize) -> Self {
        let sp = &hunt.herd.species;
        let hunting: Vec<usize> = (0..sp.len()).filter(|&i| dangerous(&sp[i])).collect();
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
            mobile: None,
        }
    }
}

/// Waves the dialect's survival makes (`PlaceCharactersSurvival`) up to its
/// limit of 26: wave `n` has `2^n` to spend; the `k`-th animal of the roster
/// (0 first) costs `3^k`; the dearest that fits is bought first, as often as
/// it fits, then the next cheaper.
pub fn mee_waves(roster: &[usize]) -> Vec<Vec<usize>> {
    (1..=26u32)
        .map(|n| {
            let mut left = 1u64 << n;
            let mut wave = Vec::new();
            for (k, &ctype) in roster.iter().enumerate().rev() {
                let cost = 3u64.saturating_pow(k as u32);
                while left >= cost && wave.len() < 255 {
                    wave.push(ctype);
                    left -= cost;
                }
            }
            wave
        })
        .collect()
}

impl SurvivalOptions {
    /// The dialect's survival: its `roster` (character types in
    /// `survivalIndex` order) from the area's box, the hunter at its spot
    /// facing its way, unlimited ammunition; the next wave 2 s after one
    /// falls (`WaveNoteTime`). `None` without a roster.
    pub fn mee(
        roster: &[usize],
        spot: &c2_formats::mee::SurvivalSpot,
        max_wave: usize,
    ) -> Option<Self> {
        if roster.is_empty() {
            return None;
        }
        let cell = |c: i32| c as f32 * 256.0 + 128.0;
        let [x0, x1, y0, y1] = spot.spawn;
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let mut waves = mee_waves(roster);
        let after = waves.pop().unwrap_or_default();
        Some(Self {
            mobile: Some(MobileWaves {
                waves,
                after,
                spawn: [
                    (cell(x0) + cell(x1)) / 2.0,
                    (cell(y0) + cell(y1)) / 2.0,
                    (x1 - x0) as f32 * 128.0 + 128.0,
                    (y1 - y0) as f32 * 128.0 + 128.0,
                ],
                hunter: [cell(spot.hunter.0), cell(spot.hunter.1)],
                first_ms: 500,
                rest_ms: 2000,
                stand_ins: false,
                hunter_alpha: Some(spot.angle.to_radians()),
            }),
            unlimited: true,
            max_wave,
            ..Default::default()
        })
    }

    /// The mobile game's Survive mode (`s`) with this hunt's species, or
    /// `None` if one of its animals is missing.
    pub fn mobile(
        hunt: &Hunt,
        s: &c2_formats::mobile_arcade::Survive,
        max_wave: usize,
    ) -> Option<Self> {
        Some(Self {
            mobile: Some(MobileWaves::new(s, &hunt.herd.species)?),
            max_wave,
            ..Default::default()
        })
    }
}

impl Survival {
    pub fn new(opts: SurvivalOptions) -> Self {
        let rest_ms = opts.mobile.as_ref().map_or(3000, |m| m.first_ms);
        Self {
            opts,
            wave: 0,
            rest_ms,
            flow: None,
        }
    }

    /// The animals of wave `n`: two more each wave, up to the cap.
    fn size(&self, n: u32) -> usize {
        (2 + n as usize).min(self.opts.max_wave.max(1))
    }
}

impl Hunt {
    /// Turns this hunt into a survival: the map's own hunting animals (and
    /// any the mobile waves bring) go, the plant-eaters stay for the
    /// scenery; the first wave comes soon. A mobile survival puts the
    /// hunter at its spot.
    pub fn start_survival(&mut self, area: &Area, opts: SurvivalOptions) {
        let waves: Vec<usize> = opts
            .mobile
            .iter()
            .flat_map(|m| m.waves.iter().flatten().chain(&m.after))
            .copied()
            .collect();
        while let Some(i) = self.herd.chars.iter().position(|c| {
            c.wave == 0
                && (waves.contains(&c.ctype)
                    || self.herd.species.get(c.ctype).is_some_and(dangerous))
        }) {
            self.remove_character(i);
        }
        if let Some(m) = opts.mobile.as_ref().filter(|m| !m.stand_ins) {
            let cell = |v: f32| (v / 256.0).clamp(1.0, 1022.0) as u16;
            self.player = crate::player::Player::new(area, (cell(m.hunter[0]), cell(m.hunter[1])));
            if let Some(a) = m.hunter_alpha {
                self.player.alpha = a;
            }
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
        // The ways to the hunter, again when he has moved a few cells.
        let (hx, hz) = (self.player.x, self.player.z);
        let cell = ((hx / 256.0) as i32, (hz / 256.0) as i32);
        let stale = s
            .flow
            .as_ref()
            .is_none_or(|f| (f.from.0 - cell.0).abs().max((f.from.1 - cell.1).abs()) >= 3);
        let waves = self.herd.chars.iter().any(|c| c.wave > 0 && c.health > 0);
        if stale && waves {
            s.flow = Some(Flow::new(area, cell));
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

    /// One animal of wave `n` at a free spot on land in the box `[x, z, half
    /// x, half z]`, coming for the hunter (`ArcadeGameGenerateWave`: 32
    /// tries, else the last). A box made for the mobile maps can take in
    /// the PC maps' mountain sides: spots with a way down to the hunter, near
    /// his height, first.
    fn spawn_in_box(&mut self, area: &Area, ctype: usize, n: u32, b: [f32; 4]) {
        let flow = self.wave_flow(area);
        let mut at = (b[0], b[1]);
        for strict in [true, false] {
            for _ in 0..32 {
                let r = |rng: &mut MsRand, h: f32| (rng.r_rand(20000) as f32 / 10000.0 - 1.0) * h;
                let (x, z) = (b[0] + r(&mut self.rng, b[2]), b[1] + r(&mut self.rng, b[3]));
                at = (x, z);
                if self.spawn_spot(area, (x, z), strict.then_some(&flow)) {
                    self.add_wave_animal(area, ctype, n, at);
                    return;
                }
            }
        }
        self.add_wave_animal(area, ctype, n, at);
    }

    /// The flow field for where the hunter stands (kept for the wave).
    fn wave_flow(&mut self, area: &Area) -> Flow {
        let cell = (
            (self.player.x / 256.0) as i32,
            (self.player.z / 256.0) as i32,
        );
        match self.survival.as_ref().and_then(|s| s.flow.clone()) {
            Some(f) if f.from == cell => f,
            _ => {
                let f = Flow::new(area, cell);
                if let Some(s) = self.survival.as_mut() {
                    s.flow = Some(f.clone());
                }
                f
            }
        }
    }

    /// Whether a wave animal may start at (`x`, `z`): dry and free; with
    /// `flow`, also with a walkable way to the hunter (where the field
    /// reaches) and not more than [`SPAWN_RISE`] above or below him.
    fn spawn_spot(&self, area: &Area, (x, z): (f32, f32), flow: Option<&Flow>) -> bool {
        let y = area.land_height(x, z);
        if area.land_up_h(x, z) - y > 32.0 {
            return false;
        }
        if let Some(f) = flow {
            let (cx, cz) = ((x / 256.0) as i32, (z / 256.0) as i32);
            if f.covers(cx, cz) && f.get(cx, cz) == u16::MAX {
                return false;
            }
            if (y - area.land_height(self.player.x, self.player.z)).abs() > SPAWN_RISE {
                return false;
            }
        }
        let mut p = [x, y, z];
        !crate::dino::check_place_collision_p(area, &mut p)
    }

    fn add_wave_animal(&mut self, area: &Area, ctype: usize, n: u32, (x, z): (f32, f32)) {
        let Some(sp) = self.herd.species.get(ctype) else {
            return;
        };
        let (hx, hz) = (self.player.x, self.player.z);
        let mut c = Character {
            ctype,
            alpha: find_vector_alpha(hx - x, hz - z),
            pos: [x, area.land_height(x, z), z],
            ..Default::default()
        };
        c.reset(sp, &mut self.rng, self.opts.opt_agres);
        c.wave = n;
        c.state = 2;
        c.afraid_time = 60_000;
        c.aware_hunter = true;
        (c.tgx, c.tgz) = (hx, hz);
        self.herd.chars.push(c);
    }

    /// One animal of wave `n` on land in a ring around the hunter, out of
    /// his sight, coming for him.
    fn spawn_wave_animal(&mut self, area: &Area, ctype: usize, n: u32) {
        let (hx, hz) = (self.player.x, self.player.z);
        let r0 = (self.opts.view_r as f32 * 0.7).clamp(20.0, 40.0) * 256.0;
        let size = 1024.0 * 256.0;
        let flow = self.wave_flow(area);
        for strict in [true, false] {
            for _ in 0..40 {
                let a = self.rng.r_rand(6283) as f32 / 1000.0;
                let r = r0 + self.rng.r_rand(2048) as f32;
                let (x, z) = (hx + a.cos() * r, hz + a.sin() * r);
                if !(4096.0..size - 4096.0).contains(&x) || !(4096.0..size - 4096.0).contains(&z) {
                    continue;
                }
                if self.spawn_spot(area, (x, z), strict.then_some(&flow)) {
                    self.add_wave_animal(area, ctype, n, (x, z));
                    return;
                }
            }
        }
    }
}
