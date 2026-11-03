use super::support::{info, setup};
use crate::dino::{ai, Character, Species};
use crate::game::{
    dangerous, GameEvent, Hunt, HuntInput, SurvivalKind, SurvivalOptions, MAX_HEALTH, WAVE_HEALTH,
};
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use c2_formats::bits::Bits;

#[test]
fn survival_sends_growing_waves_and_awards_no_trophies() {
    let (area, mut hunt, car) = setup(false);
    hunt.herd.species.push(Species::new(info(ai::VELO), &car));
    assert!(dangerous(&hunt.herd.species[2]) && !dangerous(&hunt.herd.species[1]));
    hunt.start_survival(
        &area,
        SurvivalOptions {
            roster: vec![2],
            max_wave: 3,
            ..Default::default()
        },
    );
    // The plant-eater stays; the first wave comes after a rest.
    assert_eq!(hunt.herd.chars.len(), 1);
    let cars = [None, Some(&car), Some(&car)];
    let mut out = Vec::new();
    let wave = |hunt: &Hunt, n: u32| hunt.herd.chars.iter().filter(|c| c.wave == n).count();
    let run = |hunt: &mut Hunt, ms: i32, out: &mut Vec<GameEvent>| {
        for _ in 0..ms / 10 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, out);
        }
    };
    run(&mut hunt, 3100, &mut out);
    assert_eq!((hunt.wave(), wave(&hunt, 1)), (1, 3));
    assert!(out
        .iter()
        .any(|e| matches!(e, GameEvent::Message(m) if m == "Wave 1")));
    for c in hunt.herd.chars.iter().filter(|c| c.wave == 1) {
        assert!(c.aware_hunter && c.state != 0);
        assert!(
            (1..=WAVE_HEALTH).contains(&c.health),
            "a shotgun blast kills"
        );
        let d = (c.pos[0] - hunt.player.x).hypot(c.pos[2] - hunt.player.z);
        assert!(d >= 20.0 * 256.0, "{d}");
    }
    // Killed: logged, but no score, trophy or ship.
    while let Some(i) = hunt
        .herd
        .chars
        .iter()
        .position(|c| c.wave == 1 && c.health > 0)
    {
        hunt.debug_bag(&area, i, &mut out);
    }
    assert_eq!((hunt.kills.len(), hunt.room.score), (3, 0));
    assert!(hunt.ship.tasks.is_empty() && hunt.room.bodies.iter().all(|b| b.ctype == 0));
    // The next wave (capped at 3) replaces the fallen; health is back.
    hunt.health = 1000;
    run(&mut hunt, 4100, &mut out);
    assert_eq!((hunt.wave(), wave(&hunt, 1), wave(&hunt, 2)), (2, 0, 3));
    assert_eq!(hunt.health, MAX_HEALTH);
}

