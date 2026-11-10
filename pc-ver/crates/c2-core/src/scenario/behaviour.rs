//! The behaviour script (`script = "behaviours"`): the hunter walks, then
//! runs, along his view; each animal in turn is alarmed and watched as it
//! runs; then each is slain and watched as it dies. Speeds, clips and the
//! dead body's pose are measured, so the same stages in the original
//! engine (`c2-regress run --engine original`) can be compared with ours.
//!
//! - **Hunter speed**: forward held for `move_ticks`, the ground distance
//!   over the last `measure_ticks` (after he has sped up) per second. The
//!   original caps it at 0.3 units/ms walking and 0.7 running
//!   (`ProcessPlayerMovement`), so on open ground the speeds are 300 and
//!   700 units/s.
//! - **Animal run**: `State = 2` with a long `AfraidTime`, as a shot or a
//!   heard hunter does. Herbivores flee and predators charge; either way
//!   the animal must cover ground at its running speed, judged on its
//!   fastest `measure_ticks` (a T-Rex first turns, looks and sniffs). The
//!   watch ends early when a charging animal comes within
//!   [`NEAR_HUNTER`], before it can kill him.
//! - **Death**: `Health = 0`, the original's way into `Animate*Dead`. The
//!   death clip must play, then hold its last frame, and the body must
//!   stay where it fell (a slide of a few hundred units at most).

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec, vec::Vec};

use super::director::Step;
use super::{Arrange, Cast, Hold, Scenario};
use crate::area::Area;
use crate::game::Hunt;

/// A runner this close to the hunter has shown its run; its watch ends
/// before a predator reaches him.
pub const NEAR_HUNTER: f32 = 2048.0;

/// What the behaviour script does, from the scenario file's
/// `[behaviours]`.
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviourPlan {
    /// Ticks the hunter holds forward, and the last of them measured.
    pub move_ticks: u32,
    pub measure_ticks: u32,
    /// The hunter's expected ground speeds, units per second, and how far
    /// off (a fraction) a measured speed may be.
    pub walk_speed: f32,
    pub run_speed: f32,
    pub speed_tolerance: f32,
    /// Ticks an alarmed animal is watched; over the last `measure_ticks`
    /// of them it must average at least `min_run_speed` units/s.
    pub run_ticks: u32,
    pub min_run_speed: f32,
    /// Ticks a slain animal is watched. Its pose must change on at least
    /// `min_moving_ticks`, then stay the same for the last `hold_ticks`,
    /// and it may end at most `max_drift` units from where it was slain.
    pub die_ticks: u32,
    pub min_moving_ticks: u32,
    pub hold_ticks: u32,
    pub max_drift: f32,
}

/// One sample of whatever a behaviour stage watches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub at: [f32; 2],
    pub phase: usize,
    pub ftime: i32,
    /// The animal's size (its speeds scale with it); 1 for the hunter.
    pub scale: f32,
}

/// The samples of one stage, `sample_ms` apart (our 10 ms ticks, or the
/// original's 20 ms frames).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub sample_ms: u32,
    pub samples: Vec<Sample>,
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

impl Track {
    pub fn new(sample_ms: u32) -> Self {
        Self {
            sample_ms,
            samples: Vec::new(),
        }
    }

    pub fn push(&mut self, s: Sample) {
        self.samples.push(s);
    }

    /// Ground path length over the last `n` steps, per second.
    pub fn speed(&self, n: usize) -> f32 {
        let from = self.samples.len().saturating_sub(n + 1);
        self.speed_over(from, self.samples.len())
    }

    fn speed_over(&self, from: usize, to: usize) -> f32 {
        let part = &self.samples[from..to];
        let steps = part.len().saturating_sub(1);
        if steps == 0 || self.sample_ms == 0 {
            return 0.0;
        }
        let path: f32 = part.windows(2).map(|w| dist(w[0].at, w[1].at)).sum();
        path * 1000.0 / (steps as f32 * self.sample_ms as f32)
    }

    /// The fastest `n` consecutive steps, per second (all of them when
    /// there are fewer).
    pub fn peak_speed(&self, n: usize) -> f32 {
        let len = self.samples.len();
        if len <= n + 1 {
            return self.speed_over(0, len);
        }
        (0..len - n)
            .map(|i| self.speed_over(i, i + n + 1))
            .fold(0.0, f32::max)
    }

    /// The clips played, in order, each once per run of samples.
    pub fn clips(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for s in &self.samples {
            if out.last() != Some(&s.phase) {
                out.push(s.phase);
            }
        }
        out
    }

    /// Steps on which the pose (clip, frame time) changed.
    pub fn moving(&self) -> u32 {
        self.samples
            .windows(2)
            .filter(|w| (w[0].phase, w[0].ftime) != (w[1].phase, w[1].ftime))
            .count() as u32
    }

