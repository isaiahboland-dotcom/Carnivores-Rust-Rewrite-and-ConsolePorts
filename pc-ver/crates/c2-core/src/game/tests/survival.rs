use super::support::{info, setup};
use crate::dino::{ai, Character, Species};
use crate::game::{
    dangerous, Arena, GameEvent, Hunt, HuntInput, SurvivalKind, SurvivalOptions, MAX_HEALTH,
    WAVE_HEALTH,
};
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

#[test]
fn budget_survival_filters_rosters_and_spends_without_touching_hunter_points() {
    let (area, mut hunt, car) = setup(false);
    for (name, file, cost) in [
        ("Chasmosaurus", "chasmosa.car", 18.0),
        ("Velociraptor", "velo2.car", 25.0),
        ("Mod raptor", "mods/raptor.car", 12.0),
        ("Free raptor", "free.car", 0.0),
        ("Broken raptor", "broken.car", -1.0),
    ] {
        let mut i = info(ai::VELO);
        i.name = name.into();
        i.file = file.into();
        i.base_score = 999.0;
        let mut sp = Species::new(i, &car);
        sp.hunt_price = Some(cost as i32);
        hunt.herd.species.push(sp);
    }
    let classic = SurvivalOptions::pick(&hunt, SurvivalKind::Classic, 6);
    assert_eq!(classic.roster, [(2, 18), (3, 25)]);
    let modded = SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 6);
    assert_eq!(modded.roster, [(2, 18), (3, 25), (4, 12), (5, 0)]);
    // Hardcore's waves are Modded's (only the night and the kit differ).
    let hardcore = SurvivalOptions::pick(&hunt, SurvivalKind::Hardcore, 6);
    assert_eq!(hardcore.roster, modded.roster);
    assert!(SurvivalKind::Hardcore.endless_night() && !SurvivalKind::Modded.endless_night());
    assert_eq!(SurvivalKind::Hardcore.weapon().0, "rifle.car");
    assert!(!SurvivalKind::ALL.contains(&SurvivalKind::Hardcore));
    assert!(classic.budget_ready());
    assert_eq!(classic.initial_budget(), 36);
    assert_eq!(modded.initial_budget(), 10);
    hunt.room.score = 987;
    hunt.start_survival(&area, classic);
    // Budget waves retain the mobile arena and face its fixed approach.
    assert!((hunt.player.x - 89330.13).abs() <= 128.0);
    assert!((hunt.player.z - 65388.617).abs() <= 128.0);
    assert!(hunt.player.alpha < 0.0);
    assert!(hunt.herd.chars.is_empty());
    let mut events = Vec::new();
    hunt.survival.as_mut().unwrap().rest_ms = 0;
    hunt.survival_step(&area, 10, &mut events);
    assert_eq!(hunt.wave(), 1);
    assert!((1..=2).contains(&hunt.herd.chars.len()));
    assert!(hunt.herd.chars.iter().all(|c| matches!(c.ctype, 2 | 3)));
    assert!(hunt
        .herd
        .chars
        .iter()
        .all(|c| (c.pos[0] - 84069.125).abs() <= 3000.0
            && (c.pos[2] - 45230.77).abs() <= 2000.0
            && c.pos[2] < hunt.player.z));
    assert_eq!(hunt.room.score, 987);
    // Insufficient funds never buy an expensive animal or silently skip a round.
    hunt.herd.chars.clear();
    let s = hunt.survival.as_mut().unwrap();
    s.wave = 0;
    s.rest_ms = 0;
    s.opts.roster = vec![(3, 31)];
    s.opts.starting_budget = 30;
    assert!(!s.opts.budget_ready());
    hunt.survival_step(&area, 10, &mut events);
    assert_eq!(hunt.wave(), 0);
    assert!(hunt.herd.chars.is_empty());
}

