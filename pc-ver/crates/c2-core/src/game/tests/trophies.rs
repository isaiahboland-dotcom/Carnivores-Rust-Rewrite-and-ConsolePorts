use super::support::{blank_save, info, rifle, setup, target_car, weapon_ticks};
use crate::area::Area;
use crate::dino::{ai, Herd, Species};
use crate::game::{
    GameEvent, Hunt, HuntInput, HuntOptions, Outcome, WeaponAnims, WeaponState, Weapons,
};
use crate::player::Player;
use crate::rng::MsRand;
use crate::ship::ShipEvent;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use c2_formats::bits::Bits;
use c2_formats::sav::TrophyItem;

#[test]
fn non_targets_score_half_only_in_the_original() {
    let score = |dialect: bool, targets: Bits, bonus: Bits| {
        let (area, mut hunt, _) = setup(false);
        hunt.herd.species[1].info.base_score = 100.0;
        if dialect {
            hunt.herd.species[1].tuning.mee_ai = Some(ai::STEGO);
        }
        (hunt.opts.targets, hunt.opts.no_call_bonus) = (targets, bonus);
        hunt.add_ship_task(&area, 0, &mut Vec::new());
        hunt.room.score
    };
    let stego = Bits::bit(ai::STEGO as usize);
    let none = Bits::NONE;
    assert_eq!(score(false, stego, none), 100);
    assert_eq!(score(false, none, none), 50);
    assert_eq!(score(true, none, none), 100);
    assert_eq!(score(true, stego, stego), 110);
}

#[test]
fn dialect_trophies_take_a_spot_made_for_them() {
    use c2_formats::mee::TrophySpot;
    let spot = |c: usize| TrophySpot {
        chars: vec![c],
        ..Default::default()
    };
    let (area, mut h, _) = setup(false);
    // The animal (type 1) has the second spot.
    h.herd.trophy_spots = vec![spot(0), spot(1)];
    h.add_ship_task(&area, 0, &mut Vec::new());
    assert_eq!((h.room.bodies[0].ctype, h.room.bodies[1].ctype), (0, 1));
    // None made for it: any free spot.
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![spot(0)];
    h.add_ship_task(&area, 0, &mut Vec::new());
    assert_eq!(h.room.bodies[0].ctype, 1);
    // All taken: no trophy.
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![spot(0)];
    h.room.bodies[0].ctype = 5;
    let mut ev = Vec::new();
    h.add_ship_task(&area, 0, &mut ev);
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Trophy { .. })));
}

#[test]
fn trophy_is_recorded_and_the_ship_takes_the_body() {
    let (area, mut h, car) = setup(true);
    let cars = [Some(&car), Some(&car)];
    h.ship_anim_ms = 2000;
    let mut ev = Vec::new();
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            toggle_weapon: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            fire: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    assert_eq!(h.ship.tasks.len(), 1);
    // Base score 1, halved for a non-target species.
    assert!(
        ev.contains(&GameEvent::Trophy { slot: 0, score: 0 }),
        "{ev:?}"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        GameEvent::Scored {
            ctype: 1,
            score: 0,
            ..
        }
    )));
    assert_eq!(h.room.bodies[0].ctype, 1);
    let n = h.herd.chars.len();
    let mut removed = false;
    for _ in 0..12_000 {
        ev.clear();
        h.tick(&area, &cars, &HuntInput::default(), 10, &mut ev);
        if ev
            .iter()
            .any(|e| matches!(e, GameEvent::Ship(ShipEvent::Removed { .. })))
        {
            removed = true;
            break;
        }
    }
    assert!(removed, "ship state {} at {:?}", h.ship.state, h.ship.pos);
    assert_eq!(h.herd.chars.len(), n - 1);
}

