//! Awareness (an instance setting, `_BEHAVIOR.TXT`; not in the original):
//! what an animal knows of the hunter, as explicit states.
//!
//! The original has two: calm, or alarmed and knowing exactly where the
//! hunter is for as long as the alarm lasts. With awareness on:
//!
//! - **Suspicious:** a calm animal whose senses come close to the alarm
//!   threshold without reaching it half-notices the hunter.
//! - **Investigating:** suspicious for long enough, it walks over to where
//!   it noticed him (a herbivore stops well short). Its AI stays calm; only
//!   its wander target is set.
//! - **Pursuing:** alarmed, as the original. A predator chases the hunter
//!   while it senses him and the place it last sensed him once it does not.
//!   Stealth crouch no longer counts: the senses of an alarmed predator are
//!   tested as if the hunter stood.
//! - **Searching:** at that place, or after [`LOSE_MS`] without sensing him,
//!   a predator works round it in widening steps until it senses him again
//!   or its search time is up, and then calms.
//!
//! No AI is changed: [`Hunt::check_afraid`] feeds [`Rules::sense`] the
//! original's detection value, and [`Herd::step`] hands a predator that has
//! lost the hunter the place to go as a waypoint (chased, never struck).
//! Nothing here draws from the random stream. Design: `docs/BEHAVIOR.md`.
//!
//! [`Hunt::check_afraid`]: crate::game::Hunt
//! [`Herd::step`]: super::Herd::step

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

use c2_formats::behavior as fmt;

use super::{ai, mee, Character, HunterView, Species, Target, TargetKind};
use crate::collide::{WALK_MAX, WALK_MIN};
use crate::CELL;

/// A predator that has not sensed the hunter for this long stops heading
/// for where it last did and searches, ms.
pub const LOSE_MS: i32 = 8000;
/// "There": within this of a place, units.
const REACHED: f32 = 2.0 * CELL;
/// A herbivore that goes to look stops this far short, units.
const KEEP: f32 = 12.0 * CELL;
/// A search moves on to its next point this often, ms.
const SEARCH_STEP_MS: i32 = 3000;
/// A search's first ring and its widest, cells.
const SEARCH_RINGS: (i32, i32) = (3, 10);
/// The turn between one search point and the next, radians (the golden
/// angle: successive points never line up).
const SEARCH_TURN: f32 = 2.4;

/// What an animal knows of the hunter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Unaware,
    Suspicious,
    Investigating,
    /// Alarmed: chasing, charging or fleeing, as its AI decides.
    Pursuing,
    Searching,
}

/// One character's awareness. Idle (`Unaware`, zeros) when the setting is
/// off.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Aware {
    pub level: Level,
    /// Where it last noticed or sensed the hunter (x, z).
    pub at: (f32, f32),
    /// Ms in this level.
    pub ms: i32,
    /// Pursuing: ms since it last sensed him.
    pub lost_ms: i32,
}

impl Aware {
    fn enter(&mut self, level: Level) {
        self.level = level;
        self.ms = 0;
        self.lost_ms = 0;
    }

    /// It heard something at (`x`, `z`) that alarmed it (a shot): that is
    /// where it now takes the hunter to be.
    pub fn heard(&mut self, x: f32, z: f32) {
        self.enter(Level::Pursuing);
        self.at = (x, z);
    }
}

/// One species' values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// It chases the hunter (a carnivore), so it can lose and search for
    /// him; the others only look.
    pub predator: bool,
    /// Detection values below this (the alarm threshold is 1) are noticed.
    pub suspicion: f32,
    pub investigate_ms: i32,
    pub search_ms: i32,
}

/// Every species' values (index = `Character::ctype`; `None`: it takes no
/// part and behaves as the original).
#[derive(Debug, Clone, Default)]
pub struct Rules {
    pub per_species: Vec<Option<Entry>>,
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Whether a species takes part, and as a predator: the big land animals
/// do. Ambient animals, fish, flyers and brachiosaurs keep their AIs.
fn part(sp: &Species) -> Option<bool> {
    let a = sp.runs_as();
    match sp.mee.as_ref() {
        Some(_) if a == ai::TREX => Some(true),
        Some(_) if mee::is_huntable(a) => Some(mee::ai_info(a).carnivore),
        Some(_) => None,
        None => match a {
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX => Some(true),
            ai::PARA | ai::ANKY | ai::STEGO | ai::CHASM => Some(false),
            _ => None,
        },
    }
}

impl Rules {
    /// The rules from the settings file's bytes. `None` when awareness is
    /// off in it.
    pub fn load(bytes: &[u8], species: &[Species]) -> Option<Box<Self>> {
        let file = fmt::Behavior::parse(bytes);
        if !file.awareness {
            return None;
        }
        let per_species = species
            .iter()
            .map(|sp| {
                let [suspicion, look, search] = file.for_animal(&sp.info.name);
                part(sp).map(|predator| Entry {
                    predator,
                    suspicion: suspicion as f32 / 100.0,
                    investigate_ms: look as i32 * 1000,
                    search_ms: search as i32 * 1000,
                })
            })
            .collect();
        Some(Box::new(Self { per_species }))
    }

