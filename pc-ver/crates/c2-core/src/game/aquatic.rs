//! Aquatic animals that keep near the hunter (not in the original): for
//! the consoles, whose memory holds only an animal or two of a fish or a
//! wader, so the few there are should be where the hunter can meet them.
//!
//! Each one is put in the closest water of its kind at least
//! [`NEAR_M`] metres from the hunter. One left more than [`FAR_M`] metres
//! behind is taken out and put back the same way, unless no water is
//! closer than where it is. Off unless [`Hunt::roam_aquatics`] is called.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::Hunt;
use crate::area::Area;
use crate::dino::Habitat;
use crate::CELL;
use c2_formats::map::MAP_SIZE;

/// World units to a metre (a kill's range is shown in these).
const METRE: f32 = 64.0;
/// An aquatic animal appears at least this far from the hunter, metres.
pub const NEAR_M: f32 = 200.0;
/// Farther than this from the hunter, it is put back near, metres.
pub const FAR_M: f32 = 400.0;
/// The search's step, outwards and along a ring: 4 cells.
const STEP: f32 = 4.0 * CELL;
/// A new spot must be this much closer than the old one to be taken.
const GAIN: f32 = 16.0 * CELL;
/// How often the animals' distances are looked at, ms.
const EVERY_MS: i32 = 1000;

/// The aquatic animals kept near the hunter.
#[derive(Debug, Clone, Default)]
pub struct AquaticRoam {
    /// Species (character type) and how many of each.
    pub kinds: Vec<(usize, usize)>,
    /// Time since the last look, ms.
    ms: i32,
    /// Where the hunter stood when no closer water was found: no search
    /// is made again until the hunter has moved on.
    stuck_at: Option<(f32, f32)>,
}

/// Whether `area` has water at all (a fish's depths are checked when it
/// is placed).
pub fn has_water(area: &Area) -> bool {
    (0..MAP_SIZE).step_by(2).any(|cz| {
        (0..MAP_SIZE)
            .step_by(2)
            .any(|cx| area.water_level(cx, cz).is_some())
    })
}

impl Hunt {
    /// The closest spot at least [`NEAR_M`] from the hunter where an
    /// animal of `ctype` can be: rings around the hunter, outwards, each
    /// walked from `turn` (radians).
    fn aquatic_spot(&self, area: &Area, ctype: usize, turn: f32) -> Option<[f32; 3]> {
        if self.herd.habitat(ctype) == Habitat::Land {
            return None;
        }
        let (px, pz) = (self.player.x, self.player.z);
        let span = MAP_SIZE as f32 * CELL;
        let mut r = NEAR_M * METRE;
        while r < span * 1.5 {
            let n = (core::f32::consts::TAU * r / STEP) as usize + 1;
            for k in 0..n {
                let a = turn + core::f32::consts::TAU * k as f32 / n as f32;
                let (x, z) = (px + r * a.cos(), pz + r * a.sin());
                if !(0.0..span).contains(&x) || !(0.0..span).contains(&z) {
                    continue;
                }
                let mut p = [x, area.land_height(x, z), z];
                if !self.herd.spot_blocked(area, ctype, &mut p) {
                    self.herd.swim_height(area, ctype, &mut p);
                    return Some(p);
                }
            }
            r += STEP;
        }
        None
    }

    /// A direction for a search to start in, from the hunt's own numbers.
    fn aquatic_turn(&mut self) -> f32 {
        self.rng.r_rand(628) as f32 / 100.0
    }

    /// Puts character `i` (an aquatic animal) at `p`, as newly placed.
    fn settle_aquatic(&mut self, area: &Area, i: usize, p: [f32; 3]) {
        let opt_agres = self.opts.opt_agres;
        let Some(ctype) = self.herd.chars.get(i).map(|c| c.ctype) else {
            return;
        };
        let habitat = self.herd.habitat(ctype);
        let c = &mut self.herd.chars[i];
        c.pos = p;
        c.tgx = p[0];
        c.tgz = p[2];
        c.tgtime = 0;
        c.spawn_group = None;
        if let Some(sp) = self.herd.species.get(c.ctype) {
            c.reset(sp, &mut self.rng, opt_agres);
        }
        match habitat {
            Habitat::Swims(..) => {
                c.depth = p[1];
                c.tdepth = p[1];
            }
            Habitat::Wades => c.spawn_alt = area.land_up_h(p[0], p[2]),
            Habitat::Land => {}
        }
    }

    /// Turns the roaming on for `kinds` (species, how many): any of them
    /// the map's own tables placed are taken out, and each is put in the
    /// closest water of its kind. Returns how many were placed (a species
    /// with no water it can use on the map gets none). Call after the
    /// animals are placed, before the first tick.
    pub fn roam_aquatics(&mut self, area: &Area, kinds: &[(usize, usize)]) -> usize {
        let is_kind = |ct: usize| kinds.iter().any(|k| k.0 == ct);
        self.herd.cap_population(0, is_kind);
        let mut placed = 0;
        let mut kept = Vec::new();
        for &(ctype, count) in kinds {
            let mut n = 0;
            for _ in 0..count {
                let turn = self.aquatic_turn();
                let Some(p) = self.aquatic_spot(area, ctype, turn) else {
                    break;
                };
                if !self
                    .herd
                    .spawn(ctype, p, &mut self.rng, self.opts.opt_agres)
                {
                    break;
                }
                let i = self.herd.chars.len() - 1;
                self.settle_aquatic(area, i, p);
                n += 1;
            }
            if n > 0 {
                kept.push((ctype, n));
            }
            placed += n;
        }
        self.aquatic = (!kept.is_empty()).then(|| AquaticRoam {
            kinds: kept,
            ..Default::default()
        });
        placed
    }

    /// Once a second: one living aquatic animal more than [`FAR_M`] from
    /// the hunter is put back in the closest water, if that is closer.
    pub(super) fn step_aquatics(&mut self, area: &Area, dt: i32) {
        let Some(roam) = &mut self.aquatic else {
            return;
        };
        roam.ms += dt;
        if roam.ms < EVERY_MS {
            return;
        }
        roam.ms = 0;
        let (px, pz) = (self.player.x, self.player.z);
        let moved = |at: (f32, f32)| (at.0 - px).abs() + (at.1 - pz).abs() >= GAIN;
        if roam.stuck_at.is_some_and(|at| !moved(at)) {
            return;
        }
        let kinds = roam.kinds.clone();
        let away = |p: &[f32; 3]| ((p[0] - px).powi(2) + (p[2] - pz).powi(2)).sqrt();
        let far = self.herd.chars.iter().position(|c| {
            c.health > 0 && kinds.iter().any(|k| k.0 == c.ctype) && away(&c.pos) > FAR_M * METRE
        });
        let Some(i) = far else {
            return;
        };
        let (ctype, was) = (self.herd.chars[i].ctype, away(&self.herd.chars[i].pos));
        let turn = self.aquatic_turn();
        let spot = self
            .aquatic_spot(area, ctype, turn)
            .filter(|p| away(p) + GAIN < was);
        match spot {
            Some(p) => self.settle_aquatic(area, i, p),
            // The closest water is where it is: wait for the hunter to move.
            None => {
                if let Some(r) = &mut self.aquatic {
                    r.stuck_at = Some((px, pz));
                }
            }
        }
    }
}
