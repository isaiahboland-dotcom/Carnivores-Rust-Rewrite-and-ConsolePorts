//! Advanced binoculars (gear, not in the original): looking through the
//! binoculars, each living huntable animal is outlined, green while it is
//! not after the hunter and red once it is. Kills score 40% less.

use super::{dangerous, Hunt};
use crate::dino::{predation::Goal, Character, CS_ON_WATER};

/// The kill score multiplier the advanced binoculars bring (−40%).
pub const ADVANCED_BINOCULARS_SCORE: f32 = 0.60;

/// The outline an animal shows through the advanced binoculars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outline {
    /// Not after the hunter: wandering, grazing, fleeing, or busy with
    /// other animals.
    Calm,
    /// After the hunter.
    Hostile,
}

impl Hunt {
    /// The outline of character `i` this frame: `None` unless the advanced
    /// binoculars are carried and up, and `i` is a living huntable animal
    /// (ambients, flyers and fish are left plain).
    pub fn binocular_outline(&self, i: usize) -> Option<Outline> {
        if !self.opts.advanced_binoculars || !self.bin_mode {
            return None;
        }
        let c = self.herd.chars.get(i)?;
        if c.health == 0 || !self.is_huntable(c) {
            return None;
        }
        Some(if self.hostile(c) {
            Outline::Hostile
        } else {
            Outline::Calm
        })
    }

    /// Whether `c` is after the hunter: a species that attacks him
    /// ([`dangerous`]: the carnivores and the charging Chasmosaurus, or a
    /// dialect animal with an attack), alerted or hunting (`State != 0`),
    /// and not chasing, charging, fleeing or scavenging around other
    /// animals (predation). An observer is never hunted.
    pub fn hostile(&self, c: &Character) -> bool {
        if self.opts.observer || c.health == 0 {
            return false;
        }
        let Some(sp) = self.herd.species.get(c.ctype) else {
            return false;
        };
        dangerous(sp) && c.state & !CS_ON_WATER != 0 && c.predation.goal == Goal::None
    }
}
