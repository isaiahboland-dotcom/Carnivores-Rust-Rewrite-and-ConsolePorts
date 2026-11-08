//! The scenario script, one tick at a time: [`Director::plan`] gives the
//! tick's input, [`Director::observe`] checks the hunt afterwards and moves
//! the script on. Every wait has a timeout; a stage that times out fails
//! and the script goes on with the next stage.

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use super::{
    Arrange, Cast, Event, EventKind, KillPlan, KillWatch, Scenario, Script, StageResult, Status,
};
use crate::game::{weapon_anim, Hunt, HuntInput, WeaponState};

/// A screen the script opens and closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Binoculars,
    Map,
    /// The original's change-call icon (`call_icon_time`).
    CallIcon,
    /// The PC's weapon and call pickers (frontend screens, not the core's).
    WeaponPicker,
    CallPicker,
}

impl Screen {
    pub fn key(self) -> &'static str {
        match self {
            Screen::Binoculars => "binoculars",
            Screen::Map => "map",
            Screen::CallIcon => "call_icon",
            Screen::WeaponPicker => "weapon_picker",
            Screen::CallPicker => "call_picker",
        }
    }

    /// Opened and closed by the frontend rather than by hunt input.
    pub fn frontend_only(self) -> bool {
        matches!(self, Screen::WeaponPicker | Screen::CallPicker)
    }
}

/// What the director needs from a frontend.
pub trait Frontend {
    /// Whether `s` is showing; `None`: this frontend has no such screen.
    fn showing(&self, hunt: &Hunt, s: Screen) -> Option<bool>;
    /// Opens or closes a [`Screen::frontend_only`] screen.
    fn set(&mut self, s: Screen, open: bool);
}

/// No renderer: the core's screens only.
pub struct Headless;

impl Frontend for Headless {
    fn showing(&self, hunt: &Hunt, s: Screen) -> Option<bool> {
        match s {
            Screen::Binoculars => Some(hunt.bin_mode),
            Screen::Map => Some(hunt.map_mode),
            Screen::CallIcon => Some(hunt.call_icon_time > 0),
            Screen::WeaponPicker | Screen::CallPicker => None,
        }
    }

    fn set(&mut self, _: Screen, _: bool) {}
}

#[derive(Debug, Clone, Default)]
pub struct TickPlan {
    pub input: HuntInput,
    /// A change the frontend makes to the hunt before this tick
    /// ([`super::Hold::arrange`]); kill script only.
    pub arrange: Option<Arrange>,
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Begin(String, Option<Screen>),
    End,
    Wait(u32),
    Checkpoint(String),
    VerifyAnimals,
    Select(usize),
    AwaitReady(usize),
    Fire(usize),
    AwaitFireMid(usize),
    AwaitFireEnd,
    Holster,
    AwaitHidden,
    Press(Screen, bool),
    Expect(Screen, bool),
    SaveCall,
    ExpectCallChanged,
    /// Kill script: let scenario animal `n` go, wait for its kill, watch
    /// it for `n` ticks, judge it, bring the hunter back.
    Release(usize),
    AwaitKill(usize),
    Watch(u32),
    VerifyKill,
    Revive(usize),
}

/// Lower-case letters and digits, `_` between words.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

pub struct Director {
    steps: Vec<Step>,
    at: usize,
    tick: u32,
    step_tick: u32,
    timeout: u32,
    animals: usize,
    stage: Option<(String, u32, Option<String>)>,
    shots_before: u32,
    rounds_before: i32,
    call_before: i32,
    lost: bool,
    caps: Vec<String>,
    cast: Vec<Cast>,
    kills: Option<KillPlan>,
    watch: Option<KillWatch>,
    pub events: Vec<Event>,
    pub stages: Vec<StageResult>,
}

impl Director {
    /// The script for `s`, with `weapons` the registry's names in order and
    /// `cast` the placed animals ([`super::Built::cast`]).
    pub fn new(s: &Scenario, weapons: &[String], cast: &[Cast]) -> Self {
        let steps = match &s.script {
            Script::Tour => Self::tour(s, weapons),
            Script::Kills(k) => Self::kill_steps(s, k, cast),
        };
        Self {
            steps,
            at: 0,
            tick: 0,
            step_tick: 0,
            // The call icon shows 2048 ms before it closes on its own.
            timeout: s.timeout_ticks.max(2048 / super::TICK_MS as u32 + 50),
            animals: s.species.len(),
            stage: None,
            shots_before: 0,
            rounds_before: 0,
            call_before: 0,
            lost: false,
            caps: Vec::new(),
            cast: cast.to_vec(),
            kills: match &s.script {
                Script::Kills(k) => Some(k.clone()),
                Script::Tour => None,
            },
            watch: None,
            events: Vec::new(),
            stages: Vec::new(),
        }
    }

