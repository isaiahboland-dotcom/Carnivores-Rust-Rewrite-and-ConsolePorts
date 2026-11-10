//! The kill script (`script = "kills"`): each animal in turn is set on the
//! pinned hunter, and its kill is watched. The hunter's body must play a
//! real clip and move, the killer must keep animating, and the body must
//! stay with the killer. Then the hunter is brought back for the next one.
//!
//! Kills are where a model change, a dialect kill type or a hunter-model
//! merge has broken before: a frozen or missing body, a body left floating
//! away from its killer, a killer stuck on one frame. The checkpoints'
//! state hashes cover the body's and the killer's exact poses, and the
//! render pass takes a picture at each.
//!
//! "With the killer": the body lands within reach of it, and afterwards
//! leaves the spot it fell on only by staying with it (carried, or
//! synchronized with its clip). A killer that gores and runs on, as the
//! Chasmosaurus and the animals that share its AI do, leaves the body
//! where it fell; that passes.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use super::Hold;
use crate::area::Area;
use crate::dino::{ai, find_vector_alpha};
use crate::game::{Hunt, MAX_HEALTH};

/// What the kill script does, from the scenario file's `[kills]`.
#[derive(Debug, Clone, PartialEq)]
pub struct KillPlan {
    /// How far in front of the hunter a killer is set down, world units.
    pub start_distance: f32,
    /// Most ticks a killer may take to kill.
    pub timeout_ticks: u32,
    /// Ticks from the kill to the `_mid` checkpoint, then to `_late`.
    pub mid_ticks: u32,
    pub late_ticks: u32,
    /// Ticks on which the body's and the killer's animation must change.
    pub min_moving_ticks: u32,
    /// How far the body may be from its killer when it falls, and how far
    /// it may stray (away from both its killer and where it fell), as a
    /// multiple of the killer's size (model reach times scale), and never
    /// less than `min_body_reach` world units.
    pub body_reach: f32,
    pub min_body_reach: f32,
}

/// A change to the hunt between ticks, asked for by the kill and
/// behaviour scripts and made by the frontend (`Hold::arrange`), which
/// owns the hunt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    /// Scenario animal `n` is set down in front of the hunter, alarmed and
    /// no longer pinned.
    Release(usize),
    /// The hunter is brought back: the body goes, the death camera ends,
    /// and animal `n` is calmed and pinned again where it stood.
    Revive(usize),
    /// Behaviours: the hunter may move (his view stays pinned), then is
    /// put back on his landing, standing still.
    FreeHunter,
    PinHunter,
    /// Behaviours: animal `n` is alarmed where it stands and no longer
    /// pinned; then calmed and pinned again where it was placed.
    Alarm(usize),
    Calm(usize),
    /// Behaviours: animal `n`'s health goes to 0 and it stays dead.
    Slay(usize),
}

impl Arrange {
    /// Run even when its stage fails, so the next stage starts as planned.
    pub fn restores(self) -> bool {
        matches!(
            self,
            Arrange::Revive(_) | Arrange::PinHunter | Arrange::Calm(_)
        )
    }
}

impl Hold {
    /// Makes `a` (test runs only).
    pub fn arrange(&mut self, hunt: &mut Hunt, area: &Area, a: Arrange, script: &super::Script) {
        match (a, script) {
            (Arrange::Release(n), super::Script::Kills(k)) => {
                self.release(hunt, area, n, k.start_distance)
            }
            (Arrange::Revive(n), _) => self.revive(hunt, area, n),
            (Arrange::Release(_), _) => {}
            _ => self.behave(hunt, area, a),
        }
    }

    fn release(&mut self, hunt: &mut Hunt, area: &Area, n: usize, distance: f32) {
        let Some(&(i, _, _)) = self.animals.get(n) else {
            return;
        };
        let [px, pz] = self.player;
        let yaw = self.view[0];
        let (x, z) = (px + yaw.sin() * distance, pz - yaw.cos() * distance);
        let Some(c) = hunt.herd.chars.get_mut(i) else {
            return;
        };
        c.pos = [x, area.land_height(x, z), z];
        c.alpha = find_vector_alpha(px - x, pz - z);
        c.tgalpha = c.alpha;
        // As `c2-pc --die`: hunting, for long enough to reach the hunter.
        c.state = 1;
        c.afraid_time = 20_000;
        c.aware_hunter = true;
        c.no_find_cnt = 0;
        self.released = Some(i);
    }

