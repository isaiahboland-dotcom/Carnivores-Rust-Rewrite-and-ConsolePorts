#![cfg(feature = "portable-hunt")]
use c2_core::{
    area::{Area, TimeOfDay},
    dino::{self, Character, DinoInfo, Herd, Species},
    game::{GameEvent, Hunt, HuntInput, HuntOptions, WeaponInfo, WeaponState, Weapons},
    model::{car_anim_duration_ms, pose_car},
    player::Player,
    rng::MsRand,
};
use c2_formats::{
    bits::Bits,
    car, map,
    res_txt::ResTxt,
    rsc,
    sav::{Save, TrophyItem},
};

#[test]
fn original_pistol_and_animals_load_and_simulate() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let read = |p: &str| std::fs::read(std::path::Path::new(&root).join(p)).unwrap();
    let res = ResTxt::parse(&String::from_utf8(read("_RES.TXT")).unwrap())
        .unwrap()
        .to_data();
    let mosh = car::parse(&read("MOSH.CAR")).unwrap();
    let para = car::parse(&read("PAR2.CAR")).unwrap();
    let pistol = car::parse(&read("WEAPONS/PISTOL.CAR")).unwrap();
    for name in [
        "SOUNDFX/CALLS/CALL1_A.WAV",
        "SOUNDFX/CALLS/CALL1_B.WAV",
        "SOUNDFX/CALLS/CALL1_C.WAV",
    ] {
        let wav = c2_formats::wav::parse(&read(name)).unwrap();
        assert_eq!(wav.format, Some(c2_formats::wav::WavFormat::VANILLA));
        assert!(!wav.pcm.is_empty() && wav.pcm.len() % 2 == 0);
    }
    for name in [
        "SOUNDFX/STEPS/HWALK1.WAV",
        "SOUNDFX/STEPS/HWALK2.WAV",
        "SOUNDFX/STEPS/HWALK3.WAV",
        "SOUNDFX/STEPS/FOOTW1.WAV",
        "SOUNDFX/STEPS/FOOTW2.WAV",
        "SOUNDFX/STEPS/FOOTW3.WAV",
        "SOUNDFX/A_UNDERW.WAV",
        "SOUNDFX/HUM_DIE1.WAV",
        "SOUNDFX/HUM_DIE2.WAV",
        "SOUNDFX/HUM_DIE3.WAV",
        "SOUNDFX/HUM_DIE4.WAV",
    ] {
        let wav = c2_formats::wav::parse(&read(name)).unwrap();
        assert_eq!(
            wav.format,
            Some(c2_formats::wav::WavFormat::VANILLA),
            "{name}"
        );
        assert!(!wav.pcm.is_empty() && wav.pcm.len() % 2 == 0, "{name}");
    }
    for name in [
        "MENU/PICS/WEAPON1.TGA",
        "MENU/PICS/CALL1.TGA",
        "MENU/MENUM.TGA",
        "MENU/PICS/AREA1.TGA",
        "MENU/PICS/DINO1.TGA",
        "MENU/PICS/EQUIP2.TGA",
    ] {
        let picture = c2_formats::tga::parse(&read(name)).unwrap();
        assert!(picture.width > 0 && picture.height > 0);
        assert_eq!(picture.pixels.len(), picture.width * picture.height);
    }
    let find_char = |name: &str| {
        res.characters
            .iter()
            .find(|d| {
                d.file
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(name))
            })
            .unwrap()
    };
    let weapon = res
        .weapons
        .iter()
        .find(|d| {
            d.file
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("pistol.car"))
        })
        .unwrap();
    let info = WeaponInfo::from_def(weapon);
    assert_eq!(info.shots, 16);
    let anims = core::array::from_fn(|slot| {
        pistol
            .animations
            .get(info.anim_of(slot))
            .map_or(0, |a| car_anim_duration_ms(a) as i32)
    });
    assert!(anims[0] > 0 && anims[1] > 0);
    assert!(pose_car(&pistol.animations[0], pistol.mesh.vertices.len(), 0).is_some());
    let area = Area::new(
        map::parse(&read("AREAS/AREA1.MAP")).unwrap(),
        rsc::parse(&read("AREAS/AREA1.RSC")).unwrap(),
        TimeOfDay::Day,
    );
    assert!(area
        .rsc
        .ambients
        .iter()
        .any(|a| !a.pcm.is_empty() && a.pcm.len() % 2 == 0));
    assert!(area
        .rsc
        .random_sounds
        .iter()
        .any(|pcm| !pcm.is_empty() && pcm.len() % 2 == 0));
    let player = Player::new(&area, area.landings[0]);
    let mut rng = MsRand::new(1);
    let mut herd = Herd {
        species: vec![
            Species::new(DinoInfo::from_def(find_char("mosh.car")), &mosh),
            Species::new(DinoInfo::from_def(find_char("par2.car")), &para),
        ],
        ..Default::default()
    };
    for (id, offset) in [(0, -768.0), (1, 768.0)] {
        let pos = (4..40)
            .find_map(|step| {
                let x = player.x + offset;
                let z = player.z - step as f32 * 256.0;
                let mut p = [x, area.land_height(x, z), z];
                (!dino::check_place_collision_p(&area, &mut p)).then_some(p)
            })
            .expect("valid spawn");
        let mut c = Character {
            ctype: id,
            pos,
            tgx: pos[0],
            tgz: pos[2],
            ..Default::default()
        };
        c.reset(&herd.species[id], &mut rng, 128);
        c.pos[1] = area.land_height(pos[0], pos[2]);
        assert!(dino::pose::pose(&c, [&mosh, &para][id], &[]).is_some());
        herd.chars.push(c);
    }
    let save = Save {
        player_name_raw: [0; 128],
        reg_number: 0,
        score: 0,
        rank: 0,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: Default::default(),
    };
    let mut hunt = Hunt::new(
        &area,
        player,
        herd,
        Weapons::new(vec![info], vec![anims], Bits::bit(0), false),
        rng,
        HuntOptions {
            targets: Bits::bit(10),
            radar: true,
            ..Default::default()
        },
        save,
    );
    assert!(!hunt.is_target(&hunt.herd.chars[0]) && hunt.is_target(&hunt.herd.chars[1]));
    let mut events = Vec::new();
    let cars = [Some(&mosh), Some(&para)];
    hunt.tick(
        &area,
        &cars,
        &HuntInput {
            call: true,
            ..Default::default()
        },
        16,
        &mut events,
    );
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::Call {
            call: 10,
            pos: None,
            ..
        }
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::Ambient { zone: Some(_), .. })));
    use c2_core::{
        player::PlayerEvent,
        sound::{self, SoundCmd, SoundId},
    };
    let fx = sound::commands(
        &[
            GameEvent::Player(PlayerEvent::Step),
            GameEvent::Player(PlayerEvent::StepWater),
            GameEvent::Player(PlayerEvent::Land),
            GameEvent::Ambient {
                zone: None,
                volume: 240,
            },
        ],
        &hunt,
    );
    assert!(matches!(
        fx[0],
        SoundCmd::Play {
            id: SoundId::Step(_),
            ..
        }
    ));
    assert!(matches!(
        fx[1],
        SoundCmd::Play {
            id: SoundId::StepWater(_),
            ..
        }
    ));
    assert!(matches!(
        fx[2],
        SoundCmd::Play {
            id: SoundId::Step(_),
            ..
        }
    ));
    assert!(matches!(
        fx[3],
        SoundCmd::Ambient {
            zone: None,
            volume: 240
        }
    ));
    hunt.tick(
        &area,
        &cars,
        &HuntInput {
            toggle_weapon: true,
            ..Default::default()
        },
        16,
        &mut events,
    );
    for _ in 0..300 {
        hunt.tick(&area, &cars, &HuntInput::default(), 16, &mut events);
    }
    assert_eq!(hunt.weapons.state, WeaponState::Ready);
    hunt.tick(
        &area,
        &cars,
        &HuntInput {
            fire: true,
            ..Default::default()
        },
        16,
        &mut events,
    );
    assert_eq!(hunt.weapons.shots_left[0], 15);
}
