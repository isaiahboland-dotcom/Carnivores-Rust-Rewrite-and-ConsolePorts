//! Platform-independent game logic.
//!
//! So far: the load-time preparation shared by every renderer
//! ([`color`], [`model`], [`area`]). Gameplay arrives in M3.

pub mod area;
pub mod collide;
pub mod color;
pub mod dino;
pub mod effects;
pub mod game;
pub mod load;
pub mod model;
pub mod player;
pub mod profile;
pub mod rng;
pub mod ship;
pub mod trace;

/// Height-map units to world units (`Hunt.h: ctHScale`).
pub const H_SCALE: f32 = 64.0;
/// World units per map cell.
pub const CELL: f32 = 256.0;
