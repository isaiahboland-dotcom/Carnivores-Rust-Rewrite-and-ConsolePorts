use super::support::{info, setup};
use crate::dino::{ai, DinoInfo, Species};
use crate::game::{death_cam_view, DeathCam};
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

#[test]
fn death_camera_faces_the_body_while_the_orbit_catches_up() {
    let (area, mut hunt, _) = setup(false);
    let mut out = Vec::new();
    hunt.kill_hunter(&area, Some(0), 0, false, &mut out);
    let mut d = hunt.death.expect("death camera");
    let body = hunt.herd.chars[d.body].pos;
    d.pos = [body[0] - 1200.0, body[1], body[2] + 600.0];
    d.alpha = 2.5;
    d.base = 2400.0;
    hunt.death_cam(&area, d, 16);
    let d = hunt.death.expect("death camera");
    assert!(d.pos[1] >= body[1] + d.lift);
    let (pos, yaw, _) = hunt.camera();
    let (dx, dz) = (body[0] - pos[0], body[2] - pos[2]);
    let distance = dx.hypot(dz);
    assert!((yaw.sin() - dx / distance).abs() < 1e-5);
    assert!((-yaw.cos() - dz / distance).abs() < 1e-5);
    for _ in 0..200 {
        let d = hunt.death.expect("death camera");
        hunt.death_cam(&area, d, 16);
    }
    let d = hunt.death.expect("death camera");
    let radius = (body[0] - d.pos[0]).hypot(body[2] - d.pos[2]);
    assert!(
        (radius - d.base).abs() < 1.0,
        "orbit radius {radius}, expected {}",
        d.base
    );
}

#[test]
fn scripted_death_camera_scales_with_a_large_killer() {
    let (_, mut hunt, _) = setup(false);
    let mut m = crate::dino::mee::MeeInfo::new(ai::TREX);
    m.cam_base = Some(1424.0);
    m.cam_y = Some(768.0);
    hunt.herd.species[1].mee = Some(Box::new(m));
    hunt.herd.chars[0].scale = 1.5;
    assert_eq!(
        death_cam_view(
            Some((&hunt.herd.chars[0], &hunt.herd.species[1])),
            false,
            false
        ),
        (2136.0, 1152.0)
    );
}

#[test]
fn carried_hunter_switches_clip_when_killer_walks_away() {
    let (area, mut hunt, _) = setup(false);
    let res = c2_formats::res_txt::ResTxt::parse("characters {\n{\n ai = 18\n walkAnim = 0\n killtype {\n hunterSync = TRUE\n carryCorpse = TRUE\n eatAnim = 1\n hunterAnim = 0\n hunterCarryAnim = 2\n }\n}\n}\n").expect("script");
    let m = crate::mee::info(&res.sections[0].blocks[0], ai::TREX);
    let eating = m.kill_types[0].anim;
    let walking = m.walk;
    hunt.herd.species[1].mee = Some(Box::new(m));
    hunt.herd.chars[0].phase = eating;
    let mut out = Vec::new();
    hunt.kill_hunter(&area, Some(0), 0, false, &mut out);
    let body = hunt.death.expect("death camera").body;
    assert_eq!(hunt.herd.chars[body].phase, 0);
    hunt.herd.chars[0].phase = walking;
    hunt.herd.chars[0].ftime = 321;
    hunt.herd.chars[0].pos[0] += 600.0;
    hunt.sync_killed_body(&area);
    assert_eq!(hunt.herd.chars[body].phase, 2);
    assert_eq!(hunt.herd.chars[body].ftime, 321);
    assert_eq!(hunt.herd.chars[body].pos, hunt.herd.chars[0].pos);
}

