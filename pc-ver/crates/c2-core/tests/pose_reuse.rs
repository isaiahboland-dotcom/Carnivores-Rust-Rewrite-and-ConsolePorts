use c2_core::dino::{pose, Character, PMORPH_TIME};
use c2_formats::car::{Animation, Car};
use c2_formats::model::{Mesh, Vertex};

fn car() -> Car {
    let data = vec![8, 16, 24, -8, 0, -16, 40, 48, 56, 24, 32, 16];
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
                2
            ],
            ..Default::default()
        },
        animations: vec![
            Animation {
                name_raw: [0; 32],
                kps: 2,
                frames: 2,
                data: data.clone(),
            },
            Animation {
                name_raw: [0; 32],
                kps: 2,
                frames: 2,
                data: data.iter().map(|v| v + 80).collect(),
            },
        ],
        sounds: vec![],
        anim_sounds: None,
        trailing: vec![],
    }
}

fn assert_positions(actual: &[[f32; 3]], expected: &[[f32; 3]]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
        assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
    }
}

#[test]
fn interpolation_morph_and_transform_match_known_positions() {
    let car = car();
    let mut c = Character {
        scale: 1.0,
        ftime: 250,
        ..Default::default()
    };
    let positions = pose::pose(&c, &car, &[]).unwrap_or_default();
    assert_positions(&positions, &[[2.0, 3.0, -4.0], [0.0, 1.0, 1.0]]);

    c.prev_phase = 1;
    c.prev_pf_time = 250;
    c.pp_morph_time = PMORPH_TIME / 4;
    c.scale = 2.0;
    c.beta = core::f32::consts::FRAC_PI_2;
    c.gamma = core::f32::consts::FRAC_PI_2;
    let positions = pose::pose(&c, &car, &[1, 0]).unwrap_or_default();
    assert_positions(&positions, &[[-11.0, 13.0, 9.0], [-7.0, 3.0, 5.0]]);
    let mut out = Vec::new();
    let positions = pose::pose_into(&c, &car, &[1, 0], &mut out).unwrap_or_default();
    assert_positions(positions, &[[-11.0, 13.0, 9.0], [-7.0, 3.0, 5.0]]);
}

#[test]
fn front_and_tail_bends_keep_opposite_directions() {
    let mut car = car();
    car.animations[0].frames = 1;
    car.animations[0].data = vec![800, 0, -3200, 800, 0, 3200];
    let c = Character {
        scale: 1.0,
        bend: core::f32::consts::FRAC_PI_2,
        ..Default::default()
    };
    let mut out = Vec::new();
    let positions = pose::pose_into(&c, &car, &[], &mut out).unwrap_or_default();
    assert_positions(positions, &[[-400.0, 0.0, 100.0], [-400.0, 0.0, -100.0]]);
}

#[test]
fn pose_storage_is_reused_and_invalid_clips_clear_previous_vertices() {
    let mut car = car();
    let mut c = Character {
        scale: 1.0,
        ..Default::default()
    };
    let mut out = Vec::with_capacity(32);
    let ptr = out.as_ptr();
    let capacity = out.capacity();
    for time in [0, 250, 1000, 10_000] {
        c.ftime = time;
        assert!(pose::pose_into(&c, &car, &[], &mut out).is_some());
        assert_eq!(out.len(), 2);
        assert_eq!(out.as_ptr(), ptr);
        assert_eq!(out.capacity(), capacity);
    }
    c.phase = 99;
    assert!(pose::pose_into(&c, &car, &[], &mut out).is_none());
    assert!(out.is_empty());
    assert_eq!(out.as_ptr(), ptr);
    c.phase = 0;
    assert!(pose::pose_into(&c, &car, &[], &mut out).is_some());
    car.animations[0].data.pop();
    assert!(pose::pose_into(&c, &car, &[], &mut out).is_none());
    assert!(out.is_empty());
    assert_eq!(out.as_ptr(), ptr);
}

#[test]
fn car_pose_reuses_storage_and_preserves_wrapping_and_invalid_clip_behavior() {
    use c2_core::model::pose_car_into;
    let mut car = car();
    let mut out = Vec::with_capacity(8);
    let ptr = out.as_ptr();
    assert_positions(
        pose_car_into(&car.animations[0], 2, 250, &mut out).unwrap_or_default(),
        &[[2.0, 3.0, -4.0], [0.0, 1.0, 1.0]],
    );
    assert_positions(
        pose_car_into(&car.animations[0], 2, 1000, &mut out).unwrap_or_default(),
        &[[1.0, 2.0, -3.0], [-1.0, 0.0, 2.0]],
    );
    assert_eq!(out.as_ptr(), ptr);
    assert!(pose_car_into(&car.animations[0], 3, 250, &mut out).is_none());
    assert!(out.is_empty());
    car.animations[0].kps = 0;
    assert_positions(
        pose_car_into(&car.animations[0], 2, 1234, &mut out).unwrap_or_default(),
        &[[1.0, 2.0, -3.0], [-1.0, 0.0, 2.0]],
    );
    car.animations[0].frames = 0;
    assert!(pose_car_into(&car.animations[0], 2, 0, &mut out).is_none());
    assert!(out.is_empty());
    assert_eq!(out.as_ptr(), ptr);
}
