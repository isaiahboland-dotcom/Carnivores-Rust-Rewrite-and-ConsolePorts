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