#[test]
fn synchronized_kill_keeps_the_body_on_the_killers_animation() {
    let (area, mut hunt, _) = setup(false);
    let res = c2_formats::res_txt::ResTxt::parse("characters {\n{\n ai = 18\n killtype {\n hunterSync = TRUE\n eatAnim = 0\n hunterAnim = 0\n }\n}\n}\n").expect("script");
    let m = crate::mee::info(&res.sections[0].blocks[0], ai::CHASM);
    assert!(m.kill_types[0].hunter_sync);
    hunt.herd.species[1].mee = Some(Box::new(m));
    let killer = &mut hunt.herd.chars[0];
    (
        killer.phase,
        killer.alpha,
        killer.beta,
        killer.gamma,
        killer.scale,
        killer.ftime,
    ) = (0, 1.2, 0.1, -0.2, 1.7, 123);
    let mut out = Vec::new();
    hunt.kill_hunter(&area, Some(0), 0, false, &mut out);
    let body = hunt.death.as_ref().map(|d| d.body).expect("body");
    for time in [123, 456] {
        hunt.herd.chars[0].ftime = time;
        hunt.sync_killed_body(&area);
        let (b, k) = (&hunt.herd.chars[body], &hunt.herd.chars[0]);
        assert_eq!(
            (b.pos, b.alpha, b.beta, b.gamma, b.scale, b.ftime),
            (k.pos, k.alpha, k.beta, k.gamma, k.scale, k.ftime)
        );
    }
    hunt.remove_character(0);
    assert_eq!(hunt.death.as_ref().and_then(|d| d.killer), None);
    assert_eq!(hunt.death.as_ref().map(|d| d.body), Some(body - 1));
}

#[test]
fn a_killer_naming_its_hunter_lays_that_body() {
    let (area, mut hunt, car) = setup(false);
    let own = DinoInfo {
        file: "MODS/game/HUNTER.CAR".into(),
        ..info(0)
    };
    hunt.herd.species.push(Species::new(own, &car));
    let mut m = crate::dino::mee::MeeInfo::new(ai::STEGO);
    m.hunter_file = Some("mods/game/hunter.car".into());
    hunt.herd.species[1].mee = Some(Box::new(m));
    let mut out = Vec::new();
    hunt.kill_hunter(&area, Some(0), 0, false, &mut out);
    let body = hunt.death.as_ref().map(|d| d.body).expect("dead");
    assert_eq!(hunt.herd.chars[body].ctype, 2, "its own hunter");
    // Without the name, the first hunter.
    let (area, mut hunt, _) = setup(false);
    hunt.kill_hunter(&area, Some(0), 99, false, &mut out);
    let body = hunt.death.as_ref().map(|d| d.body).expect("dead");
    assert_eq!(hunt.herd.chars[body].ctype, 0);
    assert_eq!(
        hunt.herd.chars[body].phase, 0,
        "past its animations: the first"
    );
}

#[test]
fn no_death_camera_prompts_at_once() {
    let (_, mut hunt, _) = setup(false);
    hunt.death = Some(DeathCam {
        body: 0,
        killer: None,
        time: 1,
        pos: [0.0; 3],
        alpha: 0.0,
        beta: 0.0,
        base: 824.0,
        lift: 256.0,
    });
    assert!(!hunt.show_exit());
    hunt.opts.death_anim = false;
    assert!(hunt.show_exit());
}

/// `Hunt2.cpp`: a poisonous fog zone caps health at 100000 and drains 64
/// per ms (net 60 with the 4 per ms recovery), then kills; a harmless zone,
/// or a poisonous one whose top is below the eye, does nothing.
#[test]
fn poisonous_fog_drains_health_and_kills() {
    use crate::game::{HuntInput, MAX_HEALTH};
    use c2_formats::rsc::Fog;
    let fog = |y_begin: f32, mortal: i32| Fog {
        rgb: 0,
        y_begin,
        mortal,
        transp: 512.0,
        f_limit: 200.0,
    };
    let run = |zone: u8, top: f32, ticks: usize| {
        let (mut area, mut hunt, _) = setup(false);
        area.rsc.fogs = [fog(255.0, 0), fog(top, 1)].into();
        area.map.fogs.fill(zone);
        let mut out = Vec::new();
        for _ in 0..ticks {
            hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
        }
        (hunt.health, hunt.death.is_some())
    };
    // 100000 - 640, then -600 per tick: dead on tick 167.
    assert_eq!(run(2, 255.0, 166), (360, false));
    let (health, dead) = run(2, 255.0, 167);
    assert!(health <= 0 && dead);
    assert_eq!(run(1, 255.0, 200), (MAX_HEALTH, false), "harmless zone");
    // Flat ground at 10 (640 units); the eye is above a top of 5 (320).
    assert_eq!(run(2, 5.0, 200), (MAX_HEALTH, false), "below the eye");
}
