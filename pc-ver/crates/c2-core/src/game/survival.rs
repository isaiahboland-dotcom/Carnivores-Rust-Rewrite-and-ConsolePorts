//! Survival, after the mobile game's Survive mode: Classic (stock animals)
//! or Modded (every eligible loaded animal) on Mount Ravan's ravine with the
//! shotgun. Waves spend menu prices from a configurable budget growing 10%
//! per round, independently of hunter points. Movement stays enabled;
//! ordinary wildlife is removed.

mod arena;
mod budget;
mod navigation;
mod options;
mod roster;
mod runtime;
mod spawning;

pub use arena::Arena;
pub use budget::wave_budget;
pub use options::{
    dangerous, dangerous_ai, is_trex, survival_area, survival_weapon, SurvivalKind,
    SurvivalOptions, WAVE_HEALTH,
};
pub use roster::{classic_identity, price_species, roster};
pub use runtime::Survival;
