//! Bounded fleeing (an opt-in setting, not in the original game).
//!
//! The original aims a frightened animal at a point 2048 units straight away
//! from the hunter, with no check that the point is playable, and keeps its
//! fear timer armed for as long as noises or the hunter's closeness repeat.
//! Animals therefore pile against the map's edge or run off it. With bounded
//! fleeing on ([`Herd::flee`](super::Herd)), this module:
//!
//! - picks the flee target among headings around "directly away", keeping
//!   to the playable square, clear of blocked ground and prohibited areas
//!   ([`pick_target`]);
//! - ends a flight at a distance or duration cap, unless the hunter is still
//!   closing in, and then only by a bounded extension ([`step`]);
//! - stops repeated noises from re-arming fear ([`Character::scare`]).
//!
//! With the setting off none of this runs and behaviour is the original's.
//! Limits come from `HUNTDAT/_FLEE.TXT` ([`c2_formats::flee`]) or the
//! built-in defaults; see `docs/DECISIONS.md`.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, vec, vec::Vec};

use c2_formats::flee as fmt;
use c2_formats::mee::SpawnGroup;

use super::collide::{
    check_place_collision_aquatic, check_place_collision_brahi_p, check_place_collision_fish_p,
    check_place_collision_land_brahi,
};
use super::eco::Eco;
use super::{check_place_collision_p, in_region, Character, Ctx, Species};
use crate::area::Area;
use crate::collide::WALK_CELLS;
use crate::CELL;

/// Units a flight aims at in one leg (the original's).
pub const LEG: f32 = fmt::LEG_UNITS as f32;
/// The fear timer the AIs re-arm while the hunter is near, ms.
pub const PROXIMITY_FEAR_MS: i32 = 8000;
/// How near that is, units.
pub const PROXIMITY_UNITS: f32 = fmt::NEAR_UNITS as f32;
/// Cells kept clear of the playable square's edge.
const MARGIN_CELLS: i32 = 4;
/// Distance between collision samples along a candidate heading.
const SAMPLE: f32 = 256.0;
/// A flight re-aims this often (ms), and not at all in between.
const REPICK_MS: i32 = 400;
/// Blocked-path counts (`no_way_cnt`) before a flight re-aims, and how many
/// such stalls end it.
const STALL_COUNT: i32 = 4;
const MAX_STALLS: i32 = 3;
/// The hunter must have come this much closer over a second to count as
/// still pursuing (units).
const CLOSING: f32 = 128.0;
/// Alert range inside which a hunter can pursue (the AI's own).
const PURSUIT_RANGE: f32 = PROXIMITY_UNITS;

/// One animal's limits, in engine units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleeLimits {
    /// Map units a flight may cover.
    pub max_distance: i32,
    /// Milliseconds.
    pub max_duration: i32,
    /// Milliseconds a pursuing hunter may extend a flight by.
    pub chase_extension: i32,
    /// Milliseconds after a scare during which noises are ignored.
    pub retrigger_lockout: i32,
}

impl FleeLimits {
    /// From the file's units: cells and seconds, in `resolved()` order.
    pub fn from_resolved([cells, secs, chase, lock]: [u32; 4]) -> Self {
        Self {
            max_distance: (cells as f32 * CELL) as i32,
            max_duration: secs as i32 * 1000,
            chase_extension: chase as i32 * 1000,
            retrigger_lockout: lock as i32 * 1000,
        }
    }
}

impl Default for FleeLimits {
    fn default() -> Self {
        Self::from_resolved(fmt::Limits::default().resolved())
    }
}

/// The limits of every species (index = `Character::ctype`).
#[derive(Debug, Clone, Default)]
pub struct FleeTable {
    pub per_species: Vec<FleeLimits>,
}

impl FleeTable {
    /// The table from the settings file's bytes (`None`: built-in defaults).
    pub fn load(bytes: Option<&[u8]>, species: &[Species]) -> Box<Self> {
        let file = bytes.map(fmt::Flee::parse).unwrap_or_default();
        Box::new(Self {
            per_species: species
                .iter()
                .map(|s| FleeLimits::from_resolved(file.for_animal(&s.info.name)))
                .collect(),
        })
    }

