//! The Advanced DHC OS UI's hunt preparation (PC and Android; spec
//! `ADVANCEDDHCOS.md`): its stages and each stage's carousel, over the
//! terminal's own setup state. The picks, the committed map, the rules and
//! the refusals stay the [`Terminal`]'s: every pick goes through its
//! actions, so the advanced UI starts exactly the hunts the terminal does.
//! This adds only what the presentation needs: which stage shows, and
//! where each carousel has scrolled to. A stage's highlighted item is the
//! terminal's focused row of that list ([`Terminal::rows`]), so going back
//! to a stage finds its highlight and scroll where they were.
//!
//! No rendering, no I/O, no randomness: previewing never touches the hunt.

use super::beep::Beep;
use super::terminal::{Action, Screen, Terminal};
use super::Tab;

/// How fast a carousel catches up with its highlight (per second): about
/// a quarter of a second to settle.
const EASE: f32 = 14.0;
/// Closer than this (in items), a carousel snaps onto its highlight.
const SNAP: f32 = 0.002;

/// The preparation's stages, in order. Worlds (map groups) come first
/// when an instance defines them; they are not built yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Map,
    Dinos,
    Guns,
    Gear,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Map, Stage::Dinos, Stage::Guns, Stage::Gear];

    /// The terminal list the stage picks from.
    pub fn tab(self) -> Tab {
        match self {
            Stage::Map => Tab::Area,
            Stage::Dinos => Tab::Targets,
            Stage::Guns => Tab::Arms,
            Stage::Gear => Tab::Gear,
        }
    }

    /// The stage that picks from list `tab`.
    pub fn of(tab: Tab) -> Stage {
        match tab {
            Tab::Area => Stage::Map,
            Tab::Targets => Stage::Dinos,
            Tab::Arms => Stage::Guns,
            Tab::Gear => Stage::Gear,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn next(self) -> Option<Stage> {
        Stage::ALL.get(self.index() + 1).copied()
    }

    pub fn prev(self) -> Option<Stage> {
        self.index().checked_sub(1).map(|i| Stage::ALL[i])
    }
}

/// What the player asks of the preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepAction {
    /// The carousel's previous / next item (it wraps round).
    Prev,
    Next,
    /// Highlight item `i` (a click on a neighbour).
    Highlight(usize),
    /// Enter, the pad's R, a click on the centre or the action button: on
    /// the map stage, use the highlighted map; on the others, go on to the
    /// next stage.
    Commit,
    /// Space, the pad's A: add the highlighted item to the hunt, or take
    /// it out again. On the map stage it commits.
    Add,
    /// Take item `i` of list `tab` out of the hunt (the loadout record).
    Remove(Tab, usize),
    /// The previous stage; from the first, the terminal's main menu.
    Back,
}

/// The preparation's own state: the stage shown and each carousel's
/// scroll, in items (fractional while it moves).
#[derive(Debug, Clone, PartialEq)]
pub struct Prep {
    pub stage: Stage,
    scroll: [f32; 4],
}

impl Default for Prep {
    fn default() -> Self {
        Prep {
            stage: Stage::Map,
            scroll: [0.0; 4],
        }
    }
}

/// The way from `from` to `to` round a ring of `n` items, the short way
/// (ties go forwards): in `(-n/2, n/2]`.
pub fn ring_delta(from: f32, to: f32, n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f32;
    let mut d = libm::fmodf(to - from, n);
    if d > n / 2.0 {
        d -= n;
    } else if d <= -n / 2.0 {
        d += n;
    }
    d
}

/// The share of the starting balance `score` still left with `remaining`
/// points, 0..=1: what colours the points remaining (spec §8), so spending
/// 18,000 of 20,000 warns harder than spending 20 of 100. An account of
/// nothing counts as whole until it goes below zero.
pub fn points_left(score: i32, remaining: i32) -> f32 {
    if score > 0 {
        (remaining as f32 / score as f32).clamp(0.0, 1.0)
    } else if remaining >= 0 {
        1.0
    } else {
        0.0
    }
}

impl Prep {
    /// The preparation opening on `term`'s setup: the map stage, its
    /// highlight on the committed map, every carousel at rest on its
    /// highlight.
    pub fn enter(&mut self, term: &mut Terminal) {
        self.stage = Stage::Map;
        term.rows[Tab::Area.index()] = term.area;
        if term.screen == Screen::Setup {
            term.act(Action::Tab(Tab::Area));
        }
        for s in Stage::ALL {
            self.scroll[s.index()] = term.rows[s.tab().index()] as f32;
        }
    }

