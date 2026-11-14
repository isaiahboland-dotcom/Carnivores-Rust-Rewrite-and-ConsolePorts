//! The hardcore hunt ruleset (not in the original; a recipe's
//! `[settings.hunt]`, read from `_RULES.TXT`): equipment limits, drop-off
//! and pickup zones with extraction, ammunition drops, the knife and
//! predator hesitation. Each rule is independent; a hunt without
//! [`Hunt::start_hardcore`] is the game as it was. Design:
//! `docs/HARDCORE-HUNT.md`.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use c2_formats::hardcore::{self as fmt, Reaction};

use super::{GameEvent, Hunt};
use crate::area::Area;
use crate::dino::hesitate::Hesitation;
use crate::dino::{ai, mee, Species};
use crate::CELL;

mod drops;
mod extraction;
mod knife;

pub use drops::Drop;
pub use extraction::{Pickup, ZONE_RADIUS};
pub use knife::{Knife, SWING_MS};

/// A model reaching further than this is too big to lunge (the original
/// T-Rex reaches about 1000).
const BIG_REACH: f32 = 900.0;
/// Dangerous animals closer than this to a chosen drop-off are moved out
/// to it, cells.
const CLEAR_CELLS: f32 = 30.0;

/// What the player picks next on the map before the hunt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deploy {
    /// Where the hunt starts.
    DropOff,
    /// Where the ship picks him up.
    Pickup,
}

/// The ruleset's state for one hunt.
#[derive(Debug, Clone, Default)]
pub struct Hardcore {
    pub rules: fmt::Hunt,
    /// The zone still to pick (`None`: the hunt is under way).
    pub deploy: Option<Deploy>,
    /// Where the hunt started, and the pickup zone's centre.
    pub drop_off: (f32, f32),
    pub zone: Option<(f32, f32)>,
    pub pickup: Pickup,
    pub drops: Vec<Drop>,
    pub knife: Knife,
    /// Ms the ship is still seen flying off.
    leaving_ms: i32,
    /// Seeds the drops' places.
    seed: u32,
}

/// How species `sp` reacts when no rule lists it: by size and diet. Only
/// animals that hunt the hunter hesitate.
pub fn default_reaction(sp: &Species) -> Reaction {
    let trex = sp.info.ai == ai::TREX;
    let hunts = match &sp.mee {
        Some(_) => (mee::is_huntable(sp.info.ai) && mee::ai_info(sp.info.ai).carnivore) || trex,
        None => matches!(
            sp.runs_as(),
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX
        ),
    };
    if !hunts {
        Reaction::Ignore
    } else if trex || sp.runs_as() == ai::TREX || sp.size > BIG_REACH {
        Reaction::CircleEat
    } else {
        Reaction::CircleLunge
    }
}

impl Hunt {
    /// Turns the ruleset on for this hunt. Nothing in a survival, the
    /// trophy room or an observer's hunt, or when no rule is set.
    pub fn start_hardcore(&mut self, area: &Area, rules: &fmt::Hunt) {
        if rules.is_empty() || self.survival.is_some() || self.opts.trophy || self.opts.observer {
            return;
        }
        if rules.no_radar {
            self.opts.radar = false;
        }
        if rules.no_cover_scent {
            self.opts.scent = false;
        }
        if let Some(n) = rules.max_firearms {
            self.weapons.keep_first(n as usize);
        }
        // The dodge needs the overlay too: it is what makes a strike miss.
        if rules.hesitation_s > 0 || rules.knife_dodge {
            let reactions = self
                .herd
                .species
                .iter()
                .map(|sp| {
                    if rules.hesitation_s == 0 {
                        Reaction::Ignore
                    } else {
                        rules
                            .reaction(&sp.info.name)
                            .unwrap_or_else(|| default_reaction(sp))
                    }
                })
                .collect();
            let mut hes = Hesitation::default();
            hes.circle_ms = rules.hesitation_s as i32 * 1000;
            hes.back_cos = (rules.back_angle() as f32).to_radians().cos();
            hes.second = rules
                .second_predator
                .map(|(w, p)| (w as i32 * 1000, p.min(i32::MAX as u32) as i32));
            hes.reactions = reactions;
            self.herd.hesitation = Some(Box::new(hes));
        }
        let mut h = Hardcore {
            rules: rules.clone(),
            deploy: rules.extraction.map(|_| Deploy::DropOff),
            drop_off: (self.player.x, self.player.z),
            seed: self.rng.rand() as u32,
            ..Hardcore::default()
        };
        if h.deploy.is_none() {
            h.place_drops(area);
        }
        self.hardcore = Some(Box::new(h));
    }

