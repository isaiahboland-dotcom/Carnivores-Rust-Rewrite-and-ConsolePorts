//! Spawn ecosystems (`_WORLD.TXT`'s `eco` lines, [`c2_formats::ecosystem`];
//! not in the original game): which animals may appear in which zone of
//! the map, how many, and whether they keep to it.
//!
//! The rules steer the game's own placement and AIs rather than replace
//! them: a spot the rules refuse is one more spot the placement loop
//! skips, inside its own bounded tries, so an impossible rule leaves an
//! animal out instead of hanging the hunt. Nothing already on the map is
//! removed or moved. Off ([`Herd::eco`](super::Herd) `None`), every path
//! here is skipped and the random stream is the original's.
//!
//! [`Eco`] is a hunt's rules, fixed once loaded; [`State`] is what the herd
//! keeps beside them: the time of day in force, where the hunter looks, and
//! the replacements that found no spot yet.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
use alloc::sync::Arc;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use c2_formats::ecosystem::{Contain, ZoneRule, DEFAULT_DISTANCE};
use c2_formats::world::World;
use c2_formats::zone::{Point, Shape};

use super::{index_named, Character, HunterView, Species};
use crate::area::TimeOfDay;
use crate::rng::MsRand;

/// World units per map cell.
const CELL: f32 = 256.0;
/// A step that moves an animal farther than this (x plus z) re-placed it
/// (`ReplaceCharacterForward`) rather than walked it.
const JUMP: f32 = 8.0 * CELL;
/// Soft containment: of 4 wander targets outside the zone, 3 are pulled
/// back in.
const SOFT_PULL: i32 = 3;

/// How long a replacement that found no spot waits for its next try, ms.
const RETRY_MS: i32 = 5000;
/// Replacements kept waiting at most (the oldest stay).
const MAX_DEFERRED: usize = 16;

#[derive(Debug, Clone)]
struct Zone {
    shape: Shape,
    /// A point inside, to head for from outside.
    anchor: Point,
    contain: Contain,
    /// Wandering and fleeing animals keep out.
    avoid: bool,
}

/// A zone's spawn rule at one time of day.
#[derive(Debug, Clone)]
struct ZoneTable {
    no_spawn: bool,
    cap: Option<usize>,
    /// By species: how readily it appears here (0: never).
    weights: Vec<f32>,
}

/// The rules in force at one time of day.
#[derive(Debug, Clone)]
struct Table {
    /// By zone ([`Eco::zones`]).
    zones: Vec<ZoneTable>,
    max_animals: Option<usize>,
    /// By species ([`Eco::group`]'s): the most alive at once.
    species_max: Vec<Option<usize>>,
    /// By species: the largest weight a zone gives it, at least 1.
    top: Vec<f32>,
}

/// One hunt's rules, resolved for its map and species.
#[derive(Debug, Clone)]
pub struct Eco {
    zones: Vec<Zone>,
    /// By time of day ([`TimeOfDay`] as an index).
    tables: [Table; 3],
    /// By species: the species its cap is counted under (a variant counts
    /// as its owner).
    group: Vec<usize>,
    /// Least distance from the hunter, world units: at the start of a
    /// hunt, for a replacement, for the instance rules' extra animals.
    distance: [f32; 3],
    /// By species: its own least distance, world units (0: none).
    species_distance: Vec<f32>,
    /// Replacements appear only where the hunter is not looking.
    unseen: bool,
}

/// What a new animal is, for its distance from the hunter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spawn {
    /// Placed at the start of the hunt.
    Initial = 0,
    /// A replacement for a killed animal.
    Respawn = 1,
    /// One of the instance rules' extra animals.
    Extra = 2,
}

/// Whether a spot `(dx, dz)` from the hunter is at least the original's 40
/// cells off (x plus z), the distance with the setting off.
pub fn original_distance(dx: f32, dz: f32) -> bool {
    dx.abs() + dz.abs() >= CELL * DEFAULT_DISTANCE as f32
}