    pub fn limits(&self, ctype: usize) -> FleeLimits {
        self.per_species.get(ctype).copied().unwrap_or_default()
    }
}

/// One character's current flight.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flight {
    pub active: bool,
    /// Where it began (x, z).
    pub origin: [f32; 2],
    /// Ms since it began, and ms of chase extension used.
    pub time: i32,
    pub extended: i32,
    /// Ms until the target is re-aimed, and stalls so far.
    pub repick_in: i32,
    pub stalls: i32,
    /// The hunter's distance a second ago, and when to sample it next.
    pub probe_dist: f32,
    pub probe_in: i32,
    /// The hunter has closed in over the last second.
    pub closing: bool,
    /// A noise cannot re-arm fear for this many more ms.
    pub lock: i32,
}

impl Flight {
    /// Whether the hunter is still after the animal.
    pub fn pursued(&self, aware: bool, pdist: f32) -> bool {
        aware && pdist < PURSUIT_RANGE && self.closing
    }
}

impl Character {
    /// Frightens it for `ticks` ms from a noise or the hunter's closeness.
    /// With `limits`, a repeat inside the lockout is ignored and a scare
    /// never lengthens a timer already running. Returns whether it took.
    pub fn scare(&mut self, ticks: i32, limits: Option<FleeLimits>) -> bool {
        let Some(l) = limits else {
            self.afraid_time = ticks;
            return true;
        };
        if self.flight.lock > 0 {
            return false;
        }
        self.afraid_time = self.afraid_time.max(ticks);
        self.flight.lock = l.retrigger_lockout;
        true
    }

    /// Time passing outside a flight: the lockout runs down, and a calm
    /// animal is not in flight.
    pub fn flee_tick(&mut self, dt: i32) {
        self.flight.lock = (self.flight.lock - dt).max(0);
        if self.state == 0 {
            self.flight.active = false;
        }
    }
}

/// The ecosystem zones a flight keeps to or out of: the rules, the
/// animal's home zone and where it stands.
#[derive(Clone, Copy)]
pub struct Fence<'a> {
    pub eco: &'a Eco,
    pub home: Option<u8>,
    pub from: (f32, f32),
}

/// What an animal moves through, which decides where a flight may aim.
/// Each is the placement check its AI already uses for new targets.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Medium {
    /// Dry, unblocked ground (herbivores, the small animals).
    #[default]
    Land,
    /// Ground or water, as long as it is not blocked (the carnivores and
    /// swimming ambient animals wade).
    Wet,
    /// Open water between `lo` and `hi` deep (the dialect's fish).
    Water { lo: f32, hi: f32 },
    /// Water no deeper than the wading brachiosaur stands in.
    Wading,
    /// The land brachiosaur: no water at all (`dry`), or none deeper than
    /// `level`.
    Shore { dry: bool, level: f32 },
    /// Nothing on the ground matters (flyers).
    Air,
}

impl Medium {
    fn blocked(self, area: &Area, x: f32, z: f32) -> bool {
        let mut p = [x, 0.0, z];
        match self {
            Medium::Land => check_place_collision_p(area, &mut p),
            Medium::Wet => check_place_collision_aquatic(area, &mut p),
            Medium::Water { lo, hi } => check_place_collision_fish_p(area, &p, lo, hi),
            Medium::Wading => check_place_collision_brahi_p(area, &mut p),
            Medium::Shore { dry, level } => {
                // The check compares the ground with the walker's height
                // (a step too high); a far point has only its own.
                p[1] = area.land_height(x, z);
                check_place_collision_land_brahi(area, &mut p, dry, true, &[], level)
            }
            Medium::Air => false,
        }
    }
}

/// Where a kind of animal lives: its spawn group (the dialect's avoided
/// regions) and what it moves through.
#[derive(Clone, Copy, Default)]
pub struct Habitat<'a> {
    pub group: Option<&'a SpawnGroup>,
    pub medium: Medium,
}