    /// The ruleset in force (`None`: a regular hunt).
    pub fn hardcore_rules(&self) -> Option<&fmt::Hunt> {
        self.hardcore.as_deref().map(|h| &h.rules)
    }

    /// The zone the player still has to pick before the hunt starts.
    pub fn deploying(&self) -> Option<Deploy> {
        self.hardcore.as_deref().and_then(|h| h.deploy)
    }

    /// Picks the next zone at world (`x`, `z`): the drop-off (the hunter is
    /// put there), then the pickup, which starts the hunt. Why not, if the
    /// spot can't be used.
    pub fn pick_zone(&mut self, area: &Area, x: f32, z: f32) -> Result<(), String> {
        let Some(step) = self.deploying() else {
            return Err("The hunt is under way".into());
        };
        let at = super::teleport::landing(area, x, z).ok_or("No ground to stand on there")?;
        let Some(h) = self.hardcore.as_deref_mut() else {
            return Ok(());
        };
        match step {
            Deploy::DropOff => {
                h.drop_off = at;
                h.deploy = Some(Deploy::Pickup);
                let (alpha, beta) = (self.player.alpha, self.player.beta);
                let cell = |v: f32| (v / CELL) as u16;
                self.player = crate::player::Player::new(area, (cell(at.0), cell(at.1)));
                (self.player.alpha, self.player.beta) = (alpha, beta);
                self.ship = crate::ship::Ship::new(area, at);
                self.clear_drop_off(area);
            }
            Deploy::Pickup => {
                let gap = h.rules.extraction.unwrap_or_default().zone_gap as f32;
                let d = ((at.0 - h.drop_off.0).powi(2) + (at.1 - h.drop_off.1).powi(2)).sqrt();
                if d < gap * CELL {
                    return Err(format!(
                        "Too close to the drop-off: at least {}m away",
                        (gap * CELL / 64.0) as i32
                    ));
                }
                h.zone = Some(at);
                h.deploy = None;
                h.place_drops(area);
            }
        }
        Ok(())
    }

    /// Dangerous animals standing round the chosen drop-off are moved out
    /// of it, as the original keeps them from the hunter's landing.
    fn clear_drop_off(&mut self, area: &Area) {
        let (px, pz) = (self.player.x, self.player.z);
        let species = &self.herd.species;
        for (i, c) in self.herd.chars.iter_mut().enumerate() {
            let Some(sp) = species.get(c.ctype) else {
                continue;
            };
            let (dx, dz) = (c.pos[0] - px, c.pos[2] - pz);
            let d = (dx * dx + dz * dz).sqrt();
            if c.health == 0 || !super::dangerous(sp) || d >= CLEAR_CELLS * CELL {
                continue;
            }
            // Out along its own bearing, else round from it.
            let a0 = dz.atan2(dx);
            let spot = (0..16).find_map(|k| {
                let a = a0 + (k as f32) * (i as f32 + 1.0) * 0.7;
                let (x, z) = (
                    px + a.cos() * (CLEAR_CELLS + 4.0) * CELL,
                    pz + a.sin() * (CLEAR_CELLS + 4.0) * CELL,
                );
                super::teleport::dry(area, (x / CELL) as i32, (z / CELL) as i32).then_some((x, z))
            });
            if let Some((x, z)) = spot {
                c.pos = [x, area.land_height(x, z), z];
                (c.tgx, c.tgz) = (x, z);
                c.state = 0;
                c.afraid_time = 0;
            }
        }
    }

    /// Leaving by the exit prompt forfeits the hunt (as a death: trophies
    /// and points discarded) when extraction is the way out.
    pub fn exit_forfeits(&self) -> bool {
        self.hardcore
            .as_deref()
            .is_some_and(|h| h.rules.extraction.is_some())
            && self.health > 0
    }

    /// Once a tick: the ship, the drops, the knife and the dodge.
    pub(super) fn hardcore_step(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        if self.hardcore.is_none() {
            return;
        }
        self.extraction_step(area, dt, out);
        self.drops_step(out);
        self.knife_step(area, dt);
    }
}

#[cfg(test)]
pub(super) mod test_rules {
    use super::*;

    /// Every rule on, with the defaults.
    pub fn all() -> fmt::Hunt {
        fmt::Hunt {
            no_night_vision: true,
            no_radar: true,
            no_cover_scent: true,
            max_firearms: Some(1),
            ammo_drops: 3,
            extraction: Some(fmt::Extraction::default()),
            knife: true,
            knife_dodge: true,
            hesitation_s: 10,
            back_angle: 90,
            second_predator: Some((30, 300)),
            predators: Vec::new(),
        }
    }
}
