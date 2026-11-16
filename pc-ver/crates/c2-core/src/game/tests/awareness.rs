//! Awareness (`dino::aware`, an instance setting): suspicion, going to
//! look, and a predator that loses the hunter and searches.

use super::support::setup;
use crate::area::Area;
use crate::dino::ai;
use crate::dino::aware::{seek, Entry, Level, Rules, LOSE_MS};
use crate::game::{Hunt, HuntInput};
use crate::CELL;
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec, vec::Vec};

const SEARCH_MS: i32 = 6000;

/// The hunt's one animal (a herbivore, or a raptor with `predator`) with
/// awareness on: suspicious up to 1.6 times the alarm threshold, looking
/// after 2 s, searching for 6 s. No wind, so only sight counts.
fn aware(predator: bool) -> (Area, Hunt) {
    let (area, mut hunt, _) = setup(false);
    hunt.wind.speed = 0.0;
    if predator {
        hunt.herd.species[1].info.ai = ai::VELO;
        hunt.herd.chars[0].ai = ai::VELO;
    }
    let entry = Entry {
        predator,
        suspicion: 1.6,
        investigate_ms: 2000,
        search_ms: SEARCH_MS,
    };
    hunt.herd.awareness = Some(Box::new(Rules {
        per_species: vec![None, Some(entry)],
    }));
    (area, hunt)
}

/// Puts the animal `d` units in front of the hunter, facing him.
fn stand(hunt: &mut Hunt, d: f32) {
    let c = &mut hunt.herd.chars[0];
    c.pos = [hunt.player.x, c.pos[1], hunt.player.z - d];
    (c.lookx, c.lookz) = (0.0, 1.0);
}

/// The nearest distance (a whole number of cells) at which the animal's
/// detection value is at least `k`.
fn distance_for(hunt: &mut Hunt, area: &Area, k: f32) -> f32 {
    for cells in 4..400 {
        let d = cells as f32 * CELL;
        stand(hunt, d);
        if hunt.notice(area, 0, false).is_none_or(|n| n.0 >= k) {
            return d;
        }
    }
    panic!("never reaches {k}");
}

fn checks(hunt: &mut Hunt, area: &Area, ms: i32) {
    for _ in 0..ms / 10 {
        hunt.check_afraid(area, 10);
    }
}

fn level(hunt: &Hunt) -> Level {
    hunt.herd.chars[0].aware.level
}

#[test]
fn without_the_setting_nothing_is_tracked() {
    let (area, mut hunt, _) = setup(false);
    hunt.wind.speed = 0.0;
    checks(&mut hunt, &area, 100);
    let c = &hunt.herd.chars[0];
    assert!(c.state != 0, "alarmed as ever");
    assert_eq!(c.aware, Default::default());
}

#[test]
fn a_half_noticed_hunter_is_looked_into_after_a_while() {
    let (area, mut hunt) = aware(false);
    let d = distance_for(&mut hunt, &area, 1.2);
    stand(&mut hunt, d);
    hunt.check_afraid(&area, 10);
    assert_eq!(
        (level(&hunt), hunt.herd.chars[0].state),
        (Level::Suspicious, 0)
    );
    checks(&mut hunt, &area, 1000);
    assert_eq!(level(&hunt), Level::Suspicious, "not yet");
    checks(&mut hunt, &area, 1100);
    let c = &hunt.herd.chars[0];
    assert_eq!((c.aware.level, c.state), (Level::Investigating, 0));
    // A herbivore heads his way but stops 12 cells short.
    let gap = ((c.tgx - hunt.player.x).powi(2) + (c.tgz - hunt.player.z).powi(2)).sqrt();
    assert!((gap - 12.0 * CELL).abs() < 8.0, "{gap}");
    assert!(c.tgz > c.pos[2], "towards him");
    // The look ends by its timer when nothing more comes of it.
    hunt.player.z += 200_000.0;
    checks(&mut hunt, &area, SEARCH_MS);
    assert_eq!(level(&hunt), Level::Unaware);
}

#[test]
fn suspicion_lapses_when_the_hunter_is_no_longer_noticed() {
    let (area, mut hunt) = aware(false);
    let d = distance_for(&mut hunt, &area, 1.2);
    stand(&mut hunt, d);
    checks(&mut hunt, &area, 1000);
    assert_eq!(level(&hunt), Level::Suspicious);
    let far = distance_for(&mut hunt, &area, 2.0);
    stand(&mut hunt, far);
    hunt.check_afraid(&area, 10);
    assert_eq!(level(&hunt), Level::Unaware);
}

#[test]
fn a_full_alarm_is_the_originals() {
    let (area, mut hunt) = aware(false);
    stand(&mut hunt, 2000.0);
    hunt.check_afraid(&area, 10);
    let c = &hunt.herd.chars[0];
    assert!(c.state != 0 && c.afraid_time > 0);
    assert_eq!(c.aware.level, Level::Pursuing);
    assert_eq!(seek(c, 0), None, "a herbivore is never steered");
}

/// A raptor chasing the hunter, who then gets out of its senses.
fn lost_raptor() -> (Area, Hunt, (f32, f32)) {
    let (area, mut hunt) = aware(true);
    stand(&mut hunt, 3000.0);
    hunt.check_afraid(&area, 10);
    let c = &hunt.herd.chars[0];
    assert!(c.state != 0);
    assert_eq!((c.aware.level, c.aware.lost_ms), (Level::Pursuing, 0));
    assert_eq!(seek(c, 0), None, "it has the hunter himself");
    let last = (hunt.player.x, hunt.player.z);
    hunt.player.x += 250.0 * CELL;
    (area, hunt, last)
}

