#![cfg(feature = "portable-hunt")]
extern crate alloc;
#[path = "../../../../wiiu-ver/perf-probe/src/hunt_setup.rs"]
mod hunt_setup;
use c2_core::{
    area::{Area, TimeOfDay},
    game::{GameEvent, HuntInput},
    model::PreparedModel,
};
use c2_formats::{map, rsc};

#[test]
fn wiiu_roster_on_all_five_original_maps() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    let defs = std::fs::read_to_string(root.join("_RES.TXT")).unwrap();
    let mut assets: Vec<Vec<u8>> = hunt_setup::SPECIES_FILES
        .iter()
        .map(|file| read(file))
        .collect();
    assets.extend(
        hunt_setup::WEAPON_FILES
            .iter()
            .map(|f| read(&format!("WEAPONS/{f}"))),
    );
    assets.push(read("HUNTER1.CAR"));
    let assets: Vec<&[u8]> = assets.iter().map(Vec::as_slice).collect();
    for number in 1..=5 {
        let area = Area::new(
            map::parse(&read(&format!("AREAS/AREA{number}.MAP"))).unwrap(),
            rsc::parse(&read(&format!("AREAS/AREA{number}.RSC"))).unwrap(),
            TimeOfDay::Day,
        );
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
        for selected in (0..9).map(|i| 1 << i).chain([511]) {
            let (mut hunt, cars) = hunt_setup::setup(&area, &defs, &assets, selected, true, 63, 1)
                .unwrap_or_else(|e| panic!("AREA{number} targets {selected}: {e}"));
            assert_eq!(hunt.herd.chars.len(), hunt_setup::POPULATION);
            assert_eq!(hunt.opts.targets.count(), selected.count_ones() as usize);
            for car in &cars {
                assert_eq!(
                    PreparedModel::from_mesh(&car.mesh).texture_rgba.len(),
                    256 * 256 * 4
                );
            }
            validate_population(&area, &hunt, selected);
            if selected == 511 {
                let positions = |h: &c2_core::game::Hunt| {
                    h.herd
                        .chars
                        .iter()
                        .map(|c| (c.ctype, c.pos, c.alpha))
                        .collect::<Vec<_>>()
                };
                let same = hunt_setup::restart(&area, &hunt, 1).unwrap();
                assert_eq!(positions(&hunt), positions(&same));
                for seed in [2, 42, 0x12345678, u32::MAX] {
                    let other = hunt_setup::restart(&area, &hunt, seed).unwrap();
                    validate_population(&area, &other, selected);
                    assert_ne!(positions(&hunt), positions(&other));
                }
            }
            for car in &cars {
                assert!(
                    hunt_setup::animation_radius(car).is_finite()
                        && hunt_setup::animation_radius(car) > 0.0
                );
            }
            let initial_ammo = hunt.weapons.shots_left.clone();
            let initial_pos = [hunt.player.x, hunt.player.y, hunt.player.z];
            let refs: Vec<_> = (0..=hunt_setup::HUNTER_SPECIES)
                .map(|i| Some(&cars[hunt_setup::model_for_species(i)]))
                .collect();
            let mut events = Vec::new();
            hunt.tick(
                &area,
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
                hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
            }
            for ch in &hunt.herd.chars {
                assert!(ch.pos.iter().all(|v| v.is_finite()));
                assert!(c2_core::dino::pose::pose(
                    ch,
                    &cars[hunt_setup::model_for_species(ch.ctype)],
                    hunt.herd.species[ch.ctype].anims()
                )
                .is_some());
            }
            assert!(cars
                .iter()
                .all(|car| car.mesh.faces.len() * 3 < 65536 - 8192));
            for attack_ai in [13, 14, 15, 16, 17, 18] {
                if !hunt.opts.targets.has(attack_ai as usize) {
                    continue;
                }
                hunt = hunt_setup::restart(&area, &hunt, 1).unwrap();
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
                hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
                assert_eq!(
                    hunt.health, 0,
                    "AI {attack_ai} contact must kill on AREA{number}"
                );
                assert!(hunt.death.is_some());
                let body = hunt.death.unwrap().body;
                assert_eq!(hunt.herd.chars[body].ai, 0);
                assert_eq!(hunt.herd.chars[body].ctype, hunt_setup::HUNTER_SPECIES);
                for _ in 0..750 {
                    hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
                }
                assert!(c2_core::dino::pose::pose(
                    &hunt.herd.chars[hunt.death.unwrap().body],
                    &cars[hunt_setup::HUNTER_MODEL],
                    hunt.herd.species[hunt_setup::HUNTER_SPECIES].anims()
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
            let fresh = hunt_setup::restart(&area, &hunt, 1).unwrap();
            assert!(fresh.health > 0 && fresh.death.is_none());
            assert_eq!(fresh.herd.chars.len(), hunt_setup::POPULATION);
            assert!(fresh.herd.chars.iter().all(|ch| ch.health > 0));
            assert_eq!(fresh.weapons.shots_left, initial_ammo);
            assert_eq!(
                [fresh.player.x, fresh.player.y, fresh.player.z],
                initial_pos
            );
            assert_eq!((fresh.shots_made, fresh.successes), (0, 0));
            assert_eq!(fresh.opts.targets, hunt.opts.targets);
            assert_eq!(fresh.opts.radar, hunt.opts.radar);
            for disabled in 0..4 {
                hunt_setup::configure(&mut hunt, disabled);
                let fresh = hunt_setup::restart(&area, &hunt, 1).unwrap();
                assert_eq!(fresh.opts.blood, disabled & 1 == 0);
                assert_eq!(fresh.opts.death_anim, disabled & 2 == 0);
                assert!(!hunt_setup::death_menu_ready(&fresh, 12000));
                let mut dead = fresh;
                dead.health = 0;
                assert_eq!(hunt_setup::death_menu_ready(&dead, 16), disabled & 2 != 0);
                assert!(hunt_setup::death_menu_ready(&dead, 12000));
            }
        }
    }
    for call in 1..=9 {
        for variant in ['A', 'B', 'C'] {
            let wav =
                c2_formats::wav::parse(&read(&format!("SOUNDFX/CALLS/CALL{call}_{variant}.WAV")))
                    .unwrap();
            assert_eq!(wav.format, Some(c2_formats::wav::WavFormat::VANILLA));
            assert!(!wav.pcm.is_empty() && wav.pcm.len() % 2 == 0);
        }
    }
}

fn validate_population(area: &Area, hunt: &c2_core::game::Hunt, selected: u32) {
    assert_eq!(hunt.herd.chars.len(), hunt_setup::POPULATION);
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
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    let defs = std::fs::read_to_string(root.join("_RES.TXT")).unwrap();
    let mut assets: Vec<Vec<u8>> = hunt_setup::SPECIES_FILES
        .iter()
        .map(|file| read(file))
        .collect();
    assets.extend(
        hunt_setup::WEAPON_FILES
            .iter()
            .map(|f| read(&format!("WEAPONS/{f}"))),
    );
    assets.push(read("HUNTER1.CAR"));
    let bytes: Vec<&[u8]> = assets.iter().map(Vec::as_slice).collect();
    let area = Area::new(
        map::parse(&read("AREAS/AREA1.MAP")).unwrap(),
        rsc::parse(&read("AREAS/AREA1.RSC")).unwrap(),
        TimeOfDay::Day,
    );
    let (mut hunt, cars) = hunt_setup::setup(&area, &defs, &bytes, 1, true, 63, 1).unwrap();
    assert!(hunt.opts.distant_ai);
    let refs: Vec<_> = (0..=hunt_setup::HUNTER_SPECIES)
        .map(|i| Some(&cars[hunt_setup::model_for_species(i)]))
        .collect();
    let mut events = Vec::new();
    let mut deferred = None;
    for _ in 0..8 {
        hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
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
    hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
    assert_eq!(hunt.herd.chars[i].pending_ai_ms, 0);
    hunt.opts.distant_ai = false;
    hunt.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
    assert!(hunt.herd.chars.iter().all(|c| c.pending_ai_ms == 0));
}

#[test]
fn menu_options_reach_the_hunt_and_survive_a_restart() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    let defs = std::fs::read_to_string(root.join("_RES.TXT")).unwrap();
    let mut assets: Vec<Vec<u8>> = hunt_setup::SPECIES_FILES.iter().map(|file| read(file)).collect();
    assets.extend(hunt_setup::WEAPON_FILES.iter().map(|f| read(&format!("WEAPONS/{f}"))));
    assets.push(read("HUNTER1.CAR"));
    let bytes: Vec<&[u8]> = assets.iter().map(Vec::as_slice).collect();
    let choice = hunt_setup::HuntChoice {
        time: 2,
        animals: 0b11,
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
    let area = Area::new(
        map::parse(&read("AREAS/AREA1.MAP")).unwrap(),
        rsc::parse(&read("AREAS/AREA1.RSC")).unwrap(),
        choice.time_of_day(),
    );
    assert_eq!(area.time, TimeOfDay::Night);
    let (hunt, _) = hunt_setup::setup_with(&area, &defs, &bytes, &choice, &hunt_setup::new_save(), 1).unwrap();
    let o = hunt.opts;
    assert!(o.camo && o.scent && o.tranq && o.radar && !o.observer && !o.blood && !o.death_anim);
    assert_eq!(o.dino_run_k, 0.6);
    assert_eq!(hunt.weapons.info.len(), 2);
    assert_eq!(hunt.weapons.ammo_mag, vec![1, 1]);
    assert_eq!(hunt.herd.chars.len(), hunt_setup::TARGET_POPULATION);
    assert!(hunt.herd.chars.iter().all(|c| c.ctype >= hunt_setup::AMBIENT_COUNT));
    let again = hunt_setup::restart_with(&area, &hunt, &choice, &hunt.room.clone(), 2).unwrap();
    assert!(again.opts.camo && again.opts.scent && !again.opts.blood && again.opts.dino_run_k == 0.6);
    assert_eq!(again.weapons.ammo_mag, vec![1, 1]);
    assert_eq!(again.herd.chars.len(), hunt_setup::TARGET_POPULATION);
    // Observer mode needs no weapon and carries none.
    let observe = hunt_setup::HuntChoice { observer: 1, weapons: 0, ..choice };
    let (hunt, _) = hunt_setup::setup_with(&area, &defs, &bytes, &observe, &hunt_setup::new_save(), 1).unwrap();
    assert!(hunt.opts.observer && hunt.weapons.info.is_empty());
    let none = hunt_setup::HuntChoice { weapons: 0, ..choice };
    assert!(hunt_setup::setup_with(&area, &defs, &bytes, &none, &hunt_setup::new_save(), 1).is_err());
}

// A kill takes a stand in the save with its weapon and area; the save
// converts to the original's character numbers and back; the trophy room
// stands the trophies up; a survival sends its first wave of hunters.
#[test]
fn trophies_trophy_room_and_survival() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    let defs = std::fs::read_to_string(root.join("_RES.TXT")).unwrap();
    let mut assets: Vec<Vec<u8>> = hunt_setup::SPECIES_FILES.iter().map(|file| read(file)).collect();
    assets.extend(hunt_setup::WEAPON_FILES.iter().map(|f| read(&format!("WEAPONS/{f}"))));
    assets.push(read("HUNTER1.CAR"));
    let bytes: Vec<&[u8]> = assets.iter().map(Vec::as_slice).collect();
    let load = |name: &str| {
        Area::new(
            map::parse(&read(&format!("AREAS/{name}.MAP"))).unwrap(),
            rsc::parse(&read(&format!("AREAS/{name}.RSC"))).unwrap(),
            TimeOfDay::Day,
        )
    };
    let area = load("AREA1");
    let cars: Vec<_> = assets.iter().take(hunt_setup::MODEL_COUNT).map(|b| c2_formats::car::parse(b).unwrap()).collect();
    let refs: [Option<&c2_formats::car::Car>; hunt_setup::HUNTER_SPECIES + 1] =
        std::array::from_fn(|i| cars.get(hunt_setup::model_for_species(i)));

    // A hunt: bag the first target.
    let choice = hunt_setup::HuntChoice { area: 0, animals: 0b10, weapons: 0b10, ..Default::default() };
    let (mut hunt, _) = hunt_setup::setup_with(&area, &defs, &bytes, &choice, &hunt_setup::new_save(), 1).unwrap();
    assert_eq!(hunt.area_no, 1);
    let index = hunt.herd.chars.iter().position(|c| c.ctype >= hunt_setup::AMBIENT_COUNT).unwrap();
    let ctype = hunt.herd.chars[index].ctype;
    let mut events = Vec::new();
    hunt.debug_bag(&area, index, &mut events);
    let slot = hunt.trophy_body.expect("a trophy");
    assert_eq!(hunt.room.bodies[slot].ctype as usize, ctype);
    assert_eq!(hunt.room.bodies[slot].area(), 1);
    assert_eq!(hunt.kills.len(), 1);
    // The original numbers: the hunter first, so one more.
    let save = hunt_setup::room_to_save(&hunt.room);
    assert_eq!(save.bodies[slot].ctype as usize, ctype + 1);
    let back = c2_formats::sav::parse(&c2_formats::sav::write(&save)).unwrap();
    assert_eq!(hunt_setup::room_from_save(&back).bodies, hunt.room.bodies);
    // An original save (Ankylosaurus is 8 there, 7 here).
    if let Ok(orig) = std::fs::read(root.join("../trophy00.sav")) {
        let s = c2_formats::sav::parse(&orig).unwrap();
        let room = hunt_setup::room_from_save(&s);
        for (a, b) in s.bodies.iter().zip(room.bodies.iter()).filter(|(a, _)| a.ctype > 0) {
            assert_eq!(a.ctype, b.ctype + 1);
        }
    }

    // The trophy room shows it, no weapons, nothing alive.
    let room_area = load("TROPHY");
    let trophy = hunt_setup::HuntChoice { mode: hunt_setup::MODE_TROPHY, animals: 0, weapons: 0, ..Default::default() };
    let (mut room, _) = hunt_setup::setup_with(&room_area, &defs, &bytes, &trophy, &hunt.room, 1).unwrap();
    assert!(room.opts.trophy && room.weapons.info.is_empty());
    assert_eq!(room.herd.chars.len(), 1);
    assert_eq!(room.herd.chars[0].ctype, ctype);
    for _ in 0..60 {
        room.tick(&room_area, &refs, &HuntInput::default(), 16, &mut events);
    }
    assert!(room.health > 0);
    room.press_escape();
    assert_eq!(room.outcome(), Some(c2_core::game::Outcome::Exited));

    // A survival: the map's hunters go, waves of the picked ones come.
    let survive = hunt_setup::HuntChoice { mode: hunt_setup::MODE_SURVIVAL, animals: 0b1000, weapons: 0b10, ..Default::default() };
    let (mut s, _) = hunt_setup::setup_with(&area, &defs, &bytes, &survive, &hunt_setup::new_save(), 3).unwrap();
    assert!(s.survival.is_some() && !s.opts.tranq && !s.opts.observer);
    let allo = hunt_setup::AMBIENT_COUNT + 3;
    for _ in 0..(5000 / 16) {
        events.clear();
        s.tick(&area, &refs, &HuntInput::default(), 16, &mut events);
    }
    assert_eq!(s.wave(), 1);
    assert!(s.herd.chars.iter().any(|c| c.wave > 0 && c.ctype == allo));
    let restarted = hunt_setup::restart_with(&area, &s, &survive, &hunt_setup::new_save(), 4).unwrap();
    assert!(restarted.survival.is_some() && restarted.wave() == 0);
}

// The view-model depth range the Wii U maps into its front depth slice
// (hunt_setup::GUN_NEAR..GUN_FAR) holds every gun vertex in front of the
// camera, in every frame of every weapon animation and any sway or pitch (the
// distance from the eye bounds the depth). Parts behind the camera (the
// stocks and arms reach back) are clipped, as they always were.
#[test]
fn gun_vertices_fit_the_view_model_depth_range() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let (mut near, mut far) = (f32::MAX, 0.0f32);
    for f in hunt_setup::WEAPON_FILES {
        let car = c2_formats::car::parse(&std::fs::read(root.join("WEAPONS").join(f)).unwrap()).unwrap();
        for anim in &car.animations {
            let duration = c2_core::model::car_anim_duration_ms(anim).max(1);
            for t in (0..duration).step_by(10) {
                let Some(posed) = c2_core::model::pose_car(anim, car.mesh.vertices.len(), t) else { continue };
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
    let root = std::env::var("C2_ORIGINAL_HUNTDAT").unwrap();
    let root = std::path::Path::new(&root);
    let read = |p: &str| std::fs::read(root.join(p)).unwrap();
    let defs = std::fs::read_to_string(root.join("_RES.TXT")).unwrap();
    let mut assets: Vec<Vec<u8>> = hunt_setup::SPECIES_FILES.iter().map(|file| read(file)).collect();
    assets.extend(hunt_setup::WEAPON_FILES.iter().map(|f| read(&format!("WEAPONS/{f}"))));
    assets.push(read("HUNTER1.CAR"));
    let bytes: Vec<&[u8]> = assets.iter().map(Vec::as_slice).collect();
    let choice = hunt_setup::HuntChoice {
        area: 4, time: 0, animals: 23, weapons: 63, radar: 1, camo: 1, scent: 1, double_ammo: 1, tranq: 1,
        blood: 0, death_anim: 0, ..Default::default()
    };
    let area = Area::new(
        map::parse(&read("AREAS/AREA5.MAP")).unwrap(),
        rsc::parse(&read("AREAS/AREA5.RSC")).unwrap(),
        choice.time_of_day(),
    );
    for seed in [1201724847, 597228150, 1, 2, 3] {
        let (mut hunt, cars) = hunt_setup::setup_with(&area, &defs, &bytes, &choice, &hunt_setup::new_save(), seed).unwrap();
        let refs: Vec<_> = (0..=hunt_setup::HUNTER_SPECIES).map(|i| Some(&cars[hunt_setup::model_for_species(i)])).collect();
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
                select_weapon: (t % 400 == 0).then_some((t / 400) as usize % hunt.weapons.info.len()),
                call: t % 2000 == 5,
                ..Default::default()
            };
            hunt.tick(&area, &refs, &input, 16, &mut events);
            if hunt.health <= 0 {
                hunt = hunt_setup::restart_with(&area, &hunt, &choice, &hunt.room.clone(), seed ^ t).unwrap();
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
        let sky = |s: &[u16]| if s.is_empty() { [0u64; 3] } else {
            let mut a = [0u64; 3];
            for &p in s { let c = c2_core::color::rgb555_to_rgba8(p, 255); for k in 0..3 { a[k] += c[k] as u64; } }
            a.map(|v| v / s.len() as u64)
        };
        eprintln!("AREA{n}: fade {:?} trans {:?} light avg {} {} {} skies {:?} {:?} {:?} (len {} {} {})", r.fade_rgb, r.trans_rgb,
            avg(&m.light[0]), avg(&m.light[1]), avg(&m.light[2]), sky(&r.skies[0]), sky(&r.skies[1]), sky(&r.skies[2]), r.skies[0].len(), r.skies[1].len(), r.skies[2].len());
    }
}
