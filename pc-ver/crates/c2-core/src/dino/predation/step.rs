//! Predation's hooks into the herd step ([`Herd::step_scheduled`]): feeding
//! in place of the AI, and the steered target for its step.
//!
//! [`Herd::step_scheduled`]: crate::dino::Herd::step_scheduled

use super::run::{resume_phase, Predators};
use crate::dino::{Character, Species};

/// Feeding on a kill: plays the eating clip, standing, in place of the AI.
/// `false` when it is not eating (or is dead). A kill leaves the animal
/// calm (state 0, unaware), so a shot, a noise or noticing the hunter ends
/// the meal and its AI takes over.
pub fn feeding(c: &mut Character, sp: &Species, dt: i32) -> bool {
    if c.health == 0 || c.predation.eat_ms <= 0 {
        return false;
    }
    if c.state != 0 || c.aware_hunter {
        end_meal(c, sp);
        return false;
    }
    if let Some(ph) = c.predation.eat_phase {
        c.phase = ph;
        c.ftime = (c.ftime + dt) % sp.time(ph).max(1);
    }
    c.vspeed = 0.0;
    c.predation.eat_ms = (c.predation.eat_ms - dt).max(0);
    if c.predation.eat_ms == 0 {
        end_meal(c, sp);
    }
    true
}

/// Ends a meal, interrupted or finished: off the eating clip, so the AI
/// (which never leaves it on its own) takes over.
fn end_meal(c: &mut Character, sp: &Species) {
    c.predation.eat_ms = 0;
    if let Some(ph) = c.predation.eat_phase.take() {
        if c.phase == ph {
            if let Some(walk) = resume_phase(sp) {
                c.phase = walk;
                c.ftime = 0;
            }
        }
    }
}

impl Predators {
    /// After character `i`'s AI step: `struck` is the animal its AI struck
    /// ([`Ctx::strike`](crate::dino::Ctx::strike)). A strike counts only on
    /// the animal it was steered at; then [`settle`](Predators::settle)s
    /// its goal.
    pub fn finish(
        &self,
        i: usize,
        chars: &mut [Character],
        species: &[Species],
        struck: Option<usize>,
    ) {
        let hit = struck.is_some_and(|n| chars[i].predation.goal.target() == Some(n));
        self.settle(i, chars, species, hit);
    }
}