impl<'a> Habitat<'a> {
    pub fn land(group: Option<&'a SpawnGroup>) -> Self {
        Self {
            group,
            medium: Medium::Land,
        }
    }
}

/// Where an animal may flee: its habitat and the ecosystem's zones.
#[derive(Clone, Copy, Default)]
pub struct Keep<'a> {
    pub group: Option<&'a SpawnGroup>,
    pub medium: Medium,
    pub fence: Option<Fence<'a>>,
}

/// Can an animal stand at (x, z)? `margin`: also keep clear of the edge.
fn walkable(area: &Area, keep: Keep<'_>, x: f32, z: f32, margin: bool) -> bool {
    let group = keep.group;
    if keep
        .fence
        .is_some_and(|f| !f.eco.may_aim(f.home, f.from, x, z))
    {
        return false;
    }
    let (cx, cz) = ((x / CELL) as i32, (z / CELL) as i32);
    let m = if margin { MARGIN_CELLS } else { 0 };
    let ok = |c: i32| (*WALK_CELLS.start() + m..=*WALK_CELLS.end() - m).contains(&c);
    if !ok(cx) || !ok(cz) {
        return false;
    }
    if group.is_some_and(|g| g.avoid.iter().any(|r| in_region(r, x, z))) {
        return false;
    }
    !keep.medium.blocked(area, x, z)
}

/// Is the straight way from (x, z) along (`dx`, `dz`) for `len` units clear?
/// Only where it ends must be clear of the edge: an animal already inside
/// the margin has to be able to leave it.
fn path_clear(
    area: &Area,
    group: Keep<'_>,
    (x, z): (f32, f32),
    (dx, dz): (f32, f32),
    len: f32,
) -> bool {
    let mut d = SAMPLE.min(len);
    loop {
        let end = d >= len;
        if !walkable(area, group, x + dx * d, z + dz * d, end) {
            return false;
        }
        if end {
            return true;
        }
        d = (d + SAMPLE).min(len);
    }
}

/// Where a flight from `c` aims: a point up to `reach` units off along the
/// first clear heading around `away` (a unit vector from the hunter to the
/// animal), trying +-0, 20, 40 ... 180 degrees, `first` (+1 or -1) side
/// first, and shorter legs when none is. Candidates must not end nearer the
/// hunter than the animal stands. `skip` rejects headings near an angle (a
/// blocked one). A cornered animal gets the clear candidate that gains most
/// distance from the hunter, or stays put.
pub fn pick_target(
    c: &Character,
    area: &Area,
    group: Option<&SpawnGroup>,
    away: (f32, f32),
    hunter: (f32, f32),
    reach: f32,
    first: f32,
    skip: Option<f32>,
) -> (f32, f32) {
    let keep = Keep {
        group,
        ..Keep::default()
    };
    pick_target_kept(c, area, keep, away, hunter, reach, first, skip)
}

/// [`pick_target`] that also keeps to the ecosystem's zones (`keep`).
#[allow(clippy::too_many_arguments)]
pub fn pick_target_kept(
    c: &Character,
    area: &Area,
    group: Keep<'_>,
    away: (f32, f32),
    hunter: (f32, f32),
    reach: f32,
    first: f32,
    skip: Option<f32>,
) -> (f32, f32) {
    let base = away.1.atan2(away.0);
    let now = ((c.pos[0] - hunter.0).powi(2) + (c.pos[2] - hunter.1).powi(2)).sqrt();
    let mut best: Option<(f32, (f32, f32))> = None;
    for scale in [1.0, 0.5, 0.25] {
        let len = (reach * scale).max(CELL * 2.0);
        for k in 0..=18 {
            for side in [first, -first] {
                if k == 0 && side != first {
                    continue;
                }
                let off = side * (k as f32 * 20.0).to_radians();
                if skip.is_some_and(|s| {
                    let mut d = (base + off - s) % core::f32::consts::TAU;
                    if d > core::f32::consts::PI {
                        d -= core::f32::consts::TAU;
                    } else if d < -core::f32::consts::PI {
                        d += core::f32::consts::TAU;
                    }
                    d.abs() < 20.0f32.to_radians()
                }) {
                    continue;
                }
                let (dx, dz) = ((base + off).cos(), (base + off).sin());
                if !path_clear(area, group, (c.pos[0], c.pos[2]), (dx, dz), len) {
                    continue;
                }
                let end = (c.pos[0] + dx * len, c.pos[2] + dz * len);
                let gain = ((end.0 - hunter.0).powi(2) + (end.1 - hunter.1).powi(2)).sqrt() - now;
                if gain >= 0.0 {
                    return end;
                }
                if best.is_none_or(|(g, _)| gain > g) {
                    best = Some((gain, end));
                }
            }
        }
    }
    best.map_or((c.pos[0], c.pos[2]), |(_, p)| p)
}

