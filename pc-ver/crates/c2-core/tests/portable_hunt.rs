#![cfg(feature = "portable-hunt")]

use c2_core::{
    area::Area,
    dino::Herd,
    game::{
        weapon_anim, GameEvent, Hunt, HuntInput, HuntOptions, WeaponInfo, WeaponState, Weapons,
    },
    player::Player,
    rng::MsRand,
};
use c2_formats::{
    bits::Bits,
    sav::{Save, TrophyItem},
};

#[test]
fn portable_animal_audio_fades_with_distance() {
    let eye = [0.0, 0.0, 0.0];
    let near = c2_core::sound::lr_gains(256.0, Some([0.0, 0.0, -512.0]), eye, 0.0);
    let far = c2_core::sound::lr_gains(256.0, Some([0.0, 0.0, -11000.0]), eye, 0.0);
    assert!(near.0 > 0.0 && near.1 > 0.0);
    assert_eq!(far, (0.0, 0.0));
}

#[test]
fn portable_hunt_draws_and_fires_a_weapon() {
    let area = Area::flat(10);
    let player = Player::new(&area, (500, 500));
    let rifle = WeaponInfo {
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
    };
    let weapons = Weapons::new(vec![rifle], vec![[100; 4]], Bits::bit(0), false);
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
        Herd::default(),
        weapons,
        MsRand::new(5),
        HuntOptions::default(),
        save,
    );
    let mut events = Vec::new();
    hunt.tick(
        &area,
        &[],
        &HuntInput {
            toggle_weapon: true,
            ..Default::default()
        },
        10,
        &mut events,
    );
    assert_eq!(hunt.weapons.state, WeaponState::Drawing);
    for _ in 0..12 {
        hunt.tick(&area, &[], &HuntInput::default(), 10, &mut events);
    }
    assert_eq!(hunt.weapons.state, WeaponState::Ready);
    hunt.tick(
        &area,
        &[],
        &HuntInput {
            fire: true,
            ..Default::default()
        },
        10,
        &mut events,
    );
    assert_eq!(hunt.weapons.shots_left[0], 13);
    assert!(events.contains(&GameEvent::WeaponSound {
        weapon: 0,
        slot: weapon_anim::FIRE
    }));
}