    fn revive(&mut self, hunt: &mut Hunt, area: &Area, n: usize) {
        if let Some(d) = hunt.death.take() {
            if d.body < hunt.herd.chars.len() {
                hunt.herd.chars.remove(d.body);
                for a in &mut self.animals {
                    if a.0 > d.body {
                        a.0 -= 1;
                    }
                }
            }
        }
        hunt.herd.demo_index = None;
        hunt.health = MAX_HEALTH;
        hunt.exit_prompt = false;
        self.released = None;
        let Some(&(i, x, z)) = self.animals.get(n) else {
            return;
        };
        if let Some(c) = hunt.herd.chars.get_mut(i) {
            c.pos = [x, area.land_height(x, z), z];
            c.state = 0;
            c.afraid_time = 0;
            c.aware_hunter = false;
            c.vspeed = 0.0;
        }
    }
}

/// What the director samples each tick of a kill.
#[derive(Debug, Clone, Default)]
pub struct KillWatch {
    pub killer: usize,
    pub body: usize,
    pub start_tick: u32,
    /// Ticks on which the body's, then the killer's, (clip, frame time)
    /// changed.
    pub body_moving: u32,
    pub killer_moving: u32,
    last_body: Option<(usize, i32)>,
    last_killer: Option<(usize, i32)>,
    /// Clips the body played, in order, and the killer's.
    pub body_clips: Vec<usize>,
    pub killer_clips: Vec<usize>,
    /// The body's distance from the killer when it fell, where it fell,
    /// how far it strayed at most, and the limit.
    pub start_gap: Option<f32>,
    fell_at: [f32; 2],
    pub max_stray: f32,
    pub reach: f32,
    /// The first clip that is not one of the hunter model's.
    pub bad_clip: Option<String>,
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// How a kill went, for comparing engines (the original's against ours):
/// time from release to kill, the clips played, and where the body lay.
#[derive(Debug, Clone, PartialEq)]
pub struct KillReport {
    pub stage: String,
    pub ms_to_kill: u32,
    pub body_clips: Vec<usize>,
    pub killer_clips: Vec<usize>,
    pub start_gap: f32,
    pub max_stray: f32,
}

/// A character's clip, frame time and ground position, as a kill watch
/// sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub phase: usize,
    pub ftime: i32,
    pub at: [f32; 2],
}

/// Whether `phase` is one of a hunter model's clips (`ani_ms`: each clip's
/// length), with a length above zero.
pub fn hunter_clip(ani_ms: &[i32], phase: usize) -> Result<(), String> {
    match ani_ms.get(phase) {
        Some(&ms) if ms > 0 => Ok(()),
        _ => Err(format!(
            "clip {phase} of a hunter model with {} clips",
            ani_ms.len()
        )),
    }
}

fn note(clips: &mut Vec<usize>, phase: usize) {
    if clips.last() != Some(&phase) {
        clips.push(phase);
    }
}

impl KillWatch {
    /// A kill to watch: `reach` from [`KillWatch::reach_for`]. The original
    /// engine's runner builds one from the game's memory.
    pub fn new(killer: usize, body: usize, start_tick: u32, reach: f32) -> Self {
        Self {
            killer,
            body,
            start_tick,
            reach,
            ..Default::default()
        }
    }

    /// How far the body may be from a killer of model reach `size` at
    /// `scale`.
    pub fn reach_for(size: f32, scale: f32, plan: &KillPlan) -> f32 {
        (size * scale.max(0.1) * plan.body_reach).max(plan.min_body_reach)
    }