/// A replacement waiting for a spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deferred {
    pub ctype: usize,
    /// The dialect spawn group it belongs to.
    pub group: Option<usize>,
}

/// The herd's ecosystem: the rules and what changes during the hunt.
#[derive(Debug, Clone)]
pub struct State {
    pub rules: Arc<Eco>,
    /// The time of day in force ([`TimeOfDay`] as an index).
    time: usize,
    /// The hunter as of the last step, and how far he sees (world units).
    view: Option<(HunterView, f32)>,
    /// Replacements that found no spot, oldest first.
    pub deferred: Vec<Deferred>,
    retry_in: i32,
}

/// An animal before its AI step, for [`State::after_step`].
pub struct Before(Character);

/// A point of the shape's inside: a circle's centre; a polygon's centroid,
/// or (a concave one's may lie outside) the centre of one of its corners'
/// triangles.
fn anchor(shape: &Shape) -> Point {
    let points = match shape {
        Shape::Circle { center, .. } => return *center,
        Shape::Polygon(p) => p,
    };
    let n = points.len().max(1);
    let mean = |ps: &[Point]| {
        let k = ps.len().max(1) as f32;
        [
            ps.iter().map(|p| p[0]).sum::<f32>() / k,
            ps.iter().map(|p| p[1]).sum::<f32>() / k,
        ]
    };
    let c = mean(points);
    if shape.contains(c[0], c[1]) {
        return c;
    }
    (0..n)
        .map(|i| mean(&[points[i], points[(i + 1) % n], points[(i + 2) % n]]))
        .find(|p| shape.contains(p[0], p[1]))
        .unwrap_or(c)
}

impl Eco {
    /// The rules of `world` for the map named `stem` (`area3`, a
    /// `.c2map`'s). `None` when they change nothing there.
    fn load(world: &World, stem: &str, species: &[Species]) -> Option<Self> {
        let n = species.len();
        let named = |name: &str| {
            let ct = index_named(species.iter().map(|s| s.info.name.as_str()), name);
            #[cfg(feature = "std")]
            if ct.is_none() {
                tracing::warn!(animal = %name, "ecosystem: no such animal in this game");
            }
            ct
        };
        let group: Vec<usize> = (0..n)
            .map(|ct| species[ct].variant.as_ref().map_or(ct, |v| v.owner))
            .collect();
        // A variant without a rule of its own follows its owner's.
        let spread = |set: &[bool], v: &mut [f32], fill: f32| {
            for ct in 0..n {
                if !set[ct] {
                    v[ct] = if set[group[ct]] { v[group[ct]] } else { fill };
                }
            }
        };
        // With an allow list the others never appear; a weight then scales
        // the listed ones.
        let weights_of = |r: &ZoneRule| {
            let mut weights = vec![1.0f32; n];
            let mut set = vec![false; n];
            for name in &r.allow {
                if let Some(ct) = named(name) {
                    set[ct] = true;
                }
            }
            if !r.allow.is_empty() {
                spread(&set, &mut weights, 0.0);
            }
            let mut set = vec![false; n];
            let mut scale = vec![1.0f32; n];
            for (name, w) in &r.weights {
                if let Some(ct) = named(name) {
                    scale[ct] = w.max(0.0);
                    set[ct] = true;
                }
            }
            spread(&set, &mut scale, 1.0);
            for ct in 0..n {
                weights[ct] *= scale[ct];
            }
            weights
        };
        let ruled: Vec<&c2_formats::world::Zone> = world
            .area(stem)
            .map_or(&[][..], |a| a.zones.as_slice())
            .iter()
            .filter(|z| !z.rule.is_empty())
            .collect();
        let zones: Vec<Zone> = ruled
            .iter()
            .map(|z| Zone {
                anchor: anchor(&z.shape),
                shape: z.shape.clone(),
                contain: z.rule.contain,
                avoid: z.rule.avoid,
            })
            .collect();
        let table = |time: usize| {
            let caps = world.caps.at_time(time);
            let mut species_max = vec![None; n];
            for (name, max) in &caps.species {
                if let Some(ct) = named(name) {
                    species_max[group[ct]] = Some(*max as usize);
                }
            }
            let zones: Vec<ZoneTable> = ruled
                .iter()
                .map(|z| {
                    let r = z.rule.at_time(time);
                    ZoneTable {
                        no_spawn: r.no_spawn,
                        cap: r.cap.map(|c| c as usize),
                        weights: weights_of(&r),
                    }
                })
                .collect();
            let top = (0..n)
                .map(|ct| zones.iter().map(|z| z.weights[ct]).fold(1.0f32, f32::max))
                .collect();
            Table {
                zones,
                max_animals: caps.max_animals.map(|m| m as usize),
                species_max,
                top,
            }
        };
        let tables = [table(0), table(1), table(2)];
        let s = &world.safety;
        let capped = tables
            .iter()
            .any(|t| t.max_animals.is_some() || t.species_max.iter().any(Option::is_some));
        if zones.is_empty() && !capped && s.is_empty() {
            return None;
        }
        let cells = |v: Option<u32>| v.unwrap_or(DEFAULT_DISTANCE) as f32 * CELL;
        let mut species_distance = vec![0.0f32; n];
        for (name, d) in &s.species {
            if let Some(ct) = named(name) {
                species_distance[ct] = *d as f32 * CELL;
            }
        }
        Some(Self {
            zones,
            tables,
            group,
            distance: [cells(s.initial), cells(s.respawn), cells(s.extra)],
            species_distance,
            unseen: s.unseen,
        })
    }

