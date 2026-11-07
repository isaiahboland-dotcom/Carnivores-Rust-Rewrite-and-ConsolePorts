//! Predation (an opt-in setting, not in the original game): carnivores hunt
//! the animals below them on the food chain, scavengers eat what is left,
//! and defensive herbivores charge carnivores below them.
//!
//! The settings come from `HUNTDAT/_PREDATION.TXT`
//! ([`c2_formats::predation`]). This module turns them into a table of who
//! does what to whom, per species ([`Rules`]); [`run`] steers the animals.
//!
//! The food chain is the hunt-menu price ([`Species::hunt_price`]), unless
//! the settings name an apex (above every other animal; the rest by price)
//! or the whole chain (top first; unlisted animals below it, by price).
//!
//! - A carnivore hunts a herbivore at or below it.
//! - With carnivore fights on, it hunts a carnivore priced at least 10%
//!   lower (any lower one on an explicit chain or under an apex). Within
//!   10%, the cheaper one backs off; at the same price they ignore each
//!   other.
//! - A defensive herbivore charges a carnivore strictly below it.
//!
//! The animals keep their own AI: [`run`] gives each one a target other than
//! the player ([`crate::dino::Target`]), as survival's waypoints do.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{vec, vec::Vec};

use c2_formats::predation::{Diet, Predation, Priority};

use super::{ai, mee, CarnivoreParams, HerbivoreParams, Species};

pub mod bench;
pub mod run;
mod scan;
pub mod step;
#[cfg(test)]
mod tests;

pub use run::{Goal, Track};

/// The settings file's `bytes` resolved for `species` (whose menu prices
/// must be set, `game::price_species`). `None` when it changes nothing.
pub fn load(bytes: &[u8], species: &[Species]) -> Option<alloc::boxed::Box<run::Predators>> {
    let rules = Rules::new(&Predation::parse(bytes), species);
    rules
        .any()
        .then(|| alloc::boxed::Box::new(run::Predators::new(rules)))
}

/// What a species is to the others.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    /// Takes no part (flyers, fish, ambients, brachiosaurs, unknown AIs).
    #[default]
    None,
    Herbivore,
    Carnivore,
}

/// How species `a` treats species `b` (`Rules::rel(a, b)`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rel {
    #[default]
    Ignore,
    /// `a` hunts `b`.
    Hunts,
    /// A defensive `a` charges `b`.
    Charges,
    /// `a` is the cheaper of two close carnivores and backs off from `b`.
    Yields,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Kind {
    pub role: Role,
    pub scavenger: bool,
    pub defensive: bool,
    pub drive: Drive,
}

/// How a species' AI moves relative to the "hunter".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Drive {
    /// Runs at him (carnivores).
    Chases,
    /// Runs from him (herbivores).
    #[default]
    Flees,
    /// Charges him when close, else runs (the original's Triceratops,
    /// within `256 * 16 + OptAgres / 8`).
    Charges,
}

/// The settings resolved for one hunt's species.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    pub priority: Priority,
    pub kinds: Vec<Kind>,
    /// `rel[a * n + b]`.
    rel: Vec<Rel>,
    /// Chain position (smaller: higher) and price per species, for contests.
    level: Vec<(usize, i32)>,
}

/// The role a species' AI implies.
fn auto_role(sp: &Species) -> Role {
    match &sp.mee {
        Some(m) if m.ai == ai::TREX => Role::Carnivore,
        Some(m) if mee::is_huntable(m.ai) => {
            if mee::ai_info(m.ai).carnivore {
                Role::Carnivore
            } else {
                Role::Herbivore
            }
        }
        Some(_) => Role::None,
        None => match sp.runs_as() {
            ai::TREX => Role::Carnivore,
            a if CarnivoreParams::for_ai(a).is_some() => Role::Carnivore,
            a if HerbivoreParams::for_ai(a).is_some() => Role::Herbivore,
            _ => Role::None,
        },
    }
}

/// How the species' AI moves relative to the hunter.
fn drive(sp: &Species) -> Drive {
    let chases = match &sp.mee {
        Some(m) => m.ai == ai::TREX || mee::ai_info(m.ai).carnivore,
        None => match sp.runs_as() {
            ai::TREX => true,
            a if HerbivoreParams::for_ai(a).is_some_and(|p| p.defensive) => return Drive::Charges,
            a => CarnivoreParams::for_ai(a).is_some(),
        },
    };
    if chases {
        Drive::Chases
    } else {
        Drive::Flees
    }
}

