//! Scripted regression scenarios (test-only; not part of normal play).
//!
//! A [`Scenario`] fixes everything a run depends on: seed, hunter cell and
//! view, four animals pinned around one anchor, and a script of stages
//! (warm-up, every weapon fired once, binoculars, map, the call icon and
//! the frontend's pickers). The [`Director`] turns the script into one
//! [`HuntInput`](crate::game::HuntInput) per tick, watches the hunt, and
//! records each stage as passed, failed or unsupported. Frontends drive it
//! the same way: the headless runner (`c2-regress`) and the PC renderer.
//!
//! Nothing here runs unless a frontend builds a scenario. The only state it
//! changes outside the normal tick is [`Hold`]: the hunter's view and the
//! animals' positions are pinned between ticks so the run stays comparable.

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

mod director;
mod inject;
mod record;
mod setup;
#[cfg(test)]
mod tests;

pub use director::{Director, Frontend, Headless, Screen, TickPlan};
pub use inject::Inject;
pub use record::{Event, EventKind, StageResult, Status};
pub use setup::{build, state_hash, Built, Hold, SetupError};

/// Simulation step of every scenario, ms (the PC's and the 3DS's tick).
pub const TICK_MS: i32 = 10;

/// Which character a scenario places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesRef {
    /// By `_RES.TXT` AI number (stock games: unique per huntable animal).
    Ai(i32),
    /// By position in `_RES.TXT`'s character list (dialect games, where
    /// names and AI numbers repeat).
    Index(usize),
}

/// A scenario after its file is read and its game section picked.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub id: String,
    pub version: u32,
    pub seed: u32,
    /// Landing number in the area (`Area::landings`), or an explicit cell.
    pub landing: usize,
    pub cell: Option<(u16, u16)>,
    /// The hunter's view: yaw and pitch, radians.
    pub yaw: f32,
    pub pitch: f32,
    /// The animals' anchor, from the hunter: distance (world units) and
    /// bearing relative to `yaw` (radians, positive = to the right).
    pub anchor_distance: f32,
    pub anchor_bearing: f32,
    /// Per animal: `(right, forward)` offset from the anchor in the anchor's
    /// own frame, world units.
    pub offsets: Vec<[f32; 2]>,
    pub species: Vec<SpeciesRef>,
    /// Smallest angle (radians) allowed between the line of fire and an
    /// animal's edge; set-up fails rather than risk hitting one.
    pub fire_clearance: f32,
    pub warmup_ticks: u32,
    /// Ticks a screen stays open before its checkpoint and closing.
    pub settle_ticks: u32,
    /// Most ticks any one wait may take before its stage fails.
    pub timeout_ticks: u32,
}

impl Scenario {
    /// Hard ceiling for the whole run: every stage at its timeout.
    pub fn max_ticks(&self, weapons: usize) -> u32 {
        let stages = 8 + weapons as u32 * 5;
        self.warmup_ticks + stages * (self.timeout_ticks + self.settle_ticks)
    }
}