    /// Steps at the end over which the pose stayed the same.
    pub fn held(&self) -> u32 {
        let Some(last) = self.samples.last() else {
            return 0;
        };
        self.samples
            .iter()
            .rev()
            .skip(1)
            .take_while(|s| (s.phase, s.ftime) == (last.phase, last.ftime))
            .count() as u32
    }

    /// The farthest the subject got from its first sample.
    pub fn drift(&self) -> f32 {
        let Some(first) = self.samples.first() else {
            return 0.0;
        };
        self.samples
            .iter()
            .map(|s| dist(s.at, first.at))
            .fold(0.0, f32::max)
    }
}

/// What a behaviour stage watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// The hunter, walking or running.
    Hunter { run: bool },
    /// Scenario animal `n`, alarmed.
    Runner(usize),
    /// Scenario animal `n`, slain.
    Dying(usize),
}

/// How a behaviour stage went, for comparing engines.
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviourReport {
    pub stage: String,
    /// `walk`, `run`, `animal_run` or `death`.
    pub kind: &'static str,
    /// Units per second over the measured steps.
    pub speed: f32,
    /// The subject's size: an animal's speeds are its species' times this.
    pub scale: f32,
    pub clips: Vec<usize>,
    /// How long the last pose was held, ms.
    pub held_ms: u32,
    pub drift: f32,
}

impl Subject {
    pub fn kind(self) -> &'static str {
        match self {
            Subject::Hunter { run: false } => "walk",
            Subject::Hunter { run: true } => "run",
            Subject::Runner(_) => "animal_run",
            Subject::Dying(_) => "death",
        }
    }

    /// `(check name, passed, detail)` for a watched stage. `ticks` are
    /// counts of steps in the scenario's own 10 ms ticks; a track sampled
    /// less often is judged on the same lengths of time.
    pub fn verdicts(self, t: &Track, p: &BehaviourPlan) -> Vec<(&'static str, bool, String)> {
        let per = (t.sample_ms / super::TICK_MS as u32).max(1);
        let steps = |ticks: u32| (ticks / per) as usize;
        match self {
            Subject::Hunter { run } => {
                let (name, want) = if run {
                    ("run_speed", p.run_speed)
                } else {
                    ("walk_speed", p.walk_speed)
                };
                let v = t.speed(steps(p.measure_ticks));
                let ok = (v - want).abs() <= want * p.speed_tolerance;
                let detail = format!(
                    "{v:.1} units/s, {want:.0} expected (within {:.0}%)",
                    p.speed_tolerance * 100.0
                );
                vec![(name, ok, detail)]
            }
            Subject::Runner(_) => {
                let v = t.peak_speed(steps(p.measure_ticks));
                let detail = format!(
                    "{v:.1} units/s, at least {:.0} needed; clips {:?}",
                    p.min_run_speed,
                    t.clips()
                );
                vec![("runs", v >= p.min_run_speed, detail)]
            }
            Subject::Dying(_) => {
                let (moving, held) = (t.moving(), t.held());
                let (min_moving, hold) = (p.min_moving_ticks / per, p.hold_ticks / per);
                let drift = t.drift();
                vec![
                    (
                        "death_clip_plays",
                        moving >= min_moving,
                        format!(
                            "pose changed on {moving} steps, {min_moving} needed; clips {:?}",
                            t.clips()
                        ),
                    ),
                    (
                        "death_pose_holds",
                        held >= hold,
                        format!("last pose held {held} steps, {hold} needed"),
                    ),
                    (
                        "body_stays",
                        drift <= p.max_drift,
                        format!("moved {drift:.0} units, at most {:.0}", p.max_drift),
                    ),
                ]
            }
        }
    }

    /// The stage's summary for comparisons between engines.
    pub fn report(self, stage: &str, t: &Track, p: &BehaviourPlan) -> BehaviourReport {
        let per = (t.sample_ms / super::TICK_MS as u32).max(1);
        let n = (p.measure_ticks / per) as usize;
        BehaviourReport {
            stage: stage.into(),
            kind: self.kind(),
            speed: match self {
                Subject::Runner(_) => t.peak_speed(n),
                _ => t.speed(n),
            },
            scale: t.samples.first().map_or(1.0, |s| s.scale),
            clips: t.clips(),
            held_ms: t.held() * t.sample_ms,
            drift: t.drift(),
        }
    }

    /// A runner that has come within [`NEAR_HUNTER`] of `hunter`.
    pub fn near(self, s: &Sample, hunter: [f32; 2]) -> bool {
        matches!(self, Subject::Runner(_)) && dist(s.at, hunter) < NEAR_HUNTER
    }

    /// This tick's sample of the subject, from our engine.
    pub fn sample(self, hunt: &Hunt, hold: &[usize]) -> Option<Sample> {
        match self {
            Subject::Hunter { .. } => Some(Sample {
                at: [hunt.player.x, hunt.player.z],
                phase: 0,
                ftime: 0,
                scale: 1.0,
            }),
            Subject::Runner(n) | Subject::Dying(n) => {
                let c = hunt.herd.chars.get(*hold.get(n)?)?;
                Some(Sample {
                    at: [c.pos[0], c.pos[2]],
                    phase: c.phase,
                    ftime: c.ftime,
                    scale: c.scale,
                })
            }
        }
    }
}

