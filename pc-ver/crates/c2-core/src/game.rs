//! A hunt in progress: the hunter, the herd, weapons, wind and detection,
//! advanced in fixed ticks (`Hunt2.cpp: ProcessGame`).
//!
//! Sources: `ProcessShoot`, `HideWeapon` (Hunt2.cpp), the weapon state
//! machine in `DrawPostObjects`, `MakeShot`, `MakeNoise`, `CheckAfraid`,
//! `AnimateProcesses` (wind), `AddDeadBody`, `ProcessDemoMovement`.

mod arms;
mod audio;
mod combat;
mod death;
mod events;
mod mee_weapon;
mod options;
mod session;
mod state;
mod survival;
mod teleport;
mod trophies;
mod update;
mod view;
mod weapon_actions;
mod weapon_info;
mod weapons;
mod wildlife;

pub use death::{death_cam_view, DeathCam};
pub use events::{GameEvent, HuntKill};
pub use mee_weapon::{Aqua, MeeWeapon};
pub use options::{HuntInput, HuntOptions, MAX_CALL};
pub use session::Outcome;
pub use state::{Hunt, Wind, MAX_HEALTH};
pub use survival::{
    dangerous, dangerous_ai, is_trex, modded, MobileWaves, Survival, SurvivalKind, SurvivalOptions,
    TREX_WAVE, WAVE_HEALTH,
};
pub use teleport::{Teleport, COOLDOWN_MS as TELEPORT_COOLDOWN_MS, DELAY_MS as TELEPORT_DELAY_MS};
pub use view::LifeInfo;
pub use weapon_info::WeaponInfo;
pub use weapons::{weapon_anim, Bullet, WeaponAnims, WeaponState, Weapons};

#[cfg(test)]
mod tests;
