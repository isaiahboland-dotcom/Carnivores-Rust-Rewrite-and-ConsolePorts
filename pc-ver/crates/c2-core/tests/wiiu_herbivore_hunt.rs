#![cfg(feature = "portable-load")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Wii U's hunts of the original game, loaded as the console loads them
//! (`hunt_setup::load` through an install over C2_ORIGINAL_HUNTDAT's game
//! folder).
extern crate alloc;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::large_enum_variant,
    clippy::chunks_exact_to_as_chunks
)]
#[path = "../../../../wiiu-ver/perf-probe/src/hunt_setup.rs"]
mod hunt_setup;
#[allow(dead_code)]
#[rustfmt::skip]
#[allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::large_enum_variant,
    clippy::chunks_exact_to_as_chunks
)]
#[path = "../../../../wiiu-ver/perf-probe/src/install.rs"]
mod install;
mod wiiu_common;
use c2_core::{
    area::{Area, TimeOfDay},
    game::{GameEvent, Hunt, HuntInput},
    model::PreparedModel,
};
use c2_formats::{car::Car, map, rsc};
use hunt_setup::{HuntChoice, Loaded};

/// This port's population for the original game's data.
const POPULATION: usize = hunt_setup::AMBIENT_POPULATION + hunt_setup::TARGET_POPULATION;
/// The hunter's character (the original's first).
const HUNTER: usize = 0;

fn load(inst: &install::Install, choice: &HuntChoice, seed: u32) -> Loaded {
    hunt_setup::load(Some(inst), choice, &hunt_setup::new_save(), seed)
        .unwrap_or_else(|e| panic!("load error {e}"))
}

/// A new hunt like `hunt` (`hunt_setup::restart` as main.c calls it).
fn restart(l: &Loaded, hunt: &Hunt, choice: &HuntChoice, seed: u32) -> Hunt {
    hunt_setup::restart(
        &l.area,
        hunt,
        choice,
        l.tables.as_ref(),
        l.targets,
        &l.roamers,
        &hunt_setup::new_save(),
        seed,
    )
    .unwrap()
}

fn refs(cars: &[Option<Car>]) -> Vec<Option<&Car>> {
    cars.iter().map(Option::as_ref).collect()
}

