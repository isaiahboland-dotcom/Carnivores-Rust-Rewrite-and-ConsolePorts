#![cfg(feature = "portable-dino")]

use c2_core::area::Area;
use c2_core::dino::{ai, Character, Ctx, DinoInfo, Herd, HunterView, Species};
use c2_core::rng::MsRand;
use c2_formats::car::{Animation, Car};

#[test]
fn shared_herbivore_ai_flees_a_nearby_hunter() {
    let area = Area::flat(10);
    let car = Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: (0..6)
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
        ai: ai::PARA,
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
    };
    let hunter = HunterView {
        x: 512.0 * 256.0,
        z: 512.0 * 256.0,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    let pos = [hunter.x + 800.0, 0.0, hunter.z];
    let mut herd = Herd {
        species: vec![Species::new(info, &car)],
        ..Default::default()
    };
    let mut character = Character {
        pos,
        tgx: pos[0],
        tgz: pos[2],
        ..Default::default()
    };
    character.reset(&herd.species[0], &mut MsRand::new(1), 128);
    character.pos[1] = area.land_height(pos[0], pos[2]);
    herd.chars.push(character);
    let mut rng = MsRand::new(3);
    let mut events = Vec::new();
    for t in 0..500 {
        herd.step(&mut Ctx {
            area: &area,
            rng: &mut rng,
            dt: 10,
            real_time: t * 10,
            hunter,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            render_r: 74,
            run_k: 1.0,
            events: &mut events,
        });
    }
    let c = &herd.chars[0];
    let distance = ((c.pos[0] - hunter.x).powi(2) + (c.pos[2] - hunter.z).powi(2)).sqrt();
    assert_eq!(c.state, 1);
    assert!(
        distance > 2800.0,
        "animal travelled only {distance} from hunter"
    );
    assert!(c.pos.iter().all(|v| v.is_finite()));
}