#[test]
fn trophy_room_shows_plaques_and_removes_trophies() {
    let mut area = Area::flat(10);
    area.set_trophy_mode();
    let car = target_car(true);
    let mut room = blank_save();
    room.bodies[0] = TrophyItem {
        ctype: 1,
        scale: 1.0,
        ..Default::default()
    };
    room.bodies[7] = TrophyItem {
        ctype: 1,
        scale: 0.8,
        ..Default::default()
    };
    let mut rng = MsRand::new(5);
    let mut herd = Herd {
        species: vec![
            Species::new(info(0), &car),
            Species::new(info(ai::STEGO), &car),
        ],
        chars: vec![],
        demo_index: None,
        ..Default::default()
    };
    herd.place_trophies(&area, &room, &mut rng);
    assert_eq!(herd.chars.len(), 2);
    assert_eq!((herd.chars[1].state, herd.chars[1].scale), (7, 0.8));
    // Slot 0 is cell (69, 66), facing +z (look = (cos a, sin a)); stand
    // 2.5 cells ahead of it.
    let mut player = Player::new(&area, (69, 68));
    player.z = 66.0 * 256.0 + 128.0 + 640.0;
    let weapons = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::NONE,
        false,
    );
    let opts = HuntOptions {
        trophy: true,
        ..Default::default()
    };
    let mut h = Hunt::new(&area, player, herd, weapons, rng, opts, room);
    let mut ev = Vec::new();
    h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
    assert_eq!(h.trophy_body, Some(0));
    // Locked, it stays; unlocked, R removes it.
    assert_eq!(h.toggle_lock_current(), Some(true));
    h.remove_current_trophy();
    assert_eq!(h.room.bodies[0].ctype, 1);
    assert_eq!(h.toggle_lock_current(), Some(false));
    // Moved to the next free stand after it (slot 1), and back.
    assert!(h.move_current_trophy(&area, 1));
    assert_eq!((h.room.bodies[0].ctype, h.room.bodies[1].ctype), (0, 1));
    assert_eq!(h.trophy_body, Some(1));
    assert!(h.herd.chars.iter().any(|c| c.state == 1));
    assert!(h.move_current_trophy(&area, -1));
    assert!(!h.move_current_trophy(&area, -1));
    // The mobile room is a ring: the last stand is left of stand 0.
    let mut mobile_area = Area::flat(10);
    mobile_area.set_trophy_mode();
    mobile_area.trophy_start = Some((76, 70));
    assert!(h.move_current_trophy(&mobile_area, -1));
    assert_eq!(h.trophy_body, Some(23));
    let mount = h.herd.chars.iter().find(|c| c.state == 23).unwrap();
    let (x, z) = mobile_area.landings[23];
    assert_eq!(
        (mount.pos[0], mount.pos[2]),
        (f32::from(x) * 256.0, f32::from(z) * 256.0)
    );
    assert!(h.move_current_trophy(&mobile_area, 1));
    assert_eq!(h.trophy_body, Some(0));
    h.herd.place_trophies(&area, &h.room, &mut h.rng);
    h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
    assert_eq!(h.trophy_body, Some(0));
    h.remove_current_trophy();
    assert_eq!(h.room.bodies[0].ctype, 0);
    assert_eq!(h.herd.chars.len(), 1);
    h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
    assert_eq!(h.trophy_body, None);
    // No weapons, no map; Escape leaves without touching the stats.
    h.tick(
        &area,
        &[],
        &HuntInput {
            toggle_weapon: true,
            toggle_map: true,
            ..Default::default()
        },
        10,
        &mut ev,
    );
    assert_eq!(h.weapons.state, WeaponState::Hidden);
    assert!(!h.map_mode);
    h.press_escape();
    assert_eq!(h.outcome(), Some(Outcome::Exited));
    assert_eq!(h.room.total.time, 0.0);
}

#[test]
fn dynamic_night_bonus_replaces_classic_bonus_at_kill_time() {
    for (start, dynamic, expected) in [
        (crate::area::TimeOfDay::Night, None, 10),
        (crate::area::TimeOfDay::Night, Some(false), 0),
        (crate::area::TimeOfDay::Day, Some(true), 20),
        (crate::area::TimeOfDay::Day, Some(false), 0),
    ] {
        let (mut area, mut h, _) = setup(false);
        area.time = start;
        h.opts.dynamic_night = dynamic;
        h.opts.radar = false;
        h.opts.scent = false;
        h.opts.camo = false;
        h.opts.tranq = false;
        h.opts.no_call_bonus = c2_formats::bits::Bits::NONE;
        h.opts.score_factor = 1.0;
        h.successes = 1;
        let ctype = h.herd.chars[0].ctype;
        h.herd.species[ctype].info.base_score = 1000.0;
        // Treat it as a dialect target to remove the stock non-target penalty.
        h.herd.species[ctype].tuning.mee_ai = Some(10);
        let before = h.room.score;
        let mut events = Vec::new();
        h.add_ship_task(&area, 0, &mut events);
        assert_eq!(h.room.score - before, 1000 + i32::from(expected) * 10);
        assert!(events.iter().any(
            |e| matches!(e, GameEvent::Scored { night_bonus, .. } if *night_bonus == expected)
        ));
    }
}
