//! Predation at run time: every few hundred ms each animal picks a goal
//! ([`Predators::think`]); each step the herd gives its AI the target for
//! that goal ([`Predators::steer`], a [`Target`] in place of the player),
//! then sees whether the goal was reached ([`Predators::settle`]).
//!
//! An AI that runs at its target (carnivores, Triceratops) is shown the
//! point it should reach, or one far beyond whatever it flees. An AI that
//! runs from it is shown what it flees, or, to make it approach, the point
//! mirrored through itself (it runs away from that, towards the target).
//! A hunted or charged animal is an [`TargetKind::Animal`] target, which the
//! AI strikes without its hunter-kill clip; the rest are places.
//!
//! Senses follow `CheckAfraid` (`wildlife.rs`) without its look traces,
//! which are too costly for every pair: the distance is doubled instead, for
//! the cover they would find. Hearing is added for a running animal.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{vec, vec::Vec};

use c2_formats::predation::Priority;

use super::{Drive, Kind, Rel, Role, Rules};
use crate::dino::mee::KillType;
use crate::dino::{
    ai, hunt, trex, CarnivoreParams, Character, Ctx, HunterView, Species, Target, TargetKind,
};

/// How often goals are re-picked, ms.
const THINK_MS: i32 = 300;
/// A predator gives up a chase this many cells beyond its view.
const CHASE_CELLS: f32 = 1.5;
/// A charge lasts 5 to 10 seconds; then charging rests for 20.
const CHARGE_MS: i32 = 5_000;
const CHARGE_REST_MS: i32 = 20_000;
/// After a kill: eating, and the rest before the next hunt.
const FEED_MS: i32 = 12_000;
const HUNT_REST_MS: i32 = 30_000;
/// Prey runs from a sensed predator this close (any distance when hunted),
/// and stops once it is twice as far.
const FLEE_RANGE: f32 = 4096.0;
/// A predator that is eating, resting or chasing another animal scares
/// prey only this close, so a herd can graze near it.
const CALM_FLEE_RANGE: f32 = 1024.0;
/// A charging herbivore's AI charges within this; farther, it runs.
const CHARGE_RANGE: f32 = 4000.0;
/// The farthest a chasing AI is shown its target; farther ones are
/// steered to a point this far along the way.
const LEAD: f32 = 2560.0;
/// Running off after scavenging or backing down.
const RUN_MS: i32 = 6_000;
const RUN_REST_MS: i32 = 15_000;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Goal {
    #[default]
    None,
    /// Chasing character `n` to kill it.
    Hunt(usize),
    /// Running from character `n`.
    Flee(usize),
    /// Heading for the body of character `n`.
    Scavenge(usize),
    /// A defensive herbivore charging character `n`.
    Charge(usize),
    /// Running away from a point.
    Run([f32; 2]),
}

impl Goal {
    pub fn target(self) -> Option<usize> {
        match self {
            Goal::Hunt(n) | Goal::Flee(n) | Goal::Scavenge(n) | Goal::Charge(n) => Some(n),
            Goal::None | Goal::Run(_) => None,
        }
    }

    fn with_target(self, n: Option<usize>) -> Goal {
        match (self, n) {
            (Goal::Hunt(_), Some(n)) => Goal::Hunt(n),
            (Goal::Flee(_), Some(n)) => Goal::Flee(n),
            (Goal::Scavenge(_), Some(n)) => Goal::Scavenge(n),
            (Goal::Charge(_), Some(n)) => Goal::Charge(n),
            (Goal::Run(p), _) => Goal::Run(p),
            _ => Goal::None,
        }
    }
}

/// One character's predation state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Track {
    pub goal: Goal,
    /// Ms left of a charge or a run.
    pub until: i32,
    /// Ms before it hunts, charges or scavenges again.
    pub rest: i32,
    /// Eating: ms left, and the clip.
    pub eat_ms: i32,
    pub eat_phase: Option<usize>,
    /// A body: a scavenger has eaten from it.
    pub eaten: bool,
    /// A body: killed by an animal, not the hunter.
    pub killed_by_animal: bool,
}

/// What [`Predators::steer`] tells the herd for one step.
#[derive(Debug, Clone, Copy)]
pub struct Steer {
    pub view: HunterView,
    pub kind: TargetKind,
    /// The dialect's carnivores only chase what they are aware of: set
    /// when its AI should close in on the target.
    pub aware: bool,
}

