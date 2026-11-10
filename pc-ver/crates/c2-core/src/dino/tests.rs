use super::*;
use c2_formats::car::{Animation, Car};

fn variant_herd(probabilities: [f64; 2]) -> Herd {
    let mut herd = Herd {
        species: vec![species(0, 1), species(ai::PARA, 8), species(ai::STEGO, 8)],
        ..Default::default()
    };
    for i in 1..=2 {
        herd.species[i].tuning.menu_slot = Some(0);
        herd.species[i].variant = Some(variants::Variant {
            owner: 1,
            token: 100 + i as u64,
            choices: if i == 1 {
                vec![(1, probabilities[0]), (2, probabilities[1])]
            } else {
                vec![]
            },
        });
    }
    herd
}

#[test]
fn variant_spawns_are_seeded_weighted_and_keep_the_selected_ai() {
    let mut herd = variant_herd([0.25, 0.75]);
    let mut rng = MsRand::new(71);
    let mut repeat = MsRand::new(71);
    let expected: Vec<_> = (0..4000)
        .map(|_| {
            let selected = herd.select_variant(2, &mut repeat).expect("eligible");
            // Reset consumes RNG too; mirror the real spawn below.
            let mut c = Character {
                ctype: selected,
                ..Default::default()
            };
            c.reset(&herd.species[selected], &mut repeat, 128);
            selected
        })
        .collect();
    for _ in 0..4000 {
        assert!(herd.spawn(1, [0.0; 3], &mut rng, 128));
    }
    assert_eq!(
        herd.chars.iter().map(|c| c.ctype).collect::<Vec<_>>(),
        expected
    );
    let classic = herd.chars.iter().filter(|c| c.ctype == 1).count();
    assert!((850..1150).contains(&classic), "{classic}");
    assert!(herd
        .chars
        .iter()
        .all(|c| c.ai == herd.species[c.ctype].runs_as()));
}

#[test]
fn disabled_variants_have_no_fallback_and_ordinary_species_do_not_draw_rng() {
    let mut herd = variant_herd([0.0, 0.0]);
    let mut rng = MsRand::new(91);
    let mut unchanged = rng.clone();
    assert!(!herd.spawn(1, [0.0; 3], &mut rng, 128));
    assert!(!herd.spawn(2, [0.0; 3], &mut rng, 128));
    assert_eq!(herd.select_variant(0, &mut rng), Some(0));
    assert_eq!(rng.rand(), unchanged.rand());
    assert!(herd.chars.is_empty());
}

#[test]
fn initial_dialect_variants_share_population_instead_of_duplicating_it() {
    let res = c2_formats::res_txt::ResTxt::parse("spawntable {\n spawngroup {\n spawnmin = 3\n spawnmax = 3\n }\n}\ncharacters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Owner'\n ai = 10\n spawninfo {\n spawngroup = 0\n spawnratio = 1\n }\n}\n{\n name = 'Variant'\n ai = 14\n spawninfo {\n spawngroup = 0\n spawnratio = 1\n }\n}\n}\n").expect("res");
    let tables = c2_formats::mee::Tables::read(
        &res,
        &c2_formats::mee::Hunt {
            area: Some(1),
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        },
    );
    let area = Area::flat(10);
    let mut herd = variant_herd([0.0, 1.0]);
    herd.place_mee(
        &area,
        (50.0 * 256.0, 50.0 * 256.0),
        &mut MsRand::new(7),
        &tables,
        128,
    );
    assert_eq!(herd.chars.len(), 3);
    assert!(herd.chars.iter().all(|c| c.ctype == 2));
    herd.species[1]
        .variant
        .as_mut()
        .expect("variant")
        .choices
        .clear();
    herd.place_mee(
        &area,
        (50.0 * 256.0, 50.0 * 256.0),
        &mut MsRand::new(7),
        &tables,
        128,
    );
    assert!(herd.chars.is_empty());
}