#[test]
fn wiiu_roster_on_all_five_original_maps() {
    let Some((root, inst)) = wiiu_common::original() else {
        return;
    };
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    assert_eq!(inst.areas.len(), 5);
    assert_eq!(inst.animals.len(), 9);
    assert_eq!(inst.weapons.len(), 6);
    for number in 1..=5u32 {
        for selected in (0..9).map(|i| 1u32 << i).chain([511]) {
            let choice = HuntChoice {
                area: number - 1,
                animals: wiiu_common::slots(&inst, selected),
                weapons: 63,
                radar: 1,
                ..Default::default()
            };
            let l = load(&inst, &choice, 1);
            let area = &l.area;
            if selected == 1 {
                println!(
                    "AREA{number}: {} textures, {} objects, {} ambient zones, {} random sounds",
                    area.rsc.textures.len(),
                    area.rsc.objects.len(),
                    area.rsc.ambients.len(),
                    area.rsc.random_sounds.len()
                );
                assert!(area.rsc.textures.len() <= 256);
                assert!(area.rsc.objects.len() <= 64);
                assert!(area.rsc.ambients.len() <= 64 && area.rsc.random_sounds.len() <= 64);
            }
            let mut hunt = restart(&l, &l.hunt, &choice, 1);
            assert_eq!(hunt.herd.chars.len(), POPULATION);
            assert_eq!(hunt.opts.targets.count(), selected.count_ones() as usize);
            let models: Vec<&Car> = l.cars.iter().flatten().chain(&l.weapon_cars).collect();
            for car in &models {
                assert_eq!(
                    PreparedModel::from_mesh(&car.mesh).texture_rgba.len(),
                    256 * 256 * 4
                );
                assert!(
                    hunt_setup::animation_radius(car).is_finite()
                        && hunt_setup::animation_radius(car) > 0.0
                );
                assert!(car.mesh.faces.len() * 3 < 65536 - 8192);
            }
            validate_population(area, &hunt, selected);
            if selected == 511 {
                let positions = |h: &Hunt| {
                    h.herd
                        .chars
                        .iter()
                        .map(|c| (c.ctype, c.pos, c.alpha))
                        .collect::<Vec<_>>()
                };
                let same = restart(&l, &hunt, &choice, 1);
                assert_eq!(positions(&hunt), positions(&same));
                for seed in [2, 42, 0x12345678, u32::MAX] {
                    let other = restart(&l, &hunt, &choice, seed);
                    validate_population(area, &other, selected);
                    assert_ne!(positions(&hunt), positions(&other));
                }
            }
            let initial_ammo = hunt.weapons.shots_left.clone();
            let initial_pos = [hunt.player.x, hunt.player.y, hunt.player.z];
            let refs = refs(&l.cars);
            let mut events = Vec::new();
            hunt.tick(
                area,
                &refs,
                &HuntInput {
                    call: true,
                    ..Default::default()
                },
                16,
                &mut events,
            );
            assert!(events.iter().any(|e| matches!(e, GameEvent::Call { call, .. } if hunt.opts.targets.has(*call as usize))));
            for _ in 0..100 {
                events.clear();
                hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
            }
            for ch in &hunt.herd.chars {
                assert!(ch.pos.iter().all(|v| v.is_finite()));
                let car = l.cars[ch.ctype].as_ref().unwrap();
                assert!(
                    c2_core::dino::pose::pose(ch, car, hunt.herd.species[ch.ctype].anims())
                        .is_some()
                );
            }
            for attack_ai in [13, 14, 15, 16, 17, 18] {
                if !hunt.opts.targets.has(attack_ai as usize) {
                    continue;
                }
                hunt = restart(&l, &hunt, &choice, 1);
                let ch = hunt
                    .herd
                    .chars
                    .iter_mut()
                    .find(|ch| ch.ai == attack_ai)
                    .unwrap();
                ch.alpha = 0.0;
                ch.lookx = 1.0;
                ch.lookz = 0.0;
                ch.state = 1;
                let head = if attack_ai == 14 {
                    300.0 * ch.scale
                } else {
                    108.0
                };
                ch.pos = [hunt.player.x - head, hunt.player.y, hunt.player.z];
                events.clear();
                hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
                assert_eq!(
                    hunt.health, 0,
                    "AI {attack_ai} contact must kill on AREA{number}"
                );
                assert!(hunt.death.is_some());
                let body = hunt.death.unwrap().body;
                assert_eq!(hunt.herd.chars[body].ai, 0);
                assert_eq!(hunt.herd.chars[body].ctype, HUNTER);
                for _ in 0..750 {
                    hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
                }
                assert!(c2_core::dino::pose::pose(
                    &hunt.herd.chars[hunt.death.unwrap().body],
                    l.cars[HUNTER].as_ref().unwrap(),
                    hunt.herd.species[HUNTER].anims()
                )
                .is_some());
                assert!(hunt.camera().0.iter().all(|v| v.is_finite()));
                assert!(events
                    .iter()
                    .any(|e| matches!(e, GameEvent::HunterDied { scream: true, .. })));
            }
            hunt.shots_made = 5;
            hunt.successes = 1;
            hunt.weapons.shots_left[0] = 0;
            let fresh = restart(&l, &hunt, &choice, 1);
            assert!(fresh.health > 0 && fresh.death.is_none());
            assert_eq!(fresh.herd.chars.len(), POPULATION);
            assert!(fresh.herd.chars.iter().all(|ch| ch.health > 0));
            assert_eq!(fresh.weapons.shots_left, initial_ammo);
            assert_eq!(
                [fresh.player.x, fresh.player.y, fresh.player.z],
                initial_pos
            );
            assert_eq!((fresh.shots_made, fresh.successes), (0, 0));
            assert_eq!(fresh.opts.targets, hunt.opts.targets);
            assert_eq!(fresh.opts.radar, hunt.opts.radar);
            for disabled in 0..4u32 {
                let c = HuntChoice {
                    blood: (disabled & 1 == 0) as u32,
                    death_anim: (disabled & 2 == 0) as u32,
                    ..choice
                };
                let fresh = restart(&l, &hunt, &c, 1);
                assert_eq!(fresh.opts.blood, disabled & 1 == 0);
                assert_eq!(fresh.opts.death_anim, disabled & 2 == 0);
                assert!(!hunt_setup::death_menu_ready(&fresh, 12000));
                let mut dead = fresh;
                dead.health = 0;
                assert_eq!(hunt_setup::death_menu_ready(&dead, 16), disabled & 2 != 0);
                assert!(hunt_setup::death_menu_ready(&dead, 12000));
            }
            // Every call sounds.
            assert_eq!(l.calls.len(), selected.count_ones() as usize);
            assert!(l
                .calls
                .iter()
                .all(|c| c.sounds.iter().all(|s| !s.is_empty() && s.len() % 2 == 0)));
        }
    }
    for call in 1..=9 {
        for variant in ['A', 'B', 'C'] {
            let wav =
                c2_formats::wav::parse(&read(&format!("SOUNDFX/CALLS/CALL{call}_{variant}.WAV")))
                    .unwrap();
            assert_eq!(wav.format, Some(c2_formats::wav::WavFormat::VANILLA));
            assert!(!wav.pcm.is_empty() && wav.pcm.len().is_multiple_of(2));
        }
    }
}