impl Steer {
    /// The AI's target for the step. Approaching, it is aware of the target
    /// and has heard no shot from it; otherwise it keeps what it knows of
    /// the player.
    pub fn target(self, c: &Character) -> Target {
        Target {
            view: self.view,
            kind: self.kind,
            aware: self.aware || c.aware_hunter,
            heard_shot: !self.aware && c.heard_shot,
        }
    }
}

/// The hunt's predation: its rules and when they were last thought over.
#[derive(Debug, Clone, Default)]
pub struct Predators {
    pub rules: Rules,
    since: i32,
}

fn dist2(a: &Character, b: &Character) -> f32 {
    let (dx, dz) = (a.pos[0] - b.pos[0], a.pos[2] - b.pos[2]);
    dx * dx + dz * dz
}

/// Whether `o` notices `t` (alive), or smells `t`'s body.
fn senses(o: &Character, t: &Character, sp: &Species, view_r: i32) -> bool {
    let d = [
        t.pos[0] - o.pos[0],
        t.pos[1] - o.pos[1],
        t.pos[2] - o.pos[2],
    ];
    let rl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let kr = 2.0 * rl / 256.0 / (32.0 + view_r as f32 / 2.0);
    if kr > 3.0 {
        return false;
    }
    if rl <= 0.0 {
        return true;
    }
    let klook = -(o.lookx * d[0] + o.lookz * d[2]) / rl;
    let face = (klook + 3.0) / 3.0;
    let i = &sp.info;
    if t.health == 0 {
        return kr * 0.75 / i.smell_k.max(1e-3) < 1.0;
    }
    let mut k_look = kr * face * 0.7;
    if klook > 0.3 {
        k_look *= 2.0;
    }
    if klook > 0.8 {
        k_look *= 2.0;
    }
    k_look /= i.look_k.max(1e-3);
    let k_smell = kr * face / i.smell_k.max(1e-3);
    let k_hear = if t.state != 0 {
        kr * 2.0 / i.hear_k.max(1e-3)
    } else {
        f32::INFINITY
    };
    k_look.min(k_smell).min(k_hear) < 1.0
}

/// The clip a carnivore eats a kill with: the ground-feeding one the
/// Velociraptor and Allosaurus kill the hunter with. Kills that lift, carry
/// or pose with the hunter's body are staged for him alone, so a species
/// with only those noses at the body with its sniffing clip.
pub(super) fn feed_phase(sp: &Species, kill_type: usize) -> Option<usize> {
    if let Some(m) = &sp.mee {
        let ground = |k: &KillType| {
            k.hunter_anim == hunt::EAT && !k.hunter_sync && !k.carry_corpse && k.anim != usize::MAX
        };
        return m
            .kill(kill_type)
            .filter(ground)
            .or_else(|| m.kill_types.iter().copied().find(ground))
            .map(|k| k.anim)
            .or_else(|| m.smells.first().copied());
    }
    match sp.runs_as() {
        ai::TREX => Some(trex::SMEL),
        a => CarnivoreParams::for_ai(a).map(|p| p.eat),
    }
}

/// Starts eating: `phase` from its first frame.
fn feed(c: &mut Character, phase: Option<usize>) {
    c.predation.eat_phase = phase;
    if let Some(ph) = phase {
        c.phase = ph;
        c.ftime = 0;
    }
}

impl Track {
    /// Counts down its timers.
    pub fn tick(&mut self, dt: i32) {
        self.until = (self.until - dt).max(0);
        self.rest = (self.rest - dt).max(0);
    }
}

impl Predators {
    pub fn new(rules: Rules) -> Self {
        Self { rules, since: 0 }
    }

    /// Character `removed` left the herd: renumber every goal.
    pub fn forget(chars: &mut [Character], removed: usize) {
        for c in chars {
            let t = &mut c.predation;
            let n = t.goal.target().and_then(|n| match n {
                n if n == removed => None,
                n if n > removed => Some(n - 1),
                n => Some(n),
            });
            t.goal = t.goal.with_target(n);
        }
    }

