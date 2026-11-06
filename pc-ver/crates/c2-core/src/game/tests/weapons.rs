use super::support::{rifle, setup, weapon_ticks};
use crate::area::Area;
use crate::game::{
    weapon_anim, Bullet, GameEvent, Hunt, HuntInput, WeaponAnims, WeaponInfo, WeaponState, Weapons,
};
#[cfg(not(feature = "std"))]
use alloc::{format, vec, vec::Vec};
use c2_formats::bits::Bits;
use c2_formats::car::Car;
use c2_formats::res_txt::WeaponDef;

#[test]
fn dialect_scopes_zoom_by_their_optic_and_cross_only_when_asked() {
    let def = |optic: f32, keys: &[(&str, &str)]| WeaponDef {
        optic: Some(optic),
        extra: keys
            .iter()
            .map(|&(k, v)| c2_formats::res_txt::Entry {
                key: k.into(),
                value: v.into(),
            })
            .collect(),
        ..Default::default()
    };
    let (_, mut hunt, _) = setup(false);
    let mut scoped = |d: &WeaponDef, state: WeaponState| {
        hunt.weapons.info[0] = WeaponInfo::from_def(d);
        hunt.weapons.optic_mode = hunt.weapons.info[0].optic;
        hunt.weapons.state = state;
        (hunt.zoom(), hunt.scope_cross())
    };
    // The original: 3x and the cross, whatever `optic` says.
    assert_eq!(scoped(&def(1.0, &[]), WeaponState::Ready), (3.0, true));
    assert_eq!(scoped(&def(0.5, &[]), WeaponState::Ready), (1.0, false));
    // The dialect: its own zoom, no cross unless asked.
    let rifle = def(1.1, &[("recoil", "0")]);
    assert_eq!(scoped(&rifle, WeaponState::Ready), (1.1, false));
    let sniper = def(3.0, &[("cross", "TRUE"), ("unzoom", "TRUE")]);
    assert_eq!(scoped(&sniper, WeaponState::Ready), (3.0, true));
    assert_eq!(scoped(&sniper, WeaponState::Reloading), (1.0, false));
    // A cross without a scope: drawn at the hip.
    assert_eq!(
        scoped(&def(0.0, &[("cross", "TRUE")]), WeaponState::Ready),
        (1.0, true)
    );
}

#[test]
fn resupply_refills_carried_weapons_only() {
    let mut w = Weapons::new(
        vec![rifle(), rifle()],
        vec![WeaponAnims::uniform(100); 2],
        Bits::bit(0),
        true,
    );
    w.shots_left = vec![0, 0];
    w.ammo_mag = vec![0, 0];
    w.resupply();
    assert_eq!(w.shots_left, vec![14, 0]);
    assert_eq!(w.ammo_mag, vec![1, 0]);
}

#[test]
fn weapons_past_slot_32_get_ammo() {
    let n = 50;
    let mut w = Weapons::new(
        vec![rifle(); n],
        vec![WeaponAnims::uniform(100); n],
        Bits::bit(40) | Bits::bit(2),
        false,
    );
    assert_eq!(w.shots_left[40], 14);
    assert!(w.carried(40) && w.carried(2) && !w.carried(3));
    w.shots_left[40] = 0;
    w.resupply();
    assert_eq!(w.shots_left[40], 14);
}

#[test]
fn crouching_steadies_aim_and_standing_restores_sway() {
    let (_, mut h, _) = setup(false);
    h.weapons.shakel = 4.0;
    h.player.head_y = crate::player::HEAD_STAND;
    assert_ne!(h.weapon_sway(), (0.0, 0.0));
    h.player.head_y = 110.0;
    assert_eq!(h.weapon_sway(), (0.0, 0.0));
    h.player.head_y = crate::player::HEAD_STAND;
    assert_ne!(h.weapon_sway(), (0.0, 0.0));
}