    /// The kill has happened: who did it, and the limits for this killer.
    pub fn start(hunt: &Hunt, tick: u32, plan: &KillPlan) -> Option<Self> {
        let d = hunt.death?;
        let killer = d.killer?;
        let k = hunt.herd.chars.get(killer)?;
        let size = hunt.herd.species.get(k.ctype).map_or(0.0, |s| s.size);
        let reach = Self::reach_for(size, k.scale, plan);
        let mut w = Self::new(killer, d.body, tick, reach);
        w.sample(hunt);
        Some(w)
    }

    /// One tick of the kill.
    pub fn sample(&mut self, hunt: &Hunt) {
        let chars = &hunt.herd.chars;
        let (Some(b), Some(k)) = (chars.get(self.body), chars.get(self.killer)) else {
            self.bad_clip
                .get_or_insert("the body or the killer is gone".into());
            return;
        };
        let sp = hunt.herd.species.get(b.ctype);
        let clip = match sp {
            Some(s) if s.info.ai != ai::HUNTER => {
                Err(format!("the body is species {} (not a hunter)", b.ctype))
            }
            Some(s) => hunter_clip(&s.ani_time, b.phase),
            None => Err("the body has no species".into()),
        };
        let pose = |c: &crate::dino::Character| Pose {
            phase: c.phase,
            ftime: c.ftime,
            at: [c.pos[0], c.pos[2]],
        };
        self.push(pose(b), pose(k), clip);
    }

    /// One tick of the kill, from the poses: `clip` says whether the
    /// body's clip is one of the hunter model's ([`hunter_clip`]).
    pub fn push(&mut self, b: Pose, k: Pose, clip: Result<(), String>) {
        if let Err(e) = clip {
            self.bad_clip.get_or_insert(e);
        }
        let body = (b.phase, b.ftime);
        let killer = (k.phase, k.ftime);
        if self.last_body.is_some_and(|l| l != body) {
            self.body_moving += 1;
        }
        if self.last_killer.is_some_and(|l| l != killer) {
            self.killer_moving += 1;
        }
        self.last_body = Some(body);
        self.last_killer = Some(killer);
        note(&mut self.body_clips, b.phase);
        note(&mut self.killer_clips, k.phase);
        let gap = dist(b.at, k.at);
        if self.start_gap.is_none() {
            self.start_gap = Some(gap);
            self.fell_at = b.at;
        }
        let drift = dist(b.at, self.fell_at);
        self.max_stray = self.max_stray.max(gap.min(drift));
    }

    /// The kill's summary, for comparisons between engines.
    pub fn report(&self, stage: &str, ms_to_kill: u32) -> KillReport {
        KillReport {
            stage: stage.into(),
            ms_to_kill,
            body_clips: self.body_clips.clone(),
            killer_clips: self.killer_clips.clone(),
            start_gap: self.start_gap.unwrap_or(f32::INFINITY),
            max_stray: self.max_stray,
        }
    }

    /// `(check name, passed, detail)` for the watched kill.
    pub fn verdicts(&self, plan: &KillPlan) -> [(&'static str, bool, String); 4] {
        let min = plan.min_moving_ticks;
        [
            (
                "hunter_clip",
                self.bad_clip.is_none(),
                self.bad_clip
                    .clone()
                    .unwrap_or_else(|| format!("clips {:?}", self.body_clips)),
            ),
            (
                "hunter_animates",
                self.body_moving >= min,
                format!("body moved on {} ticks, {min} needed", self.body_moving),
            ),
            (
                "killer_animates",
                self.killer_moving >= min,
                format!(
                    "killer moved on {} ticks, {min} needed; clips {:?}",
                    self.killer_moving, self.killer_clips
                ),
            ),
            (
                "body_with_killer",
                self.start_gap.is_some_and(|g| g <= self.reach) && self.max_stray <= self.reach,
                format!(
                    "fell {:.0} from the killer, strayed {:.0}, limit {:.0}",
                    self.start_gap.unwrap_or(f32::INFINITY),
                    self.max_stray,
                    self.reach
                ),
            ),
        ]
    }
}