fn validate_population(area: &Area, hunt: &Hunt, selected: u32) {
    assert_eq!(hunt.herd.chars.len(), POPULATION);
    let mut counts = [0usize; 9];
    let mut min = [f32::MAX; 2];
    let mut max = [f32::MIN; 2];
    for (i, ch) in hunt.herd.chars.iter().enumerate() {
        let target = ch.ai >= 10;
        if target {
            counts[(ch.ai - 10) as usize] += 1;
            assert!(hunt.opts.targets.has(ch.ai as usize));
        }
        let clearance: f32 = if target { 10240.0 } else { 2048.0 };
        let dx = ch.pos[0] - hunt.player.x;
        let dz = ch.pos[2] - hunt.player.z;
        assert!(dx * dx + dz * dz >= clearance * clearance);
        let mut p = ch.pos;
        assert!(!c2_core::dino::check_place_collision_p(area, &mut p));
        let altitude = if matches!(ch.ai, 3 | 4) { 2048.0 } else { 0.0 };
        assert!((ch.pos[1] - p[1] - altitude).abs() < 0.01);
        for other in &hunt.herd.chars[..i] {
            let dx = ch.pos[0] - other.pos[0];
            let dz = ch.pos[2] - other.pos[2];
            assert!(dx * dx + dz * dz >= 2048.0 * 2048.0);
        }
        for (axis, coord) in [ch.pos[0], ch.pos[2]].into_iter().enumerate() {
            min[axis] = min[axis].min(coord);
            max[axis] = max[axis].max(coord);
        }
    }
    assert_eq!(counts.iter().sum::<usize>(), hunt_setup::TARGET_POPULATION);
    let n = selected.count_ones() as usize;
    for (i, count) in counts.into_iter().enumerate() {
        if selected & (1 << i) == 0 {
            assert_eq!(count, 0);
        } else {
            assert!((hunt_setup::TARGET_POPULATION / n
                ..=hunt_setup::TARGET_POPULATION.div_ceil(n))
                .contains(&count));
        }
    }
    assert!(max[0] - min[0] > 100000.0 && max[1] - min[1] > 100000.0);
}

#[test]
fn actor_culling_keeps_boundary_spheres() {
    let visible = |p, r| hunt_setup::actor_visible(p, r, [0.0; 3], 0.0, 0.0);
    assert!(visible([0.0, 0.0, -1000.0], 100.0));
    assert!(!visible([0.0, 0.0, 1000.0], 100.0));
    assert!(!visible([0.0, 0.0, -20000.0], 100.0));
    assert!(visible([0.0, 0.0, -19000.0], 100.0));
    assert!(visible([1400.0, 0.0, -1000.0], 100.0));
    assert!(!visible([2000.0, 0.0, -1000.0], 100.0));
    assert!(hunt_setup::actor_visible(
        [1000.0, 0.0, 0.0],
        100.0,
        [0.0; 3],
        core::f32::consts::FRAC_PI_2,
        0.0
    ));
}

