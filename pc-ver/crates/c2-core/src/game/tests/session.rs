use super::support::setup;
use crate::game::{GameEvent, Hunt, HuntInput, SurvivalOptions, MAX_HEALTH, TELEPORT_COOLDOWN_MS};
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

#[test]
fn teleport_takes_five_seconds_then_cools_down_a_minute() {
    let (area, mut hunt, car) = setup(false);
    let cars = [None, Some(&car)];
    let mut out = Vec::new();
    let to = (600.0 * 256.0, 610.0 * 256.0);
    assert!(
        hunt.request_teleport(&area, to.0, to.1).is_err(),
        "off by default"
    );
    hunt.opts.teleport = true;
    hunt.request_teleport(&area, to.0, to.1).expect("starts");
    let run = |hunt: &mut Hunt, ms: i32, out: &mut Vec<GameEvent>| {
        for _ in 0..ms / 10 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, out);
        }
    };
    run(&mut hunt, 4_900, &mut out);
    assert!((hunt.player.x - to.0).abs() > 1000.0, "not yet");
    run(&mut hunt, 200, &mut out);
    assert!((hunt.player.x - to.0).abs() < 256.0 && (hunt.player.z - to.1).abs() < 256.0);
    assert!(hunt
        .request_teleport(&area, 500.0 * 256.0, 500.0 * 256.0)
        .is_err());
    run(&mut hunt, TELEPORT_COOLDOWN_MS, &mut out);
    assert!(hunt
        .request_teleport(&area, 500.0 * 256.0, 500.0 * 256.0)
        .is_ok());
    // Not in a survival.
    hunt.start_survival(
        &area,
        SurvivalOptions {
            roster: vec![(1, 0)],
            ..Default::default()
        },
    );
    hunt.cancel_teleport();
    assert!(!hunt.can_teleport());
}

#[test]
fn pause_freezes_and_health_recovers() {
    let (area, mut h, _car) = setup(true);
    let mut ev = Vec::new();
    h.health = MAX_HEALTH - 100;
    h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
    assert_eq!(h.health, MAX_HEALTH - 60);
    h.toggle_pause();
    let (t, pos) = (h.real_time, h.herd.chars[0].pos);
    h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
    assert_eq!((h.real_time, h.health), (t, MAX_HEALTH - 60));
    assert_eq!(h.herd.chars[0].pos, pos);
    h.press_escape();
    assert!(!h.paused && !h.exit_prompt);
}

/// An observer's hunt drops the saved camouflage, scent and tranquilizer
/// but keeps the radar; a normal hunt keeps them all.
#[test]
fn observer_hunt_masks_the_saved_accessories() {
    use crate::dino::Herd;
    use crate::game::HuntOptions;
    use crate::rng::MsRand;
    let (area, h, _) = setup(false);
    let saved = HuntOptions {
        camo: true,
        scent: true,
        tranq: true,
        half_tranq: true,
        radar: true,
        ..Default::default()
    };
    for observer in [false, true] {
        let opts = HuntOptions { observer, ..saved };
        let hunt = Hunt::new(
            &area,
            h.player.clone(),
            Herd::default(),
            h.weapons.clone(),
            MsRand::new(1),
            opts,
            super::support::blank_save(),
        );
        let o = hunt.opts;
        assert_eq!(
            (o.camo, o.scent, o.tranq, o.half_tranq),
            (!observer, !observer, !observer, !observer),
            "observer {observer}"
        );
        assert!(o.radar, "the radar stays (observer {observer})");
    }
}
