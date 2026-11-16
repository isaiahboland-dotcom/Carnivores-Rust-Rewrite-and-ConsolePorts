//! The behaviour script on the synthetic flat area.

use super::tests::{run_with, scenario, status, three_guns};
use super::*;
#[cfg(not(feature = "std"))]
use alloc::vec;

fn plan() -> BehaviourPlan {
    BehaviourPlan {
        move_ticks: 300,
        measure_ticks: 150,
        walk_speed: 300.0,
        run_speed: 700.0,
        speed_tolerance: 0.05,
        run_ticks: 300,
        min_run_speed: 100.0,
        die_ticks: 300,
        min_moving_ticks: 10,
        hold_ticks: 100,
        max_drift: 256.0,
        flee: None,
    }
}

fn behaviour_scenario() -> Scenario {
    let mut s = scenario();
    s.anchor_distance = 7000.0;
    s.anchor_bearing = core::f32::consts::FRAC_PI_2;
    s.script = Script::Behaviours(plan());
    s
}

#[test]
fn the_hunter_walks_runs_and_every_animal_runs_and_dies() {
    let r = run_with(&behaviour_scenario(), three_guns(), None);
    assert!(!r.director.failed(), "{:#?}", r.director.stages);
    for stage in [
        "hunter_walk",
        "hunter_run",
        "run_01_trex",
        "run_04_chasmo",
        "die_01_trex",
        "die_04_chasmo",
        "animals_dead",
    ] {
        assert_eq!(status(&r, stage), &Status::Passed, "{stage}");
    }
    let reports = &r.director.behaviour_reports;
    assert_eq!(reports.len(), 2 + 4 + 4);
    let walk = &reports[0];
    assert_eq!((walk.stage.as_str(), walk.kind), ("hunter_walk", "walk"));
    assert!((walk.speed - 300.0).abs() < 1.0, "{}", walk.speed);
    assert!(
        (reports[1].speed - 700.0).abs() < 1.0,
        "{}",
        reports[1].speed
    );
    let death = &reports[6];
    assert_eq!(death.kind, "death");
    assert!(death.held_ms >= 1000, "{death:?}");
    let caps: Vec<&str> = r.hashes.iter().map(|h| h.0.as_str()).collect();
    assert!(caps.contains(&"hunter_run") && caps.contains(&"die_02_allo"));
    assert_eq!(caps.last(), Some(&"end"));
}

#[test]
fn behaviours_repeat_exactly() {
    let a = run_with(&behaviour_scenario(), three_guns(), None);
    let b = run_with(&behaviour_scenario(), three_guns(), None);
    assert_eq!(a.hashes, b.hashes);
    assert_eq!(a.director.behaviour_reports, b.director.behaviour_reports);
}

#[test]
fn a_stuck_hunter_fails_the_speed_check() {
    let mut s = behaviour_scenario();
    if let Script::Behaviours(p) = &mut s.script {
        p.walk_speed = 900.0;
    }
    let r = run_with(&s, three_guns(), None);
    assert!(
        matches!(status(&r, "hunter_walk"), Status::Failed(m) if m.contains("walk_speed")),
        "{:?}",
        status(&r, "hunter_walk")
    );
    // The next stage still starts from the landing.
    assert_eq!(status(&r, "hunter_run"), &Status::Passed);
}

#[test]
fn track_measures() {
    let mut t = Track::new(10);
    for i in 0..=10 {
        t.push(Sample {
            at: [i as f32 * 3.0, 0.0],
            phase: if i < 5 { 1 } else { 2 },
            ftime: if i < 8 { i * 10 } else { 70 },
            scale: 1.0,
        });
    }
    assert!((t.speed(5) - 300.0).abs() < 1e-3);
    assert!((t.speed(100) - 300.0).abs() < 1e-3);
    assert_eq!(t.clips(), vec![1, 2]);
    assert_eq!(t.moving(), 7);
    assert_eq!(t.held(), 3);
    assert!((t.drift() - 30.0).abs() < 1e-3);
    assert_eq!(Track::new(10).speed(5), 0.0);
    // Standing, then 2 s at 600 units/s, then standing again.
    let mut p = Track::new(10);
    for i in 0..400 {
        let x = (i.clamp(100, 300) - 100) as f32 * 6.0;
        p.push(Sample {
            at: [x, 0.0],
            phase: 0,
            ftime: 0,
            scale: 1.0,
        });
    }
    assert!((p.peak_speed(150) - 600.0).abs() < 1e-2);
    assert_eq!(p.speed(50), 0.0);
}

#[test]
fn stage_names_are_shared() {
    let cast = ["T-Rex".into(), "Stegosaurus".into()];
    let with_flee = stage_names(&cast, true);
    assert_eq!(with_flee.len(), 8);
    assert_eq!(with_flee[4].0, "flee_01_t_rex");
    assert_eq!(with_flee[4].1, Subject::Fleeing(0));
    let names = stage_names(&cast, false);
    let n: Vec<&str> = names.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(
        n,
        [
            "hunter_walk",
            "hunter_run",
            "run_01_t_rex",
            "run_02_stegosaurus",
            "die_01_t_rex",
            "die_02_stegosaurus"
        ]
    );
}

/// Two herbivores with the bounded-flight stages: each flight ends well
/// inside the limit, and a limit it cannot keep fails the stage.
fn flee_scenario(max_distance: f32) -> Scenario {
    let mut s = behaviour_scenario();
    s.species = [12, 14].map(SpeciesRef::Ai).to_vec();
    s.offsets.truncate(2);
    s.script = Script::Behaviours(BehaviourPlan {
        flee: Some(FleePlan {
            ticks: 2500,
            max_distance,
        }),
        ..plan()
    });
    s
}

#[test]
fn a_bounded_flight_ends_within_its_limit() {
    // The default limit is 24 cells; two more for the last step.
    let r = run_with(&flee_scenario(26.0 * 256.0), three_guns(), None);
    assert!(!r.director.failed(), "{:#?}", r.director.stages);
    for stage in ["flee_01_stego", "flee_02_chasmo", "die_02_chasmo"] {
        assert_eq!(status(&r, stage), &Status::Passed, "{stage}");
    }
    let flights: Vec<_> = r
        .director
        .behaviour_reports
        .iter()
        .filter(|b| b.kind == "bounded_flight")
        .collect();
    assert_eq!(flights.len(), 2);
    assert!(flights.iter().all(|f| f.drift > 2048.0), "{flights:?}");
}

#[test]
fn a_flight_past_the_limit_fails_its_stage() {
    let r = run_with(&flee_scenario(1024.0), three_guns(), None);
    assert!(matches!(status(&r, "flee_01_stego"), Status::Failed(_)));
}