    fn opening(s: &Scenario) -> Vec<Step> {
        vec![
            Step::Begin("spawn".into(), None),
            Step::VerifyAnimals,
            Step::End,
            Step::Begin("warmup".into(), None),
            Step::Wait(s.warmup_ticks),
            Step::Checkpoint("warmup".into()),
            Step::End,
        ]
    }

    fn closing() -> [Step; 4] {
        [
            Step::Begin("animals_present".into(), None),
            Step::VerifyAnimals,
            Step::Checkpoint("end".into()),
            Step::End,
        ]
    }

    /// Each animal kills the hunter in turn.
    fn kill_steps(s: &Scenario, k: &KillPlan, cast: &[Cast]) -> Vec<Step> {
        let mut steps = Self::opening(s);
        for (n, c) in cast.iter().enumerate() {
            let stage = format!("kill_{:02}_{}", n + 1, slug(&c.name));
            steps.extend([
                Step::Begin(stage.clone(), None),
                Step::Release(n),
                Step::AwaitKill(n),
                Step::Checkpoint(format!("{stage}_start")),
                Step::Watch(k.mid_ticks),
                Step::Checkpoint(format!("{stage}_mid")),
                Step::Watch(k.late_ticks),
                Step::Checkpoint(format!("{stage}_late")),
                Step::VerifyKill,
                Step::Revive(n),
                Step::End,
            ]);
        }
        steps.extend(Self::closing());
        steps
    }

    /// Every weapon, then every screen.
    fn tour(s: &Scenario, weapons: &[String]) -> Vec<Step> {
        let mut steps = vec![];
        steps.extend(Self::opening(s));
        for (i, name) in weapons.iter().enumerate() {
            let stage = format!("weapon_{:02}_{}", i + 1, slug(name));
            steps.extend([
                Step::Begin(stage.clone(), None),
                Step::Select(i),
                Step::AwaitReady(i),
                Step::Fire(i),
                Step::AwaitFireMid(i),
                Step::Checkpoint(format!("{stage}_fire")),
                Step::AwaitFireEnd,
                Step::Holster,
                Step::AwaitHidden,
                Step::End,
            ]);
        }
        for screen in [Screen::Binoculars, Screen::Map] {
            steps.extend(Self::open_close(screen, s.settle_ticks));
        }
        steps.extend([
            Step::Begin("call_icon".into(), Some(Screen::CallIcon)),
            Step::Press(Screen::CallIcon, true),
            Step::Expect(Screen::CallIcon, true),
            Step::SaveCall,
            Step::Press(Screen::CallIcon, true),
            Step::ExpectCallChanged,
            Step::Checkpoint("call_icon_open".into()),
            Step::Expect(Screen::CallIcon, false),
            Step::End,
        ]);
        for screen in [Screen::WeaponPicker, Screen::CallPicker] {
            steps.extend(Self::open_close(screen, s.settle_ticks));
        }
        steps.extend(Self::closing());
        steps
    }

