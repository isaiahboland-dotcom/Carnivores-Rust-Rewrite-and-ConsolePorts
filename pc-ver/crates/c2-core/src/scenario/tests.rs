//! Synthetic scenario runs: no game data, a flat area, triangle animals.

use super::*;
use crate::area::Area;
use crate::dino::{DinoInfo, Species};
use crate::game::{GameEvent, WeaponAnims, WeaponInfo};
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};
use c2_formats::car::{Animation, Car};
use c2_formats::sav::{Save, TrophyItem};

fn car() -> Car {
    Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: (0..16)
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
    }
}

fn species(ai: i32, name: &str) -> Species {
    let info = DinoInfo {
        name: name.into(),
        file: String::new(),
        ai,
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
    Species::new(info, &car())
}

fn roster() -> Vec<Species> {
    vec![
        species(0, "Hunter"),
        species(12, "Stego"),
        species(13, "Allo"),
        species(14, "Chasmo"),
        species(18, "TRex"),
    ]
}

fn weapon(name: &str, shots: i32) -> WeaponInfo {
    WeaponInfo {
        name: name.into(),
        file: String::new(),
        pic: String::new(),
        power: 1.0,
        prec: 1.8,
        loud: 1.0,
        rate: 1.0,
        shots,
        reload: 0,
        trace_c: 0,
        optic: false,
        fall: false,
        mee: None,
    }
}

fn save() -> Save {
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

pub(super) fn scenario() -> Scenario {
    Scenario {
        id: "synthetic".into(),
        version: 1,
        seed: 7,
        landing: 0,
        cell: Some((500, 500)),
        yaw: 0.0,
        pitch: 0.0,
        anchor_distance: 3000.0,
        anchor_bearing: 0.5,
        offsets: vec![[-300.0, 0.0], [300.0, 0.0], [-300.0, 600.0], [300.0, 600.0]],
        species: [18, 13, 12, 14].map(SpeciesRef::Ai).to_vec(),
        fire_clearance: 0.05,
        warmup_ticks: 50,
        settle_ticks: 5,
        timeout_ticks: 300,
        script: Script::Tour,
    }
}

pub(super) struct Run {
    pub director: Director,
    pub hashes: Vec<(String, u64)>,
    pub ticks: u32,
}

pub(super) fn run_with(s: &Scenario, weapons: Vec<WeaponInfo>, sabotage: Option<u32>) -> Run {
    let area = Area::flat(10);
    let names: Vec<String> = weapons.iter().map(|w| w.name.clone()).collect();
    let anims = vec![WeaponAnims::uniform(100); weapons.len()];
    let mut b = build(&area, roster(), weapons, anims, s, save()).expect("builds");
    let model = car();
    let cars: Vec<Option<&Car>> = (0..5).map(|_| Some(&model)).collect();
    let mut d = Director::new(s, &names, &b.cast);
    let mut fe = Headless;
    let mut hashes = Vec::new();
    let mut out: Vec<GameEvent> = Vec::new();
    while !d.done() {
        if d.tick() > s.max_ticks(names.len()) {
            d.abort("tick ceiling");
            break;
        }
        if sabotage == Some(d.tick()) {
            b.hunt.herd.chars[1].health = 0;
        }
        let plan = d.plan(&b.hunt, &mut fe);
        if let Some(a) = plan.arrange {
            b.hold.arrange(&mut b.hunt, &area, a, &s.script);
        }
        b.hunt.tick(&area, &cars, &plan.input, TICK_MS, &mut out);
        out.clear();
        let lost = b.hold.apply(&mut b.hunt, &area);
        for c in d.observe(&b.hunt, &mut fe, &lost) {
            hashes.push((c, state_hash(&b.hunt)));
        }
    }
    Run {
        ticks: d.tick(),
        director: d,
        hashes,
    }
}

pub(super) fn status<'a>(r: &'a Run, stage: &str) -> &'a Status {
    &r.director
        .stages
        .iter()
        .find(|s| s.name == stage)
        .expect("stage ran")
        .status
}

pub(super) fn three_guns() -> Vec<WeaponInfo> {
    vec![
        weapon("Pistol", 8),
        weapon("Shotgun", 6),
        weapon("Rifle", 10),
    ]
}