impl Rules {
    pub fn new(p: &Predation, species: &[Species]) -> Self {
        let n = species.len();
        let name_is =
            |sp: &Species, name: &str| sp.info.name.trim().eq_ignore_ascii_case(name.trim());
        let kinds: Vec<Kind> = species
            .iter()
            .map(|sp| {
                let tags = p.animal(&sp.info.name);
                let role = match (auto_role(sp), tags.and_then(|t| t.diet)) {
                    (Role::None, _) => Role::None,
                    (_, Some(Diet::Herbivore)) => Role::Herbivore,
                    (_, Some(Diet::Carnivore)) => Role::Carnivore,
                    (r, None) => r,
                };
                Kind {
                    role,
                    scavenger: tags.is_some_and(|t| t.scavenger) && role != Role::None,
                    defensive: tags.is_some_and(|t| t.defensive) && role == Role::Herbivore,
                    drive: drive(sp),
                }
            })
            .collect();
        // Chain position: the explicit list, else the apex first, else all
        // level (prices decide).
        let level: Vec<(usize, i32)> = species
            .iter()
            .map(|sp| {
                let pos = if !p.chain.is_empty() {
                    p.chain
                        .iter()
                        .position(|c| name_is(sp, c))
                        .unwrap_or(p.chain.len())
                } else if p.apex.as_deref().is_some_and(|a| name_is(sp, a)) {
                    0
                } else {
                    1
                };
                (pos, sp.hunt_price.unwrap_or(0).max(0))
            })
            .collect();
        let mut rules = Self {
            priority: p.priority,
            kinds,
            rel: vec![Rel::Ignore; n * n],
            level,
        };
        for a in 0..n {
            for b in 0..n {
                rules.rel[a * n + b] = rules.relation(a, b, p.carnivore_fights);
            }
        }
        rules
    }

    fn relation(&self, a: usize, b: usize, fights: bool) -> Rel {
        let (ka, kb) = (self.kinds[a], self.kinds[b]);
        if a == b || ka.role == Role::None || kb.role == Role::None {
            return Rel::Ignore;
        }
        let ((pa, ca), (pb, cb)) = (self.level[a], self.level[b]);
        // Higher on the chain, then by price.
        let above = pa < pb || (pa == pb && ca > cb);
        let at_or_above = pa < pb || (pa == pb && ca >= cb);
        match (ka.role, kb.role) {
            (Role::Carnivore, Role::Herbivore) if at_or_above => Rel::Hunts,
            (Role::Herbivore, Role::Carnivore) if ka.defensive && above => Rel::Charges,
            (Role::Carnivore, Role::Carnivore) if fights => {
                if pa != pb {
                    return if pa < pb { Rel::Hunts } else { Rel::Ignore };
                }
                // Strict: two unpriced carnivores (0 and 0) ignore each other.
                if cb < ca && cb as f32 <= ca as f32 * 0.9 {
                    Rel::Hunts
                } else if ca < cb && (ca as f32) > cb as f32 * 0.9 {
                    Rel::Yields
                } else {
                    Rel::Ignore
                }
            }
            _ => Rel::Ignore,
        }
    }

    /// How species `a` treats species `b`.
    pub fn rel(&self, a: usize, b: usize) -> Rel {
        let n = self.kinds.len();
        if a >= n || b >= n {
            return Rel::Ignore;
        }
        self.rel[a * n + b]
    }

    pub fn kind(&self, ctype: usize) -> Kind {
        self.kinds.get(ctype).copied().unwrap_or_default()
    }

    /// Two predators after the same prey: `Some(true)` if `a` clearly
    /// outranks `b` (higher on an explicit chain, or priced more than 10%
    /// above), `Some(false)` if `b` does, `None` if it comes down to who
    /// is closer.
    pub fn outranks(&self, a: usize, b: usize) -> Option<bool> {
        let (Some(&(pa, ca)), Some(&(pb, cb))) = (self.level.get(a), self.level.get(b)) else {
            return None;
        };
        if pa != pb {
            return Some(pa < pb);
        }
        if cb < ca && cb as f32 <= ca as f32 * 0.9 {
            Some(true)
        } else if ca < cb && ca as f32 <= cb as f32 * 0.9 {
            Some(false)
        } else {
            None
        }
    }

    /// Whether any species takes part (else the settings change nothing).
    pub fn any(&self) -> bool {
        self.rel.iter().any(|r| *r != Rel::Ignore) || self.kinds.iter().any(|k| k.scavenger)
    }
}