#[test]
fn distant_ai_bounds_pending_time_and_resumes_immediately() {
    let Some((_, inst)) = wiiu_common::original() else {
        return;
    };
    let choice = HuntChoice {
        animals: wiiu_common::slots(&inst, 1),
        weapons: 63,
        ..Default::default()
    };
    let l = load(&inst, &choice, 1);
    let (area, mut hunt) = (&l.area, restart(&l, &l.hunt, &choice, 1));
    assert!(hunt.opts.distant_ai);
    let refs = refs(&l.cars);
    let mut events = Vec::new();
    let mut deferred = None;
    for _ in 0..8 {
        hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
        assert!(hunt
            .herd
            .chars
            .iter()
            .all(|c| (0..=48).contains(&c.pending_ai_ms)));
        deferred = hunt.herd.chars.iter().position(|c| c.pending_ai_ms > 0);
        if deferred.is_some() {
            break;
        }
    }
    let i = deferred.expect("far calm animals should defer work");
    hunt.herd.chars[i].afraid_time = 1000;
    hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
    assert_eq!(hunt.herd.chars[i].pending_ai_ms, 0);
    hunt.opts.distant_ai = false;
    hunt.tick(area, &refs, &HuntInput::default(), 16, &mut events);
    assert!(hunt.herd.chars.iter().all(|c| c.pending_ai_ms == 0));
}

#[test]
fn menu_options_reach_the_hunt_and_survive_a_restart() {
    let Some((_, inst)) = wiiu_common::original() else {
        return;
    };
    let choice = HuntChoice {
        time: 2,
        animals: wiiu_common::slots(&inst, 0b11),
        weapons: 0b101,
        camo: 1,
        scent: 1,
        double_ammo: 1,
        tranq: 1,
        run_pct: 60,
        blood: 0,
        death_anim: 0,
        ambient: 0,
        ..Default::default()
    };
    let l = load(&inst, &choice, 1);
    assert_eq!(l.area.time, TimeOfDay::Night);
    let hunt = &l.hunt;
    let o = &hunt.opts;
    assert!(o.camo && o.scent && o.tranq && o.radar && !o.observer && !o.blood && !o.death_anim);
    assert_eq!(o.dino_run_k, 0.6);
    assert_eq!(hunt.weapons.info.len(), 2);
    assert_eq!(l.weapon_ids, vec![0, 2]);
    assert_eq!(hunt.weapons.ammo_mag, vec![1, 1]);
    assert_eq!(hunt.herd.chars.len(), hunt_setup::TARGET_POPULATION);
    assert!(hunt
        .herd
        .chars
        .iter()
        .all(|c| hunt.herd.species[c.ctype].info.ai >= 10));
    let again = restart(&l, hunt, &choice, 2);
    assert!(
        again.opts.camo && again.opts.scent && !again.opts.blood && again.opts.dino_run_k == 0.6
    );
    assert_eq!(again.weapons.ammo_mag, vec![1, 1]);
    assert_eq!(again.herd.chars.len(), hunt_setup::TARGET_POPULATION);
    // Observer mode needs no weapon and carries none.
    let observe = HuntChoice {
        observer: 1,
        weapons: 0,
        ..choice
    };
    let l = load(&inst, &observe, 1);
    assert!(l.hunt.opts.observer && l.hunt.weapons.info.is_empty());
    let none = HuntChoice {
        weapons: 0,
        ..choice
    };
    assert!(hunt_setup::load(Some(&inst), &none, &hunt_setup::new_save(), 1).is_err());
}

