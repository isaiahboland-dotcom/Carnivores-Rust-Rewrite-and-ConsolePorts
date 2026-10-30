#![cfg_attr(not(feature = "std"), no_std)]

//! Platform-independent game logic.
//!
//! The default `std` feature enables the full game. Without it, only the
//! portable player, area, collision, random-number, color and model code
//! are available. `portable-dino` and `portable-hunt` additionally expose
//! the shared simulation on allocation-capable no_std targets.

extern crate alloc;

#[cfg(not(feature = "std"))]
mod portable_math;

pub mod area;
pub mod collide;
pub mod color;
#[cfg(any(feature = "std", feature = "portable-dino"))]
pub mod dino;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod effects;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod game;
#[cfg(feature = "std")]
pub mod load;
#[cfg(feature = "std")]
pub mod mee;
pub mod model;
pub mod player;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod prof;
pub mod profile;
pub mod record;
pub mod rng;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod ship;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod sound;
#[cfg(any(feature = "std", feature = "portable-hunt"))]
pub mod trace;

/// Height-map units to world units (`Hunt.h: ctHScale`).
pub const H_SCALE: f32 = 64.0;
/// World units per map cell.
pub const CELL: f32 = 256.0;