#[test]
fn budget_survival_requires_a_working_dialect_attack() {
    let (_, mut hunt, car) = setup(false);
    let mut i = info(ai::VELO);
    i.base_score = 18.0;
    let mut sp = Species::new(i, &car);
    sp.hunt_price = Some(18);
    sp.mee = Some(alloc::boxed::Box::new(crate::dino::mee::MeeInfo {
        ai: ai::VELO,
        kill_dist: 0,
        run_speed: 0.8,
        ..crate::dino::mee::MeeInfo::new(ai::VELO)
    }));
    hunt.herd.species.push(sp);
    assert!(!SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 6).budget_ready());
    hunt.herd.species[2].mee.as_mut().unwrap().kill_dist = 25;
    assert!(SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 6).budget_ready());
    hunt.herd.species[2].ani_time.clear();
    assert!(!SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 6).budget_ready());
}

#[test]
fn survival_sends_growing_waves_and_awards_no_trophies() {
    let (area, mut hunt, car) = setup(false);
    hunt.herd.species.push(Species::new(info(ai::VELO), &car));
    assert!(dangerous(&hunt.herd.species[2]) && !dangerous(&hunt.herd.species[1]));
    hunt.start_survival(
        &area,
        SurvivalOptions {
            roster: vec![(2, 0)],
            max_wave: 3,
            arena: open_arena(),
            ..Default::default()
        },
    );
    // Even plant-eaters go: only the wave roster may populate survival.
    assert!(hunt.herd.chars.is_empty());
    hunt.add_secondary_one(&area, 1);
    assert!(
        hunt.herd.chars.is_empty(),
        "no ambient replacements in survival"
    );
    let cars = [None, Some(&car), Some(&car)];
    let mut out = Vec::new();
    let wave = |hunt: &Hunt, n: u32| hunt.herd.chars.iter().filter(|c| c.wave == n).count();
    let run = |hunt: &mut Hunt, ms: i32, out: &mut Vec<GameEvent>| {
        for _ in 0..ms / 10 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, out);
        }
    };
    run(&mut hunt, 600, &mut out);
    assert_eq!((hunt.wave(), wave(&hunt, 1)), (1, 3));
    assert!(out
        .iter()
        .any(|e| matches!(e, GameEvent::Message(m) if m.starts_with("Wave 1"))));
    for c in hunt.herd.chars.iter().filter(|c| c.wave == 1) {
        assert!(c.aware_hunter && c.state != 0);
        assert!(
            (1..=WAVE_HEALTH).contains(&c.health),
            "a shotgun blast kills"
        );
        assert!(open_arena().spawn[1] - c.pos[2] <= 1024.0, "from the box");
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
    run(&mut hunt, 2100, &mut out);
    assert_eq!((hunt.wave(), wave(&hunt, 1), wave(&hunt, 2)), (2, 0, 3));
    assert_eq!(hunt.health, MAX_HEALTH);
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
    let arena = Arena {
        hunter: [390.0 * 256.0, 500.0 * 256.0],
        spawn: [390.0 * 256.0, 400.0 * 256.0, 1000.0, 1000.0],
        ..Arena::ravine()
    };
    hunt.start_survival(&area, priced(3, arena));
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
    let arena = Arena {
        hunter: [100_000.0, 100_000.0],
        spawn: [100_000.0, 106_000.0, 2000.0, 500.0],
        ..Arena::ravine()
    };
    hunt.start_survival(&area, priced(6, arena));
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
fn blocked_budget_survival_box_never_falls_back_behind_the_hunter_or_skips_a_wave() {
    let (mut area, mut hunt, car) = setup(false);
    // Entire spawn box is on a plateau with no route down to the hunter.
    for z in 0..1024 {
        for x in 0..388 {
            area.map.height[z * 1024 + x] = 250;
        }
    }
    hunt.herd.species.push(Species::new(info(ai::ALLO), &car));
    hunt.start_survival(
        &area,
        SurvivalOptions {
            max_wave: 3,
            roster: vec![(2, 18)],
            arena: Arena {
                hunter: [400.0 * 256.0, 400.0 * 256.0],
                spawn: [380.0 * 256.0, 400.0 * 256.0, 512.0, 512.0],
                first_ms: 0,
                rest_ms: 100,
                ..Arena::ravine()
            },
            ..Default::default()
        },
    );
    let mut out = Vec::new();
    for _ in 0..25 {
        hunt.tick(
            &area,
            &[None, Some(&car), Some(&car)],
            &HuntInput::default(),
            10,
            &mut out,
        );
    }
    assert!(
        hunt.herd.chars.is_empty(),
        "no ring fallback outside the box"
    );
    assert_eq!(hunt.wave(), 0, "blocked first wave remains pending");
    assert!(out
        .iter()
        .any(|e| matches!(e, GameEvent::Message(m) if m.contains("spawn blocked"))));
    let before = (hunt.player.x, hunt.player.z);
    let input = HuntInput {
        controls: crate::player::Controls {
            strafe_right: true,
            ..Default::default()
        },
        ..Default::default()
    };
    for _ in 0..20 {
        hunt.tick(&area, &[None, Some(&car), Some(&car)], &input, 10, &mut out);
    }
    assert_ne!(
        (hunt.player.x, hunt.player.z),
        before,
        "survival still allows dodging"
    );
}

#[test]
fn survival_menu_prices_override_scores_and_roamers_use_their_ai_donor() {
    use c2_formats::{menu_txt::MenuData, res_txt::ResTxt};
    let (_, mut hunt, car) = setup(false);
    let menu = MenuData::from_res(&ResTxt::parse(
        "characters {\n{\n name = 'First'\n ai = 15\n price = 3\n}\n{\n name = 'Second'\n ai = 15\n price = 4\n}\n}\nprices {\n dino = 50\n dino = 100\n survival_start = 150\n}\n"
    ).unwrap());
    assert_eq!(menu.survival_budget, 150);
    for slot in [0, 1, -1] {
        let mut i = info(ai::VELO);
        i.base_score = 999.0;
        let mut sp = Species::new(i, &car);
        sp.tuning.menu_slot = Some(slot);
        hunt.herd.species.push(sp);
    }
    crate::game::price_species(&mut hunt.herd.species, &menu);
    assert_eq!(hunt.herd.species[2].hunt_price, Some(50));
    assert_eq!(hunt.herd.species[3].hunt_price, Some(100));
    assert_eq!(hunt.herd.species[4].hunt_price, Some(50));
    let mut opts = SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 6);
    assert_eq!(opts.roster, [(2, 50), (3, 100), (4, 50)]);
    assert_eq!(opts.initial_budget(), 100);
    assert_eq!(opts.wave_budget(2), 110);
    opts.starting_budget = menu.survival_budget;
    assert_eq!(opts.wave_budget(1), 150);
    assert_eq!(opts.wave_budget(2), 165);
    opts.starting_budget = 20;
    assert!(!opts.budget_ready());
}

