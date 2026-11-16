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
fn variants_share_call_target_and_trophy_spots_but_save_their_own_model() {
    use crate::dino::variants::Variant;
    let (area, mut hunt, _) = setup(false);
    hunt.herd.species.push(hunt.herd.species[1].clone());
    for i in 1..=2 {
        let sp = &mut hunt.herd.species[i];
        sp.info.base_score = 100.0;
        sp.tuning.menu_slot = Some(0);
        sp.variant = Some(Variant {
            owner: 1,
            token: 100 + i as u64,
            choices: vec![],
        });
    }
    hunt.herd.chars[0].ctype = 2;
    hunt.opts.targets = Bits::bit(10);
    assert!(hunt.is_target(&hunt.herd.chars[0]));
    assert_eq!(hunt.call_bit(&hunt.herd.chars[0]), Some(10));
    hunt.herd.trophy_spots = vec![
        c2_formats::mee::TrophySpot {
            chars: vec![99],
            ..Default::default()
        },
        c2_formats::mee::TrophySpot {
            chars: vec![1],
            ..Default::default()
        },
    ];
    hunt.add_ship_task(&area, 0, &mut Vec::new());
    assert_eq!(hunt.room.score, 100);
    assert_eq!(hunt.room.bodies[1].ctype, 2);
    assert_eq!(hunt.room.bodies[1].variant_token(), 102);
    assert_eq!(hunt.room.bodies[0].ctype, 0);
}

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
    // None made for it: a plain stand past the spots, leaving the spot to
    // its own animal (a combined game: one add-on's spots aren't the room).
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![spot(0)];
    h.add_ship_task(&area, 0, &mut Vec::new());
    assert_eq!((h.room.bodies[0].ctype, h.room.bodies[1].ctype), (0, 1));
    // Plain stands all taken: any free spot.
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![spot(0)];
    for b in &mut h.room.bodies[1..] {
        b.ctype = 5;
    }
    h.add_ship_task(&area, 0, &mut Vec::new());
    assert_eq!(h.room.bodies[0].ctype, 1);
    // All taken: no trophy.
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![spot(0)];
    for b in &mut h.room.bodies {
        b.ctype = 5;
    }
    let mut ev = Vec::new();
    h.add_ship_task(&area, 0, &mut ev);
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Trophy { .. })));
}

/// A combined game where one add-on brings two `tropinfo` spots (the
/// nightmare-run report: two trophies, then none whatever was killed).
#[test]
fn one_add_ons_spots_do_not_cap_the_room() {
    use c2_formats::mee::TrophySpot;
    let (area, mut h, _) = setup(false);
    h.herd.trophy_spots = vec![
        TrophySpot {
            chars: vec![99],
            pos: 17,
            ..Default::default()
        };
        2
    ];
    let mut slots = Vec::new();
    for _ in 0..5 {
        let mut ev = Vec::new();
        h.add_ship_task(&area, 0, &mut ev);
        slots.extend(ev.iter().filter_map(|e| match e {
            GameEvent::Trophy { slot, .. } => Some(*slot),
            _ => None,
        }));
    }
    assert_eq!(
        slots,
        [2, 3, 4, 5, 6],
        "every kill a trophy, spots left free"
    );

    // In the room: each on its own landing cell, the spots' cell left out.
    let mut room_area = Area::flat(10);
    room_area.set_trophy_mode();
    let car = target_car(true);
    let mut herd = Herd {
        species: vec![
            Species::new(info(0), &car),
            Species::new(info(ai::STEGO), &car),
        ],
        trophy_spots: h.herd.trophy_spots.clone(),
        ..Default::default()
    };
    let mut room = blank_save();
    for b in &mut room.bodies {
        b.ctype = 1;
        b.scale = 1.0;
    }
    herd.place_trophies(&room_area, &room, &mut MsRand::new(1));
    // Slots 0 and 1 on the two spots (old saves put others there); 2..23
    // on 22 of the 23 landing cells the spots don't use.
    assert_eq!(herd.chars.len(), 24);
    let mut cells: Vec<(i32, i32)> = herd
        .chars
        .iter()
        .filter(|c| c.state >= 2)
        .map(|c| ((c.pos[0] / 256.0) as i32, (c.pos[2] / 256.0) as i32))
        .collect();
    let (sx, sz) = room_area.landings[17];
    assert!(!cells.contains(&(i32::from(sx), i32::from(sz))));
    cells.sort_unstable();
    cells.dedup();
    assert_eq!(cells.len(), 22, "no two trophies on one stand");
    // Plain stands face the room's middle (look = (cos a, sin a)).
    let (mx, mz) = (76.5 * 256.0, 76.5 * 256.0);
    for c in herd.chars.iter().filter(|c| c.state >= 2) {
        let (dx, dz) = (mx - c.pos[0], mz - c.pos[2]);
        assert!(
            c.alpha.cos() * dx + c.alpha.sin() * dz > 0.0,
            "slot {}",
            c.state
        );
    }
    // The plaque is found 2.5 cells ahead of the nose, so every look vector
    // follows the facing the spot set (it once kept `reset`'s).
    for c in &herd.chars {
        assert!(
            (c.lookx - c.alpha.cos()).abs() < 1e-5 && (c.lookz - c.alpha.sin()).abs() < 1e-5,
            "slot {} looks the wrong way",
            c.state
        );
    }
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
    for (start, dynamic, night_vision, expected) in [
        (crate::area::TimeOfDay::Night, None, false, 10),
        (crate::area::TimeOfDay::Night, None, true, 10),
        (crate::area::TimeOfDay::Night, Some(false), false, 0),
        (crate::area::TimeOfDay::Day, Some(true), false, 20),
        (crate::area::TimeOfDay::Day, Some(true), true, 0),
        (crate::area::TimeOfDay::Night, Some(true), true, 0),
        (crate::area::TimeOfDay::Day, Some(false), false, 0),
        (crate::area::TimeOfDay::Day, Some(false), true, 0),
    ] {
        let (mut area, mut h, _) = setup(false);
        area.time = start;
        h.opts.dynamic_night = dynamic;
        h.opts.night_vision_active = night_vision;
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

#[test]
fn a_variant_scores_by_its_parents_rules() {
    use crate::dino::variants::Variant;
    // The parent (type 1) is a Modder's Edition dialect animal, or not; the
    // variant (type 2) has its own, lower score and no dialect.
    let score = |parent_dialect: bool| {
        let (area, mut hunt, _) = setup(false);
        hunt.herd.species.push(hunt.herd.species[1].clone());
        for (i, token) in [(1, 101), (2, 102)] {
            let sp = &mut hunt.herd.species[i];
            sp.variant = Some(Variant {
                owner: 1,
                token,
                choices: vec![],
            });
            sp.tuning.mee_ai = None;
        }
        hunt.herd.species[1].info.base_score = 100.0;
        hunt.herd.species[1].tuning.mee_ai = parent_dialect.then_some(ai::STEGO);
        hunt.herd.species[2].info.base_score = 10.0;
        hunt.herd.chars[0].ctype = 2;
        hunt.opts.targets = Bits::NONE;
        hunt.add_ship_task(&area, 0, &mut Vec::new());
        hunt.room.score
    };
    // Not a target and not a dialect kill: halved, from the parent's 100.
    assert_eq!(score(false), 50);
    // The parent's dialect keeps the full score.
    assert_eq!(score(true), 100);
}
