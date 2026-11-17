//! Advanced binoculars: who is outlined, in which colour, and the score.

use super::support::setup;
use crate::area::Area;
use crate::dino::{ai, predation::Goal, CS_ON_WATER};
use crate::game::{Hunt, Outline, ADVANCED_BINOCULARS_SCORE};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// The target turned into species `ai_id`, binoculars carried and up.
fn looking_at(ai_id: i32) -> Hunt {
    let (_, mut h, _) = setup(false);
    h.herd.species[1].info.ai = ai_id;
    h.herd.chars[0].ai = ai_id;
    h.opts.advanced_binoculars = true;
    h.bin_mode = true;
    h
}

#[test]
fn outlines_need_the_gear_and_the_binoculars_up() {
    let mut h = looking_at(ai::ALLO);
    assert_eq!(h.binocular_outline(0), Some(Outline::Calm));
    h.bin_mode = false;
    assert_eq!(h.binocular_outline(0), None, "binoculars down");
    h.bin_mode = true;
    h.opts.advanced_binoculars = false;
    assert_eq!(h.binocular_outline(0), None, "plain binoculars");
    assert_eq!(h.binocular_outline(9), None, "no such animal");
}

#[test]
fn bodies_and_ambients_are_not_outlined() {
    let mut h = looking_at(ai::ALLO);
    h.herd.chars[0].health = 0;
    assert_eq!(h.binocular_outline(0), None, "a body");
    let mut h = looking_at(ai::MOSH);
    h.herd.chars[0].state = 1;
    assert_eq!(h.binocular_outline(0), None, "an ambient");
}

#[test]
fn a_carnivore_turns_red_once_it_is_after_the_hunter() {
    for kind in [ai::ALLO, ai::VELO, ai::TREX, ai::CHASM] {
        let mut h = looking_at(kind);
        h.herd.chars[0].state = 0;
        assert_eq!(h.binocular_outline(0), Some(Outline::Calm), "{kind} calm");
        // Wading alone (the original's on-water flag) is not hunting.
        h.herd.chars[0].state = CS_ON_WATER;
        assert_eq!(h.binocular_outline(0), Some(Outline::Calm), "{kind} wading");
        for state in [1, 2, 1 | CS_ON_WATER] {
            h.herd.chars[0].state = state;
            assert_eq!(
                h.binocular_outline(0),
                Some(Outline::Hostile),
                "{kind} state {state:#x}"
            );
        }
    }
}

#[test]
fn a_fleeing_herbivore_stays_green() {
    for kind in [ai::PARA, ai::STEGO, ai::ANKY] {
        let mut h = looking_at(kind);
        h.herd.chars[0].state = 1;
        assert_eq!(h.binocular_outline(0), Some(Outline::Calm), "{kind}");
    }
}

#[test]
fn a_predator_busy_with_other_animals_stays_green() {
    let mut h = looking_at(ai::ALLO);
    h.herd.chars[0].state = 2;
    for goal in [
        Goal::Hunt(3),
        Goal::Scavenge(3),
        Goal::Flee(3),
        Goal::Run([0.0; 2]),
    ] {
        h.herd.chars[0].predation.goal = goal;
        assert_eq!(h.binocular_outline(0), Some(Outline::Calm), "{goal:?}");
    }
    h.herd.chars[0].predation.goal = Goal::None;
    assert_eq!(h.binocular_outline(0), Some(Outline::Hostile));
}

#[test]
fn an_observer_is_never_hunted() {
    let mut h = looking_at(ai::TREX);
    h.herd.chars[0].state = 1;
    h.opts.observer = true;
    assert_eq!(h.binocular_outline(0), Some(Outline::Calm));
}

/// A 1000-point dialect kill (no non-target halving, no other factor).
pub(super) fn kill_score(area: &mut Area, h: &mut Hunt) -> i32 {
    h.opts.radar = false;
    h.opts.scent = false;
    h.opts.camo = false;
    h.opts.tranq = false;
    h.opts.score_factor = 1.0;
    h.successes = 1;
    area.time = crate::area::TimeOfDay::Day;
    let ctype = h.herd.chars[0].ctype;
    h.herd.species[ctype].info.base_score = 1000.0;
    h.herd.species[ctype].tuning.mee_ai = Some(10);
    let before = h.room.score;
    let mut events = Vec::new();
    h.add_ship_task(area, 0, &mut events);
    h.room.score - before
}

#[test]
fn the_gear_takes_forty_percent_of_a_kill() {
    let (mut area, mut plain, _) = setup(false);
    assert_eq!(kill_score(&mut area, &mut plain), 1000);
    let (mut area, mut geared, _) = setup(false);
    geared.opts.advanced_binoculars = true;
    assert_eq!(kill_score(&mut area, &mut geared), 600);
    assert_eq!(ADVANCED_BINOCULARS_SCORE, 0.60);
}

#[test]
fn the_terminal_prices_the_gear_into_its_earnings() {
    let k = crate::dhc::earnings_factor(["Advanced binoculars"], false);
    assert!((k - 0.60).abs() < 1e-6, "{k}");
    let k = crate::dhc::earnings_factor(["Radar", "advanced binoculars"], false);
    assert!((k - 0.70 * 0.60).abs() < 1e-6, "{k}");
}
