//! Synthetic predation tests: made-up species on a flat area.

use c2_formats::car::{Animation, Car};
use c2_formats::predation::{Animal, Diet, Predation, Priority};

use super::run::{feed_phase, Goal, Predators};
use super::{Rel, Role, Rules};
use crate::area::Area;
use crate::dino::mee::{KillType, MeeInfo};
use crate::dino::{
    ai, hunt, CarnivoreParams, Character, Ctx, DinoEvent, DinoInfo, Herd, HunterView, Species,
};
use crate::rng::MsRand;

fn species(name: &str, ai_id: i32, price: i32) -> Species {
    let car = Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: (0..12)
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
        name: name.into(),
        file: String::new(),
        ai: ai_id,
        health0: 5,
        mass: 1.0,
        length: 1.0,
        radius: 128.0,
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
    let mut sp = Species::new(info, &car);
    sp.hunt_price = Some(price);
    sp
}

/// Raptor 10, Stego 5, Allo 20, Spino 19, T-Rex 30.
fn roster() -> Vec<Species> {
    vec![
        species("Raptor", ai::VELO, 10),
        species("Stego", ai::STEGO, 5),
        species("Allo", ai::ALLO, 20),
        species("Spino", ai::SPINO, 19),
        species("Rex", ai::TREX, 30),
    ]
}

const RAPTOR: usize = 0;
const STEGO: usize = 1;
const ALLO: usize = 2;
const SPINO: usize = 3;
const REX: usize = 4;

fn fights() -> Predation {
    Predation {
        carnivore_fights: true,
        ..Predation::default()
    }
}

#[test]
fn prices_make_the_food_chain() {
    let r = Rules::new(&fights(), &roster());
    assert_eq!(r.kind(STEGO).role, Role::Herbivore);
    assert_eq!(r.kind(REX).role, Role::Carnivore);
    assert_eq!(r.rel(RAPTOR, STEGO), Rel::Hunts);
    assert_eq!(r.rel(STEGO, RAPTOR), Rel::Ignore);
    assert_eq!(r.rel(ALLO, RAPTOR), Rel::Hunts);
    assert_eq!(r.rel(RAPTOR, ALLO), Rel::Ignore);
    // Within 10%: the cheaper backs off, the dearer stands.
    assert_eq!(r.rel(SPINO, ALLO), Rel::Yields);
    assert_eq!(r.rel(ALLO, SPINO), Rel::Ignore);
    assert_eq!(r.outranks(ALLO, SPINO), None);
    assert_eq!(r.outranks(REX, ALLO), Some(true));
}

#[test]
fn without_fights_carnivores_leave_each_other_alone() {
    let r = Rules::new(&Predation::default(), &roster());
    assert_eq!(r.rel(ALLO, RAPTOR), Rel::Ignore);
    assert_eq!(r.rel(RAPTOR, STEGO), Rel::Hunts);
}

#[test]
fn an_apex_or_a_chain_overrides_prices() {
    let apex = Predation {
        apex: Some("raptor".into()),
        ..fights()
    };
    let r = Rules::new(&apex, &roster());
    assert_eq!(r.rel(RAPTOR, REX), Rel::Hunts);
    assert_eq!(r.rel(REX, RAPTOR), Rel::Ignore);
    assert_eq!(r.rel(REX, ALLO), Rel::Hunts);

    let chain = Predation {
        chain: vec!["Spino".into(), "Stego".into(), "Raptor".into()],
        ..fights()
    };
    let r = Rules::new(&chain, &roster());
    assert_eq!(
        r.rel(SPINO, REX),
        Rel::Hunts,
        "unlisted animals rank below the chain"
    );
    assert_eq!(
        r.rel(RAPTOR, STEGO),
        Rel::Ignore,
        "the stego is above the raptor"
    );
    assert_eq!(
        r.rel(REX, ALLO),
        Rel::Hunts,
        "unlisted ones compare by price"
    );
}

#[test]
fn tags_change_diet_and_add_charges() {
    let p = Predation {
        animals: vec![
            Animal {
                name: "Rex".into(),
                diet: Some(Diet::Herbivore),
                ..Animal::default()
            },
            Animal {
                name: "Stego".into(),
                defensive: true,
                ..Animal::default()
            },
        ],
        chain: vec!["Rex".into(), "Stego".into(), "Raptor".into()],
        ..fights()
    };
    let r = Rules::new(&p, &roster());
    assert_eq!(r.kind(REX).role, Role::Herbivore);
    assert_eq!(
        r.rel(ALLO, REX),
        Rel::Ignore,
        "the rex is higher on the chain"
    );
    assert_eq!(r.rel(STEGO, RAPTOR), Rel::Charges);
    assert_eq!(r.rel(RAPTOR, STEGO), Rel::Ignore);
}