#[test]
fn dialect_weapons_fly_kick_track_and_fire_once_per_pull() {
    let res = c2_formats::res_txt::ResTxt::parse(
        "weapons {\n{\n name = 'Rifle'\n land_power = 4\n land_veloc = 23.0\n land_prec = 1.8\n \
         shots = 7\n recoil = 4\n shake = 0.05\n radar = TRUE\n radR = 255\n radG = 0\n \
         radB = 0\n radTime = 1000\n land_fall = 0.6\n}\n}\n.\n",
    )
    .unwrap_or_default();
    let block = crate::mee::vanilla_keys_for_test(&res.sections[0].blocks[0]);
    let info = WeaponInfo::from_def(&WeaponDef::from_block(&block));
    let m = info.mee.clone().expect("dialect keys");
    assert_eq!((m.veloc, m.fall, m.recoil), (23.0, 0.6, 4.0));
    assert_eq!(m.radar, Some(([255, 0, 0], 1000)));
    assert!(!info.fall, "land_fall isn't the crossbow flag");

    let (area, mut h, car) = setup(false);
    h.weapons = Weapons::new(
        vec![info],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        false,
    );
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    let toggle = HuntInput {
        toggle_weapon: true,
        ..Default::default()
    };
    weapon_ticks(&mut h, &area, &cars, toggle, 1, &mut ev);
    assert!((h.weapons.shakel - 0.2).abs() < 0.01, "shake * 4");
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    let fire = HuntInput {
        fire: true,
        ..Default::default()
    };
    let beta = h.player.beta;
    let health = h.herd.chars[0].health;
    weapon_ticks(&mut h, &area, &cars, fire, 1, &mut ev);
    assert_eq!(h.bullets.len(), 1, "a round in flight, not a trace");
    // 2000 units off at 1472 a 17 ms step: there within a few ticks.
    weapon_ticks(&mut h, &area, &cars, fire, 4, &mut ev);
    assert!(h.bullets.is_empty());
    assert!(h.herd.chars[0].health < health, "hit");
    assert!(h.herd.chars[0].tracked.is_some(), "tracked");
    assert!(h.player.beta < beta, "the recoil lifts the aim");
    // Held past the fire animation: no second shot until released.
    weapon_ticks(&mut h, &area, &cars, fire, 20, &mut ev);
    assert_eq!(h.weapons.rounds(0), 6);
    assert_eq!(h.weapons.chambered[0], 1, "the next round chambered");
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 1, &mut ev);
    weapon_ticks(&mut h, &area, &cars, fire, 1, &mut ev);
    assert_eq!(h.weapons.rounds(0), 5);
    // The tracker runs out after its second.
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 120, &mut ev);
    assert!(h.herd.chars[0].tracked.is_none());
}

#[test]
fn a_loaded_one_by_one_weapon_reloads_by_hand_in_part_or_full() {
    // Two rounds a load (`reload = 2`), a part reload animation.
    let (area, mut h, car) = chamber_weapon(
        " reload = 2\n rldAnimFull = 3\n rldAnimPart = 4\n",
        &[weapon_anim::PART],
    );
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    assert_eq!((h.weapons.chambered[0], h.weapons.shots_left[0]), (2, 4));
    shoot(&mut h, &area, &car, &mut ev);
    assert_eq!(h.weapons.chambered[0], 1, "no round from the reserve");
    // One left in: a part reload, one round.
    let reload = HuntInput {
        reload: true,
        ..Default::default()
    };
    weapon_ticks(&mut h, &area, &cars, reload, 1, &mut ev);
    assert_eq!(h.weapons.state, WeaponState::PartReload);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!((h.weapons.chambered[0], h.weapons.shots_left[0]), (2, 3));
    // Both fired: empty, the trigger clicks.
    shoot(&mut h, &area, &car, &mut ev);
    shoot(&mut h, &area, &car, &mut ev);
    assert_eq!(h.weapons.chambered[0], 0);
    ev.clear();
    shoot(&mut h, &area, &car, &mut ev);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Click(_))));
    // Empty with a full load left: the full reload.
    weapon_ticks(&mut h, &area, &cars, reload, 1, &mut ev);
    assert_eq!(h.weapons.state, WeaponState::Reloading);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!((h.weapons.chambered[0], h.weapons.shots_left[0]), (2, 1));
}