/// One step of a flight from the hunter. Returns the target to aim at, or
/// `None` when the flight is over (the caller calms the animal).
/// `habitat` is where the animal may go, `away` the unit vector from the
/// hunter to the animal, `pdist` the hunter's distance, `aware` whether the
/// animal knows about him.
pub fn step(
    c: &mut Character,
    ctx: &mut Ctx<'_>,
    limits: FleeLimits,
    habitat: Habitat<'_>,
    away: (f32, f32),
    pdist: f32,
    aware: bool,
) -> Option<(f32, f32)> {
    let dt = ctx.dt;
    let hunter = (ctx.target.view.x, ctx.target.view.z);
    let eco = ctx.eco.clone();
    let keep = |c: &Character| Keep {
        group: habitat.group,
        medium: habitat.medium,
        fence: eco.as_deref().map(|eco| Fence {
            eco,
            home: c.home_zone,
            from: (c.pos[0], c.pos[2]),
        }),
    };
    if !c.flight.active {
        c.flight = Flight {
            active: true,
            origin: [c.pos[0], c.pos[2]],
            probe_dist: pdist,
            probe_in: 1000,
            lock: c.flight.lock,
            ..Flight::default()
        };
        let first = if ctx.rng.r_rand(2) == 0 { 1.0 } else { -1.0 };
        let t = pick_target_kept(
            c,
            ctx.area,
            keep(c),
            away,
            hunter,
            LEG.min(limits.max_distance as f32),
            first,
            None,
        );
        (c.tgx, c.tgz) = t;
    }
    let f = &mut c.flight;
    f.time += dt;
    f.probe_in -= dt;
    if f.probe_in <= 0 {
        f.closing = pdist < f.probe_dist - CLOSING;
        f.probe_dist = pdist;
        f.probe_in = 1000;
    }
    let travelled = ((c.pos[0] - f.origin[0]).powi(2) + (c.pos[2] - f.origin[1]).powi(2)).sqrt();
    if travelled >= limits.max_distance as f32 || f.time >= limits.max_duration {
        if f.pursued(aware, pdist) && f.extended < limits.chase_extension {
            f.extended += dt;
        } else {
            f.active = false;
            return None;
        }
    }
    // A stalled flight (a slope or obstacle the heading keeps hitting)
    // re-aims away from the blocked heading; too many stalls end it.
    let stalled = c.no_way_cnt > STALL_COUNT;
    if stalled {
        c.no_way_cnt = 0;
        c.flight.stalls += 1;
        if c.flight.stalls > MAX_STALLS {
            c.flight.active = false;
            return None;
        }
    }
    c.flight.repick_in -= dt;
    let reached = ((c.tgx - c.pos[0]).powi(2) + (c.tgz - c.pos[2]).powi(2)).sqrt() < CELL;
    if stalled || reached || c.flight.repick_in <= 0 {
        c.flight.repick_in = REPICK_MS;
        let left = (limits.max_distance as f32 - travelled).max(CELL * 2.0);
        let first = if ctx.rng.r_rand(2) == 0 { 1.0 } else { -1.0 };
        let skip = stalled.then(|| c.alpha);
        (c.tgx, c.tgz) = pick_target_kept(
            c,
            ctx.area,
            keep(c),
            away,
            hunter,
            LEG.min(left),
            first,
            skip,
        );
    }
    Some((c.tgx, c.tgz))
}