fn hunter(area: &Area) -> HunterView {
    let (x, z) = (300.0 * 256.0, 300.0 * 256.0);
    HunterView {
        x,
        z,
        y: area.land_height(x, z) + 220.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    }
}

fn herd(p: &Predation, area: &Area, at: &[(usize, f32)]) -> Herd {
    let species = roster();
    let rules = Rules::new(p, &species);
    let mut h = Herd {
        species,
        predation: Some(Box::new(Predators::new(rules))),
        ..Default::default()
    };
    for &(ctype, dx) in at {
        let (x, z) = (512.0 * 256.0 + dx, 512.0 * 256.0);
        let mut c = Character {
            ctype,
            pos: [x, area.land_height(x, z), z],
            tgx: x,
            tgz: z,
            ..Default::default()
        };
        c.reset(&h.species[ctype], &mut MsRand::new(1), 128);
        h.chars.push(c);
    }
    h
}

fn run(h: &mut Herd, area: &Area, ms: i32, until: impl Fn(&Herd) -> bool) -> Vec<DinoEvent> {
    let mut rng = MsRand::new(5);
    let mut events = Vec::new();
    let hv = hunter(area);
    for t in 0..ms / 20 {
        let mut ctx = Ctx {
            area,
            rng: &mut rng,
            dt: 20,
            real_time: t * 20,
            hunter: hv,
            target: Default::default(),
            struck: None,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            render_r: 74,
            run_k: 1.0,
            events: &mut events,
        };
        h.step(&mut ctx);
        if until(h) {
            break;
        }
    }
    events
}

#[test]
fn a_raptor_kills_a_stego_and_the_hunter_is_left_alive() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 1500.0)],
    );
    let ev = run(&mut h, &area, 60_000, |h| h.chars[1].health == 0);
    assert_eq!(h.chars[1].health, 0, "the stego got away");
    assert!(h.chars[1].predation.killed_by_animal);
    assert!(!ev.iter().any(|e| matches!(e, DinoEvent::KillHunter { .. })));
}

#[test]
fn prey_runs_from_a_predator_it_senses() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 2500.0)],
    );
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(h.chars[1].predation.goal, Goal::Flee(0));
    assert_eq!(h.chars[0].predation.goal, Goal::Hunt(1));
}

#[test]
fn hunter_first_predators_drop_prey_for_the_hunter() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 2500.0)],
    );
    h.chars[0].aware_hunter = true;
    h.chars[0].state = 1;
    h.chars[0].afraid_time = 60_000;
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(h.chars[0].predation.goal, Goal::None);

    let prey_first = Predation {
        priority: Priority::Prey,
        ..Predation::default()
    };
    let mut h = herd(&prey_first, &area, &[(RAPTOR, 0.0), (STEGO, 2500.0)]);
    h.chars[0].aware_hunter = true;
    h.chars[0].state = 1;
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(h.chars[0].predation.goal, Goal::Hunt(1));
}

#[test]
fn a_scavenger_eats_from_a_body_once_and_runs() {
    let area = Area::flat(10);
    let p = Predation {
        animals: vec![Animal {
            name: "Raptor".into(),
            scavenger: true,
            ..Animal::default()
        }],
        ..Predation::default()
    };
    let mut h = herd(&p, &area, &[(RAPTOR, 0.0), (STEGO, 1500.0)]);
    h.chars[1].health = 0;
    run(&mut h, &area, 60_000, |h| h.chars[1].predation.eaten);
    assert!(h.chars[1].predation.eaten, "the body was never found");
    assert!(matches!(h.chars[0].predation.goal, Goal::Run(_)));
}

#[test]
fn a_scavenger_keeps_off_while_a_bigger_carnivore_is_near() {
    let area = Area::flat(10);
    let p = Predation {
        carnivore_fights: true,
        animals: vec![Animal {
            name: "Raptor".into(),
            scavenger: true,
            ..Animal::default()
        }],
        ..Predation::default()
    };
    let mut h = herd(
        &p,
        &area,
        &[(RAPTOR, 0.0), (STEGO, 1500.0), (ALLO, -1500.0)],
    );
    h.chars[1].health = 0;
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(h.chars[0].predation.goal, Goal::Flee(2));
}

