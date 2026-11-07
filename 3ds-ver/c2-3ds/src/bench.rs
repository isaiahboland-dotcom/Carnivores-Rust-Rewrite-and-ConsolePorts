//! Measurements on the console, written to `bench.txt` beside the packs.
//! In a hunt, SELECT (pause) then X runs [`Sequence`]: every count in
//! [`COUNTS`] placed as the game places them, then gathered about the
//! player (start an observer hunt: its health refills), frame figures for
//! each, then the herd alone. The test script (`shot.rs`) has the parts:
//!
//! - `animals N [CELLS]`: the hunt's main population made N
//!   ([`c2_core::dino::predation::bench::fill`]), and with CELLS moved
//!   within that many cells of the player; the frame figures start over.
//! - `prof NAME`: a line of frame figures since the last start: frames,
//!   fps, and mean / worst ms of the whole frame and of its parts.
//! - `bench N[,N...] [SECONDS]`: the herd alone, stepped headless at each
//!   count with predation on and off (as `c2-parity predation --count`);
//!   the live hunt is left as it was.

use std::io::Write;

use c2_core::dino::predation::{bench, run::Predators, Rules};
use c2_core::dino::{Herd, HunterView};
use c2_core::rng::MsRand;
use c2_formats::predation::Predation;

use crate::hunt::Game;

/// The parts of a frame, as the pause screen splits it.
const PARTS: [&str; 5] = ["frame", "sim", "world", "2d", "wait"];

/// Frame times since the last [`Frames::reset`].
#[derive(Debug, Default)]
pub struct Frames {
    count: u32,
    total: [f64; PARTS.len()],
    worst: [f32; PARTS.len()],
}

impl Frames {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One frame: its whole time and its parts (sim, world, 2D, wait), ms.
    pub fn add(&mut self, frame_ms: f32, parts: [f32; 4]) {
        self.count += 1;
        for (i, v) in core::iter::once(frame_ms).chain(parts).enumerate() {
            self.total[i] += f64::from(v);
            self.worst[i] = self.worst[i].max(v);
        }
    }

    fn line(&self, name: &str, animals: usize, target: &str) -> String {
        let n = f64::from(self.count.max(1));
        let fps = 1000.0 / (self.total[0] / n).max(1e-3);
        let parts: Vec<String> = PARTS
            .iter()
            .zip(self.total.iter().zip(&self.worst))
            .map(|(p, (t, w))| format!("{p} {:.1}/{w:.1}", t / n))
            .collect();
        format!(
            "prof {name} [{target}] {animals} animals: {} frames, {fps:.1} fps; ms mean/worst: {}",
            self.count,
            parts.join(", ")
        )
    }
}