/// Lower-case letters and digits, `_` between words.
pub(super) fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    String::from(out.trim_end_matches('_'))
}

/// The stage names, in order, as both engines run them.
pub fn stage_names(cast_names: &[String]) -> Vec<(String, Subject)> {
    let mut out = vec![
        ("hunter_walk".into(), Subject::Hunter { run: false }),
        ("hunter_run".into(), Subject::Hunter { run: true }),
    ];
    for (n, name) in cast_names.iter().enumerate() {
        out.push((
            format!("run_{:02}_{}", n + 1, slug(name)),
            Subject::Runner(n),
        ));
    }
    for (n, name) in cast_names.iter().enumerate() {
        out.push((
            format!("die_{:02}_{}", n + 1, slug(name)),
            Subject::Dying(n),
        ));
    }
    out
}

/// The script's steps: walk, run, each animal running, each dying.
pub(super) fn steps(s: &Scenario, p: &BehaviourPlan, cast: &[Cast]) -> Vec<Step> {
    let names: Vec<String> = cast.iter().map(|c| c.name.clone()).collect();
    let mut steps = super::Director::opening(s);
    for (stage, subject) in stage_names(&names) {
        steps.push(Step::Begin(stage.clone(), None));
        let (before, ticks, after) = match subject {
            Subject::Hunter { .. } => (Arrange::FreeHunter, p.move_ticks, Arrange::PinHunter),
            Subject::Runner(n) => (Arrange::Alarm(n), p.run_ticks, Arrange::Calm(n)),
            // The dead stay dead; `Calm` of a dead animal does nothing.
            Subject::Dying(n) => (Arrange::Slay(n), p.die_ticks, Arrange::Calm(n)),
        };
        steps.extend([
            Step::Do(before),
            Step::Behave(subject, ticks),
            Step::Checkpoint(stage),
            Step::VerifyBehaviour(subject),
            Step::Do(after),
            Step::End,
        ]);
    }
    steps.extend([
        Step::Begin("animals_dead".into(), None),
        Step::VerifyDead,
        Step::Checkpoint("end".into()),
        Step::End,
    ]);
    steps
}

impl Hold {
    /// The behaviour script's changes to the hunt (test runs only).
    pub(super) fn behave(&mut self, hunt: &mut Hunt, area: &Area, a: Arrange) {
        match a {
            Arrange::FreeHunter => self.hunter_free = true,
            Arrange::PinHunter => {
                self.hunter_free = false;
                let p = &mut hunt.player;
                [p.x, p.z] = self.player;
                p.y = area.land_qh(p.x, p.z, f32::MAX);
                (p.vspeed, p.sspeed, p.yspeed) = (0.0, 0.0, 0.0);
            }
            Arrange::Alarm(n) => {
                let Some(&(i, _, _)) = self.animals.get(n) else {
                    return;
                };
                if let Some(c) = hunt.herd.chars.get_mut(i) {
                    // A shot or a heard hunter: `State = 2` starts the
                    // run on the next frame, `AfraidTime` keeps it going.
                    c.state = 2;
                    c.afraid_time = 20_000;
                    c.aware_hunter = true;
                    c.no_find_cnt = 0;
                    self.released = Some(i);
                }
            }
            Arrange::Slay(n) => {
                let Some(&(i, _, _)) = self.animals.get(n) else {
                    return;
                };
                if let Some(c) = hunt.herd.chars.get_mut(i) {
                    // From a standstill: a pinned animal may still be
                    // running on the spot, and its speed would carry the
                    // body away (`Animate*Dead` slows it over ~1 s).
                    c.health = 0;
                    c.vspeed = 0.0;
                    self.dead.push(i);
                }
            }
            Arrange::Calm(n) => {
                self.released = None;
                let Some(&(i, x, z)) = self.animals.get(n) else {
                    return;
                };
                match hunt.herd.chars.get_mut(i) {
                    Some(c) if c.health > 0 => {
                        c.pos = [x, area.land_height(x, z), z];
                        c.state = 0;
                        c.afraid_time = 0;
                        c.aware_hunter = false;
                        c.vspeed = 0.0;
                    }
                    _ => {}
                }
            }
            Arrange::Release(_) | Arrange::Revive(_) => {}
        }
    }
}
