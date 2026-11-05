//! Multi ammo (lethal rounds and darts) and advanced observer mode.

use super::support::{rifle, setup, weapon_ticks};
use crate::game::{GameEvent, HuntInput, WeaponAnims, Weapons};
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use c2_formats::bits::Bits;

fn fire(
    h: &mut crate::game::Hunt,
    area: &crate::area::Area,
    car: &c2_formats::car::Car,
) -> Vec<GameEvent> {
    let cars = [Some(car), Some(car)];
    let mut ev = Vec::new();
    let draw = HuntInput {
        toggle_weapon: true,
        ..Default::default()
    };
    weapon_ticks(h, area, &cars, draw, 1, &mut ev);
    weapon_ticks(h, area, &cars, HuntInput::default(), 12, &mut ev);
    let shot = HuntInput {
        fire: true,
        ..Default::default()
    };
    weapon_ticks(h, area, &cars, shot, 1, &mut ev);
    ev
}

#[test]
fn half_tranq_splits_every_round_and_swaps() {
    // 14 rounds and a spare magazine of 14.
    let mut w = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        true,
    );
    w.split_tranq();
    assert_eq!(w.kinds(0), (14, 14));
    assert!(!w.fires_darts(0));
    assert!(w.swap_kind(0));
    assert!(w.fires_darts(0));
    assert_eq!(w.kinds(0), (14, 14));
    // Odd counts keep the extra round lethal.
    let mut w = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        false,
    );
    w.shots_left[0] = 5;
    w.split_tranq();
    assert_eq!(w.kinds(0), (3, 2));
    // Resupply gives both kinds their half again.
    w.shots_left[0] = 0;
    w.resupply();
    assert_eq!(w.kinds(0), (7, 7));
}

#[test]
fn without_multi_ammo_nothing_swaps() {
    let mut w = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        false,
    );
    assert!(!w.multi_ammo());
    assert!(!w.swap_kind(0));
    assert_eq!(w.kinds(0), (14, 0));
}

#[test]
fn a_dart_marks_the_animal_and_the_last_round_swaps_kind() {
    let (area, mut h, car) = setup(true);
    h.weapons.split_tranq();
    h.weapons.swap_kind(0);
    assert_eq!(h.weapons.kinds(0), (7, 7));
    let ev = fire(&mut h, &area, &car);
    assert!(
        ev.iter()
            .any(|e| matches!(e, GameEvent::Killed { index: 0, .. })),
        "{ev:?}"
    );
    assert!(h.herd.chars[0].darted, "a dart's kill sleeps");
    assert!(
        ev.iter()
            .any(|e| matches!(e, GameEvent::Scored { tranq: true, .. })),
        "{ev:?}"
    );
    assert_eq!(h.weapons.kinds(0), (7, 6));
    // Darts used up: lethal rounds are loaded on their own.
    let (area2, mut h2, car2) = setup(false);
    h2.weapons.split_tranq();
    h2.weapons.swap_kind(0);
    h2.weapons.shots_left[0] = 1;
    fire(&mut h2, &area2, &car2);
    assert!(!h2.weapons.fires_darts(0));
    assert_eq!(h2.weapons.kinds(0), (7, 0));
}

#[test]
fn advanced_observer_flies_stops_time_and_teleports_at_once() {
    let (area, mut h, _) = setup(false);
    assert!(!h.toggle_fly(), "only in advanced observer mode");
    h.opts.observer = true;
    h.opts.free_observer = true;
    assert!(h.can_teleport());
    assert!(h.toggle_fly());
    let up = HuntInput {
        controls: crate::player::Controls {
            jump: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let y0 = h.player.y;
    let mut ev = Vec::new();
    for _ in 0..10 {
        h.tick(&area, &[], &up, 20, &mut ev);
    }
    assert!(
        h.player.y > y0 + 100.0,
        "flying rises: {} -> {}",
        y0,
        h.player.y
    );
    // Time stopped: the animal holds still while the hunter still moves.
    assert!(h.toggle_time_stop());
    let dino = h.herd.chars[0].pos;
    let t = h.real_time;
    let y1 = h.player.y;
    for _ in 0..10 {
        h.tick(&area, &[], &up, 20, &mut ev);
    }
    assert_eq!(h.herd.chars[0].pos, dino);
    assert_eq!(h.real_time, t);
    assert!(h.player.y > y1);
    // A teleport with no wait and no cooldown.
    let (x, z) = (h.player.x + 4096.0, h.player.z);
    assert!(h.request_teleport(&area, x, z).is_ok());
    h.tick(&area, &[], &HuntInput::default(), 20, &mut ev);
    assert!((h.player.x - x).abs() < 512.0);
    assert!(h.request_teleport(&area, x - 4096.0, z).is_ok());
}