#[test]
fn a_defensive_herbivore_charges_and_the_carnivore_runs() {
    let area = Area::flat(10);
    let p = Predation {
        chain: vec!["Stego".into(), "Raptor".into()],
        animals: vec![Animal {
            name: "Stego".into(),
            defensive: true,
            ..Animal::default()
        }],
        ..Predation::default()
    };
    let mut h = herd(&p, &area, &[(RAPTOR, 0.0), (STEGO, 1500.0)]);
    run(&mut h, &area, 1_000, |_| false);
    assert_eq!(h.chars[1].predation.goal, Goal::Charge(0));
    assert_eq!(h.chars[0].predation.goal, Goal::Flee(1));
    // A charge ends within ten seconds, then rests.
    run(&mut h, &area, 12_000, |_| false);
    assert_ne!(h.chars[1].predation.goal, Goal::Charge(0));
}

#[test]
fn removing_a_character_renumbers_goals() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 5000.0), (STEGO, 1500.0)],
    );
    h.chars[0].predation.goal = Goal::Hunt(2);
    h.chars[2].predation.goal = Goal::Flee(0);
    h.chars.remove(1);
    h.forget(1);
    assert_eq!(h.chars[0].predation.goal, Goal::Hunt(1));
    assert_eq!(h.chars[1].predation.goal, Goal::Flee(0));
    h.chars.remove(1);
    h.forget(1);
    assert_eq!(h.chars[0].predation.goal, Goal::None);
}

#[test]
fn a_far_chase_closes_in_rather_than_giving_up() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 7000.0)],
    );
    h.chars[0].predation.goal = Goal::Hunt(1);
    let gap = |h: &Herd| (h.chars[0].pos[0] - h.chars[1].pos[0]).abs();
    let start = gap(&h);
    run(&mut h, &area, 3_000, |_| false);
    assert!(gap(&h) < start - 200.0, "{} -> {}", start, gap(&h));
}

#[test]
fn carnivores_eat_kills_with_the_ground_feeding_clip() {
    let area = Area::flat(10);
    for (eater, clip) in [
        (RAPTOR, CarnivoreParams::for_ai(ai::VELO).unwrap().eat),
        // The Rex's only kill lifts the hunter: it sniffs at the body.
        (REX, crate::dino::trex::SMEL),
    ] {
        let mut h = herd(
            &Predation::default(),
            &area,
            &[(eater, 0.0), (STEGO, 1500.0)],
        );
        run(&mut h, &area, 60_000, |h| h.chars[1].health == 0);
        assert_eq!(h.chars[1].health, 0, "the stego got away");
        assert!(h.chars[0].predation.eat_ms > 0);
        assert_eq!(h.chars[0].phase, clip);
        run(&mut h, &area, 2_000, |_| false);
        assert_eq!(h.chars[0].phase, clip);
    }
}

#[test]
fn dialect_carnivores_skip_kills_staged_for_the_hunter() {
    let ground = KillType {
        hunter_anim: hunt::EAT,
        anim: 3,
        ..Default::default()
    };
    let synced = KillType {
        hunter_anim: 7,
        anim: 6,
        hunter_sync: true,
        dont_loop: true,
        ..Default::default()
    };
    let mut m = MeeInfo::new(0);
    m.smells = vec![5];
    m.kill_types = vec![synced, ground];
    let mut sp = species("Sucho", ai::VELO, 10);
    sp.mee = Some(Box::new(m.clone()));
    assert_eq!(feed_phase(&sp, 0), Some(3), "a ground kill of another type");
    m.kill_types = vec![synced];
    sp.mee = Some(Box::new(m));
    assert_eq!(feed_phase(&sp, 0), Some(5), "only staged kills: sniffs");
}

#[test]
fn a_charging_herbivore_gores_without_its_kill_clip() {
    let area = Area::flat(10);
    let mut h = herd(
        &Predation::default(),
        &area,
        &[(RAPTOR, 0.0), (STEGO, 100.0)],
    );
    h.chars[1].predation.goal = Goal::Charge(0);
    let run_clip = h.chars[1].phase;
    // Its AI struck the animal (no kill clip: that is for the hunter).
    let p = h.predation.as_deref().unwrap();
    p.settle(1, &mut h.chars, &h.species, true);
    assert_eq!(h.chars[0].health, 0);
    assert_eq!(h.chars[1].phase, run_clip);
    assert_eq!(h.chars[1].predation.eat_phase, None);
}

mod bench;
mod regressions;
mod targets;
