//! A kill made by predation is the animal's, not the hunter's: it scores
//! nothing, calls no ship and counts as no success. Play Vanilla relies on
//! this, sharing saves between hunts with and without predation
//! (`docs/RECIPE-SETTINGS.md`).

use alloc::{string::String, vec, vec::Vec};

use c2_formats::bits::Bits;
use c2_formats::predation::Predation;
use c2_formats::sav::{Save, TrophyItem};

use super::{herd, RAPTOR, STEGO};
use crate::area::Area;
use crate::game::{GameEvent, Hunt, HuntInput, HuntOptions, WeaponAnims, WeaponInfo, Weapons};
use crate::player::Player;
use crate::rng::MsRand;

fn rifle() -> WeaponInfo {
    WeaponInfo {
        allow_tranq: true,
        allow_trophy: true,
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

#[test]
fn a_predation_kill_scores_nothing_for_the_hunter() {
    let area = Area::flat(10);
    let h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 1500.0)],
    );
    // The hunter far off in a corner, out of the animals' way.
    let player = Player::new(&area, (60, 60));
    let weapons = Weapons::new(
        vec![rifle()],
        vec![WeaponAnims::uniform(100)],
        Bits::bit(0),
        false,
    );
    let save = Save {
        player_name_raw: [0; 128],
        reg_number: 0,
        score: 120,
        rank: 1,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: Default::default(),
    };
    let opts = HuntOptions::default();
    let mut hunt = Hunt::new(&area, player, h, weapons, MsRand::new(5), opts, save);
    let score = hunt.room.score;
    let cars = vec![None; hunt.herd.species.len()];
    let mut events: Vec<GameEvent> = Vec::new();
    for _ in 0..3_000 {
        hunt.tick(&area, &cars, &HuntInput::default(), 20, &mut events);
        if hunt.herd.chars[STEGO].health == 0 {
            break;
        }
    }
    assert_eq!(
        hunt.herd.chars[STEGO].health, 0,
        "the raptor never caught the stego"
    );
    assert!(hunt.herd.chars[STEGO].predation.killed_by_animal);
    assert_eq!(hunt.room.score, score, "a predation kill changed the score");
    assert!(
        hunt.kills.is_empty(),
        "a predation kill was recorded as the hunter's"
    );
    assert!(
        hunt.ship.tasks.is_empty(),
        "the ship was called for a predation kill"
    );
    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::Scored { .. })),
        "a predation kill was scored"
    );
}