    /// The highlighted item of `stage`.
    pub fn highlight(&self, term: &Terminal, stage: Stage) -> usize {
        term.rows[stage.tab().index()]
    }

    /// Where `stage`'s carousel has scrolled to, in items.
    pub fn scroll(&self, stage: Stage) -> f32 {
        self.scroll[stage.index()]
    }

    /// How far item `i` of `stage`'s ring of `n` sits from the carousel's
    /// centre, in items (negative: to the left).
    pub fn offset(&self, stage: Stage, n: usize, i: usize) -> f32 {
        ring_delta(self.scroll(stage), i as f32, n)
    }

    /// Whether `stage`'s carousel is at rest on its highlight.
    pub fn settled(&self, term: &Terminal, stage: Stage) -> bool {
        let n = term.cat.list(stage.tab()).len();
        let h = self.highlight(term, stage) as f32;
        ring_delta(self.scroll(stage), h, n).abs() < SNAP
    }

    /// Moves the carousels on by `dt` seconds towards their highlights.
    pub fn step(&mut self, term: &Terminal, dt: f32) {
        let k = 1.0 - libm::expf(-EASE * dt.max(0.0));
        for s in Stage::ALL {
            let n = term.cat.list(s.tab()).len();
            let h = self.highlight(term, s) as f32;
            let pos = &mut self.scroll[s.index()];
            let d = ring_delta(*pos, h, n);
            *pos = if d.abs() < SNAP {
                *pos + d
            } else {
                *pos + d * k
            };
            if n > 0 {
                let r = libm::fmodf(*pos, n as f32);
                *pos = if r < 0.0 { r + n as f32 } else { r };
            }
        }
    }

    /// Follows the terminal's list when something else changed it (the
    /// terminal's own setup screen, standing in for a stage not built yet).
    pub fn follow(&mut self, term: &Terminal) {
        self.stage = Stage::of(term.tab);
    }

    /// Does `a`; the beep it makes, as the terminal's (none: nothing
    /// changed).
    pub fn act(&mut self, term: &mut Terminal, a: PrepAction) -> Option<Beep> {
        if term.screen != Screen::Setup {
            return None;
        }
        let tab = self.stage.tab();
        let n = term.cat.list(tab).len();
        let h = self.highlight(term, self.stage);
        match a {
            PrepAction::Prev | PrepAction::Next if n > 1 => {
                let to = if a == PrepAction::Next {
                    (h + 1) % n
                } else {
                    (h + n - 1) % n
                };
                term.rows[tab.index()] = to;
                Some(Beep::Tick)
            }
            PrepAction::Highlight(i) if i < n && i != h => {
                term.rows[tab.index()] = i;
                Some(Beep::Tick)
            }
            PrepAction::Commit | PrepAction::Add if self.stage == Stage::Map => {
                if n == 0 {
                    return None;
                }
                self.pick(term, tab, h).or_else(|| {
                    (term.area == h).then(|| {
                        self.go(term, Stage::Dinos);
                        Beep::Select
                    })
                })
            }
            PrepAction::Commit => {
                let next = self.stage.next()?;
                self.go(term, next);
                Some(Beep::Select)
            }
            PrepAction::Add if h < n => self.pick(term, tab, h).or(Some(Beep::Select)),
            PrepAction::Remove(t, i) if t != Tab::Area && term.picked(t, i) => {
                self.pick(term, t, i).or(Some(Beep::Back))
            }
            PrepAction::Back => match self.stage.prev() {
                Some(s) => {
                    self.go(term, s);
                    Some(Beep::Back)
                }
                None => term.act_beep(Action::Back),
            },
            _ => None,
        }
    }

    /// Picks (or drops) item `i` of `tab` through the terminal, keeping the
    /// terminal on the stage's list. The refusal beep, if refused.
    fn pick(&mut self, term: &mut Terminal, tab: Tab, i: usize) -> Option<Beep> {
        let (row, list) = (term.rows[tab.index()], term.tab);
        term.act(Action::Tab(tab));
        let beep = term.act_beep(Action::Pick(i));
        term.rows[tab.index()] = row;
        term.act(Action::Tab(list));
        beep.filter(|b| *b == Beep::Refuse)
    }

    fn go(&mut self, term: &mut Terminal, s: Stage) {
        self.stage = s;
        term.act(Action::Tab(s.tab()));
    }
}
