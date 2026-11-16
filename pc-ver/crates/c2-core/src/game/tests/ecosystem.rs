//! Spawn ecosystems ([`crate::dino::eco`]) on the animals that come during
//! a hunt: replacements and the instance rules' extra animals.

use super::support::setup;
use crate::area::TimeOfDay;
use crate::dino::eco::State;
use crate::game::Hunt;
use c2_formats::rules::Rules;
use c2_formats::world::World;

fn with_rules(text: &str) -> (crate::area::Area, Hunt) {
    let (area, mut h, _) = setup(false);
    h.herd.species[1].info.name = "Stego".into();
    h.herd.eco = State::load(
        &World::parse(text.as_bytes()),
        "area1",
        &h.herd.species,
        TimeOfDay::Day,
    );
    assert!(h.herd.eco.is_some());
    (area, h)
}

fn alive(h: &Hunt) -> usize {
    h.herd.chars.iter().filter(|c| c.health > 0).count()
}

#[test]
fn caps_hold_across_respawns() {
    let (area, mut h) =
        with_rules("area area1\nzone all polygon 0 0 1024 0 1024 1024 0 1024\neco cap 3\n");
    for round in 0..6 {
        for _ in 0..10 {
            h.add_secondary_one(&area, 1);
        }
        assert_eq!(alive(&h), 3, "round {round}");
        // One dies: its body stays, and one more may come.
        if let Some(c) = h.herd.chars.iter_mut().find(|c| c.health > 0) {
            c.health = 0;
        }
    }
    assert_eq!(h.herd.chars.len(), 3 + 5, "five bodies and the three alive");
}

#[test]
fn the_map_and_species_caps_stop_replacements_and_extra_animals() {
    let (area, mut h) = with_rules("species_max 4 stego\n");
    let rules = Rules::parse(b"secret 9 area1 stego\n");
    assert_eq!(h.apply_spawn_rules(&area, &rules, "area1"), 3);
    h.add_secondary_one(&area, 1);
    assert_eq!(alive(&h), 4);
    let (area, mut h) = with_rules("max_animals 2\n");
    for _ in 0..5 {
        h.add_secondary_one(&area, 1);
    }
    assert_eq!(alive(&h), 2);
}

#[test]
fn replacements_keep_out_of_a_no_spawn_zone() {
    // The hunter stands at cell 500: half the spots about him are refused.
    let (area, mut h) = with_rules(
        "area area1\nzone east polygon 500 0 1024 0 1024 1024 500 1024\neco no_spawn 1\n",
    );
    for _ in 0..40 {
        h.add_secondary_one(&area, 1);
    }
    assert!(h.herd.chars.len() > 20, "{}", h.herd.chars.len());
    for c in &h.herd.chars[1..] {
        assert!(c.pos[0] / 256.0 < 500.0, "{:?}", c.pos);
    }
}

#[test]
fn off_the_replacements_are_the_originals() {
    let (area, mut plain, _) = setup(false);
    // Rules that refuse nothing draw nothing from the random stream.
    let (_, mut ruled) = with_rules(
        "max_animals 99\narea area1\nzone all polygon 0 0 1024 0 1024 1024 0 1024\neco cap 99\n",
    );
    for _ in 0..8 {
        plain.add_secondary_one(&area, 1);
        ruled.add_secondary_one(&area, 1);
    }
    let spots =
        |h: &Hunt| -> alloc::vec::Vec<[f32; 3]> { h.herd.chars.iter().map(|c| c.pos).collect() };
    assert_eq!(spots(&plain), spots(&ruled));
}

#[test]
fn a_replacement_without_room_waits_and_comes_when_there_is() {
    let (area, mut h) = with_rules("max_animals 1\n");
    let waiting = |h: &Hunt| h.herd.eco.as_ref().map_or(0, |e| e.deferred.len());
    h.add_secondary_one(&area, 1);
    assert_eq!((alive(&h), waiting(&h)), (1, 1));
    // Still no room: it goes on waiting, tried every few seconds.
    h.retry_replacements(&area, 6000);
    assert_eq!((alive(&h), waiting(&h)), (1, 1));
    h.herd.chars[0].health = 0;
    h.retry_replacements(&area, 1000);
    assert_eq!(alive(&h), 0, "not before its next try");
    h.retry_replacements(&area, 5000);
    assert_eq!((alive(&h), waiting(&h)), (1, 0));
    // Without an ecosystem a failed replacement is dropped, as it always was.
    let (area, mut plain, _) = setup(false);
    plain.add_secondary_one(&area, 1);
    plain.retry_replacements(&area, 60_000);
    assert_eq!(plain.herd.chars.len(), 2);
}

#[test]
fn replacements_respect_their_own_distance() {
    let (area, mut h) = with_rules("safety respawn 70\n");
    for _ in 0..12 {
        h.add_secondary_one(&area, 1);
    }
    assert!(h.herd.chars.len() > 4, "{}", h.herd.chars.len());
    let (px, pz) = (h.player.x, h.player.z);
    for c in &h.herd.chars[1..] {
        let d = ((c.pos[0] - px).abs() + (c.pos[2] - pz).abs()) / 256.0;
        assert!(d >= 70.0, "{d}");
    }
}