#[test]
fn modded_survival_waves_buy_the_dearest_first() {
    let w = crate::game::survival::mee_waves(&[7, 8, 9]);
    assert_eq!(w.len(), 26);
    // 2, 4, 8, 16 points; costs 1, 3, 9.
    assert_eq!(w[0], [7, 7]);
    assert_eq!(w[1], [8, 7]);
    assert_eq!(w[2], [8, 8, 7, 7]);
    assert_eq!(w[3], [9, 8, 8, 7]);
    let spot = c2_formats::mee::SurvivalSpot {
        hunter: (345, 245),
        angle: 90.0,
        spawn: [318, 336, 170, 180],
    };
    let o = SurvivalOptions::mee(&[7, 8, 9], &spot, 6).expect("roster");
    let m = o.mobile.expect("waves");
    assert!(o.unlimited);
    assert_eq!(m.hunter, [345.0 * 256.0 + 128.0, 245.0 * 256.0 + 128.0]);
    assert!((m.spawn[0] - (327.0 * 256.0 + 128.0)).abs() < 1.0);
    assert!((m.hunter_alpha.unwrap_or(0.0) - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!(SurvivalOptions::mee(&[], &spot, 6).is_none());
}

#[test]
fn mobile_survival_on_ice_age_takes_stand_ins_round_the_hunter() {
    let (area, mut hunt, car) = setup(false);
    for (name, a) in [("Smilodon", ai::ALLO), ("Wooly rhinoceros", ai::CHASM)] {
        let mut i = info(a);
        i.name = name.into();
        hunt.herd.species.push(Species::new(i, &car));
    }
    let sv = c2_formats::mobile_arcade::Survive {
        area: 5,
        hunter: [100_000.0, 100_000.0],
        spawn: [100_000.0, 110_000.0, 500.0, 500.0],
        first_ms: 500,
        rest_ms: 2000,
        waves: vec![vec!["Allosaurus".into(), "Chasmosaurus".into()]],
        after: vec!["Allosaurus".into()],
        stand_ins: false,
    };
    let start = (hunt.player.x, hunt.player.z);
    let o = SurvivalOptions::mobile(&hunt, &sv, 8).expect("stand-ins");
    let m = o.mobile.clone().unwrap_or_default();
    assert!(m.stand_ins);
    assert_eq!(m.waves, vec![vec![2, 3]]);
    hunt.start_survival(&area, o);
    // Not moved to Mount Ravan's spot.
    assert_eq!((hunt.player.x, hunt.player.z), start);
    let cars = [None, Some(&car), Some(&car), Some(&car)];
    let mut out = Vec::new();
    for _ in 0..60 {
        hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    }
    let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
    assert_eq!(wave.len(), 2);
    // Round the hunter, not in the box.
    assert!(wave.iter().all(|c| (c.pos[2] - 110_000.0).abs() > 500.0));
}

#[test]
fn mobile_survival_sends_its_waves_from_its_box() {
    let (area, mut hunt, car) = setup(false);
    for (name, a) in [("Chasmosaurus", ai::CHASM), ("Allosaurus", ai::ALLO)] {
        let mut i = info(a);
        i.name = name.into();
        hunt.herd.species.push(Species::new(i, &car));
    }
    let sv = c2_formats::mobile_arcade::Survive {
        area: 5,
        hunter: [100_000.0, 100_000.0],
        spawn: [100_000.0, 110_000.0, 500.0, 500.0],
        first_ms: 500,
        rest_ms: 2000,
        waves: vec![vec!["Chasmosaurus".into()], vec!["Allosaurus".into(); 2]],
        after: vec!["Chasmosaurus".into(); 3],
        stand_ins: false,
    };
    let missing = c2_formats::mobile_arcade::Survive {
        after: vec!["Utahraptor".into()],
        ..sv.clone()
    };
    assert!(SurvivalOptions::mobile(&hunt, &missing, 8).is_none());
    let o = SurvivalOptions::mobile(&hunt, &sv, 8).unwrap();
    hunt.start_survival(&area, o);
    assert!((hunt.player.x - 100_000.0).abs() < 256.0);
    let cars = [None, Some(&car), Some(&car), Some(&car)];
    let mut out = Vec::new();
    for _ in 0..60 {
        hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    }
    let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
    assert_eq!(wave.len(), 1);
    let c = wave[0];
    assert_eq!(c.ctype, 2);
    assert!((c.pos[2] - 110_000.0).abs() <= 500.0 && (c.pos[0] - 100_000.0).abs() <= 500.0);
    // 39 cells off on open ground, the Chasmosaurus comes straight on
    // (no lure: the straight line is a way; walls: `flow_tests`).
    assert_eq!(c.lure, None);
    assert!((c.tgz - hunt.player.z).abs() < 1.0 && (c.tgx - hunt.player.x).abs() < 1.0);
    // Then the next wave in turn, 2 s after this one falls.
    hunt.debug_bag(
        &area,
        hunt.herd.chars.iter().position(|c| c.wave == 1).unwrap(),
        &mut out,
    );
    for _ in 0..210 {
        hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    }
    let w2: Vec<usize> = hunt
        .herd
        .chars
        .iter()
        .filter(|c| c.wave == 2)
        .map(|c| c.ctype)
        .collect();
    assert_eq!(w2, [3, 3]);
}

#[test]
fn box_survival_waves_keep_to_the_ravine_floor() {
    let (mut area, mut hunt, car) = setup(false);
    // High ground everywhere, a ravine floor (cells x 380..400) running
    // from the spawn box (z ~ 400) down to the hunter (z ~ 500).
    for z in 0..1024 {
        for x in 0..1024 {
            let floor = (380..400).contains(&x) && (360..540).contains(&z);
            area.map.height[z * 1024 + x] = if floor { 10 } else { 60 };
        }
    }
    let mut i = info(ai::CHASM);
    i.name = "Chasmosaurus".into();
    hunt.herd.species.push(Species::new(i, &car));
    let sv = c2_formats::mobile_arcade::Survive {
        area: 5,
        hunter: [390.0 * 256.0, 500.0 * 256.0],
        spawn: [390.0 * 256.0, 400.0 * 256.0, 1000.0, 1000.0],
        first_ms: 500,
        rest_ms: 2000,
        waves: vec![vec!["Chasmosaurus".into(); 3]],
        after: vec![],
        stand_ins: false,
    };
    hunt.start_survival(&area, SurvivalOptions::mobile(&hunt, &sv, 8).unwrap());
    let cars = [None, Some(&car), Some(&car)];
    let mut out = Vec::new();
    for _ in 0..60 {
        hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    }
    let i = hunt
        .herd
        .chars
        .iter()
        .position(|c| c.wave == 1)
        .expect("a wave");
    let on_floor = |c: &Character| (380.0 * 256.0..400.0 * 256.0).contains(&c.pos[0]);
    assert!(on_floor(&hunt.herd.chars[i]));
    // Pushed up the wall: back on the floor next step.
    hunt.herd.chars[i].pos[0] = 420.0 * 256.0;
    hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    assert!(
        on_floor(&hunt.herd.chars[i]),
        "{}",
        hunt.herd.chars[i].pos[0] / 256.0
    );
}

#[test]
fn survival_waves_start_by_the_hunter_not_up_a_mountain() {
    let (mut area, mut hunt, car) = setup(false);
    // The box's west half a plateau far above the hunter.
    for z in 0..1024 {
        for x in 0..388 {
            area.map.height[z * 1024 + x] = 250;
        }
    }
    let mut i = info(ai::CHASM);
    i.name = "Chasmosaurus".into();
    hunt.herd.species.push(Species::new(i, &car));
    let sv = c2_formats::mobile_arcade::Survive {
        area: 5,
        hunter: [100_000.0, 100_000.0],
        spawn: [100_000.0, 106_000.0, 2000.0, 500.0],
        first_ms: 500,
        rest_ms: 2000,
        waves: vec![vec!["Chasmosaurus".into(); 6]],
        after: vec![],
        stand_ins: false,
    };
    hunt.start_survival(&area, SurvivalOptions::mobile(&hunt, &sv, 8).unwrap());
    let cars = [None, Some(&car), Some(&car)];
    let mut out = Vec::new();
    for _ in 0..60 {
        hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
    }
    let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
    assert_eq!(wave.len(), 6);
    assert!(
        wave.iter().all(|c| c.pos[0] >= 388.0 * 256.0),
        "none on the plateau"
    );
}

#[test]
fn survival_picks_waves_mods_and_the_t_rex() {
    let (_, mut hunt, car) = setup(false);
    for (name, a) in [
        ("Allosaurus", ai::ALLO),
        ("Utahraptor", ai::VELO),
        ("T-Rex", ai::TREX),
    ] {
        let mut i = info(a);
        i.name = name.into();
        hunt.herd.species.push(Species::new(i, &car));
    }
    // Nothing hunting picked: every hunter but the T-Rex; the mod's
    // animal is the hardcore extra.
    let o = SurvivalOptions::pick(&hunt, SurvivalKind::Hardcore, 5);
    assert_eq!((o.roster, o.hardcore, o.trex), (vec![2, 3], vec![3], None));
    // The allosaur picked: it alone comes; custom brings the T-Rex.
    hunt.opts.targets = Bits::bit(ai::ALLO as usize);
    let o = SurvivalOptions::pick(&hunt, SurvivalKind::Custom, 5);
    assert_eq!((o.roster, o.trex, o.unlimited), (vec![2], Some(4), true));
}