#[test]
fn trophy_variant_identity_survives_reordering_and_missing_members() {
    let mut herd = variant_herd([0.5, 0.5]);
    let mut room =
        c2_formats::sav::parse(&vec![0; c2_formats::sav::FILE_SIZE]).expect("blank save");
    room.bodies[0].ctype = 2;
    room.bodies[0].set_variant_token(102);
    room.bodies[1].ctype = 1; // legacy trophy
    herd.species.swap(1, 2);
    variants::remap_trophies(&herd.species, &mut room);
    assert_eq!(room.bodies[0].ctype, 1);
    assert_eq!(room.bodies[1].ctype, 1);
    let bytes = c2_formats::sav::write(&room);
    let mut room = c2_formats::sav::parse(&bytes).expect("save round trip");
    variants::remap_trophies(&[], &mut room);
    assert_eq!(room.bodies[0].ctype, -1);
    assert_eq!(room.bodies[0].variant_token(), 102);
    variants::remap_trophies(&herd.species, &mut room);
    assert_eq!(room.bodies[0].ctype, 1);
}

/// A one-vertex species with `n` 1-second, 2-frame animations.
fn species(ai_id: i32, n: usize) -> Species {
    let car = Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: (0..n)
            .map(|_| Animation {
                name_raw: [0; 32],
                kps: 2,
                frames: 2,
                data: vec![0; 6],
            })
            .collect(),
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    };
    let info = DinoInfo {
        name: "test".into(),
        file: String::new(),
        ai: ai_id,
        health0: 5,
        mass: 1.0,
        length: 1.0,
        radius: 256.0,
        base_score: 1.0,
        smell_k: 1.0,
        hear_k: 1.0,
        look_k: 1.0,
        sh_delta: 0.0,
        scale0: 1000,
        scale_a: 0,
        danger_call: false,
        runs_as: None,
    };
    Species::new(info, &car)
}

/// Carnivores Triassic's Gojirasaurus: AI 19 (a patched exe's slot)
/// with the T-Rex's clips runs as the T-Rex, keeping its own number.
#[test]
fn an_extra_slot_ai_runs_as_the_stock_ai_its_clips_fit() {
    let named = |names: &[&str]| Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: names
            .iter()
            .map(|n| {
                let mut name_raw = [0; 32];
                name_raw[..n.len()].copy_from_slice(n.as_bytes());
                Animation {
                    name_raw,
                    kps: 2,
                    frames: 2,
                    data: vec![0; 6],
                }
            })
            .collect(),
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    };
    let gojira = named(&[
        "run", "walk", "roar", "run2", "ldown", "lup", "sup", "sdown", "die", "kill3",
    ]);
    let mut info = species(19, 0).info;
    info.danger_call = true;
    let sp = Species::new(info.clone(), &gojira);
    assert_eq!(
        (sp.stock_ai, sp.runs_as(), sp.info.ai),
        (Some(ai::TREX), ai::TREX, 19)
    );
    // The Allosaurus' layout (death 6th of 8).
    let allo = named(&["run", "wlk", "swim", "sld", "jmp", "die", "eat", "slp"]);
    assert_eq!(Species::new(info.clone(), &allo).stock_ai, Some(ai::ALLO));
    // A model naming no death where any AI has one: as before.
    assert_eq!(Species::new(info, &named(&["a", "b"])).stock_ai, None);
    // Known AIs are left alone.
    assert_eq!(Species::new(species(13, 0).info, &gojira).stock_ai, None);
    // Named by the data (`runsAs`): taken, whatever the clips.
    let mut named_as = species(22, 0).info;
    named_as.runs_as = Some(ai::VELO);
    assert_eq!(Species::new(named_as, &allo).stock_ai, Some(ai::VELO));
}

