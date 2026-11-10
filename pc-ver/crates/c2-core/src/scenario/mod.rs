//! Scripted regression scenarios (test-only; not part of normal play).
//!
//! A [`Scenario`] fixes everything a run depends on: seed, hunter cell and
//! view, four animals pinned around one anchor, and a script of stages
//! (warm-up, every weapon fired once, binoculars, map, the call icon and
//! the frontend's pickers); with the kill script ([`kills`]), each
//! animal killing the hunter in turn; with the behaviour script
//! ([`behaviour`]), the hunter walking and running and each animal running
//! and dying. The [`Director`] turns the script into one
//! [`HuntInput`](crate::game::HuntInput) per tick, watches the hunt, and
//! records each stage as passed, failed or unsupported. Frontends drive it
//! the same way: the headless runner (`c2-regress`) and the PC renderer.
//!
//! Nothing here runs unless a frontend builds a scenario. The only state it
//! changes outside the normal tick is [`Hold`]: the hunter's view and the
//! animals' positions are pinned between ticks so the run stays comparable.

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

mod behaviour;
mod director;
mod inject;
mod kills;
mod record;
mod setup;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_behaviour;

pub use behaviour::{stage_names, BehaviourPlan, BehaviourReport, Sample, Subject, Track};
pub use director::{Director, Frontend, Headless, Screen, TickPlan};
pub use inject::Inject;
pub use kills::{hunter_clip, Arrange, KillPlan, KillReport, KillWatch, Pose};
pub use record::{Event, EventKind, StageResult, Status};
pub use setup::{build, layout, state_hash, Built, Cast, Hold, Layout, SetupError};

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
    /// By model file, ignoring case and folders (`diablo.car`): add-on
    /// packs, whose character indices move when their mods change.
    Model(String),
}

/// What a run does with the hunt it builds.
#[derive(Debug, Clone, PartialEq)]
pub enum Script {
    /// Every weapon fired once, then every screen opened (the canonical
    /// scenario).
    Tour,
    /// Each animal kills the hunter in turn ([`kills`]).
    Kills(KillPlan),
    /// The hunter walks and runs, each animal runs, then dies
    /// ([`behaviour`]).
    Behaviours(BehaviourPlan),
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
    pub script: Script,
}

impl Scenario {
    /// Hard ceiling for the whole run: every stage at its timeout.
    pub fn max_ticks(&self, weapons: usize) -> u32 {
        let per = self.timeout_ticks + self.settle_ticks;
        match &self.script {
            Script::Tour => self.warmup_ticks + (8 + weapons as u32 * 5) * per,
            Script::Kills(k) => {
                let kill = k.timeout_ticks.max(self.timeout_ticks) + k.mid_ticks + k.late_ticks + 4;
                self.warmup_ticks + 4 * per + self.species.len() as u32 * kill
            }
            Script::Behaviours(b) => {
                let n = self.species.len() as u32;
                let stages = 2 * (b.move_ticks + 4) + n * (b.run_ticks + b.die_ticks + 8);
                self.warmup_ticks + 4 * per + stages
            }
        }
    }
}