    fn open_close(screen: Screen, settle: u32) -> [Step; 8] {
        let k = screen.key();
        [
            Step::Begin(k.into(), Some(screen)),
            Step::Press(screen, true),
            Step::Expect(screen, true),
            Step::Wait(settle),
            Step::Checkpoint(format!("{k}_open")),
            Step::Press(screen, false),
            Step::Expect(screen, false),
            Step::End,
        ]
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    pub fn done(&self) -> bool {
        self.at >= self.steps.len()
    }

    /// Any stage failed (unsupported ones do not count).
    pub fn failed(&self) -> bool {
        self.stages
            .iter()
            .any(|s| matches!(s.status, Status::Failed(_)))
    }

    fn log(&mut self, kind: EventKind, name: &str, ok: Option<bool>, detail: String) {
        let stage = self.stage.as_ref().map_or("", |s| &s.0).to_string();
        self.events.push(Event {
            tick: self.tick,
            kind,
            stage,
            name: name.into(),
            ok,
            detail,
        });
    }

    fn check(&mut self, name: &str, ok: bool, detail: String) {
        self.log(EventKind::Verify, name, Some(ok), detail.clone());
        if !ok {
            self.fail(format!("{name}: {detail}"));
        }
    }

    /// Fails the current stage (the first reason is kept).
    fn fail(&mut self, reason: String) {
        if let Some((_, _, r)) = &mut self.stage {
            r.get_or_insert(reason);
        }
    }

    /// Skips to the current stage's end, or to its revive: a failed kill
    /// still brings the hunter back and pins its animal again.
    fn skip_stage(&mut self) {
        while let Some(step) = self.steps.get(self.at) {
            if matches!(step, Step::End | Step::Revive(_)) {
                break;
            }
            self.at += 1;
        }
        self.watch = None;
    }

    fn next(&mut self) {
        self.at += 1;
        self.step_tick = self.tick;
    }

    /// Steps that take no tick: stage starts and ends, checkpoints, and the
    /// frontend's own screen toggles.
    fn run_instant(&mut self, hunt: &Hunt, fe: &mut dyn Frontend) {
        while let Some(step) = self.steps.get(self.at).cloned() {
            match step {
                Step::Begin(name, screen) => {
                    self.stage = Some((name.clone(), self.tick, None));
                    self.log(EventKind::StageStart, &name, None, String::new());
                    self.next();
                    if let Some(s) = screen.filter(|&s| fe.showing(hunt, s).is_none()) {
                        let why = format!("this frontend has no {} screen", s.key());
                        self.end_stage(Status::Unsupported(why));
                        self.skip_stage();
                        self.next();
                    }
                }
                Step::End => {
                    let status = match self.stage.as_ref().and_then(|s| s.2.clone()) {
                        Some(r) => Status::Failed(r),
                        None => Status::Passed,
                    };
                    self.end_stage(status);
                    self.next();
                }
                Step::Checkpoint(name) => {
                    self.log(EventKind::Checkpoint, &name, None, String::new());
                    self.caps.push(name);
                    self.next();
                }
                Step::VerifyAnimals => {
                    let alive = hunt.herd.chars.iter().filter(|c| c.health > 0).count();
                    let ok = alive == self.animals && hunt.herd.chars.len() == self.animals;
                    let detail = format!(
                        "{alive} alive of {} placed, {} expected",
                        hunt.herd.chars.len(),
                        self.animals
                    );
                    self.check("animal_count", ok, detail);
                    self.next();
                }
                Step::SaveCall => {
                    self.call_before = hunt.target_call;
                    self.next();
                }
                Step::VerifyKill => {
                    self.verify_kill();
                    self.next();
                }
                Step::Press(s, open) if s.frontend_only() => {
                    fe.set(s, open);
                    let verb = if open { "open" } else { "close" };
                    self.log(
                        EventKind::Action,
                        &format!("{verb}_{}", s.key()),
                        None,
                        "".into(),
                    );
                    self.next();
                }
                _ => return,
            }
        }
    }

    fn end_stage(&mut self, status: Status) {
        if let Some((name, start, _)) = self.stage.take() {
            let detail = status.reason().to_string();
            self.events.push(Event {
                tick: self.tick,
                kind: EventKind::StageEnd,
                stage: name.clone(),
                name: status.key().into(),
                ok: Some(!matches!(status, Status::Failed(_))),
                detail,
            });
            self.stages.push(StageResult {
                name,
                status,
                start_tick: start,
                end_tick: self.tick,
            });
        }
    }

    /// The input for the coming tick.
    pub fn plan(&mut self, hunt: &Hunt, fe: &mut dyn Frontend) -> TickPlan {
        self.run_instant(hunt, fe);
        let mut input = HuntInput::default();
        let mut arrange = None;
        let action = match self.steps.get(self.at) {
            Some(&Step::Release(n)) => {
                arrange = Some(Arrange::Release(n));
                Some(format!("release_animal {n}"))
            }
            Some(&Step::Revive(n)) => {
                arrange = Some(Arrange::Revive(n));
                Some("revive_hunter".into())
            }
            Some(&Step::Select(i)) if hunt.weapons.ftime == 0 => {
                input.select_weapon = Some(i);
                Some(format!("select_weapon {i}"))
            }
            Some(&Step::Fire(i)) => {
                self.shots_before = hunt.shots_made;
                self.rounds_before = hunt.weapons.rounds(i);
                input.fire = true;
                Some("fire".into())
            }
            Some(Step::Holster) => {
                input.toggle_weapon = true;
                Some("hide_weapon".into())
            }
            Some(&Step::Press(s, _)) => {
                match s {
                    Screen::Binoculars => input.toggle_binoculars = true,
                    Screen::Map => input.toggle_map = true,
                    _ => input.change_call = true,
                }
                Some(format!("press_{}", s.key()))
            }
            _ => None,
        };
        if let Some(a) = action {
            self.log(EventKind::Action, &a, None, String::new());
        }
        TickPlan { input, arrange }
    }

    /// Checks the hunt after a tick; `lost` lists animals [`super::Hold`]
    /// found gone or dead. Returns the checkpoints reached.
    pub fn observe(&mut self, hunt: &Hunt, fe: &mut dyn Frontend, lost: &[usize]) -> Vec<String> {
        self.tick += 1;
        if !lost.is_empty() && !self.lost {
            self.lost = true;
            self.check("animals_kept", false, format!("lost animals {lost:?}"));
        }
        if let Some(w) = &mut self.watch {
            w.sample(hunt);
        }
        let w = &hunt.weapons;
        let done = match self.steps.get(self.at).cloned() {
            None => return core::mem::take(&mut self.caps),
            Some(Step::Wait(n) | Step::Watch(n)) => self.tick - self.step_tick >= n,
            Some(Step::Select(_) | Step::Holster | Step::Press(..)) => true,
            Some(Step::Release(_) | Step::Revive(_)) => true,
            Some(Step::AwaitKill(n)) => self.await_kill(hunt, n),
            Some(Step::AwaitReady(i)) => {
                w.current == i && w.state == WeaponState::Ready && w.ftime == 0
            }
            Some(Step::Fire(i)) => {
                let shots = hunt.shots_made - self.shots_before;
                let rounds = w.rounds(i);
                let detail = format!(
                    "shots fired {shots}, rounds {} -> {rounds}",
                    self.rounds_before
                );
                self.check("fired_once", shots == 1, detail);
                true
            }
            Some(Step::AwaitFireMid(_)) => {
                let ms = w.cur_anims().ms[weapon_anim::FIRE];
                ms <= 0 || w.ftime >= ms / 2 || w.ftime == 0
            }
            Some(Step::AwaitFireEnd) => w.state == WeaponState::Ready && w.ftime == 0,
            Some(Step::AwaitHidden) => w.state == WeaponState::Hidden,
            Some(Step::Expect(s, want)) => {
                let now = fe.showing(hunt, s);
                if now == Some(want) {
                    let name = format!("{}_{}", s.key(), if want { "open" } else { "closed" });
                    self.log(EventKind::Verify, &name, Some(true), String::new());
                }
                now == Some(want)
            }
            Some(Step::ExpectCallChanged) => {
                let n = hunt.opts.targets.iter().filter(|&a| a >= 10).count();
                let changed = hunt.target_call != self.call_before;
                let detail = format!("call {} -> {}", self.call_before, hunt.target_call);
                self.check("call_changed", changed || n < 2, detail);
                true
            }
            Some(_) => true,
        };
        // A killer may finish a sniff and roar before it charges.
        let timeout = match (&self.steps[self.at], &self.kills) {
            (Step::AwaitKill(_), Some(k)) => self.timeout.max(k.timeout_ticks),
            _ => self.timeout,
        };
        if done {
            self.next();
        } else if self.tick - self.step_tick > timeout {
            let step = format!("{:?}", self.steps[self.at]);
            self.check("timeout", false, format!("waiting for {step}"));
            self.skip_stage();
        }
        self.run_instant(hunt, fe);
        core::mem::take(&mut self.caps)
    }

    /// Whether animal `n`'s kill has happened; starts watching it.
    fn await_kill(&mut self, hunt: &Hunt, n: usize) -> bool {
        let Some(plan) = &self.kills else {
            return true;
        };
        let Some(w) = KillWatch::start(hunt, self.tick, plan) else {
            return hunt.death.is_some();
        };
        let want = self.cast.get(n).map(|c| c.index);
        let body = hunt.herd.chars.get(w.body).map_or(0, |b| b.phase);
        let detail = format!(
            "character {} (expected {want:?}), body clip {body}, after {} ticks",
            w.killer,
            self.tick - self.step_tick
        );
        self.check("killed_by", want == Some(w.killer), detail);
        self.watch = Some(w);
        true
    }

    /// Judges the watched kill.
    fn verify_kill(&mut self) {
        let (Some(w), Some(plan)) = (self.watch.take(), self.kills.clone()) else {
            self.check("kill_watched", false, "no kill to judge".into());
            return;
        };
        for (name, ok, detail) in w.verdicts(&plan) {
            self.check(name, ok, detail);
        }
    }

    /// Ends the run early (the whole-run tick ceiling): the open stage fails.
    pub fn abort(&mut self, reason: &str) {
        if self.stage.is_some() {
            self.check("aborted", false, reason.into());
            self.end_stage(Status::Failed(reason.into()));
        }
        self.at = self.steps.len();
    }
}