    /// Picks each animal's goal, every [`THINK_MS`].
    pub fn think(&mut self, chars: &mut [Character], species: &[Species], ctx: &mut Ctx<'_>) {
        self.since += ctx.dt;
        if self.since < THINK_MS {
            return;
        }
        self.since = 0;
        let mut next: Vec<Goal> = chars.iter().map(|c| c.predation.goal).collect();
        for (i, g) in next.iter_mut().enumerate() {
            *g = self.pick(i, chars, species, ctx);
        }
        // Two predators after one prey: the clearly higher one, else the
        // closer one, keeps it.
        for i in 0..chars.len() {
            let Goal::Hunt(p) = next[i] else { continue };
            for j in 0..chars.len() {
                if i == j || next[j] != Goal::Hunt(p) || next[i] != Goal::Hunt(p) {
                    continue;
                }
                let (a, b) = (chars[i].ctype, chars[j].ctype);
                let i_wins = self.rules.outranks(a, b).unwrap_or_else(|| {
                    chars
                        .get(p)
                        .is_some_and(|prey| dist2(&chars[i], prey) <= dist2(&chars[j], prey))
                });
                if i_wins {
                    next[j] = Goal::None;
                } else {
                    next[i] = Goal::None;
                }
            }
        }
        self.flee_as_packs(chars, &mut next);
        for (i, g) in next.into_iter().enumerate() {
            let t = &mut chars[i].predation;
            if t.goal == g {
                continue;
            }
            match g {
                Goal::Charge(_) => t.until = CHARGE_MS + ctx.rng.r_rand(CHARGE_MS),
                Goal::Run(_) if !matches!(t.goal, Goal::Run(_)) => {
                    t.until = RUN_MS;
                    t.rest = t.rest.max(RUN_REST_MS);
                }
                _ => {}
            }
            if matches!(t.goal, Goal::Charge(_)) && !matches!(g, Goal::Charge(_)) {
                t.rest = t.rest.max(CHARGE_REST_MS);
            }
            t.goal = g;
            if g == Goal::None {
                calm(&mut chars[i]);
            }
        }
    }

    /// A dialect pack runs together: a member fleeing a predator takes the
    /// packmates that have no goal of their own with it. (Its AI's pack
    /// alarm is kept for the player, so they would otherwise run from him.)
    fn flee_as_packs(&self, chars: &[Character], next: &mut [Goal]) {
        for i in 0..chars.len() {
            let (Goal::Flee(j), Some(pack)) = (next[i], chars[i].pack) else {
                continue;
            };
            for k in 0..chars.len() {
                let m = &chars[k];
                if k != j
                    && next[k] == Goal::None
                    && m.pack == Some(pack)
                    && !Self::idle(m)
                    && self.rules.kind(m.ctype).role != Role::None
                {
                    next[k] = Goal::Flee(j);
                }
            }
        }
    }

    /// Whether `c` takes no part right now.
    fn idle(c: &Character) -> bool {
        c.health == 0 || c.state_f == 0xff || c.wave > 0 || c.lure.is_some()
    }

    /// The nearest other character that `pred` holds of, and `i` senses.
    fn nearest(
        &self,
        i: usize,
        chars: &[Character],
        species: &[Species],
        view_r: i32,
        pred: impl Fn(usize, &Character) -> bool,
    ) -> Option<usize> {
        let c = &chars[i];
        let sp = species.get(c.ctype)?;
        chars
            .iter()
            .enumerate()
            .filter(|&(j, t)| j != i && t.state_f != 0xff && pred(j, t) && senses(c, t, sp, view_r))
            .min_by(|a, b| dist2(c, a.1).total_cmp(&dist2(c, b.1)))
            .map(|(j, _)| j)
    }

