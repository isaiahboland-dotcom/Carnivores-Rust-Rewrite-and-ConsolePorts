//! Shared synthetic hunt fixtures.

use crate::area::Area;
use crate::dino::{ai, Character, DinoInfo, Herd, Species, PI};
use crate::game::{GameEvent, Hunt, HuntInput, HuntOptions, WeaponAnims, WeaponInfo, Weapons};
use crate::player::Player;
use crate::rng::MsRand;
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};
use c2_formats::bits::Bits;
use c2_formats::car::{Animation, Car};
use c2_formats::model::{face_flags, Face, Mesh, Vertex};
use c2_formats::sav::{Save, TrophyItem};

/// A "dinosaur" that is one big upright triangle (800 wide, 800 tall).
pub(super) fn target_car(mortal: bool) -> Car {
    let tri = [-3200i16, 0, 0, 3200, 0, 0, 0, 6400, 0];
    let data: Vec<i16> = tri.iter().chain(tri.iter()).copied().collect();
    Car {
        name_raw: [0; 32],
        mesh: Mesh {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    owner: 0,
                    hide: 0
                };
                3
            ],
            faces: vec![Face {
                v: [0, 1, 2],
                tu: [0; 3],
                tv: [0; 3],
                flags: if mortal { face_flags::MORTAL } else { 0 },
                dmask: 0,
                distant: 0,
                next: 0,
                group: 0,
                reserved: [0; 12],
            }],
            texture: vec![],
        },
        animations: (0..8)
            .map(|_| Animation {
                name_raw: [0; 32],
                kps: 2,
                frames: 2,
                data: data.clone(),
            })
            .collect(),
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    }
}

pub(super) fn info(ai_id: i32) -> DinoInfo {
    DinoInfo {
        name: "t".into(),
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
    }
}

pub(super) fn rifle() -> WeaponInfo {
    WeaponInfo {
        name: "Rifle".into(),
        file: String::new(),
        pic: String::new(),
        power: 4.0,
        prec: 1.8,
        loud: 1.5,
        rate: 1.0,
        shots: 14,
        reload: 0,
        trace_c: 0,
        optic: false,
        fall: false,
        mee: None,
    }
}

pub(super) fn blank_save() -> Save {
    Save {
        player_name_raw: [0; 128],
        reg_number: 0,
        score: 0,
        rank: 0,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: Default::default(),
    }
}

/// Hunter at the map centre looking -z; a Stegosaurus-AI target 2000 ahead.
pub(super) fn setup(mortal: bool) -> (Area, Hunt, Car) {
    let area = Area::flat(10);
    let car = target_car(mortal);
    let player = Player::new(&area, (500, 500));
    let mut rng = MsRand::new(5);
    let species = vec![
        Species::new(info(0), &car),
        Species::new(info(ai::STEGO), &car),
    ];
    let mut herd = Herd {
        species,
        chars: vec![],
        demo_index: None,
        ..Default::default()
    };
    let mut c = Character {
        ctype: 1,
        alpha: PI / 2.0,
        pos: [player.x, 640.0, player.z - 2000.0],
        ..Default::default()
    };
    c.reset(&herd.species[1], &mut rng, 128);
    herd.chars.push(c);
    let weapons = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        false,
    );
    let opts = HuntOptions::default();
    let hunt = Hunt::new(&area, player, herd, weapons, rng, opts, blank_save());
    (area, hunt, car)
}

/// Ticks without stepping the herd (keeps the target still).
pub(super) fn weapon_ticks(
    h: &mut Hunt,
    area: &Area,
    cars: &[Option<&Car>],
    input: HuntInput,
    n: usize,
    out: &mut Vec<GameEvent>,
) {
    for _ in 0..n {
        h.weapon_input(area, cars, &input, out);
        h.apply_recoil(10);
        h.weapon_step(10, out);
        h.animate_bullets(area, cars, 10, out);
        h.track_step(10);
    }
}
