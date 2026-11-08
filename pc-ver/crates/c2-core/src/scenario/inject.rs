//! Deliberate faults, to prove the checks and the comparator catch them
//! (`c2-regress run --inject ...`). Applied by the runners around the
//! script; nothing here is reachable from play.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String};

use super::{Hold, TickPlan};
use crate::game::Hunt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Inject {
    /// Weapon `n` (0-based) is never fired: its stage must fail.
    DropShot(usize),
    /// Animal `n` dies after warm-up: the animal checks must fail.
    KillAnimal(usize),
    /// The view turns by this many degrees: screenshots must differ.
    NudgeView(f32),
    /// The hunter's body is held on its first frame (kill script): the
    /// kill checks must fail.
    FreezeBody,
}

impl Inject {
    /// `drop-shot:N`, `kill-animal:N`, `nudge-view:DEG`, `freeze-body`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (k, v) = s.split_once(':').unwrap_or((s, ""));
        let bad = || {
            format!(
                "--inject {s:?}: expected drop-shot:N, kill-animal:N, nudge-view:DEG or freeze-body"
            )
        };
        match k {
            "drop-shot" => v.parse().map(Inject::DropShot).map_err(|_| bad()),
            "kill-animal" => v.parse().map(Inject::KillAnimal).map_err(|_| bad()),
            "nudge-view" => v.parse().map(Inject::NudgeView).map_err(|_| bad()),
            "freeze-body" if v.is_empty() => Ok(Inject::FreezeBody),
            _ => Err(bad()),
        }
    }

    pub fn adjust_hold(&self, hold: &mut Hold) {
        match *self {
            Inject::NudgeView(deg) => hold.view[0] += deg.to_radians(),
            Inject::FreezeBody => hold.freeze_body = true,
            _ => {}
        }
    }

    pub fn adjust_plan(&self, plan: &mut TickPlan, hunt: &Hunt) {
        if let Inject::DropShot(n) = *self {
            if hunt.weapons.current == n {
                plan.input.fire = false;
            }
        }
    }

    /// Before the tick numbered `tick`.
    pub fn adjust_hunt(&self, tick: u32, warmup: u32, hunt: &mut Hunt) {
        if let Inject::KillAnimal(n) = *self {
            if tick == warmup + 1 {
                if let Some(c) = hunt.herd.chars.get_mut(n) {
                    c.health = 0;
                }
            }
        }
    }
}