#[test]
fn a_predator_that_loses_the_hunter_heads_for_where_it_last_sensed_him() {
    let (area, mut hunt, last) = lost_raptor();
    checks(&mut hunt, &area, 1000);
    let c = &hunt.herd.chars[0];
    assert_eq!(c.aware.level, Level::Pursuing);
    assert_eq!(seek(c, 0), Some(last));
    assert!(c.state != 0, "still alarmed");
}

#[test]
fn it_then_searches_round_that_place_and_calms_by_its_timer() {
    let (area, mut hunt, last) = lost_raptor();
    checks(&mut hunt, &area, LOSE_MS);
    assert_eq!(level(&hunt), Level::Searching);
    // The search's points ring the place, each farther round.
    let mut points = Vec::new();
    for _ in 0..SEARCH_MS / 1000 - 1 {
        checks(&mut hunt, &area, 1000);
        let c = &hunt.herd.chars[0];
        assert!(c.state != 0, "searching at {} ms", c.aware.ms);
        let p = seek(c, 0).expect("a point to search");
        let r = ((p.0 - last.0).powi(2) + (p.1 - last.1).powi(2)).sqrt();
        assert!((2.9 * CELL..=10.1 * CELL).contains(&r), "{r}");
        if points.last() != Some(&p) {
            points.push(p);
        }
    }
    assert!(points.len() >= 2, "it moves on: {points:?}");
    checks(&mut hunt, &area, 1100);
    let c = &hunt.herd.chars[0];
    assert_eq!(
        (c.state, c.afraid_time, c.aware.level),
        (0, 0, Level::Unaware)
    );
}

#[test]
fn sensing_him_again_ends_the_search() {
    let (area, mut hunt, _) = lost_raptor();
    checks(&mut hunt, &area, LOSE_MS + 1000);
    assert_eq!(level(&hunt), Level::Searching);
    hunt.player.x -= 250.0 * CELL;
    hunt.check_afraid(&area, 10);
    let c = &hunt.herd.chars[0];
    assert_eq!((c.aware.level, c.aware.lost_ms), (Level::Pursuing, 0));
    assert_eq!(seek(c, 0), None);
}

/// Stealth crouch hides the hunter from a calm animal and from nothing
/// that is already after him.
#[test]
fn crouching_helps_only_before_the_pursuit() {
    let (area, mut hunt) = aware(true);
    hunt.opts.stealth = true;
    // Where a standing hunter is noticed and a crouched one is not (smell
    // is the same either way, so the margin is small).
    let mut d = 0.0;
    for cells in 4..400 {
        d = cells as f32 * CELL;
        stand(&mut hunt, d);
        hunt.player.head_y = 220.0;
        let standing = hunt.notice(&area, 0, false).map(|n| n.0);
        hunt.player.head_y = 110.0;
        let crouched = hunt.notice(&area, 0, false).map(|n| n.0);
        if standing.is_some_and(|k| k < 0.95) && crouched.is_none_or(|k| k > 1.05) {
            break;
        }
    }
    stand(&mut hunt, d);
    hunt.player.head_y = 110.0;
    checks(&mut hunt, &area, 1000);
    assert_eq!(hunt.herd.chars[0].state, 0, "crouched: not alarmed");
    assert_ne!(level(&hunt), Level::Pursuing);

    // Alarmed (he stood up), then he crouches again: it still has him.
    hunt.player.head_y = 220.0;
    hunt.check_afraid(&area, 10);
    assert_eq!(level(&hunt), Level::Pursuing);
    hunt.player.head_y = 110.0;
    checks(&mut hunt, &area, 3000);
    let c = &hunt.herd.chars[0];
    assert!(c.state != 0);
    assert_eq!((c.aware.level, c.aware.lost_ms), (Level::Pursuing, 0));
}

#[test]
fn a_search_is_not_cut_short_by_crouching() {
    let (area, mut hunt, _) = lost_raptor();
    hunt.opts.stealth = true;
    checks(&mut hunt, &area, LOSE_MS);
    hunt.player.head_y = 110.0;
    checks(&mut hunt, &area, SEARCH_MS - 100);
    assert_eq!(level(&hunt), Level::Searching);
    assert!(hunt.herd.chars[0].state != 0);
    checks(&mut hunt, &area, 200);
    assert_eq!(level(&hunt), Level::Unaware);
}

#[test]
fn a_shot_heard_moves_the_search_to_where_it_came_from() {
    let (area, mut hunt, last) = lost_raptor();
    checks(&mut hunt, &area, LOSE_MS + 500);
    assert_eq!(level(&hunt), Level::Searching);
    let c = hunt.herd.chars[0].pos;
    let shot = [c[0] + 20.0 * CELL, c[1], c[2]];
    hunt.make_noise(shot, 60.0 * CELL);
    let c = &hunt.herd.chars[0];
    assert_eq!(c.aware.level, Level::Pursuing);
    assert_eq!(c.aware.at, (shot[0], shot[2]));
    assert_ne!(c.aware.at, last);
}

/// In the hunt itself the lost raptor runs to the place, not after the
/// hunter, and the run repeats exactly.
#[test]
fn the_lost_predator_runs_to_the_place_in_a_hunt() {
    let run = || {
        let (area, mut hunt, last) = lost_raptor();
        let mut out = Vec::new();
        for _ in 0..100 {
            hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
        }
        let c = &hunt.herd.chars[0];
        ((c.tgx, c.tgz), c.pos, c.aware, last)
    };
    let (target, pos, a, last) = run();
    assert_eq!(a.level, Level::Pursuing);
    assert_eq!(target, last, "aimed at where it lost him");
    let left = ((pos[0] - last.0).powi(2) + (pos[2] - last.1).powi(2)).sqrt();
    assert!(left < 2900.0, "closing on it: {left}");
    assert_eq!(run().1, pos);
}