    fn pick(&self, i: usize, chars: &[Character], species: &[Species], ctx: &Ctx<'_>) -> Goal {
        let c = &chars[i];
        let t = c.predation;
        let k: Kind = self.rules.kind(c.ctype);
        if Self::idle(c) || k.role == Role::None {
            return Goal::None;
        }
        if t.eat_ms > 0 || (matches!(t.goal, Goal::Run(_)) && t.until > 0) {
            return t.goal;
        }
        let alive = |n: usize| {
            chars
                .get(n)
                .is_some_and(|x| x.health != 0 && !Self::idle(x))
        };
        let r = &self.rules;
        let vr = ctx.view_r;
        // Being charged, or a predator (or a bigger rival) close by.
        if let Some(j) =
            (0..chars.len()).find(|&j| alive(j) && chars[j].predation.goal == Goal::Charge(i))
        {
            return Goal::Flee(j);
        }
        if let Goal::Flee(j) = t.goal {
            let far = 2.0 * menace(i, &chars[j]).min(FLEE_RANGE);
            if alive(j) && dist2(c, &chars[j]) < far * far {
                return t.goal;
            }
        }
        if let Some(j) = self.nearest(i, chars, species, vr, |j, x| {
            let m = menace(i, x);
            alive(j) && r.rel(x.ctype, c.ctype) == Rel::Hunts && dist2(c, x) < m * m
        }) {
            return Goal::Flee(j);
        }
        if let Some(j) = self.nearest(i, chars, species, vr, |j, x| {
            alive(j) && r.rel(c.ctype, x.ctype) == Rel::Yields
        }) {
            let p = chars[j].pos;
            return Goal::Run([p[0], p[2]]);
        }
        // A charge runs its course.
        if let Goal::Charge(j) = t.goal {
            if t.until > 0 && alive(j) {
                return t.goal;
            }
            return Goal::None;
        }
        if t.rest > 0 {
            return Goal::None;
        }
        if k.defensive {
            if let Some(j) = self.nearest(i, chars, species, vr, |j, x| {
                alive(j) && r.rel(c.ctype, x.ctype) == Rel::Charges
            }) {
                return Goal::Charge(j);
            }
        }
        if k.role == Role::Carnivore {
            let busy = r.priority == Priority::Hunter && c.aware_hunter && c.state != 0;
            if !busy {
                let give_up = vr as f32 * 256.0 * CHASE_CELLS;
                if let Goal::Hunt(j) = t.goal {
                    if alive(j) && dist2(c, &chars[j]) < give_up * give_up {
                        return t.goal;
                    }
                }
                if let Some(j) = self.nearest(i, chars, species, vr, |j, x| {
                    alive(j) && r.rel(c.ctype, x.ctype) == Rel::Hunts
                }) {
                    return Goal::Hunt(j);
                }
            }
        }
        if k.scavenger && !(r.priority == Priority::Hunter && c.aware_hunter && c.state != 0) {
            let body = |j: usize, x: &Character| {
                x.health == 0
                    && !x.predation.eaten
                    && r.kind(x.ctype).role == Role::Herbivore
                    && j != i
            };
            if let Goal::Scavenge(j) = t.goal {
                if chars.get(j).is_some_and(|x| body(j, x)) {
                    return t.goal;
                }
            }
            if let Some(j) = self.nearest(i, chars, species, vr, body) {
                return Goal::Scavenge(j);
            }
        }
        Goal::None
    }

    /// For one step of character `i`: where its AI should think the hunter
    /// is, alarming it as needed. `None`: it runs as usual.
    pub fn steer(&self, i: usize, chars: &mut [Character], species: &[Species]) -> Option<Steer> {
        let c = chars.get(i)?;
        if c.health == 0 || c.predation.eat_ms > 0 {
            return None;
        }
        let k = self.rules.kind(c.ctype);
        let p = c.pos;
        let near =
            |x: f32, z: f32| (x - p[0]).powi(2) + (z - p[2]).powi(2) < CHARGE_RANGE * CHARGE_RANGE;
        let toward = |x: f32, z: f32, y: f32| -> HunterView {
            let direct = match k.drive {
                Drive::Chases => true,
                Drive::Flees => false,
                Drive::Charges => near(x, z),
            };
            if direct {
                // Within the distance its AI keeps up a chase (Allosaurus
                // gives up past `ctViewR * 128`): a point on the way.
                let (dx, dz) = (x - p[0], z - p[2]);
                let d = (dx * dx + dz * dz).sqrt();
                if d > LEAD {
                    view(p[0] + dx / d * LEAD, p[1] + 160.0, p[2] + dz / d * LEAD)
                } else {
                    view(x, y, z)
                }
            } else {
                view(2.0 * p[0] - x, y, 2.0 * p[2] - z)
            }
        };
        let away = |x: f32, z: f32| -> HunterView {
            let l = match k.drive {
                Drive::Chases => 4096.0,
                Drive::Charges => CHARGE_RANGE / 2.0,
                Drive::Flees => return view(x, p[1] + 160.0, z),
            };
            let (dx, dz) = (p[0] - x, p[2] - z);
            let d = (dx * dx + dz * dz).sqrt().max(1.0);
            view(p[0] + dx / d * l, p[1] + 160.0, p[2] + dz / d * l)
        };
        let (v, kind) = match c.predation.goal {
            Goal::None => return None,
            Goal::Hunt(j) | Goal::Charge(j) => {
                let t = chars.get(j)?.pos;
                (toward(t[0], t[2], t[1] + 160.0), TargetKind::Animal(j))
            }
            Goal::Scavenge(j) => {
                let t = chars.get(j)?.pos;
                (toward(t[0], t[2], t[1] + 160.0), TargetKind::Place)
            }
            Goal::Flee(j) => {
                let t = chars.get(j)?.pos;
                (away(t[0], t[2]), TargetKind::Place)
            }
            Goal::Run([x, z]) => (away(x, z), TargetKind::Place),
        };
        let mee = species.get(c.ctype).is_some_and(|s| s.mee.is_some());
        let c = &mut chars[i];
        if c.state == 0 {
            c.state = 2;
            c.no_find_cnt = 0;
        }
        c.afraid_time = c.afraid_time.max(6_000);
        Some(Steer {
            view: v,
            kind,
            aware: mee
                && k.drive == Drive::Chases
                && !matches!(c.predation.goal, Goal::Flee(_) | Goal::Run(_)),
        })
    }

