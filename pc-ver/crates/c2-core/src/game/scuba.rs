//! Scuba gear (gear, not in the original; offered only where the instance's
//! rules list it): faster diving and swimming (`player.rs`), a longer view
//! under water (`fog.rs`), an oxygen tank that makes drowning take twice as
//! long, and kills scoring 15% less.
//!
//! Health comes back at [`HEALTH_REGEN`] a millisecond all the time, under
//! water too, so drowning nets `DROWN_RATE - HEALTH_REGEN` (8 a ms: 16 s
//! from full health). The tank halves that net loss (4 a ms: 32 s).

use super::Hunt;

/// The kill score multiplier scuba gear brings (−15%).
pub const SCUBA_SCORE: f32 = 0.85;

/// Health regained per millisecond (of `MAX_HEALTH`), always.
pub(super) const HEALTH_REGEN: i32 = 4;

/// Health lost per millisecond under water (of `MAX_HEALTH`), before the
/// regain.
const DROWN_RATE: i32 = 12;

impl Hunt {
    /// Health lost per millisecond under water: the original's rate, or
    /// with scuba gear's oxygen tank one that drowns half as fast once the
    /// regain is counted.
    pub(super) fn drown_rate(&self) -> i32 {
        if self.opts.scuba {
            HEALTH_REGEN + (DROWN_RATE - HEALTH_REGEN) / 2
        } else {
            DROWN_RATE
        }
    }
}