    pub fn entry(&self, ctype: usize) -> Option<Entry> {
        self.per_species.get(ctype).copied().flatten()
    }

    /// Whether [`Self::sense`] wants an alarmed animal of `ctype` tested
    /// again, as if the hunter stood (a predator's pursuit).
    pub fn tracks(&self, ctype: usize) -> bool {
        self.entry(ctype).is_some_and(|e| e.predator)
    }

    /// One check of `c`'s senses. `k` is the original's detection value
    /// (below 1: it senses the hunter; `None`: he is out of range), tested
    /// with the stealth crouch for a calm animal and without for an alarmed
    /// predator. `hunter` is where he is, `dt` the ms since the last check.
    pub fn sense(&self, c: &mut Character, k: Option<f32>, hunter: (f32, f32), dt: i32) {
        let Some(e) = self.entry(c.ctype) else {
            return;
        };
        // The dead know nothing; a survival wave's animal always knows.
        if c.health == 0 || c.wave > 0 {
            c.aware = Aware::default();
            return;
        }
        if c.state == 0 {
            calm(c, e, k, hunter, dt);
            return;
        }
        let here = (c.pos[0], c.pos[2]);
        let a = &mut c.aware;
        // Alarmed, by this check or by a shot, a call or its pack.
        if !matches!(a.level, Level::Pursuing | Level::Searching) {
            a.enter(Level::Pursuing);
            a.at = hunter;
        }
        if !e.predator {
            a.at = hunter;
            return;
        }
        if k.is_some_and(|k| k < 1.0) {
            a.enter(Level::Pursuing);
            a.at = hunter;
            return;
        }
        if a.level == Level::Pursuing {
            a.lost_ms += dt;
            if a.lost_ms >= LOSE_MS || dist(here, a.at) < REACHED {
                a.enter(Level::Searching);
            }
        } else {
            a.ms += dt;
            if a.ms >= e.search_ms {
                // Given up: calm, by the timer and nothing else.
                c.state = 0;
                c.afraid_time = 0;
                c.aware_hunter = false;
                c.heard_shot = false;
                c.aware = Aware::default();
            }
        }
    }
}

/// A calm animal: half-noticing, then going to look.
fn calm(c: &mut Character, e: Entry, k: Option<f32>, hunter: (f32, f32), dt: i32) {
    let here = (c.pos[0], c.pos[2]);
    let a = &mut c.aware;
    let half = k.is_some_and(|k| (1.0..e.suspicion).contains(&k));
    match a.level {
        // Its AI calmed it (the hunter died, it lost interest).
        Level::Pursuing | Level::Searching => *a = Aware::default(),
        Level::Unaware if half => {
            a.enter(Level::Suspicious);
            a.at = hunter;
        }
        Level::Unaware => {}
        Level::Suspicious if half => {
            a.ms += dt;
            a.at = hunter;
            if a.ms >= e.investigate_ms {
                a.enter(Level::Investigating);
            }
        }
        Level::Suspicious => *a = Aware::default(),
        Level::Investigating => {
            a.ms += dt;
            let away = dist(here, a.at);
            // A predator goes all the way; the others stop short.
            let to = if e.predator || away < 1.0 {
                a.at
            } else {
                let k = KEEP.min(away) / away;
                (
                    a.at.0 + (here.0 - a.at.0) * k,
                    a.at.1 + (here.1 - a.at.1) * k,
                )
            };
            if a.ms >= e.search_ms || dist(here, to) < REACHED {
                *a = Aware::default();
            } else {
                // The calm AI walks to its wander target.
                (c.tgx, c.tgz) = to;
                c.tgtime = 0;
            }
        }
    }
}

/// Where a predator that does not sense the hunter heads: the place it
/// last did, or the current point of its search round it. `None`: it
/// senses him (or takes no part), and its AI has the hunter himself.
pub fn seek(c: &Character, index: usize) -> Option<(f32, f32)> {
    let a = &c.aware;
    match a.level {
        Level::Pursuing if a.lost_ms > 0 => Some(a.at),
        Level::Searching => {
            let n = a.ms / SEARCH_STEP_MS;
            let ring = (SEARCH_RINGS.0 + n).min(SEARCH_RINGS.1) as f32 * CELL;
            let turn = (index as f32 + n as f32) * SEARCH_TURN;
            Some((
                (a.at.0 + turn.cos() * ring).clamp(WALK_MIN, WALK_MAX),
                (a.at.1 + turn.sin() * ring).clamp(WALK_MIN, WALK_MAX),
            ))
        }
        _ => None,
    }
}

/// [`seek`] as an AI target: a waypoint the animal still takes for the
/// hunter (it runs there and keeps its pack alarmed) but cannot strike.
pub fn target(c: &Character, index: usize, hunter: HunterView) -> Option<Target> {
    let (x, z) = seek(c, index)?;
    Some(Target {
        view: HunterView { x, z, ..hunter },
        kind: TargetKind::Waypoint,
        aware: true,
        heard_shot: c.heard_shot,
    })
}