#[test]
fn every_stage_passes_or_is_reported_unsupported() {
    let r = run_with(&scenario(), three_guns(), None);
    assert!(!r.director.failed(), "{:#?}", r.director.stages);
    for stage in [
        "spawn",
        "warmup",
        "weapon_01_pistol",
        "weapon_02_shotgun",
        "weapon_03_rifle",
        "binoculars",
        "map",
        "call_icon",
        "animals_present",
    ] {
        assert_eq!(status(&r, stage), &Status::Passed, "{stage}");
    }
    for stage in ["weapon_picker", "call_picker"] {
        assert!(
            matches!(status(&r, stage), Status::Unsupported(_)),
            "{stage}"
        );
    }
    let fired = r
        .director
        .events
        .iter()
        .filter(|e| e.name == "fired_once" && e.ok == Some(true))
        .count();
    assert_eq!(fired, 3);
    let caps: Vec<&str> = r.hashes.iter().map(|h| h.0.as_str()).collect();
    assert!(caps.contains(&"weapon_02_shotgun_fire"));
    assert!(caps.contains(&"map_open"));
    assert_eq!(caps.last(), Some(&"end"));
}

#[test]
fn runs_are_deterministic() {
    let a = run_with(&scenario(), three_guns(), None);
    let b = run_with(&scenario(), three_guns(), None);
    assert_eq!(a.hashes, b.hashes);
    assert_eq!(a.ticks, b.ticks);
    assert_eq!(a.director.events, b.director.events);
    let mut s = scenario();
    s.seed = 8;
    let c = run_with(&s, three_guns(), None);
    assert_ne!(a.hashes, c.hashes, "the seed reaches the state");
}

#[test]
fn a_lost_animal_fails_the_run() {
    let r = run_with(&scenario(), three_guns(), Some(80));
    assert!(r.director.failed());
    assert!(matches!(status(&r, "animals_present"), Status::Failed(_)));
}

#[test]
fn an_empty_weapon_times_out_and_the_script_goes_on() {
    let mut s = scenario();
    s.timeout_ticks = 30;
    let r = run_with(&s, vec![weapon("Pistol", 8), weapon("Empty", 0)], None);
    assert!(matches!(status(&r, "weapon_02_empty"), Status::Failed(m) if m.contains("timeout")));
    assert_eq!(status(&r, "weapon_01_pistol"), &Status::Passed);
    assert_eq!(status(&r, "map"), &Status::Passed);
}

#[test]
fn injected_faults_are_caught() {
    assert_eq!(Inject::parse("drop-shot:1"), Ok(Inject::DropShot(1)));
    assert!(Inject::parse("drop-shot").is_err());
    assert!(Inject::parse("melt:1").is_err());
    let area = Area::flat(10);
    let s = scenario();
    let guns = three_guns();
    let names: Vec<String> = guns.iter().map(|w| w.name.clone()).collect();
    let anims = vec![WeaponAnims::uniform(100); guns.len()];
    let mut b = build(&area, roster(), guns, anims, &s, save()).expect("builds");
    let model = car();
    let cars: Vec<Option<&Car>> = (0..5).map(|_| Some(&model)).collect();
    let mut d = Director::new(&s, &names, &b.cast);
    let inject = Inject::DropShot(1);
    let mut out = Vec::new();
    while !d.done() {
        let mut plan = d.plan(&b.hunt, &mut Headless);
        inject.adjust_plan(&mut plan, &b.hunt);
        b.hunt.tick(&area, &cars, &plan.input, TICK_MS, &mut out);
        let lost = b.hold.apply(&mut b.hunt, &area);
        d.observe(&b.hunt, &mut Headless, &lost);
    }
    let failed: Vec<&str> = d
        .stages
        .iter()
        .filter(|s| matches!(s.status, Status::Failed(_)))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(failed, ["weapon_02_shotgun"]);
}

#[test]
fn bad_setups_are_refused() {
    let area = Area::flat(10);
    let try_build = |s: &Scenario| {
        build(
            &area,
            roster(),
            vec![weapon("Pistol", 8)],
            vec![WeaponAnims::uniform(100)],
            s,
            save(),
        )
        .err()
    };
    let mut s = scenario();
    s.species[2] = SpeciesRef::Ai(16);
    assert!(matches!(try_build(&s), Some(SetupError::MissingSpecies(_))));
    let mut s = scenario();
    s.anchor_bearing = 0.0;
    assert!(matches!(
        try_build(&s),
        Some(SetupError::InLineOfFire { .. })
    ));
    let mut s = scenario();
    s.offsets.pop();
    assert!(matches!(try_build(&s), Some(SetupError::WrongCount { .. })));
    let mut s = scenario();
    s.cell = None;
    s.landing = 9;
    assert!(matches!(try_build(&s), Some(SetupError::NoLanding(9))));
    // A Modder's Edition placeholder model counts as none.
    let mut blank = roster();
    blank[1].info.file = "ANIMALS/blank.car".into();
    let r = build(
        &area,
        blank,
        vec![weapon("Pistol", 8)],
        vec![WeaponAnims::uniform(100)],
        &scenario(),
        save(),
    );
    assert!(matches!(r.err(), Some(SetupError::NoModel(_))));
}