    /// After character `i`'s step: `struck` when its AI struck the animal
    /// it was steered at ([`Ctx::strike`]). Kills, eats, or ends the goal.
    /// Carnivores eat with their ground-feeding clip ([`feed_phase`]);
    /// herbivores gore and run on, as the original's Chasmosaurus does
    /// (their AIs play no kill clip for an animal).
    pub fn settle(&self, i: usize, chars: &mut [Character], species: &[Species], struck: bool) {
        let Some(c) = chars.get(i) else { return };
        let goal = c.predation.goal;
        let chases = self.rules.kind(c.ctype).drive != Drive::Flees;
        let Some(j) = goal.target() else {
            if matches!(goal, Goal::Run(_)) && c.predation.until == 0 {
                chars[i].predation.goal = Goal::None;
                calm(&mut chars[i]);
            }
            return;
        };
        let Some(t) = chars.get(j) else {
            chars[i].predation.goal = Goal::None;
            return;
        };
        let radius = |n: usize| {
            species
                .get(chars[n].ctype)
                .map_or(0.0, |s| s.info.radius.max(0.0))
        };
        let reach = if chases { 200.0 } else { 256.0 } + radius(j) * 0.5 + radius(i) * 0.25;
        let reached = struck || dist2(&chars[i], t) < reach * reach;
        match goal {
            Goal::Hunt(_) | Goal::Charge(_) if reached && t.health != 0 => {
                let tp = &mut chars[j];
                tp.health = 0;
                tp.blood_ttime = 0;
                tp.predation = Track {
                    killed_by_animal: true,
                    ..Track::default()
                };
                let sp = species.get(chars[i].ctype);
                let c = &mut chars[i];
                c.state = 0;
                c.afraid_time = 0;
                c.predation.goal = Goal::None;
                if matches!(goal, Goal::Hunt(_)) {
                    c.predation.rest = HUNT_REST_MS;
                    feed(c, sp.and_then(|s| feed_phase(s, c.kill_type)));
                    if c.predation.eat_phase.is_some() {
                        c.predation.eat_ms = FEED_MS;
                    }
                } else {
                    c.predation.rest = CHARGE_REST_MS;
                }
            }
            Goal::Charge(_) if c.predation.until == 0 => {
                let c = &mut chars[i];
                c.predation.goal = Goal::None;
                c.predation.rest = CHARGE_REST_MS;
                calm(c);
            }
            Goal::Scavenge(_) if reached && t.health == 0 => {
                let at = [t.pos[0], t.pos[2]];
                chars[j].predation.eaten = true;
                let sp = species.get(chars[i].ctype);
                let c = &mut chars[i];
                c.state = 0;
                c.afraid_time = 0;
                feed(c, sp.and_then(|s| feed_phase(s, c.kill_type)));
                c.predation.eat_ms = match (c.predation.eat_phase, sp) {
                    (Some(ph), Some(sp)) => sp.time(ph),
                    _ => 0,
                };
                c.predation.goal = Goal::Run(at);
                c.predation.until = c.predation.eat_ms + RUN_MS;
                c.predation.rest = RUN_REST_MS;
            }
            _ => {}
        }
    }
}

/// How close predator `x` must be for character `i` to run from it: any
/// distance when it is after `i`, [`CALM_FLEE_RANGE`] while it eats, rests
/// or chases another animal, else [`FLEE_RANGE`].
fn menace(i: usize, x: &Character) -> f32 {
    let t = &x.predation;
    match t.goal {
        Goal::Hunt(n) | Goal::Charge(n) if n == i => f32::INFINITY,
        Goal::Hunt(_) | Goal::Charge(_) => CALM_FLEE_RANGE,
        _ if t.eat_ms > 0 || t.rest > 0 => CALM_FLEE_RANGE,
        _ => FLEE_RANGE,
    }
}

/// A goal ended: an animal that never noticed the hunter settles down
/// (its AI was alarmed only to steer it).
fn calm(c: &mut Character) {
    if !c.aware_hunter && c.health != 0 {
        c.state = 0;
        c.afraid_time = 0;
    }
}

fn view(x: f32, y: f32, z: f32) -> HunterView {
    HunterView {
        alpha: 0.0,
        x,
        y,
        z,
        head_y: 220.0,
        alive: true,
        underwater: false,
    }
}
