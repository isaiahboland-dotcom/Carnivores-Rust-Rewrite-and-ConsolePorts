//! A hunt in progress: the hunter, the herd, weapons, wind and detection,
//! advanced in fixed ticks (`Hunt2.cpp: ProcessGame`).
//!
//! Sources: `ProcessShoot`, `HideWeapon` (Hunt2.cpp), the weapon state
//! machine in `DrawPostObjects`, `MakeShot`, `MakeNoise`, `CheckAfraid`,
//! `AnimateProcesses` (wind), `AddDeadBody`, `ProcessDemoMovement`.

mod ammo_kinds;
mod aquatic;
mod arms;
mod audio;
mod binoculars;
mod combat;
mod death;
mod events;
pub mod hardcore;
mod mee_weapon;
mod observer;
mod options;
mod scuba;
mod session;
mod spawn_rules;
mod state;
mod survival;
mod teleport;
mod trophies;
pub mod trophy_room;
mod update;
mod view;
mod weapon_actions;
mod weapon_info;
mod weapons;
mod wildlife;

pub use aquatic::{has_water, AquaticRoam, FAR_M as AQUATIC_FAR_M, NEAR_M as AQUATIC_NEAR_M};
pub use binoculars::{Outline, ADVANCED_BINOCULARS_SCORE};
pub use death::{death_cam_view, DeathCam};
pub use events::{GameEvent, HuntKill};
pub use hardcore::Hardcore;
pub use mee_weapon::{Aqua, MeeWeapon};
pub use options::{HuntInput, HuntOptions, MAX_CALL};
pub use scuba::SCUBA_SCORE;
pub use session::Outcome;
pub use spawn_rules::{index_named, species_named};
pub use state::{Hunt, Wind, MAX_HEALTH};
pub use survival::{
    classic_identity, dangerous, dangerous_ai, is_trex, price_species, roster, survival_area,
    survival_weapon, wave_budget, Arena, Survival, SurvivalKind, SurvivalOptions, WAVE_HEALTH,
};
pub use teleport::{Teleport, COOLDOWN_MS as TELEPORT_COOLDOWN_MS, DELAY_MS as TELEPORT_DELAY_MS};
pub use view::LifeInfo;
pub use weapon_info::WeaponInfo;
pub use weapons::{weapon_anim, Bullet, WeaponAnims, WeaponState, Weapons};

#[cfg(test)]
mod tests;
