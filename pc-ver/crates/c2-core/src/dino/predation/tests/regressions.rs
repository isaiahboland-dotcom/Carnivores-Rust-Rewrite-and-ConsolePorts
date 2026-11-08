//! Regressions from the 2026-11-06 predation review: feeding that ignored
//! the hunter, unpriced carnivores hunting each other, and prey that kept
//! fleeing a predator that had lost interest.

use c2_formats::predation::Predation;

use super::{fights, herd, run, species, ALLO, RAPTOR, REX, STEGO};
use crate::area::Area;
use crate::dino::predation::run::Goal;
use crate::dino::predation::{Rel, Rules};
use crate::dino::{ai, Herd};

/// A raptor that has just killed the stego and is eating it.
fn feeding_raptor(area: &Area) -> Herd {
    let mut h = herd(
        &Predation::default(),
        area,
        &[(RAPTOR, 0.0), (STEGO, 1500.0)],
    );
    run(&mut h, area, 60_000, |h| h.chars[1].health == 0);
    assert_eq!(h.chars[1].health, 0, "the stego got away");
    assert!(h.chars[0].predation.eat_ms > 0);
    h
}

#[test]
fn a_shot_or_a_noise_ends_feeding() {
    let area = Area::flat(10);
    // Shot: `Hunt::damage` marks it aware and alarmed for a minute.
    let mut h = feeding_raptor(&area);
    let c = &mut h.chars[0];
    (c.state, c.aware_hunter, c.afraid_time) = (2, true, 60_000);
    run(&mut h, &area, 100, |_| false);
    assert_eq!(h.chars[0].predation.eat_ms, 0, "kept eating after a shot");

    // A shot heard (`make_noise`): alarmed, nothing else.
    let mut h = feeding_raptor(&area);
    let c = &mut h.chars[0];
    (c.state, c.afraid_time) = (2, 10_000);
    run(&mut h, &area, 100, |_| false);
    assert_eq!(h.chars[0].predation.eat_ms, 0, "kept eating through a shot");

    // Left alone, it eats on.
    let mut h = feeding_raptor(&area);
    run(&mut h, &area, 1_000, |_| false);
    assert!(h.chars[0].predation.eat_ms > 0);
}

#[test]
fn unpriced_carnivores_leave_each_other_alone() {
    let roster = vec![
        species("Roamer", ai::VELO, 0),
        species("Other", ai::ALLO, 0),
        species("Priced", ai::SPINO, 10),
    ];
    let r = Rules::new(&fights(), &roster);
    assert_eq!(r.rel(0, 1), Rel::Ignore);
    assert_eq!(r.rel(1, 0), Rel::Ignore);
    assert_eq!(r.outranks(0, 1), None);
    // A priced carnivore still outranks and hunts an unpriced one.
    assert_eq!(r.rel(2, 0), Rel::Hunts);
    assert_eq!(r.rel(0, 2), Rel::Ignore);
    assert_eq!(r.outranks(2, 0), Some(true));
    assert_eq!(r.outranks(0, 2), Some(false));
    // Equal prices stand off.
    let even = vec![species("A", ai::VELO, 20), species("B", ai::ALLO, 20)];
    let r = Rules::new(&fights(), &even);
    assert_eq!((r.rel(0, 1), r.rel(1, 0)), (Rel::Ignore, Rel::Ignore));
    assert_eq!(r.outranks(0, 1), None);
}

#[test]
fn prey_stops_fleeing_a_predator_that_is_not_after_it() {
    let area = Area::flat(10);
    // Already fleeing a raptor that has started to eat, 2500 away.
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 2500.0)],
    );
    h.chars[0].predation.eat_ms = 60_000;
    h.chars[0].predation.rest = 60_000;
    h.chars[1].predation.goal = Goal::Flee(0);
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(
        h.chars[1].predation.goal,
        Goal::None,
        "fled an eating raptor"
    );

    // A resting raptor (not eating) is ignored too, unless very close.
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 2500.0)],
    );
    h.chars[0].predation.rest = 60_000;
    run(&mut h, &area, 2_000, |_| false);
    assert_eq!(h.chars[1].predation.goal, Goal::None);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 700.0)],
    );
    h.chars[0].predation.rest = 60_000;
    run(&mut h, &area, 1_000, |h| {
        h.chars[1].predation.goal != Goal::None
    });
    assert_eq!(h.chars[1].predation.goal, Goal::Flee(0));
}

#[test]
fn a_rival_after_other_prey_does_not_scare_a_herd() {
    let area = Area::flat(10);
    // The allosaur is after the raptor; the stego 2500 away grazes on.
    let mut h = herd(
        &fights(),
        &area,
        &[(ALLO, 0.0), (STEGO, 2500.0), (RAPTOR, -1500.0)],
    );
    h.chars[0].predation.goal = Goal::Hunt(2);
    h.chars[1].predation.goal = Goal::Flee(0);
    run(&mut h, &area, 400, |_| false);
    assert_ne!(h.chars[1].predation.goal, Goal::Flee(0));
}

/// 2026-11-07 audit P1: a meal that ran its course left the carnivore in
/// its eating clip, which its AI never leaves on its own (Allosaurus and
/// Velociraptor stood eating forever). The Rex recovers from its sniff clip.
#[test]
fn a_finished_meal_hands_the_carnivore_back_to_its_ai() {
    let area = Area::flat(10);
    for eater in [RAPTOR, ALLO, REX] {
        let mut h = herd(
            &Predation::default(),
            &area,
            &[(eater, 0.0), (STEGO, 1500.0)],
        );
        run(&mut h, &area, 60_000, |h| h.chars[1].health == 0);
        assert_eq!(h.chars[1].health, 0, "the stego got away from {eater}");
        let clip = h.chars[0].predation.eat_phase;
        assert!(clip.is_some());
        run(&mut h, &area, 14_000, |_| false);
        let c = &h.chars[0];
        assert_eq!(c.predation.eat_ms, 0, "{eater} still eating");
        assert_ne!(Some(c.phase), clip, "{eater} stuck in its eating clip");
        let at = c.pos;
        run(&mut h, &area, 10_000, |_| false);
        let moved = (h.chars[0].pos[0] - at[0]).hypot(h.chars[0].pos[2] - at[2]);
        assert!(
            moved > 100.0,
            "{eater} stood still after its meal ({moved})"
        );
    }
}

/// From the v0.5.0 source audit: goals are re-picked every `THINK_MS`, but
/// a goal is settled on every AI step, so an animal shot dead within reach
/// of its target could still kill it or eat from it before the next pick.
#[test]
fn a_dead_animal_neither_kills_nor_eats() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 100.0)],
    );
    h.chars[0].health = 0;
    h.chars[0].predation.goal = Goal::Hunt(1);
    let p = h.predation.take().unwrap();
    p.settle(0, &mut h.chars, &h.species, true);
    assert_ne!(h.chars[1].health, 0, "a dead raptor killed the stego");
    assert_eq!(h.chars[0].predation.eat_phase, None);

    h.chars[1].health = 0;
    h.chars[0].predation.goal = Goal::Scavenge(1);
    p.settle(0, &mut h.chars, &h.species, true);
    assert!(
        !h.chars[1].predation.eaten,
        "a dead raptor ate from the body"
    );
}