#[test]
fn a_rack_weapon_racks_after_each_shot_and_switches_mode() {
    let (area, mut h, car) = chamber_weapon(
        " rckAnim = 5\n mustRack = TRUE\n semiAuto = TRUE\n fullAuto = TRUE\n modAnim = 6\n",
        &[weapon_anim::RACK, weapon_anim::MODE],
    );
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    assert_eq!(h.weapons.fire_mode[0], 1, "full auto to start with");
    shoot(&mut h, &area, &car, &mut ev);
    assert_eq!(h.weapons.chambered[0], 0, "must rack");
    let rack = HuntInput {
        rack: true,
        ..Default::default()
    };
    weapon_ticks(&mut h, &area, &cars, rack, 1, &mut ev);
    assert_eq!(h.weapons.state, WeaponState::Racking);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!((h.weapons.chambered[0], h.weapons.shots_left[0]), (1, 4));
    let switch = HuntInput {
        switch_mode: true,
        ..Default::default()
    };
    weapon_ticks(&mut h, &area, &cars, switch, 1, &mut ev);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!(h.weapons.fire_mode[0], 0, "semi auto");
}

#[test]
fn a_magazine_weapon_swaps_for_its_spare_and_auto_reloads() {
    let (area, mut h, car) = chamber_weapon(" rldAnimFull = 3\n autoReload = TRUE\n", &[]);
    let mut ev = Vec::new();
    h.weapons.ammo_mag[0] = 1;
    h.weapons.mag_shots[0] = 6;
    for _ in 0..6 {
        shoot(&mut h, &area, &car, &mut ev);
    }
    assert_eq!(h.weapons.state, WeaponState::Reloading, "on its own");
    let cars = [Some(&car), Some(&car)];
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    // The magazine ran dry: the spare went in on its own.
    assert_eq!(h.weapons.ammo_mag[0], 0);
    assert_eq!((h.weapons.chambered[0], h.weapons.shots_left[0]), (1, 5));
}

#[test]
fn a_harpoon_fires_under_water_with_its_own_numbers_and_a_bolt_is_picked_up() {
    let (area, mut h, car) = chamber_weapon(
        " harpoon = TRUE\n aqua_power = 9\n aqua_prec = 1.5\n retrieve = TRUE\n",
        &[],
    );
    let m = h.weapons.mee().cloned().expect("dialect");
    assert_eq!((m.aqua.power, m.aqua.prec), (9.0, 1.5));
    assert_eq!(m.aqua.veloc, 0.0, "the land number where not given");
    let mut ev = Vec::new();
    h.player.underwater = true;
    shoot(&mut h, &area, &car, &mut ev);
    assert_ne!(h.weapons.state, WeaponState::Hiding, "kept out under water");
    assert_eq!(h.weapons.rounds(0), 5);
    // A bolt lying by the hunter is picked up again.
    h.player.underwater = false;
    h.bullets.push(Bullet {
        pos: [h.player.x + 100.0, h.player.y, h.player.z],
        vel: [0.0; 3],
        land_vel: [0.0; 3],
        fall_total: 0.0,
        aq_state: 0,
        stuck: true,
        ftime: 0,
        weapon: 0,
        dart: false,
    });
    let cars = [Some(&car), Some(&car)];
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 1, &mut ev);
    assert!(h.bullets.is_empty());
    assert_eq!(h.weapons.rounds(0), 6);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Collect(_))));
}

