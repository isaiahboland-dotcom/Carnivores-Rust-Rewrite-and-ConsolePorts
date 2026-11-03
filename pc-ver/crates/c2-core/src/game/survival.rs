//! Survival (README M13), after the mobile games' mode: waves of dangerous
//! animals, picked at random, come for the hunter (always: they never
//! back off), each wave bigger; between waves a short
//! rest (health and ammunition back). It ends when the hunter dies. Kills
//! give no trophy and no score; the frontends record them (and the wave
//! reached) in the hunter's record. Ammunition never runs out, and wave
//! animals have [`WAVE_HEALTH`] (the owner's: a direct shotgun blast kills,
//! or a couple of glancing ones).
//!
//! - Hardcore: each wave also brings 1-3 animals from a pool (the frontends
//!   give the modded ones).
//! - Custom: the T-Rex may come from wave 15.
//! - Mobile: the mobile game's own Survive mode, as read from the player's
//!   APK (`c2_formats::mobile_arcade`): its waves, in turn, from a box on
//!   Mount Ravan; the hunter at its spot.

mod navigation;
mod options;
mod runtime;
mod spawning;

pub use options::{
    dangerous, dangerous_ai, is_trex, modded, MobileWaves, SurvivalKind, SurvivalOptions,
    TREX_WAVE, WAVE_HEALTH,
};
pub use runtime::Survival;

#[cfg(test)]
pub(super) use options::mee_waves;
