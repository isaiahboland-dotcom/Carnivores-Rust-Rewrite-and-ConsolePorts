//! Survival, after the mobile game's Survive mode: Classic (stock animals)
//! or Modded (every eligible loaded animal) on Mount Ravan's ravine with the
//! shotgun. Waves spend menu prices from a configurable budget growing 10%
//! per round, independently of hunter points. Movement stays enabled;
//! ordinary wildlife is removed.

mod budget;
mod navigation;
mod options;
mod roster;
mod runtime;
mod spawning;

pub use budget::wave_budget;
pub use options::{
    dangerous, dangerous_ai, is_trex, Arena, SurvivalKind, SurvivalOptions, WAVE_HEALTH,
};
pub use roster::{classic_identity, price_species, roster};
pub use runtime::Survival;