#[test]
fn draw_fire_and_ammo() {
    let (area, mut h, car) = setup(false);
    let cars = [Some(&car), Some(&car)];
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
    assert_eq!(h.weapons.state, WeaponState::Drawing);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!(h.weapons.state, WeaponState::Ready);
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
    assert_eq!(h.weapons.shots_left[0], 13);
    assert!(ev.contains(&GameEvent::WeaponSound {
        weapon: 0,
        sound: weapon_anim::FIRE
    }));
    // Held fire doesn't re-fire until the fire animation finishes.
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            fire: true,
            ..Default::default()
        },
        5,
        &mut ev,
    );
    assert_eq!(h.weapons.shots_left[0], 13);
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            fire: true,
            ..Default::default()
        },
        12,
        &mut ev,
    );
    assert_eq!(h.weapons.shots_left[0], 12);
}

/// A dialect weapon from its `_RES.TXT` keys, drawn and ready, its
/// animations 100 ms (those in `extra` too).
fn chamber_weapon(keys: &str, extra: &[usize]) -> (Area, Hunt, Car) {
    let res = c2_formats::res_txt::ResTxt::parse(&format!(
        "weapons {{\n{{\n name = 'Gun'\n land_power = 4\n shots = 6\n getAnim = 0\n{keys}}}\n}}\n.\n"
    ))
    .unwrap_or_default();
    let block = crate::mee::vanilla_keys_for_test(&res.sections[0].blocks[0]);
    let info = WeaponInfo::from_def(&WeaponDef::from_block(&block));
    let mut a = WeaponAnims::uniform(100);
    a.ms[weapon_anim::RELOAD] = if info.anim_of(weapon_anim::RELOAD).is_some() {
        100
    } else {
        0
    };
    for &slot in extra {
        a.ms[slot] = 100;
    }
    let (area, mut h, car) = setup(false);
    h.weapons = Weapons::new(vec![info], vec![a], Bits::bit(0), false);
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    let toggle = HuntInput {
        toggle_weapon: true,
        ..Default::default()
    };
    weapon_ticks(&mut h, &area, &cars, toggle, 1, &mut ev);
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    assert_eq!(h.weapons.state, WeaponState::Ready);
    (area, h, car)
}

/// Fires once and lets the fire animation finish.
fn shoot(h: &mut Hunt, area: &Area, car: &Car, ev: &mut Vec<GameEvent>) {
    let cars = [Some(car), Some(car)];
    let fire = HuntInput {
        fire: true,
        ..Default::default()
    };
    weapon_ticks(h, area, &cars, fire, 1, ev);
    weapon_ticks(h, area, &cars, HuntInput::default(), 12, ev);
}

#[test]
fn the_map_puts_the_gun_away_only_when_asked() {
    for holsters in [false, true] {
        let (area, mut h, _) = setup(false);
        h.opts.map_holsters = holsters;
        let mut ev = Vec::new();
        let tick = |h: &mut Hunt, input: HuntInput, n: usize, ev: &mut Vec<GameEvent>| {
            for _ in 0..n {
                h.tick(&area, &[], &input, 10, ev);
            }
        };
        let press = |weapon: bool| HuntInput {
            toggle_weapon: weapon,
            toggle_map: !weapon,
            ..Default::default()
        };
        tick(&mut h, press(true), 1, &mut ev);
        tick(&mut h, HuntInput::default(), 40, &mut ev);
        assert_eq!(h.weapons.state, WeaponState::Ready);
        tick(&mut h, press(false), 1, &mut ev);
        tick(&mut h, HuntInput::default(), 40, &mut ev);
        if holsters {
            assert_eq!(h.weapons.state, WeaponState::Hidden);
            assert!(h.map_mode, "the map opens once the gun is away");
        } else {
            // The original: no map with a weapon out.
            assert_eq!(h.weapons.state, WeaponState::Ready);
            assert!(!h.map_mode);
            continue;
        }
        // The gun again: the map closes as it comes out.
        tick(&mut h, press(true), 1, &mut ev);
        assert!(!h.map_mode);
        tick(&mut h, HuntInput::default(), 40, &mut ev);
        assert_eq!(h.weapons.state, WeaponState::Ready);
        assert!(!h.map_pending);
    }
}
