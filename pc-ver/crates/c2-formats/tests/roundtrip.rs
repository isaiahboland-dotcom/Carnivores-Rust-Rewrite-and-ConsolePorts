//! Synthetic round-trip tests. Fixtures are generated here; no game data.

use c2_formats::model::{Bone, Face, Mesh, StaticModel, Vertex, TEXTURE_WIDTH};
use c2_formats::{car, map, rsc, three_df, FormatErrorKind};

fn mesh(verts: usize, faces: usize, tex_rows: usize) -> Mesh {
    Mesh {
        vertices: (0..verts)
            .map(|i| Vertex {
                x: i as f32,
                y: -(i as f32) * 0.5,
                z: 3.25,
                owner: (i % 3) as i16,
                hide: 0,
            })
            .collect(),
        faces: (0..faces)
            .map(|i| Face {
                v: [0, 1, (i % verts.max(1)) as i32],
                tu: [0, 255, 128],
                tv: [10, 20, 30],
                flags: (i as u16) & 0x0f,
                dmask: 0,
                distant: i as i32,
                next: -1,
                group: 0,
                reserved: [i as u8; 12],
            })
            .collect(),
        texture: (0..TEXTURE_WIDTH * tex_rows).map(|i| i as u16).collect(),
    }
}

fn static_model() -> StaticModel {
    let mut name_raw = [0u8; 32];
    name_raw[..4].copy_from_slice(b"root");
    StaticModel {
        mesh: mesh(5, 4, 2),
        bones: vec![Bone {
            name_raw,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            owner: -1,
            hide: 0,
        }],
    }
}

#[test]
fn three_df_roundtrip() {
    let m = static_model();
    let bytes = three_df::write(&m);
    assert_eq!(
        bytes.len(),
        16 + 4 * 64 + 5 * 16 + 48 + TEXTURE_WIDTH * 2 * 2
    );
    assert_eq!(three_df::parse(&bytes).ok(), Some(m));
}

fn sample_car(anim_sounds: bool) -> car::Car {
    let mesh = mesh(3, 1, 1);
    let mut name_raw = [0u8; 32];
    name_raw[..5].copy_from_slice(b"Dino1");
    car::Car {
        name_raw,
        animations: vec![car::Animation {
            name_raw: [b'a'; 32],
            kps: 10,
            frames: 2,
            data: (0..2 * 3 * 3).collect(),
        }],
        sounds: vec![car::Sound {
            name_raw: [b's'; 32],
            pcm: vec![1, 2, 3, 4],
        }],
        anim_sounds: anim_sounds.then_some([-1; car::ANIFX_SLOTS]),
        mesh,
    }
}

#[test]
fn car_roundtrip_with_and_without_anifx() {
    for with in [true, false] {
        let c = sample_car(with);
        let parsed = car::parse(&car::write(&c));
        assert_eq!(parsed.as_ref().ok(), Some(&c));
        assert_eq!(c.name(), "Dino1");
    }
}

#[test]
fn truncated_input_is_an_error_not_a_panic() {
    let bytes = car::write(&sample_car(true));
    for cut in [0, 7, 32, 50, 100, bytes.len() - 300] {
        assert!(car::parse(&bytes[..cut]).is_err(), "cut at {cut}");
    }
}

#[test]
fn absurd_counts_are_rejected() {
    let mut bytes = three_df::write(&static_model());
    bytes[0..4].copy_from_slice(&i32::MAX.to_le_bytes());
    let err = three_df::parse(&bytes).err().map(|e| e.kind);
    assert!(matches!(err, Some(FormatErrorKind::BadCount(_))));

    bytes[0..4].copy_from_slice(&(-1i32).to_le_bytes());
    assert!(three_df::parse(&bytes).is_err());
}

#[test]
fn map_roundtrip() {
    let cells = |seed: u8| {
        (0..map::CELLS)
            .map(|i| (i as u8).wrapping_add(seed))
            .collect::<Vec<u8>>()
    };
    let words = |seed: u16| {
        (0..map::CELLS)
            .map(|i| (i as u16).wrapping_mul(seed))
            .collect::<Vec<u16>>()
    };
    let m = map::Map {
        height: cells(1),
        texture1: words(3),
        texture2: words(5),
        objects: cells(2),
        flags: words(7),
        light: [cells(3), cells(4), cells(5)],
        water: cells(6),
        object_height: cells(7),
        fogs: vec![1; map::HALF_CELLS],
        ambient: vec![2; map::HALF_CELLS],
    };
    let bytes = map::write(&m);
    assert_eq!(bytes.len(), map::FILE_SIZE);
    assert_eq!(map::parse(&bytes).ok(), Some(m));
}

#[test]
fn rsc_roundtrip() {
    let anim_model = static_model();
    let vc = anim_model.mesh.vertices.len();
    let objects = vec![
        rsc::Object {
            info: rsc::ObjectInfo {
                radius: 10,
                y_lo: 0,
                y_hi: 100,
                line_length: 128,
                line_intensity: 1,
                circle_radius: 2,
                circle_intensity: 3,
                flags: rsc::object_flags::PLACE_GROUND,
                grass_radius: 0,
                default_light: 0,
                last_ani_time: 0,
                bound_r: 0.0,
                reserved: [0; 16],
            },
            model: static_model(),
            sprite: vec![7; rsc::SPRITE_SIZE * rsc::SPRITE_SIZE],
            animation: None,
        },
        rsc::Object {
            info: rsc::ObjectInfo {
                radius: 1,
                y_lo: 0,
                y_hi: 0,
                line_length: 0,
                line_intensity: 0,
                circle_radius: 0,
                circle_intensity: 0,
                flags: rsc::object_flags::ANIMATED,
                grass_radius: 0,
                default_light: 0,
                last_ani_time: 0,
                bound_r: 0.0,
                reserved: [9; 16],
            },
            sprite: vec![8; rsc::SPRITE_SIZE * rsc::SPRITE_SIZE],
            // `frames_stored` is one less than the stored keyframes.
            animation: Some(rsc::ObjectAnimation {
                vcount_a: vc as i32,
                vcount: vc,
                kps: 8,
                frames_stored: 1,
                data: vec![5; 2 * vc * 3],
            }),
            model: anim_model,
        },
    ];
    let r = rsc::Rsc {
        fade_rgb: [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        trans_rgb: [[9, 8, 7], [6, 5, 4], [3, 2, 1]],
        textures: vec![vec![1; 128 * 128], vec![2; 128 * 128]],
        objects,
        skies: [vec![1; 256 * 256], vec![2; 256 * 256], vec![3; 256 * 256]],
        sky_map: vec![4; 128 * 128],
        fogs: vec![rsc::Fog {
            rgb: 0x112233,
            y_begin: 1.5,
            mortal: 0,
            transp: 0.5,
            f_limit: 100.0,
        }],
        random_sounds: vec![vec![1, 2], vec![]],
        ambients: vec![rsc::Ambient {
            pcm: vec![9; 10],
            random: [rsc::RandomSoundRef {
                number: 1,
                volume: 2,
                freq: 3,
                envir: 4,
                flags: 5,
            }; rsc::AMBIENT_RANDOM_SLOTS],
            random_count: 1,
            volume: 200,
        }],
        waters: vec![rsc::Water {
            texture_index: 1,
            level: 40,
            transp: 0.7,
            fog_rgb: 0,
        }],
    };
    assert_eq!(rsc::parse(&rsc::write(&r)).ok(), Some(r));
}