// A kill takes a stand in the save with its weapon and area, numbered as
// the original numbers characters; the trophy room stands the trophies up;
// a survival sends its first wave of hunters.
#[test]
fn trophies_trophy_room_and_survival() {
    let Some((root, inst)) = wiiu_common::original() else {
        return;
    };
    // A hunt: bag the first target.
    let choice = HuntChoice {
        area: 0,
        animals: wiiu_common::slots(&inst, 0b10),
        weapons: 0b10,
        ..Default::default()
    };
    let l = load(&inst, &choice, 1);
    let (area, mut hunt) = (&l.area, restart(&l, &l.hunt, &choice, 1));
    assert_eq!(hunt.area_no, 1);
    let index = hunt.herd.chars.iter().position(|c| c.ai >= 10).unwrap();
    let ctype = hunt.herd.chars[index].ctype;
    let mut events = Vec::new();
    hunt.debug_bag(area, index, &mut events);
    let slot = hunt.trophy_body.expect("a trophy");
    assert_eq!(hunt.room.bodies[slot].ctype as usize, ctype);
    assert_eq!(hunt.room.bodies[slot].area(), 1);
    assert_eq!(hunt.kills.len(), 1);
    // The original's numbers: Ankylosaurus (slot 1) is 8.
    assert_eq!(ctype, 8);
    let back = c2_formats::sav::parse(&c2_formats::sav::write(&hunt.room)).unwrap();
    assert_eq!(back.bodies, hunt.room.bodies);
    if let Ok(orig) = std::fs::read(root.join("../trophy00.sav")) {
        let s = c2_formats::sav::parse(&orig).unwrap();
        let species = &hunt.herd.species;
        assert!(s.bodies.iter().filter(|b| b.ctype > 0).all(|b| species
            .get(b.ctype as usize)
            .is_some_and(|s| s.info.ai >= 10)));
    }

    // The trophy room shows it, no weapons, nothing alive.
    let trophy = HuntChoice {
        mode: hunt_setup::MODE_TROPHY,
        animals: 0,
        weapons: 0,
        ..Default::default()
    };
    let r = hunt_setup::load(Some(&inst), &trophy, &hunt.room, 1).unwrap();
    let mut room = r.hunt;
    assert!(room.opts.trophy && room.weapons.info.is_empty());
    assert_eq!(room.herd.chars.len(), 1);
    assert_eq!(room.herd.chars[0].ctype, ctype);
    let room_refs = refs(&r.cars);
    for _ in 0..60 {
        room.tick(&r.area, &room_refs, &HuntInput::default(), 16, &mut events);
    }
    assert!(room.health > 0);
    room.press_escape();
    assert_eq!(room.outcome(), Some(c2_core::game::Outcome::Exited));

    // A survival: the map's animals go; the menu prices buy its waves
    // (the hunt's picks don't matter).
    let survive = HuntChoice {
        mode: hunt_setup::MODE_SURVIVAL,
        animals: wiiu_common::slots(&inst, 0b1000),
        weapons: 0b10,
        ..Default::default()
    };
    let sl = load(&inst, &survive, 3);
    let mut s = restart(&sl, &sl.hunt, &survive, 3);
    assert!(s.survival.is_some() && !s.opts.tranq && !s.opts.observer);
    let srefs = refs(&sl.cars);
    for _ in 0..(5000 / 16) {
        events.clear();
        s.tick(&sl.area, &srefs, &HuntInput::default(), 16, &mut events);
    }
    assert_eq!(s.wave(), 1);
    let opts = &s.survival.as_ref().unwrap().opts;
    let price = |ct: usize| opts.roster.iter().find(|r| r.0 == ct).map(|r| r.1);
    let wave: Vec<_> = s.herd.chars.iter().filter(|c| c.wave == 1).collect();
    assert!(!wave.is_empty() && wave.iter().all(|c| price(c.ctype).is_some()));
    let cost: u32 = wave.iter().filter_map(|c| price(c.ctype)).sum();
    assert!(cost <= opts.wave_budget(1));
    let restarted = restart(&sl, &s, &survive, 4);
    assert!(restarted.survival.is_some() && restarted.wave() == 0);
}