/// Appends `line` to `bench.txt`.
fn append(line: &str) {
    let path = format!("{}/bench.txt", crate::dir());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// The populations [`Sequence`] measures.
pub const COUNTS: [usize; 3] = [20, 40, 60];
/// Frames: settling after a change, then measuring (at 30 fps or so,
/// about 30 and 20 s), the herd's seconds per run, and the gathering radius
/// in cells (inside the Old 3DS's draw distance).
const SETTLE: u32 = 120;
const MEASURE: u32 = 900;
const MEASURE_NEAR: u32 = 600;
const HERD_SECS: u32 = 30;
const NEAR_CELLS: f32 = 12.0;

/// A script command ([`crate::shot::Script::bench`]) or a [`Sequence`] step.
#[derive(Debug, Clone)]
pub enum Cmd {
    Animals(usize, Option<f32>),
    Prof(String),
    Herd(Vec<usize>, u32),
    /// Only starts the frame figures over.
    Reset,
    Done,
}

/// The whole measurement, one command after another.
pub struct Sequence {
    /// Frames to wait before each command, and the command.
    plan: Vec<(u32, Cmd)>,
    next: usize,
    since: u32,
}

impl Sequence {
    pub fn new(frame: u32) -> Self {
        let mut plan = Vec::new();
        for n in COUNTS {
            plan.push((0, Cmd::Animals(n, None)));
            plan.push((SETTLE, Cmd::Reset));
            plan.push((MEASURE, Cmd::Prof(format!("{n}-placed"))));
            plan.push((0, Cmd::Animals(n, Some(NEAR_CELLS))));
            plan.push((SETTLE, Cmd::Reset));
            plan.push((MEASURE_NEAR, Cmd::Prof(format!("{n}-near"))));
        }
        plan.push((0, Cmd::Herd(COUNTS.to_vec(), HERD_SECS)));
        plan.push((0, Cmd::Done));
        Self {
            plan,
            next: 0,
            since: frame,
        }
    }

    /// The command due at `frame`, if any.
    pub fn poll(&mut self, frame: u32) -> Option<Cmd> {
        let (wait, cmd) = self.plan.get(self.next)?;
        if frame < self.since + wait {
            return None;
        }
        self.next += 1;
        self.since = frame;
        Some(cmd.clone())
    }
}

/// Microseconds since boot.
fn now_us() -> u64 {
    (crate::ticks() as f64 / crate::TICKS_PER_SEC * 1e6) as u64
}

/// Runs `cmd` on the live hunt; a message for the screen.
pub fn run(cmd: &Cmd, game: &mut Game, frames: &mut Frames) -> Option<String> {
    let target = game.info.target.clone();
    let msg = match cmd {
        Cmd::Animals(n, near) => {
            let p = &game.hunt.player;
            let at = (p.x, p.z);
            let mut rng = game.hunt.rng.clone();
            bench::fill(&mut game.hunt.herd, &game.area, at, &mut rng, *n);
            if let Some(cells) = near {
                bench::gather(&mut game.hunt.herd, &game.area, at, &mut rng, *cells);
            }
            game.hunt.rng = rng;
            let got = bench::population(&game.hunt.herd);
            let near = near.map_or(String::new(), |c| format!(" within {c} cells"));
            append(&format!("animals {n} [{target}]: {got} placed{near}"));
            Some(format!("Bench: {got} animals{near}"))
        }
        Cmd::Prof(name) => {
            let animals = bench::population(&game.hunt.herd);
            append(&frames.line(name, animals, &target));
            None
        }
        Cmd::Herd(counts, secs) => {
            herd(game, counts, *secs, &target);
            None
        }
        Cmd::Reset => None,
        Cmd::Done => Some("Bench done: bench.txt".into()),
    };
    // The command's own frame is not one to count.
    frames.reset();
    msg
}

/// `bench`: a copy of the hunt's herd at each count, predation on and off.
fn herd(game: &Game, counts: &[usize], secs: u32, target: &str) {
    let live = &game.hunt.herd;
    let (predators, from) = match live.predation.as_deref() {
        Some(p) => (p.clone(), "the hunt's"),
        None => {
            let settings = Predation {
                carnivore_fights: true,
                ..Predation::default()
            };
            (
                Predators::new(Rules::new(&settings, &live.species)),
                "default",
            )
        }
    };
    let p = &game.hunt.player;
    let hunter = HunterView {
        x: p.x,
        z: p.z,
        y: game.area.land_height(p.x, p.z) + 220.0,
        head_y: 220.0,
        alive: false,
        ..Default::default()
    };
    append(&format!(
        "bench [{target}] {secs} s per run, {from} predation rules, {} species",
        live.species.len()
    ));
    for &n in counts {
        for on in [true, false] {
            let mut herd = Herd {
                species: live.species.clone(),
                chars: live.chars.clone(),
                packs: live.packs.clone(),
                groups: live.groups.clone(),
                predation: on.then(|| Box::new(predators.clone())),
                ..Default::default()
            };
            let mut rng = MsRand::new(777);
            bench::fill(&mut herd, &game.area, (p.x, p.z), &mut rng, n);
            let s = bench::measure(&mut herd, &game.area, hunter, &mut rng, secs, &mut now_us);
            append(&format!(
                "bench {} predation {}: step {:.2} ms mean, {:.2} ms worst; think {:.2} ms mean, {:.2} ms worst ({}); kills {}",
                s.animals,
                if on { "on " } else { "off" },
                s.step_mean_us as f64 / 1000.0,
                s.step_worst_us as f64 / 1000.0,
                s.think_mean_us as f64 / 1000.0,
                s.think_worst_us as f64 / 1000.0,
                s.thinks,
                s.kills
            ));
        }
    }
}