    fn inside(&self, zone: usize, x: f32, z: f32) -> bool {
        self.zones[zone].shape.contains(x / CELL, z / CELL)
    }

    fn hard(&self, home: Option<u8>) -> Option<usize> {
        home.map(usize::from).filter(|&h| {
            self.zones
                .get(h)
                .is_some_and(|z| z.contain == Contain::Hard)
        })
    }

    /// Whether an animal standing at `from` may aim for (x, z) when it
    /// flees or wanders: not into a zone to avoid (one it is already in
    /// does not count, so it can leave), nor out of its hard zone `home`.
    pub fn may_aim(&self, home: Option<u8>, from: (f32, f32), x: f32, z: f32) -> bool {
        let into_avoided = (0..self.zones.len()).any(|i| {
            self.zones[i].avoid && self.inside(i, x, z) && !self.inside(i, from.0, from.1)
        });
        if into_avoided {
            return false;
        }
        self.hard(home)
            .is_none_or(|h| self.inside(h, x, z) || !self.inside(h, from.0, from.1))
    }
}

impl State {
    /// The ecosystem of `world` for the map named `stem` (`area3`, a
    /// `.c2map`'s), starting at time of day `time`. `None` when the rules
    /// change nothing there.
    pub fn load(world: &World, stem: &str, species: &[Species], time: TimeOfDay) -> Option<Self> {
        Some(Self {
            rules: Arc::new(Eco::load(world, stem, species)?),
            time: time as usize,
            view: None,
            deferred: Vec::new(),
            retry_in: RETRY_MS,
        })
    }

    /// The time of day changed (dynamic time): its rules are in force for
    /// what is placed from now on. Nothing on the map is removed.
    pub fn set_time(&mut self, time: TimeOfDay) {
        self.time = time as usize;
    }

    /// The hunter as the step sees him, and the cells he sees.
    pub fn set_view(&mut self, hunter: HunterView, render_r: i32) {
        self.view = Some((hunter, (render_r * 256) as f32));
    }

    fn table(&self) -> &Table {
        &self.rules.tables[self.time.min(2)]
    }

