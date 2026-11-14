//! Predator hesitation (the hardcore hunt ruleset; not in the original): a
//! carnivore that reaches the hunter circles him instead of attacking at
//! once. It attacks when his back is turned to it or when its time is up;
//! a second predator arriving inside the encounter window attacks at once
//! if it is expensive enough. Design: `docs/HARDCORE-HUNT.md`.
//!
//! No AI is changed: while an animal circles, [`Herd::step`] hands its AI a
//! point on a circle round the hunter as a waypoint (chased, never struck),
//! and the hunter again once it attacks.
//!
//! [`Herd::step`]: super::Herd::step

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use c2_formats::hardcore::Reaction;

use super::{Character, HunterView, Species, Target, TargetKind};

/// A circling animal runs at this share of its speed, a lunging one at
/// this multiple.
const CIRCLE_RUN_K: f32 = 0.55;
const LUNGE_RUN_K: f32 = 1.5;
/// How far round the circle ahead of the animal its waypoint is, radians.
const LEAD: f32 = 0.7;

impl HunterView {
    /// Whether the hunter's back is turned to world (`x`, `z`): the angle
    /// between his view direction and the direction there is more than the
    /// one whose cosine is `cos_limit` (0: the forward half).
    pub fn back_turned_to(&self, x: f32, z: f32, cos_limit: f32) -> bool {
        let (dx, dz) = (x - self.x, z - self.z);
        let l = (dx * dx + dz * dz).sqrt();
        if l < 1.0 {
            return false;
        }
        // Forward is (sin a, -cos a), as the hunter moves.
        let (s, c) = self.alpha.sin_cos();
        (dx * s - dz * c) / l < cos_limit
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Circling,
    /// It goes for the hunter (`lunge`: faster than it runs).
    Attacking {
        lunge: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Encounter {
    index: usize,
    /// Ms since it began.
    ms: i32,
    /// 1 or -1: which way round it circles.
    dir: f32,
    phase: Phase,
}

/// What the herd's step does for one character.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Steer {
    /// The target in place of the hunter (`None`: the hunter).
    pub target: Option<Target>,
    /// Multiplies its running speed.
    pub run_k: f32,
}

/// The rule's settings and the encounters under way.
#[derive(Debug, Clone, Default)]
pub struct Hesitation {
    /// Ms an animal circles before it attacks anyway.
    pub circle_ms: i32,
    /// Cosine of the angle past which the hunter's back is turned.
    pub back_cos: f32,
    /// The second-predator rule: the window (ms) and the price above which
    /// a later arrival attacks at once.
    pub second: Option<(i32, i32)>,
    /// Per species: how it reacts.
    pub reactions: Vec<Reaction>,
    /// No strike lands (the knife's dodge): every animal's target is a
    /// waypoint where the hunter stands.
    pub hunter_safe: bool,
    encounters: Vec<Encounter>,
    /// Ms left of the window the latest first encounter opened.
    window_ms: i32,
}

/// The circle an animal keeps to, by its size.
fn orbit(sp: &Species, c: &Character) -> f32 {
    (1024.0 + 1.2 * sp.size * c.scale.max(0.1)).clamp(1400.0, 3600.0)
}

impl Hesitation {
    /// Whether animal `index` is circling the hunter.
    pub fn circling(&self, index: usize) -> bool {
        self.encounters
            .iter()
            .any(|e| e.index == index && e.phase == Phase::Circling)
    }

    /// Whether any encounter is under way (an animal circling or attacking
    /// after it).
    pub fn engaged(&self) -> bool {
        !self.encounters.is_empty()
    }

    /// Character `index` was removed: renumber what refers past it.
    pub fn forget(&mut self, index: usize) {
        self.encounters.retain(|e| e.index != index);
        for e in &mut self.encounters {
            if e.index > index {
                e.index -= 1;
            }
        }
    }

    /// Once a step, before the characters: the window runs down, and
    /// encounters of animals that died or are gone end.
    pub fn begin(&mut self, chars: &[Character], dt: i32) {
        self.window_ms = (self.window_ms - dt).max(0);
        self.encounters
            .retain(|e| chars.get(e.index).is_some_and(|c| c.health != 0));
    }

    /// Animal `index`'s step: starts, advances or ends its encounter.
    /// `free`: its AI is after the hunter this step (predation has not
    /// sent it elsewhere).
    pub fn steer(
        &mut self,
        index: usize,
        c: &Character,
        sp: &Species,
        hunter: HunterView,
        dt: i32,
        free: bool,
    ) -> Steer {
        let mut out = Steer {
            target: None,
            run_k: 1.0,
        };
        let safe = Target {
            view: hunter,
            kind: TargetKind::Waypoint,
            ..Target::hunter(hunter, c)
        };
        let reaction = self
            .reactions
            .get(c.ctype)
            .copied()
            .unwrap_or(Reaction::Ignore);
        let (dx, dz) = (c.pos[0] - hunter.x, c.pos[2] - hunter.z);
        let dist = (dx * dx + dz * dz).sqrt();
        let r = orbit(sp, c);
        let at = self.encounters.iter().position(|e| e.index == index);
        let hunting = free
            && c.health != 0
            && c.state != 0
            && hunter.alive
            && !hunter.observer
            && reaction != Reaction::Ignore;
        let Some(at) = at else {
            if hunting && dist < r * 1.25 {
                // The second-predator rule: inside the window of an earlier
                // encounter, an expensive animal does not wait.
                let rush = self.second.is_some_and(|(_, min)| {
                    (self.engaged() || self.window_ms > 0) && sp.hunt_price.unwrap_or(0) > min
                });
                if !self.engaged() && self.window_ms == 0 {
                    self.window_ms = self.second.map_or(0, |(w, _)| w);
                }
                self.encounters.push(Encounter {
                    index,
                    ms: 0,
                    // Which side it came in on decides the way round.
                    dir: if (index + c.ctype).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    },
                    phase: if rush {
                        Phase::Attacking { lunge: false }
                    } else {
                        Phase::Circling
                    },
                });
                return self.steer(index, c, sp, hunter, 0, free);
            }
            if self.hunter_safe {
                out.target = Some(safe);
            }
            return out;
        };
        // It lost interest, was sent elsewhere or fell far behind: over.
        if !hunting || dist > r * 4.0 {
            self.encounters.swap_remove(at);
            if self.hunter_safe {
                out.target = Some(safe);
            }
            return out;
        }
        let e = &mut self.encounters[at];
        e.ms += dt;
        if e.phase == Phase::Circling {
            if hunter.back_turned_to(c.pos[0], c.pos[2], self.back_cos) {
                e.phase = Phase::Attacking {
                    lunge: reaction == Reaction::CircleLunge,
                };
            } else if e.ms >= self.circle_ms {
                e.phase = Phase::Attacking { lunge: false };
            }
        }
        match e.phase {
            Phase::Circling => {
                let a = dz.atan2(dx) + e.dir * LEAD;
                out.target = Some(Target {
                    view: HunterView {
                        x: hunter.x + a.cos() * r,
                        z: hunter.z + a.sin() * r,
                        ..hunter
                    },
                    ..safe
                });
                out.run_k = CIRCLE_RUN_K;
            }
            Phase::Attacking { lunge } => {
                if lunge {
                    out.run_k = LUNGE_RUN_K;
                }
                if self.hunter_safe {
                    out.target = Some(safe);
                }
            }
        }
        out
    }

    /// A strike missed (the dodge): the attackers circle again, the other
    /// way round, for half their time.
    pub fn dodged(&mut self) {
        let half = self.circle_ms / 2;
        for e in &mut self.encounters {
            if e.phase != Phase::Circling {
                e.phase = Phase::Circling;
                e.ms = half;
                e.dir = -e.dir;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dino::PI;

    fn species(price: i32, size: f32) -> Species {
        let mut sp = crate::dino::tests::species(crate::dino::ai::VELO, 8);
        sp.hunt_price = Some(price);
        sp.size = size;
        sp
    }

    fn rule(second: Option<(i32, i32)>) -> Hesitation {
        Hesitation {
            circle_ms: 10_000,
            back_cos: 0.0,
            second,
            reactions: alloc::vec![Reaction::CircleLunge, Reaction::CircleEat, Reaction::Ignore],
            ..Hesitation::default()
        }
    }

    /// The hunter at the origin looking -z; an animal `d` ahead of him
    /// (`ahead`) or behind, hunting.
    fn scene(ctype: usize, d: f32, ahead: bool) -> (HunterView, Character) {
        let hunter = HunterView {
            alive: true,
            ..HunterView::default()
        };
        let c = Character {
            ctype,
            state: 1,
            health: 5,
            scale: 1.0,
            pos: [0.0, 0.0, if ahead { -d } else { d }],
            ..Character::default()
        };
        (hunter, c)
    }

    #[test]
    fn the_back_is_the_half_behind_the_view_direction() {
        let h = HunterView::default();
        assert!(!h.back_turned_to(0.0, -500.0, 0.0), "straight ahead");
        assert!(
            !h.back_turned_to(500.0, -10.0, 0.0),
            "just ahead of the side"
        );
        assert!(h.back_turned_to(500.0, 10.0, 0.0), "just behind the side");
        assert!(h.back_turned_to(0.0, 500.0, 0.0), "straight behind");
        let turned = HunterView { alpha: PI, ..h };
        assert!(turned.back_turned_to(0.0, -500.0, 0.0));
        // A narrower back arc (120 degrees): the side is not behind.
        assert!(!h.back_turned_to(500.0, 10.0, (120.0f32).to_radians().cos()));
    }

    #[test]
    fn it_circles_while_faced_and_attacks_when_the_time_is_up() {
        let sp = species(100, 300.0);
        let mut h = rule(None);
        let (hunter, c) = scene(0, 1500.0, true);
        let s = h.steer(0, &c, &sp, hunter, 100, true);
        let t = s.target.unwrap_or_default();
        assert_eq!(t.kind, TargetKind::Waypoint, "a waypoint is never struck");
        assert!(!t.strikes() && h.circling(0));
        assert!(s.run_k < 1.0);
        let r = (t.view.x.powi(2) + t.view.z.powi(2)).sqrt();
        assert!((r - orbit(&sp, &c)).abs() < 1.0, "on the circle: {r}");
        // Faced the whole time: the waypoint, until the time is up.
        for _ in 0..99 {
            assert!(h.steer(0, &c, &sp, hunter, 100, true).target.is_some());
        }
        let s = h.steer(0, &c, &sp, hunter, 100, true);
        assert_eq!(
            s,
            Steer {
                target: None,
                run_k: 1.0
            },
            "the hunter, at its own speed"
        );
        assert!(!h.circling(0) && h.engaged());
    }

    #[test]
    fn a_turned_back_ends_the_circling_by_reaction() {
        let sp = species(100, 300.0);
        for (ctype, run_k) in [(0, LUNGE_RUN_K), (1, 1.0)] {
            let mut h = rule(None);
            let (hunter, c) = scene(ctype, 1500.0, true);
            assert!(h.steer(0, &c, &sp, hunter, 100, true).target.is_some());
            let (_, behind) = scene(ctype, 1500.0, false);
            let s = h.steer(0, &behind, &sp, hunter, 100, true);
            assert_eq!(
                s,
                Steer {
                    target: None,
                    run_k
                },
                "reaction {ctype}"
            );
        }
    }

    #[test]
    fn ignore_far_calm_and_steered_animals_are_left_alone() {
        let sp = species(100, 300.0);
        let mut h = rule(None);
        let none = Steer {
            target: None,
            run_k: 1.0,
        };
        let (hunter, c) = scene(2, 1500.0, true);
        assert_eq!(h.steer(0, &c, &sp, hunter, 100, true), none, "ignore");
        let (_, far) = scene(0, 9000.0, true);
        assert_eq!(h.steer(0, &far, &sp, hunter, 100, true), none, "far");
        let (_, mut calm) = scene(0, 1500.0, true);
        calm.state = 0;
        assert_eq!(h.steer(0, &calm, &sp, hunter, 100, true), none, "calm");
        let (_, c) = scene(0, 1500.0, true);
        assert_eq!(h.steer(0, &c, &sp, hunter, 100, false), none, "steered");
        let observer = HunterView {
            observer: true,
            ..hunter
        };
        assert_eq!(h.steer(0, &c, &sp, observer, 100, true), none, "observer");
        assert!(!h.engaged());
    }

    #[test]
    fn an_encounter_ends_when_the_animal_loses_interest_or_dies() {
        let sp = species(100, 300.0);
        let mut h = rule(None);
        let (hunter, mut c) = scene(0, 1500.0, true);
        h.steer(0, &c, &sp, hunter, 100, true);
        c.state = 0;
        h.steer(0, &c, &sp, hunter, 100, true);
        assert!(!h.engaged());
        c.state = 1;
        h.steer(0, &c, &sp, hunter, 100, true);
        c.health = 0;
        h.begin(&[c], 100);
        assert!(!h.engaged());
    }

    #[test]
    fn a_second_predator_attacks_at_once_only_above_the_price() {
        let (cheap, dear) = (species(300, 300.0), species(500, 300.0));
        let (hunter, c) = scene(0, 1500.0, true);
        let none = Steer {
            target: None,
            run_k: 1.0,
        };
        // The first, however dear, hesitates.
        let mut h = rule(Some((30_000, 300)));
        assert!(h.steer(0, &c, &dear, hunter, 100, true).target.is_some());
        // The second: at the price it hesitates, above it it attacks.
        assert!(h.steer(1, &c, &cheap, hunter, 100, true).target.is_some());
        assert_eq!(h.steer(2, &c, &dear, hunter, 100, true), none);
        // Faced, with the back turned to neither: no kill from the cheap one.
        assert!(h.circling(1) && !h.circling(2));
    }

    #[test]
    fn the_window_outlasts_the_first_encounter_and_then_closes() {
        let dear = species(500, 300.0);
        let (hunter, mut c) = scene(0, 1500.0, true);
        let mut h = rule(Some((30_000, 300)));
        h.steer(0, &c, &dear, hunter, 100, true);
        c.health = 0;
        h.begin(core::slice::from_ref(&c), 10_000);
        assert!(!h.engaged());
        c.health = 5;
        // 10 s in: still inside the window.
        assert!(h.steer(1, &c, &dear, hunter, 100, true).target.is_none());
        // Past it, an arrival is a first predator again.
        let mut h = rule(Some((30_000, 300)));
        h.steer(0, &c, &dear, hunter, 100, true);
        h.forget(0);
        h.begin(&[], 30_000);
        assert!(h.steer(1, &c, &dear, hunter, 100, true).target.is_some());
        // Without the rule, nothing rushes.
        let mut h = rule(None);
        h.steer(0, &c, &dear, hunter, 100, true);
        assert!(h.steer(1, &c, &dear, hunter, 100, true).target.is_some());
    }

    #[test]
    fn the_dodge_spares_the_hunter_and_sends_attackers_round_again() {
        let sp = species(100, 300.0);
        let mut h = rule(None);
        let (hunter, c) = scene(0, 1500.0, true);
        let (_, behind) = scene(0, 1500.0, false);
        h.steer(0, &c, &sp, hunter, 100, true);
        assert!(h.steer(0, &behind, &sp, hunter, 100, true).target.is_none());
        h.hunter_safe = true;
        let s = h.steer(0, &behind, &sp, hunter, 100, true);
        let t = s.target.unwrap_or_default();
        assert!(!t.strikes(), "no strike lands");
        assert_eq!((t.view.x, t.view.z), (hunter.x, hunter.z));
        // An animal with no encounter is no danger either.
        let (_, other) = scene(2, 300.0, true);
        assert!(h.steer(5, &other, &sp, hunter, 100, true).target.is_some());
        h.dodged();
        h.hunter_safe = false;
        assert!(h.circling(0));
        assert!(h.steer(0, &c, &sp, hunter, 100, true).target.is_some());
    }

    #[test]
    fn removed_characters_renumber_the_encounters() {
        let sp = species(100, 300.0);
        let mut h = rule(None);
        let (hunter, c) = scene(0, 1500.0, true);
        h.steer(1, &c, &sp, hunter, 100, true);
        h.steer(4, &c, &sp, hunter, 100, true);
        h.forget(1);
        assert!(!h.circling(1) && !h.circling(4) && h.circling(3));
    }
}
