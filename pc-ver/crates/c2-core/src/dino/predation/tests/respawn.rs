//! 2026-11-07 audit P3: predation only ever lowered the population. An
//! animal's kill comes back as a new animal once out of the hunter's sight;
//! the hunter's own kills don't.

use c2_formats::predation::Predation;

use super::{herd, hunter, run, RAPTOR, STEGO};
use crate::area::Area;
use crate::dino::predation::run::BODY_MS;
use crate::dino::Herd;

/// A raptor that has just killed the stego (index 1).
fn killed_stego(area: &Area) -> Herd {
    let mut h = herd(
        &Predation::default(),
        area,
        &[(RAPTOR, 0.0), (STEGO, 1500.0)],
    );
    run(&mut h, area, 60_000, |h| h.chars[1].health == 0);
    assert_eq!(h.chars[1].health, 0, "the stego got away");
    assert!(h.chars[1].predation.killed_by_animal);
    h
}

fn distance_to_hunter(h: &Herd, i: usize, area: &Area) -> f32 {
    let v = hunter(area);
    (h.chars[i].pos[0] - v.x).hypot(h.chars[i].pos[2] - v.z)
}

#[test]
fn an_animals_kill_comes_back_out_of_sight() {
    let area = Area::flat(10);
    let mut h = killed_stego(&area);
    let n = h.chars.len();
    let at = h.chars[1].pos;
    run(&mut h, &area, BODY_MS - 2_000, |_| false);
    assert_eq!(h.chars[1].health, 0, "came back before the body time");

    run(&mut h, &area, 4_000, |_| false);
    let c = &h.chars[1];
    assert_eq!(h.chars.len(), n, "the herd grew or shrank");
    assert_eq!(c.ctype, STEGO);
    assert_eq!(
        c.health, h.species[STEGO].info.health0,
        "not a fresh animal"
    );
    assert!(!c.predation.killed_by_animal);
    assert_ne!(c.pos, at, "came back where it died");
    // `run` steps with a 74-cell render distance.
    assert!(distance_to_hunter(&h, 1, &area) >= 74.0 * 256.0);
}

#[test]
fn a_body_the_hunter_can_see_stays() {
    let area = Area::flat(10);
    let mut h = killed_stego(&area);
    let v = hunter(&area);
    h.chars[1].pos = [v.x + 2048.0, v.y, v.z];
    h.chars[0].pos = [v.x - 20.0 * 256.0, v.y, v.z];
    run(&mut h, &area, BODY_MS + 10_000, |_| false);
    assert_eq!(h.chars[1].health, 0, "came back in the hunter's sight");
}

#[test]
fn the_hunters_kills_never_come_back() {
    let area = Area::flat(10);
    let mut h = herd(&Predation::default(), &area, &[(STEGO, 0.0)]);
    h.chars[0].health = 0;
    run(&mut h, &area, BODY_MS + 10_000, |_| false);
    assert_eq!(h.chars[0].health, 0);
}
