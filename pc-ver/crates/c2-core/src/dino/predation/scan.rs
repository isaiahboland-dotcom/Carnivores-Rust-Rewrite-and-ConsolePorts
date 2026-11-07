//! What one animal finds around it, in one pass over the herd
//! ([`Scan::new`]): who is charging it, and the nearest sensed animal of
//! each kind its goal may turn to. [`super::run::Predators::think`] used to
//! walk the herd once per kind, with a square root per pair; now each pair
//! is looked at once, and the root is taken only for animals within reach
//! of the senses.
//!
//! Senses follow `CheckAfraid` (`wildlife.rs`) without its look traces,
//! which are too costly for every pair: the distance is doubled instead, for
//! the cover they would find. Hearing is added for a running animal.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::run::{menace, Goal, Predators};
use super::{Kind, Rel, Role, Rules};
use crate::dino::{Character, Species};

/// The nearest character of one kind so far: index and squared distance.
#[derive(Debug, Clone, Copy, Default)]
struct Near(Option<(usize, f32)>);

impl Near {
    /// Keeps `j` if it is nearer; on a tie, the earlier one stays.
    fn offer(&mut self, j: usize, d2: f32) {
        if self
            .0
            .is_none_or(|(_, b)| d2.total_cmp(&b) == core::cmp::Ordering::Less)
        {
            self.0 = Some((j, d2));
        }
    }

    fn get(self) -> Option<usize> {
        self.0.map(|(j, _)| j)
    }
}

/// Which kinds a scan looks for: the ones the animal's goal can turn to.
#[derive(Debug, Clone, Copy)]
pub(super) struct Wants {
    pub charged: bool,
    pub prey: bool,
    pub body: bool,
}

/// What character `i` finds about it.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Scan {
    /// The first living animal charging it (sensed or not).
    pub charger: Option<usize>,
    /// The nearest sensed animal that hunts it, within [`menace`] range.
    predator: Near,
    /// The nearest sensed rival it backs off from.
    yielder: Near,
    /// The nearest sensed carnivore a defensive herbivore charges.
    charged: Near,
    /// The nearest sensed animal it hunts.
    prey: Near,
    /// The nearest smelled herbivore body not yet eaten.
    body: Near,
}

impl Scan {
    pub fn new(
        i: usize,
        chars: &[Character],
        species: &[Species],
        rules: &Rules,
        view_r: i32,
        wants: Wants,
    ) -> Self {
        let mut s = Scan::default();
        let c = &chars[i];
        let sp = species.get(c.ctype);
        let reach2 = reach(view_r).powi(2);
        for (j, t) in chars.iter().enumerate() {
            if j == i || t.state_f == 0xff {
                continue;
            }
            let alive = t.health != 0 && !Predators::idle(t);
            if alive && s.charger.is_none() && t.predation.goal == Goal::Charge(i) {
                s.charger = Some(j);
            }
            let Some(sp) = sp else { continue };
            let d2 = dist2(c, t);
            let (mut predator, mut yielder, mut charged, mut prey, mut body) =
                (false, false, false, false, false);
            if alive {
                let m = menace(i, t);
                predator = rules.rel(t.ctype, c.ctype) == Rel::Hunts && d2 < m * m;
                let rel = rules.rel(c.ctype, t.ctype);
                yielder = rel == Rel::Yields;
                charged = wants.charged && rel == Rel::Charges;
                prey = wants.prey && rel == Rel::Hunts;
            }
            if wants.body && t.health == 0 && !t.predation.eaten {
                body = rules.kind(t.ctype).role == Role::Herbivore;
            }
            if !(predator || yielder || charged || prey || body) {
                continue;
            }
            // Out of reach of every sense (a margin left for rounding: the
            // exact test is `senses`').
            let dy = t.pos[1] - c.pos[1];
            if d2 + dy * dy > reach2 * 1.01 || !senses(c, t, sp, view_r) {
                continue;
            }
            for (want, near) in [
                (predator, &mut s.predator),
                (yielder, &mut s.yielder),
                (charged, &mut s.charged),
                (prey, &mut s.prey),
                (body, &mut s.body),
            ] {
                if want {
                    near.offer(j, d2);
                }
            }
        }
        s
    }

    pub fn predator(&self) -> Option<usize> {
        self.predator.get()
    }
    pub fn yielder(&self) -> Option<usize> {
        self.yielder.get()
    }
    pub fn charged(&self) -> Option<usize> {
        self.charged.get()
    }
    pub fn prey(&self) -> Option<usize> {
        self.prey.get()
    }
    pub fn body(&self) -> Option<usize> {
        self.body.get()
    }
}

impl Wants {
    /// The kinds `k`'s goal can turn to; `busy`: a carnivore after the
    /// player (with the hunter first) neither hunts nor scavenges.
    pub fn for_kind(k: Kind, busy: bool) -> Self {
        Self {
            charged: k.defensive,
            prey: k.role == Role::Carnivore && !busy,
            body: k.scavenger && !busy,
        }
    }
}

pub(super) fn dist2(a: &Character, b: &Character) -> f32 {
    let (dx, dz) = (a.pos[0] - b.pos[0], a.pos[2] - b.pos[2]);
    dx * dx + dz * dz
}

/// The farthest any animal senses another, for `view_r`: where `senses`'
/// `kr` reaches 3.
fn reach(view_r: i32) -> f32 {
    3.0 * 256.0 * (32.0 + view_r as f32 / 2.0) / 2.0
}

/// Whether `o` notices `t` (alive), or smells `t`'s body.
pub(super) fn senses(o: &Character, t: &Character, sp: &Species, view_r: i32) -> bool {
    let d = [
        t.pos[0] - o.pos[0],
        t.pos[1] - o.pos[1],
        t.pos[2] - o.pos[2],
    ];
    let rl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let kr = 2.0 * rl / 256.0 / (32.0 + view_r as f32 / 2.0);
    if kr > 3.0 {
        return false;
    }
    if rl <= 0.0 {
        return true;
    }
    let klook = -(o.lookx * d[0] + o.lookz * d[2]) / rl;
    let face = (klook + 3.0) / 3.0;
    let i = &sp.info;
    if t.health == 0 {
        return kr * 0.75 / i.smell_k.max(1e-3) < 1.0;
    }
    let mut k_look = kr * face * 0.7;
    if klook > 0.3 {
        k_look *= 2.0;
    }
    if klook > 0.8 {
        k_look *= 2.0;
    }
    k_look /= i.look_k.max(1e-3);
    let k_smell = kr * face / i.smell_k.max(1e-3);
    let k_hear = if t.state != 0 {
        kr * 2.0 / i.hear_k.max(1e-3)
    } else {
        f32::INFINITY
    };
    k_look.min(k_smell).min(k_hear) < 1.0
}