#[test]
fn survival_free_animals_keep_their_price_and_are_bounded_by_wave_cap() {
    let (area, mut hunt, car) = setup(false);
    let mut sp = Species::new(info(ai::VELO), &car);
    sp.hunt_price = Some(0);
    hunt.herd.species.push(sp);
    let opts = SurvivalOptions::pick(&hunt, SurvivalKind::Modded, 3);
    assert_eq!(opts.initial_budget(), 10);
    assert!(opts.budget_ready());
    hunt.start_survival(&area, opts);
    hunt.survival.as_mut().unwrap().rest_ms = 0;
    hunt.survival_step(&area, 10, &mut Vec::new());
    assert_eq!(hunt.wave(), 1);
    assert_eq!(hunt.herd.chars.len(), 3);
}

/// Flat test ground: the hunter in the middle, the box 16 cells north.
fn open_arena() -> Arena {
    Arena {
        hunter: [512.0 * 256.0, 512.0 * 256.0],
        spawn: [512.0 * 256.0, 496.0 * 256.0, 512.0, 512.0],
        ..Arena::ravine()
    }
}

/// Every wave animal (character type 2) free, up to `max_wave` a wave.
fn priced(max_wave: usize, arena: Arena) -> SurvivalOptions {
    SurvivalOptions {
        roster: vec![(2, 0)],
        max_wave,
        arena,
        unlimited: true,
        ..Default::default()
    }
}