fn run(herd: &mut Herd, area: &Area, hunter: HunterView, ms: i32) -> Vec<DinoEvent> {
    let mut rng = MsRand::new(3);
    let mut events = Vec::new();
    for t in 0..ms / 10 {
        let mut ctx = Ctx {
            area,
            rng: &mut rng,
            dt: 10,
            real_time: t * 10,
            hunter,
            target: Default::default(),
            struck: None,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            render_r: 74,
            run_k: 1.0,
            events: &mut events,
        };
        herd.step(&mut ctx);
    }
    events
}

fn herd_with(area: &Area, ai_id: i32, n_anims: usize, pos: [f32; 3]) -> Herd {
    let mut rng = MsRand::new(1);
    let mut herd = Herd {
        species: vec![species(ai_id, n_anims)],
        chars: vec![],
        demo_index: None,
        ..Default::default()
    };
    let mut c = Character {
        pos,
        tgx: pos[0],
        tgz: pos[2],
        ..Default::default()
    };
    c.reset(&herd.species[0], &mut rng, 128);
    c.pos[1] = area.land_height(pos[0], pos[2]);
    herd.chars.push(c);
    herd
}

#[test]
fn impossible_spawn_group_does_not_starve_later_targets() {
    let raw = c2_formats::res_txt::ResTxt::parse("spawntable {\nspawngroup {\nspawnmin = 1\nspawnmax = 1\nregion {\nxmin = 49\nxmax = 51\nymin = 49\nymax = 51\n}\n}\nspawngroup {\nspawnmin = 3\nspawnmax = 3\n}\n}\ncharacters {\n{\nai = 10\nspawninfo {\nspawngroup = 0\nspawnratio = 1\n}\n}\n{\nai = 13\nspawninfo {\nspawngroup = 1\nspawnratio = 1\n}\n}\n}\n").unwrap_or_else(|e| panic!("{e}"));
    let tables = c2_formats::mee::Tables::read(
        &raw,
        &c2_formats::mee::Hunt {
            area: Some(1),
            time: 1,
            picked: Default::default(),
        },
    );
    let mut herd = Herd {
        species: vec![species(ai::PARA, 8), species(ai::VELO, 8)],
        ..Default::default()
    };
    herd.place_mee(
        &Area::flat(10),
        (50.0 * 256.0, 50.0 * 256.0),
        &mut MsRand::new(7),
        &tables,
        128,
    );
    assert_eq!(herd.chars.len(), 3);
    assert!(herd.chars.iter().all(|c| c.ctype == 1));
}