// The view-model depth range the Wii U maps into its front depth slice
// (hunt_setup::GUN_NEAR..GUN_FAR) holds every gun vertex in front of the
// camera, in every frame of every weapon animation and any sway or pitch (the
// distance from the eye bounds the depth). Parts behind the camera (the
// stocks and arms reach back) are clipped, as they always were.
#[test]
fn gun_vertices_fit_the_view_model_depth_range() {
    let Some((_, inst)) = wiiu_common::original() else {
        return;
    };
    let choice = HuntChoice {
        weapons: 63,
        ..Default::default()
    };
    let l = load(&inst, &choice, 1);
    let (mut near, mut far) = (f32::MAX, 0.0f32);
    for car in &l.weapon_cars {
        for anim in &car.animations {
            let duration = c2_core::model::car_anim_duration_ms(anim).max(1);
            for t in (0..duration).step_by(10) {
                let Some(posed) = c2_core::model::pose_car(anim, car.mesh.vertices.len(), t) else {
                    continue;
                };
                for v in posed {
                    if -v[2] > 0.0 {
                        near = near.min(-v[2]);
                    }
                    far = far.max((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt());
                }
            }
        }
    }
    eprintln!("gun vertices ahead: nearest {near:.2}, farthest {far:.1}");
    assert!(far < hunt_setup::GUN_FAR / 2.0);
    assert_eq!(hunt_setup::gun_clip_z(hunt_setup::GUN_NEAR), 0.0);
    let z = hunt_setup::gun_clip_z(hunt_setup::GUN_FAR) / hunt_setup::GUN_FAR;
    assert!((z - 1.0).abs() < 1e-6);
}

// The owner's crashed session (AREA5 at dawn, four herbivore targets, every
// weapon and gear, blood and death camera off), driven hard: crouched and
// sprinting at once, moving, turning, firing and switching weapons.
#[test]
#[ignore]
fn crouch_and_sprint_stress() {
    let (_, inst) = wiiu_common::original().unwrap();
    let choice = HuntChoice {
        area: 4,
        time: 0,
        animals: wiiu_common::slots(&inst, 23),
        weapons: 63,
        radar: 1,
        camo: 1,
        scent: 1,
        double_ammo: 1,
        tranq: 1,
        blood: 0,
        death_anim: 0,
        ..Default::default()
    };
    for seed in [1201724847, 597228150, 1, 2, 3] {
        let l = load(&inst, &choice, seed);
        let (area, mut hunt) = (&l.area, restart(&l, &l.hunt, &choice, seed));
        let refs = refs(&l.cars);
        let mut events = Vec::new();
        for t in 0..20_000u32 {
            let phase = t / 500 % 4;
            let controls = c2_core::player::Controls {
                forward: phase != 3,
                strafe_left: phase == 1,
                turn_right: t % 300 < 40,
                look_down: t % 900 < 30,
                run: phase >= 1,
                crouch: phase <= 1 || phase == 3,
                jump: t % 777 == 0,
                ..Default::default()
            };
            let input = HuntInput {
                controls,
                fire: t % 97 < 3,
                toggle_weapon: t % 1500 == 10,
                select_weapon: (t % 400 == 0)
                    .then_some((t / 400) as usize % hunt.weapons.info.len()),
                call: t % 2000 == 5,
                ..Default::default()
            };
            hunt.tick(area, &refs, &input, 16, &mut events);
            if hunt.health <= 0 {
                hunt = restart(&l, &hunt, &choice, seed ^ t);
            }
        }
    }
}

#[test]
#[ignore]
fn time_of_day_data() {
    let root = std::env::var("C2_ORIGINAL_HUNTDAT").unwrap();
    for n in 1..=5 {
        let read = |p: String| std::fs::read(std::path::Path::new(&root).join(p)).unwrap();
        let m = map::parse(&read(format!("AREAS/AREA{n}.MAP"))).unwrap();
        let r = rsc::parse(&read(format!("AREAS/AREA{n}.RSC"))).unwrap();
        let avg = |l: &[u8]| l.iter().map(|&v| v as u64).sum::<u64>() / l.len() as u64;
        let sky = |s: &[u16]| {
            if s.is_empty() {
                [0u64; 3]
            } else {
                let mut a = [0u64; 3];
                for &p in s {
                    let c = c2_core::color::rgb555_to_rgba8(p, 255);
                    for k in 0..3 {
                        a[k] += c[k] as u64;
                    }
                }
                a.map(|v| v / s.len() as u64)
            }
        };
        eprintln!(
            "AREA{n}: fade {:?} trans {:?} light avg {} {} {} skies {:?} {:?} {:?} (len {} {} {})",
            r.fade_rgb,
            r.trans_rgb,
            avg(&m.light[0]),
            avg(&m.light[1]),
            avg(&m.light[2]),
            sky(&r.skies[0]),
            sky(&r.skies[1]),
            sky(&r.skies[2]),
            r.skies[0].len(),
            r.skies[1].len(),
            r.skies[2].len()
        );
    }
}
