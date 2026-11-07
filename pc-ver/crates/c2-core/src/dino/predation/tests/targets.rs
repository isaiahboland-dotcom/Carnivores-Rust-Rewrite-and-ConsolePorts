//! The AI's target is explicit ([`crate::dino::Target`]): a steered animal
//! chases its prey while `ctx.hunter` stays the player, its kills go to
//! the prey, and its pack hears only of the player. Stock and dialect AIs.

use std::cell::Cell;

use c2_formats::predation::Predation;

use super::super::run::{Goal, Predators};
use super::{herd, run, species, Rules, RAPTOR, STEGO};
use crate::area::Area;
use crate::dino::mee::{DeathType, KillType, MeeInfo};
use crate::dino::{ai, DinoEvent, Herd, Pack, Species};

/// A dialect species on the predation roster's model.
fn dialect(name: &str, ai_id: i32, price: i32, aggress: i32) -> Species {
    let mut m = MeeInfo::new(ai_id);
    for a in 0..8 {
        m.add_anim(a);
    }
    (m.run, m.walk, m.jump) = (0, 1, 2);
    (m.run_speed, m.walk_speed, m.jump_speed) = (1.2, 0.3, 1.5);
    m.aggress = aggress;
    if aggress == 0 {
        // Prey runs slower than its hunters, as in the stock roster.
        m.run_speed = 0.8;
    }
    m.kill_dist = 300;
    // Its only kill is staged with the hunter's body: animals sniff.
    m.kill_types = vec![KillType {
        hunter_anim: 7,
        anim: 6,
        hunter_sync: true,
        ..Default::default()
    }];
    m.smells = vec![5];
    m.death_types = vec![DeathType {
        die: 4,
        sleep: 3,
        ..Default::default()
    }];
    let mut sp = species(name, ai_id, price);
    sp.mee = Some(Box::new(m));
    sp
}

/// A herd of `roster` at `(ctype, dx)` east of the map centre (the hunter
/// is ~300 m south-west, beyond hearing).
fn herd_of(roster: Vec<Species>, at: &[(usize, f32)]) -> (Area, Herd) {
    let area = Area::flat(10);
    let mut h = herd(&Predation::default(), &area, &[]);
    let rules = Rules::new(&Predation::default(), &roster);
    h.species = roster;
    h.predation = Some(Box::new(Predators::new(rules)));
    for &(ctype, dx) in at {
        let mut one = herd(&Predation::default(), &area, &[(0, dx)]);
        let mut c = one.chars.remove(0);
        c.ctype = ctype;
        c.reset(&h.species[ctype], &mut crate::rng::MsRand::new(1), 128);
        h.chars.push(c);
    }
    (area, h)
}

fn hunter_kills(ev: &[DinoEvent]) -> usize {
    ev.iter()
        .filter(|e| matches!(e, DinoEvent::KillHunter { .. }))
        .count()
}

#[test]
fn a_steered_chase_is_heard_from_the_player_not_the_prey() {
    // Every clip has a sound; the chase is far beyond the player's hearing
    // (68 cells), so none may reach him.
    for stock in [true, false] {
        let roster = if stock {
            vec![
                species("Raptor", ai::VELO, 10),
                species("Stego", ai::STEGO, 5),
            ]
        } else {
            vec![
                dialect("Raptor", ai::VELO, 10, 50),
                dialect("Stego", ai::STEGO, 5, 0),
            ]
        };
        let (area, mut h) = herd_of(roster, &[(RAPTOR, 0.0), (STEGO, 1500.0)]);
        for sp in &mut h.species {
            sp.anifx = vec![0; 12];
        }
        let ev = run(&mut h, &area, 60_000, |h| h.chars[1].health == 0);
        assert_eq!(h.chars[1].health, 0, "stock {stock}: the stego got away");
        let sounds = ev
            .iter()
            .filter(|e| matches!(e, DinoEvent::Sound { .. }))
            .count();
        assert_eq!(sounds, 0, "stock {stock}: heard {sounds} sounds 300 m away");
        assert_eq!(hunter_kills(&ev), 0, "stock {stock}");
    }
}

#[test]
fn a_dialect_kill_strikes_the_prey_without_the_hunter_kill_clip() {
    let (area, mut h) = herd_of(
        vec![
            dialect("Raptor", ai::VELO, 10, 50),
            dialect("Stego", ai::STEGO, 5, 0),
        ],
        &[(RAPTOR, 0.0), (STEGO, 1500.0)],
    );
    let ev = run(&mut h, &area, 60_000, |h| h.chars[1].health == 0);
    assert_eq!(h.chars[1].health, 0, "the stego got away");
    assert!(h.chars[1].predation.killed_by_animal);
    assert_eq!(hunter_kills(&ev), 0);
    // Not the staged kill clip (6): it sniffs at the body.
    assert_eq!(h.chars[0].phase, 5);
    assert!(h.chars[0].predation.eat_ms > 0);
}

#[test]
fn a_hunting_pack_member_raises_no_pack_alarm() {
    // Two dialect raptors in one pack; the nearer hunts the stego, the
    // other has no goal. Neither the alarm nor the attack flag may be
    // raised, so the idle one never turns on the (far) hunter.
    let (area, mut h) = herd_of(
        vec![
            dialect("Raptor", ai::VELO, 10, 50),
            dialect("Stego", ai::STEGO, 5, 0),
        ],
        &[(RAPTOR, 0.0), (RAPTOR, -900.0), (STEGO, 1500.0)],
    );
    h.packs.push(Pack {
        leader: 0,
        ..Default::default()
    });
    for c in &mut h.chars[..2] {
        c.pack = Some(0);
        c.pack_density = 4.0;
    }
    let raised = Cell::new(false);
    let ev = run(&mut h, &area, 60_000, |h| {
        let p = h.packs[0];
        raised.set(raised.get() || p.alert || p.attack);
        h.chars[2].health == 0
    });
    assert_eq!(h.chars[2].health, 0, "the stego got away");
    assert!(!raised.get(), "a pack flag was raised by a prey chase");
    assert_eq!(hunter_kills(&ev), 0);
    assert!(!h.chars[1].aware_hunter);
}

#[test]
fn a_dialect_herd_flees_the_predator_not_the_hunter() {
    // Two dialect stegos in one pack west of a raptor; the far one cannot
    // sense it. When the near one flees, the far one flees the raptor too
    // (west), not the hunter (who is south-west, so away is north-east).
    let (area, mut h) = herd_of(
        vec![
            dialect("Raptor", ai::VELO, 10, 50),
            dialect("Stego", ai::STEGO, 5, 0),
        ],
        &[(RAPTOR, 0.0), (STEGO, -2500.0), (STEGO, -9000.0)],
    );
    h.packs.push(Pack {
        leader: 1,
        ..Default::default()
    });
    for c in &mut h.chars[1..] {
        c.pack = Some(0);
        c.pack_density = 0.0;
    }
    let far = h.chars[2].pos;
    run(&mut h, &area, 600, |_| false);
    assert_eq!(h.chars[1].predation.goal, Goal::Flee(0));
    assert_eq!(
        h.chars[2].predation.goal,
        Goal::Flee(0),
        "the pack ran apart"
    );
    run(&mut h, &area, 3_000, |_| false);
    let moved = [h.chars[2].pos[0] - far[0], h.chars[2].pos[2] - far[2]];
    assert!(moved[0] < -300.0, "did not run from the raptor: {moved:?}");
}