#[test]
fn mee_placement_follows_groups_and_packs() {
    use c2_formats::mee::{
        Character as MeeChar, PackGroup, PackLink, Region, SpawnGroup, SpawnLink, Tables,
    };
    let area = Area::flat(10);
    let region = Region {
        xmin: 300,
        xmax: 900,
        ymin: 300,
        ymax: 900,
    };
    let group = |n: i32| SpawnGroup {
        min: n,
        max: n,
        regions: vec![region],
        ..Default::default()
    };
    let link = |g: usize| SpawnLink {
        group: g,
        ratio: 1.0,
    };
    let t = Tables {
        spawn_groups: vec![group(3), group(2)],
        pack_groups: vec![PackGroup {
            min: 3,
            max: 3,
            density: 0.0,
            spawn: vec![link(1)],
        }],
        characters: vec![
            MeeChar::default(),
            MeeChar {
                spawn: vec![link(0)],
                ..Default::default()
            },
            MeeChar {
                packs: vec![PackLink {
                    group: 0,
                    ratio: 1.0,
                }],
                ..Default::default()
            },
        ],
        trophies: Vec::new(),
        snow: None,
        survival: None,
        trophy_start: None,
    };
    let mut herd = Herd {
        species: vec![
            species(ai::HUNTER, 2),
            species(ai::PARA, 8),
            species(ai::VELO, 8),
        ],
        chars: vec![],
        demo_index: None,
        ..Default::default()
    };
    let mut rng = MsRand::new(7);
    let hunter = (50.0 * 256.0, 50.0 * 256.0);
    herd.place_mee(&area, hunter, &mut rng, &t, 128);
    let count = |h: &Herd, ct: usize| h.chars.iter().filter(|c| c.ctype == ct).count();
    // Three solo parasaurs; two packs of three raptors.
    assert_eq!((count(&herd, 1), count(&herd, 2)), (3, 6));
    // Capped: packs are never split (6 raptors count against 7, so
    // one parasaur stays); the packs' leaders still point at raptors.
    herd.cap_placed(7, |ct| ct >= 1);
    assert_eq!((count(&herd, 1), count(&herd, 2)), (1, 6));
    assert!(herd.packs.iter().all(|p| herd.chars[p.leader].ctype == 2));
    for c in &herd.chars {
        assert!(
            c.pos[0] > 300.0 * 256.0 && c.pos[0] < 900.0 * 256.0,
            "{:?}",
            c.pos
        );
    }
    // A strict secret cap includes packs and repairs a removed leader.
    for (i, p) in herd.packs.iter_mut().enumerate() {
        p.leader = herd.chars.iter().rposition(|c| c.pack == Some(i)).unwrap();
    }
    herd.cap_population(2, |ct| ct == 2);
    assert_eq!((count(&herd, 1), count(&herd, 2)), (1, 2));
    for c in &herd.chars {
        if let Some(p) = c.pack {
            assert_eq!(herd.chars[herd.packs[p].leader].pack, Some(p));
        }
    }
    herd.cap_population(0, |ct| ct == 2);
    assert_eq!((count(&herd, 1), count(&herd, 2)), (1, 0));
    assert!(herd.packs.is_empty());
    // A family with no AI here yet is never placed.
    herd.species[1].info.ai = -1;
    herd.place_mee(&area, hunter, &mut rng, &t, 128);
    assert_eq!(count(&herd, 1), 0);
}

fn dist(c: &Character, h: &HunterView) -> f32 {
    ((c.pos[0] - h.x).powi(2) + (c.pos[2] - h.z).powi(2)).sqrt()
}

#[test]
fn parasaurolophus_flees_a_close_hunter() {
    let area = Area::flat(10);
    let hunter = HunterView {
        x: 512.0 * 256.0,
        z: 512.0 * 256.0,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    let mut herd = herd_with(&area, ai::PARA, 6, [hunter.x + 800.0, 0.0, hunter.z]);
    let d0 = dist(&herd.chars[0], &hunter);
    run(&mut herd, &area, hunter, 5000);
    let c = &herd.chars[0];
    assert_eq!(c.state, 1, "alarmed");
    assert!(
        dist(c, &hunter) > d0 + 2000.0,
        "ran from {d0} to {}",
        dist(c, &hunter)
    );
    assert!(c.pos.iter().all(|v| v.is_finite()));
}

#[test]
fn hunting_trex_kills_at_close_range() {
    let area = Area::flat(10);
    let hunter = HunterView {
        x: 512.0 * 256.0,
        z: 512.0 * 256.0,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    let mut herd = herd_with(&area, ai::TREX, 11, [hunter.x + 3000.0, 0.0, hunter.z]);
    herd.chars[0].state = 1; // already hunting
    let events = run(&mut herd, &area, hunter, 10_000);
    assert!(
        events.iter().any(|e| matches!(
            e,
            DinoEvent::KillHunter {
                kind: hunt::KILL,
                ..
            }
        )),
        "no kill in {events:?}"
    );
}

#[test]
fn survival_waypoints_cannot_kill_the_hunter() {
    let area = Area::flat(10);
    let hunter = HunterView {
        x: 512.0 * 256.0,
        z: 512.0 * 256.0,
        y: area.land_height(512.0 * 256.0, 512.0 * 256.0),
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    for kind in [ai::ALLO, ai::TREX, ai::CHASM] {
        let mut herd = herd_with(&area, kind, 12, [hunter.x + 10_000.0, hunter.y, hunter.z]);
        let c = &mut herd.chars[0];
        c.wave = 1;
        c.state = 1;
        c.afraid_time = 60_000;
        c.lure = Some((c.pos[0] + 200.0, c.pos[2]));
        let events = run(&mut herd, &area, hunter, 100);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, DinoEvent::KillHunter { .. })),
            "AI {kind} killed at its waypoint"
        );
        assert!(herd.chars[0].pos[0] > hunter.x + 9000.0);
    }
}