fn kill_scenario(species: &[i32]) -> Scenario {
    let mut s = scenario();
    s.species = species.iter().map(|&a| SpeciesRef::Ai(a)).collect();
    s.offsets = vec![[0.0, -1500.0], [0.0, 0.0], [0.0, 1500.0]][..species.len()].to_vec();
    s.anchor_distance = 6000.0;
    s.anchor_bearing = core::f32::consts::FRAC_PI_2;
    s.script = Script::Kills(KillPlan {
        start_distance: 600.0,
        timeout_ticks: 1500,
        mid_ticks: 50,
        late_ticks: 50,
        min_moving_ticks: 10,
        body_reach: 3.0,
        min_body_reach: 1024.0,
    });
    s
}

#[test]
fn each_animal_kills_and_the_hunter_comes_back() {
    let s = kill_scenario(&[13, 18]);
    let r = run_with(&s, three_guns(), None);
    assert!(!r.director.failed(), "{:#?}", r.director.stages);
    for stage in ["kill_01_allo", "kill_02_trex", "animals_present"] {
        assert_eq!(status(&r, stage), &Status::Passed, "{stage}");
    }
    let caps: Vec<&str> = r.hashes.iter().map(|h| h.0.as_str()).collect();
    for c in [
        "kill_01_allo_start",
        "kill_01_allo_mid",
        "kill_02_trex_late",
    ] {
        assert!(caps.contains(&c), "{c} in {caps:?}");
    }
    let passed = |name: &str| {
        r.director
            .events
            .iter()
            .filter(|e| e.name == name && e.ok == Some(true))
            .count()
    };
    for check in [
        "killed_by",
        "hunter_clip",
        "hunter_animates",
        "killer_animates",
    ] {
        assert_eq!(passed(check), 2, "{check}");
    }
    assert_eq!(passed("body_with_killer"), 2);
    // Nothing was fired, and the two kills repeat exactly.
    assert_eq!(passed("fired_once"), 0);
    let again = run_with(&s, three_guns(), None);
    assert_eq!(r.hashes, again.hashes);
}

#[test]
fn an_animal_that_never_kills_fails_and_the_next_still_runs() {
    // The Stegosaurus runs from the hunter; the Allosaurus after it kills.
    let mut s = kill_scenario(&[12, 13]);
    if let Script::Kills(k) = &mut s.script {
        k.timeout_ticks = 200;
    }
    let r = run_with(&s, three_guns(), None);
    assert!(matches!(status(&r, "kill_01_stego"), Status::Failed(m) if m.contains("timeout")));
    assert_eq!(status(&r, "kill_02_allo"), &Status::Passed);
    assert_eq!(status(&r, "animals_present"), &Status::Passed);
}

#[test]
fn a_frozen_body_fails_its_kill() {
    assert_eq!(Inject::parse("freeze-body"), Ok(Inject::FreezeBody));
    assert!(Inject::parse("freeze-body:1").is_err());
    let area = Area::flat(10);
    let s = kill_scenario(&[13]);
    let mut b = build(
        &area,
        roster(),
        three_guns(),
        vec![WeaponAnims::uniform(100); 3],
        &s,
        save(),
    )
    .expect("builds");
    Inject::FreezeBody.adjust_hold(&mut b.hold);
    let model = car();
    let cars: Vec<Option<&Car>> = (0..5).map(|_| Some(&model)).collect();
    let mut d = Director::new(&s, &[], &b.cast);
    let mut out = Vec::new();
    while !d.done() && d.tick() < s.max_ticks(0) {
        let plan = d.plan(&b.hunt, &mut Headless);
        if let Some(a) = plan.arrange {
            b.hold.arrange(&mut b.hunt, &area, a, &s.script);
        }
        b.hunt.tick(&area, &cars, &plan.input, TICK_MS, &mut out);
        let lost = b.hold.apply(&mut b.hunt, &area);
        d.observe(&b.hunt, &mut Headless, &lost);
    }
    let kill = d
        .stages
        .iter()
        .find(|x| x.name == "kill_01_allo")
        .expect("ran");
    assert!(
        matches!(&kill.status, Status::Failed(m) if m.contains("hunter_animates")),
        "{:?}",
        kill.status
    );
}
