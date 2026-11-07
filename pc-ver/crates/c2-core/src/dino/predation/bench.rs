//! What predation costs on a herd of a given size: `c2-parity predation
//! --count`, and the same measurement on the consoles (the clock is the
//! caller's, so it runs without `std`).
//!
//! [`fill`] grows (or trims) the main population to a count, [`gather`]
//! brings it about the hunter; [`measure`]
//! steps the herd headless with the hunter away, timing every step and, on
//! a copy of the herd every [`THINK_MS`], [`Predators::think`] alone.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{vec, vec::Vec};

use super::run::{Predators, THINK_MS};
use crate::area::Area;
use crate::dino::{check_place_collision_p, Ctx, DinoEvent, Herd, HunterView};
use crate::rng::MsRand;

/// The hunt's step, ms.
const STEP_MS: i32 = 20;

/// One measured run. Times in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// Main-population animals (AI 10 and up) at the start.
    pub animals: usize,
    pub steps: u32,
    pub step_mean_us: u64,
    pub step_worst_us: u64,
    /// `think()` alone, once per [`THINK_MS`]; 0 with predation off.
    pub thinks: u32,
    pub think_mean_us: u64,
    pub think_worst_us: u64,
    /// Animals killed by animals by the end.
    pub kills: usize,
}

/// Whether character `ctype` counts towards the main population.
fn main_pop(herd: &Herd, ctype: usize) -> bool {
    herd.species.get(ctype).is_some_and(|s| s.info.ai >= 10)
}

/// Main-population animals (AI 10 and up) in `herd`.
pub fn population(herd: &Herd) -> usize {
    herd.chars
        .iter()
        .filter(|c| main_pop(herd, c.ctype))
        .count()
}

/// Makes the main population exactly `n`: extra animals copy the species
/// mix already placed (else every main species, in turn), where
/// `PlaceCharacters` puts them (about the map's centre, at least 40 cells
/// from the hunter at `(hx, hz)`). Fewer if there is no room.
pub fn fill(herd: &mut Herd, area: &Area, (hx, hz): (f32, f32), rng: &mut MsRand, n: usize) {
    let have = population(herd);
    if have > n {
        let m = |ct: usize| herd.species.get(ct).is_some_and(|s| s.info.ai >= 10);
        let counted: Vec<bool> = (0..herd.species.len()).map(m).collect();
        herd.cap_population(n, |ct| counted.get(ct).copied().unwrap_or(false));
        return;
    }
    let mut kinds: Vec<usize> = herd
        .chars
        .iter()
        .map(|c| c.ctype)
        .filter(|&ct| main_pop(herd, ct))
        .collect();
    if kinds.is_empty() {
        kinds = (0..herd.species.len())
            .filter(|&ct| main_pop(herd, ct))
            .collect();
    }
    if kinds.is_empty() {
        return;
    }
    let mut tries = 0;
    let mut k = 0;
    while population(herd) < n && tries < 10_240 {
        tries += 1;
        let mut p = [
            512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
            0.0,
            512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
        ];
        p[1] = area.land_height(p[0], p[2]);
        if (p[0] - hx).abs() + (p[2] - hz).abs() < 256.0 * 40.0
            || check_place_collision_p(area, &mut p)
        {
            continue;
        }
        herd.spawn(kinds[k % kinds.len()], p, rng, 128);
        k += 1;
    }
}

/// Moves every living main-population animal to a free spot within `cells`
/// of the hunter at `(hx, hz)`, to load a console's drawing. Ones with no
/// spot found stay where they are.
pub fn gather(herd: &mut Herd, area: &Area, (hx, hz): (f32, f32), rng: &mut MsRand, cells: f32) {
    let r = (cells * 256.0).max(256.0) as i32;
    for i in 0..herd.chars.len() {
        let c = &herd.chars[i];
        if c.health == 0 || !main_pop(herd, c.ctype) {
            continue;
        }
        for _ in 0..64 {
            let mut p = [hx + rng.si_rand(r) as f32, 0.0, hz + rng.si_rand(r) as f32];
            p[1] = area.land_height(p[0], p[2]);
            if !check_place_collision_p(area, &mut p) {
                let c = &mut herd.chars[i];
                c.pos = p;
                c.tgx = p[0];
                c.tgz = p[2];
                break;
            }
        }
    }
}

/// Steps `herd` for `seconds` (20 ms steps) with the hunter away at
/// `hunter`. `now_us` is a microsecond clock.
pub fn measure(
    herd: &mut Herd,
    area: &Area,
    hunter: HunterView,
    rng: &mut MsRand,
    seconds: u32,
    now_us: &mut dyn FnMut() -> u64,
) -> Stats {
    let mut s = Stats {
        animals: population(herd),
        steps: seconds * (1000 / STEP_MS as u32),
        ..Stats::default()
    };
    let (mut step_total, mut think_total) = (0u64, 0u64);
    let mut events = Vec::new();
    let mut scratch_events = Vec::new();
    let think_every = (THINK_MS / STEP_MS).max(1) as u32;
    for t in 0..s.steps {
        if t % think_every == 0 {
            if let Some(p) = herd.predation.as_deref() {
                let mut p: Predators = p.clone();
                let mut chars = herd.chars.clone();
                let mut r = rng.clone();
                scratch_events.clear();
                let mut c = ctx(area, hunter, t, &mut r, &mut scratch_events, THINK_MS);
                let t0 = now_us();
                p.think(&mut chars, &herd.species, &mut c);
                let us = now_us().saturating_sub(t0);
                s.thinks += 1;
                think_total += us;
                s.think_worst_us = s.think_worst_us.max(us);
            }
        }
        let mut c = ctx(area, hunter, t, rng, &mut events, STEP_MS);
        let t0 = now_us();
        herd.step(&mut c);
        let us = now_us().saturating_sub(t0);
        step_total += us;
        s.step_worst_us = s.step_worst_us.max(us);
        events.clear();
    }
    s.step_mean_us = step_total / u64::from(s.steps.max(1));
    s.think_mean_us = think_total / u64::from(s.thinks.max(1));
    s.kills = herd
        .chars
        .iter()
        .filter(|c| c.predation.killed_by_animal)
        .count();
    s
}

/// The context of step `t`: the hunter away, default options.
fn ctx<'a>(
    area: &'a Area,
    hunter: HunterView,
    t: u32,
    rng: &'a mut MsRand,
    events: &'a mut Vec<DinoEvent>,
    dt: i32,
) -> Ctx<'a> {
    Ctx {
        area,
        rng,
        dt,
        real_time: t as i32 * STEP_MS,
        hunter,
        target: Default::default(),
        struck: None,
        opt_agres: 128,
        tranq: false,
        view_r: 42,
        render_r: 42,
        run_k: 1.0,
        events,
    }
}