#[test]
fn a_survival_raptor_keeps_coming_from_afar() {
    let area = Area::flat(10);
    let hunter = HunterView {
        x: 512.0 * 256.0,
        z: 512.0 * 256.0,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    // Beyond where a hunting raptor gives up (74 * 160 + 128 / 8).
    let start = [hunter.x + 14_000.0, 0.0, hunter.z];
    let moved = |wave| {
        let mut herd = herd_with(&area, ai::VELO, 10, start);
        herd.chars[0].state = 1;
        herd.chars[0].wave = wave;
        run(&mut herd, &area, hunter, 3000);
        dist(&herd.chars[0], &hunter) - 14_000.0
    };
    let (hunt, wave) = (moved(0), moved(1));
    assert!(hunt > 0.0, "a hunt's raptor goes off ({hunt})");
    assert!(wave < -1000.0, "a wave's comes on ({wave})");
}

#[test]
fn find_vector_alpha_matches_atan2() {
    for (x, y) in [
        (1.0, 0.0),
        (0.0, 1.0),
        (-1.0, 0.5),
        (-0.3, -2.0),
        (4.0, -1.0),
    ] {
        let want = f32::atan2(y, x).rem_euclid(2.0 * PI);
        assert!((find_vector_alpha(x, y) - want).abs() < 0.002, "{x},{y}");
    }
}

#[test]
fn mee_placement_has_no_128_character_cap() {
    // The Modder's Edition engine stops at 128 character entries
    // (`ChInfo[128]`); skin variants are entries of their own, so a game
    // with several common and rare skins per animal runs past it. Here 200
    // entries, each in a spawn group of its own, and the last spawns only
    // on area 2 (a map-specific skin, through `overwrite area2`).
    const N: usize = 200;
    let mut text = String::from("spawntable {\n");
    for _ in 0..N {
        text.push_str("spawngroup {\nspawnmin = 1\nspawnmax = 1\n}\n");
    }
    text.push_str("}\ncharacters {\n");
    for i in 0..N - 1 {
        text.push_str(&format!(
            "{{\nname = 'Skin {i}'\nai = 10\nspawninfo {{\nspawngroup = {i}\nspawnratio = 1\n}}\n}}\n"
        ));
    }
    text.push_str(&format!(
        "{{\nname = 'Map skin'\nai = 10\noverwrite area2 {{\nspawninfo {{\nspawngroup = {}\nspawnratio = 1\n}}\n}}\n}}\n}}\n",
        N - 1
    ));
    let raw = c2_formats::res_txt::ResTxt::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    let place = |area: u32| {
        let tables = c2_formats::mee::Tables::read(
            &raw,
            &c2_formats::mee::Hunt {
                area: Some(area),
                time: 1,
                picked: Default::default(),
            },
        );
        assert_eq!(tables.characters.len(), N);
        let mut herd = Herd {
            species: (0..N).map(|_| species(ai::PARA, 8)).collect(),
            ..Default::default()
        };
        herd.place_mee(
            &Area::flat(10),
            (50.0 * 256.0, 50.0 * 256.0),
            &mut MsRand::new(7),
            &tables,
            128,
        );
        let mut types: Vec<usize> = herd.chars.iter().map(|c| c.ctype).collect();
        types.sort_unstable();
        types.dedup();
        types
    };
    let on_1 = place(1);
    assert_eq!(
        on_1,
        (0..N - 1).collect::<Vec<_>>(),
        "every entry but the map skin"
    );
    let on_2 = place(2);
    assert_eq!(
        on_2,
        (0..N).collect::<Vec<_>>(),
        "the map skin joins on area 2"
    );
}