    /// Whether one more animal of `ctype` fits under the map's caps.
    /// Bodies do not count.
    pub fn room_for(&self, ctype: usize, chars: &[Character]) -> bool {
        let (e, t) = (&*self.rules, self.table());
        let alive = |c: &&Character| c.health > 0;
        if t.max_animals
            .is_some_and(|max| chars.iter().filter(alive).count() >= max)
        {
            return false;
        }
        let Some(&g) = e.group.get(ctype) else {
            return true;
        };
        t.species_max[g].is_none_or(|max| {
            let same = |c: &&Character| e.group.get(c.ctype) == Some(&g);
            chars.iter().filter(alive).filter(same).count() < max
        })
    }

    /// Whether an animal of `ctype` may be put at (x, z), world units:
    /// every zone the spot is in must take it (not a no-spawn zone, the
    /// animal allowed, the zone under its cap), and the spot is then kept
    /// with the chance its weight gives it. `skip`: an animal not counted
    /// (the one being moved). Draws from `rng` only for a species some zone
    /// weights above or below 1.
    pub fn spot_ok(
        &self,
        ctype: usize,
        x: f32,
        z: f32,
        chars: &[Character],
        skip: Option<usize>,
        rng: &mut MsRand,
    ) -> bool {
        let (e, t) = (&*self.rules, self.table());
        let mut weight = 1.0f32;
        for (i, zone) in t.zones.iter().enumerate() {
            if !e.inside(i, x, z) {
                continue;
            }
            let w = zone.weights.get(ctype).copied().unwrap_or(1.0);
            if zone.no_spawn || w <= 0.0 {
                return false;
            }
            let full = zone.cap.is_some_and(|cap| {
                chars
                    .iter()
                    .enumerate()
                    .filter(|(j, c)| {
                        Some(*j) != skip && c.health > 0 && e.inside(i, c.pos[0], c.pos[2])
                    })
                    .count()
                    >= cap
            });
            if full {
                return false;
            }
            weight *= w;
        }
        let top = t.top.get(ctype).copied().unwrap_or(1.0);
        // Every spot at the top weight: nothing to draw for.
        if weight >= top {
            return true;
        }
        (rng.r_rand(9999) as f32) < weight / top * 10000.0
    }

    /// Whether a new animal of `ctype` at `p` is far enough from the hunter
    /// at (`hx`, `hz`): its kind's distance or the species' own, whichever
    /// is larger (x plus z, as the original measures it), and for a
    /// replacement out of his sight when the rules ask for that.
    pub fn far_enough(
        &self,
        kind: Spawn,
        ctype: usize,
        p: &[f32; 3],
        (hx, hz): (f32, f32),
    ) -> bool {
        let e = &*self.rules;
        let own = e.species_distance.get(ctype).copied().unwrap_or(0.0);
        if (p[0] - hx).abs() + (p[2] - hz).abs() < e.distance[kind as usize].max(own) {
            return false;
        }
        !(kind == Spawn::Respawn && e.unseen && self.sees(p[0], p[2]))
    }

    /// Whether the hunter is looking towards (x, z) and it is within his
    /// sight: within 60 degrees of where he faces.
    fn sees(&self, x: f32, z: f32) -> bool {
        let Some((h, sight)) = self.view else {
            return false;
        };
        let (dx, dz) = (x - h.x, z - h.z);
        let d = (dx * dx + dz * dz).sqrt();
        if d >= sight {
            return false;
        }
        // He faces (sin alpha, -cos alpha), as `ReplaceCharacterForward`.
        let (sa, ca) = h.alpha.sin_cos();
        d < CELL || (dx * sa - dz * ca) / d > 0.5
    }

    /// The zone an animal put at (x, z) keeps to: the first one there that
    /// contains its animals.
    pub fn home(&self, x: f32, z: f32) -> Option<u8> {
        let e = &*self.rules;
        (0..e.zones.len().min(usize::from(u8::MAX)))
            .find(|&i| e.zones[i].contain != Contain::Free && e.inside(i, x, z))
            .map(|i| i as u8)
    }

    /// A replacement found no spot: it is tried again later
    /// ([`Self::due`]). Beyond a few, the newest are dropped.
    pub fn defer(&mut self, d: Deferred) {
        if self.deferred.len() < MAX_DEFERRED {
            self.deferred.push(d);
        }
    }

    /// Time passes: the replacements to try again now (taken out; the
    /// caller defers those that fail again).
    pub fn due(&mut self, dt: i32) -> Vec<Deferred> {
        if self.deferred.is_empty() {
            self.retry_in = RETRY_MS;
            return Vec::new();
        }
        self.retry_in -= dt;
        if self.retry_in > 0 {
            return Vec::new();
        }
        self.retry_in = RETRY_MS;
        core::mem::take(&mut self.deferred)
    }

    /// Before character `c`'s AI step.
    pub fn before(&self, c: &Character) -> Before {
        Before(c.clone())
    }

    /// After character `index`'s AI step.
    ///
    /// - Re-placed ahead of the hunter (`ReplaceCharacterForward`): the new
    ///   spot must pass [`Self::spot_ok`] (and be in its zone, for a hard
    ///   one), else the step is undone; its home is then the new spot's.
    /// - Hard containment: a step out of its zone slides along the edge.
    /// - A calm animal's new wander target is kept in its zone (soft: 3
    ///   times in 4) and out of the zones to avoid: pulled back along its
    ///   way to the last point allowed, or, from outside its zone, aimed
    ///   at the zone.
    pub fn after_step(
        &self,
        index: usize,
        before: Before,
        chars: &mut [Character],
        rng: &mut MsRand,
    ) {
        let e = &*self.rules;
        let old = before.0;
        let Some(c) = chars.get(index) else { return };
        if c.health <= 0 {
            return;
        }
        let (ctype, pos) = (c.ctype, c.pos);
        let home = c.home_zone.map(usize::from).filter(|&h| h < e.zones.len());
        let hard = e.hard(c.home_zone);
        if (pos[0] - old.pos[0]).abs() + (pos[2] - old.pos[2]).abs() > JUMP {
            let leaves = hard.is_some_and(|h| !e.inside(h, pos[0], pos[2]));
            if leaves || !self.spot_ok(ctype, pos[0], pos[2], chars, Some(index), rng) {
                chars[index] = old;
            } else {
                chars[index].home_zone = self.home(pos[0], pos[2]);
            }
            return;
        }
        let c = &mut chars[index];
        if let Some(h) = hard {
            if !e.inside(h, c.pos[0], c.pos[2]) && e.inside(h, old.pos[0], old.pos[2]) {
                let (x, z) = if e.inside(h, c.pos[0], old.pos[2]) {
                    (c.pos[0], old.pos[2])
                } else if e.inside(h, old.pos[0], c.pos[2]) {
                    (old.pos[0], c.pos[2])
                } else {
                    (old.pos[0], old.pos[2])
                };
                c.pos[0] = x;
                c.pos[2] = z;
            }
        }
        let retargeted = c.tgx != old.tgx || c.tgz != old.tgz;
        if !retargeted || c.state != 0 {
            return;
        }
        let from = (c.pos[0], c.pos[2]);
        // Its zone holds this target: always for a hard one, else by draw.
        let kept = home.filter(|&h| {
            !e.inside(h, c.tgx, c.tgz) && (hard.is_some() || rng.r_rand(SOFT_PULL) != 0)
        });
        let allowed =
            |x: f32, z: f32| kept.is_none_or(|h| e.inside(h, x, z)) && e.may_aim(None, from, x, z);
        if allowed(c.tgx, c.tgz) {
            return;
        }
        if let Some(h) = kept.filter(|&h| !e.inside(h, from.0, from.1)) {
            let a = e.zones[h].anchor;
            c.tgx = a[0] * CELL;
            c.tgz = a[1] * CELL;
            return;
        }
        // The farthest point towards the target still allowed, a little short.
        let (dx, dz) = (c.tgx - from.0, c.tgz - from.1);
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..8 {
            let mid = (lo + hi) / 2.0;
            if allowed(from.0 + dx * mid, from.1 + dz * mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        c.tgx = from.0 + dx * lo * 0.9;
        c.tgz = from.1 + dz * lo * 0.9;
    }
}
