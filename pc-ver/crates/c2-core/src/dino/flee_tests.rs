//! Bounded fleeing ([`super::flee`]).

use super::flee::{self, pick_target, FleeLimits, FleeTable};
use super::tests::{dist, herd_with, species};
use super::*;
use c2_formats::mee::{Region, SpawnGroup};

const CELLS: i32 = 256;

fn at(x: f32, z: f32) -> Character {
    Character {
        pos: [x, 0.0, z],
        ..Default::default()
    }
}

fn cell(n: i32) -> f32 {
    n as f32 * 256.0 + 128.0
}

/// Inside the aim margin: cells 40..=976.
fn inside(p: (f32, f32)) -> bool {
    let r = 40.0 * 256.0..977.0 * 256.0;
    r.contains(&p.0) && r.contains(&p.1)
}

/// Inside where animals may walk (cells 36..=980), give or take a step.
fn walkable(p: &[f32; 3]) -> bool {
    let r = 36.0 * 256.0 - 64.0..981.0 * 256.0 + 64.0;
    r.contains(&p[0]) && r.contains(&p[2])
}

fn hunter_view(x: f32, z: f32) -> HunterView {
    HunterView {
        x,
        z,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    }
}

#[test]
fn edges_and_corners_never_aim_off_the_playable_square() {
    let area = Area::flat(10);
    let _ = CELLS;
    // Animals stand at each edge and corner with the hunter just inside
    // (so "directly away" points outward).
    for (ax, az) in [
        (41, 500),
        (975, 500),
        (500, 41),
        (500, 975),
        (41, 41),
        (975, 975),
        (41, 975),
        (975, 41),
    ] {
        let (px, pz) = (cell(ax), cell(az));
        let (hx, hz) = (
            cell(if ax < 100 {
                ax + 3
            } else if ax > 900 {
                ax - 3
            } else {
                ax
            }),
            cell(if az < 100 {
                az + 3
            } else if az > 900 {
                az - 3
            } else {
                az
            }),
        );
        let (dx, dz) = (px - hx, pz - hz);
        let l = (dx * dx + dz * dz).sqrt();
        let c = at(px, pz);
        for first in [1.0, -1.0] {
            let t = pick_target(
                &c,
                &area,
                None,
                (dx / l, dz / l),
                (hx, hz),
                2048.0,
                first,
                None,
            );
            assert!(inside(t), "({ax},{az}) aimed at {t:?}");
        }
    }
}

#[test]
fn a_wall_is_slid_along_not_run_into() {
    let mut area = Area::flat(10);
    // A wall of blocked cells along x = 520.
    for z in 400..600 {
        area.map.flags[crate::area::idx(520, z)] |= crate::area::cell_flags::NO_WAY;
    }
    let c = at(cell(517), cell(500));
    let hunter = (cell(515), cell(500));
    let t = pick_target(&c, &area, None, (1.0, 0.0), hunter, 2048.0, 1.0, None);
    assert_ne!(t, (c.pos[0], c.pos[2]), "not stuck in place");
    assert!(
        t.0 < cell(519) && (t.1 - c.pos[2]).abs() > 256.0,
        "slides along: {t:?}"
    );
}

#[test]
fn prohibited_areas_are_avoided() {
    let area = Area::flat(10);
    let group = SpawnGroup {
        avoid: vec![Region {
            xmin: 508,
            xmax: 530,
            ymin: 490,
            ymax: 510,
        }],
        ..Default::default()
    };
    let c = at(cell(505), cell(500));
    let t = pick_target(
        &c,
        &area,
        Some(&group),
        (1.0, 0.0),
        (cell(503), cell(500)),
        2048.0,
        1.0,
        None,
    );
    assert!(
        !(508.0 * 256.0..530.0 * 256.0).contains(&t.0)
            || !(490.0 * 256.0..510.0 * 256.0).contains(&t.1),
        "{t:?}"
    );
}

#[test]
fn the_pick_is_deterministic() {
    let area = Area::flat(10);
    let c = at(cell(41), cell(500));
    let a = pick_target(
        &c,
        &area,
        None,
        (-1.0, 0.0),
        (cell(44), cell(500)),
        2048.0,
        1.0,
        None,
    );
    let b = pick_target(
        &c,
        &area,
        None,
        (-1.0, 0.0),
        (cell(44), cell(500)),
        2048.0,
        1.0,
        None,
    );
    assert_eq!(a, b);
}

#[test]
fn repeated_noises_arm_fear_once() {
    let l = FleeLimits {
        retrigger_lockout: 4000,
        ..FleeLimits::default()
    };
    let mut c = Character::default();
    assert!(c.scare(12_000, Some(l)));
    c.afraid_time -= 5000;
    for _ in 0..10 {
        assert!(!c.scare(14_000, Some(l)), "ignored inside the lockout");
    }
    assert_eq!(c.afraid_time, 7000, "ten repeats lengthened nothing");
    c.flee_tick(4000);
    assert!(c.scare(9000, Some(l)), "the lockout expires");
    assert_eq!(c.afraid_time, 9000);
    // Without the setting a scare always re-arms, as in the original.
    c.afraid_time = 100;
    assert!(c.scare(5000, None) && c.afraid_time == 5000);
    assert!(c.scare(300, None) && c.afraid_time == 300);
}

fn run_bounded(herd: &mut Herd, area: &Area, hunter: HunterView, ms: i32) {
    let mut rng = MsRand::new(3);
    let mut events = Vec::new();
    for t in 0..ms / 10 {
        let mut ctx = Ctx {
            area,
            rng: &mut rng,
            dt: 10,
            real_time: t * 10,
            hunter,
            target: Default::default(),
            struck: None,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            render_r: 74,
            run_k: 1.0,
            flee: None,
            eco: None,
            events: &mut events,
        };
        herd.step(&mut ctx);
    }
}

fn table(limits: FleeLimits) -> Option<Box<FleeTable>> {
    Some(Box::new(FleeTable {
        per_species: vec![limits],
    }))
}

#[test]
fn a_cornered_animal_stays_on_the_map_for_a_minute() {
    // Every fleeing family, at the west edge (and the south-west corner)
    // with the hunter inside it, re-frightened all the time.
    for ai_id in [ai::PARA, ai::ANKY, ai::STEGO, ai::CHASM, ai::MOSH, ai::GALL] {
        for (px, pz) in [(42, 500), (42, 975)] {
            cornered_for_a_minute(ai_id, px, pz);
        }
    }
}

fn cornered_for_a_minute(ai_id: i32, px: i32, pz: i32) {
    let area = Area::flat(10);
    let h = hunter_view(cell(px + 18), cell(pz - 18));
    let mut herd = herd_with(&area, ai_id, 12, [cell(px), 0.0, cell(pz)]);
    herd.flee = table(FleeLimits::default());
    // Only flights are bounded: a calm animal wanders as in the original.
    for sec in 0..60 {
        herd.chars[0].state = 1;
        herd.chars[0].afraid_time = 20_000;
        for _ in 0..10 {
            run_bounded(&mut herd, &area, h, 100);
            let c = &herd.chars[0];
            assert!(
                c.state == 0 || walkable(&c.pos),
                "ai {ai_id} at ({px},{pz}) t={sec}s left the square: {:?} aiming ({}, {})",
                c.pos,
                c.tgx,
                c.tgz
            );
        }
    }
}

#[test]
fn a_flight_ends_at_its_distance_cap() {
    let area = Area::flat(10);
    let h = hunter_view(cell(500), cell(500));
    let mut herd = herd_with(&area, ai::PARA, 6, [h.x + 800.0, 0.0, h.z]);
    let start = herd.chars[0].pos;
    herd.flee = table(FleeLimits {
        max_distance: 1024,
        max_duration: 60_000,
        chase_extension: 0,
        retrigger_lockout: 4000,
    });
    // Step until the flight ends (the animal calms), then measure.
    let mut fled = false;
    for _ in 0..120 {
        run_bounded(&mut herd, &area, h, 100);
        fled |= herd.chars[0].state != 0;
        if fled && herd.chars[0].state == 0 {
            break;
        }
    }
    let c = &herd.chars[0];
    let travelled = ((c.pos[0] - start[0]).powi(2) + (c.pos[2] - start[2]).powi(2)).sqrt();
    assert!(fled && c.state == 0, "the flight ended");
    assert!(travelled < 1024.0 + 400.0, "ran {travelled}");
    assert!(dist(c, &h) > 800.0);
}

#[test]
fn limits_off_leave_the_original_target() {
    let area = Area::flat(10);
    let h = hunter_view(cell(500), cell(500));
    let mut herd = herd_with(&area, ai::PARA, 6, [h.x + 800.0, 0.0, h.z]);
    herd.chars[0].state = 1;
    herd.chars[0].afraid_time = 8000;
    run_bounded(&mut herd, &area, h, 20);
    let c = &herd.chars[0];
    assert!(
        (c.tgx - (c.pos[0] + 2048.0)).abs() < 100.0,
        "straight away: {}",
        c.tgx
    );
    let _ = species;
}

/// Runs `flee::step` with the hunter `gap(t)` units away at time `t` (ms);
/// returns when the flight ended (ms), or `None` after `until`.
fn flight_length(
    limits: FleeLimits,
    aware: bool,
    gap: impl Fn(i32) -> f32,
    until: i32,
) -> Option<i32> {
    let area = Area::flat(10);
    let mut c = at(cell(500), cell(500));
    c.state = 1;
    let mut rng = MsRand::new(5);
    let mut events = Vec::new();
    for t in (0..until).step_by(100) {
        let mut ctx = Ctx {
            area: &area,
            rng: &mut rng,
            dt: 100,
            real_time: t,
            hunter: hunter_view(c.pos[0] - gap(t), c.pos[2]),
            target: Default::default(),
            struck: None,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            render_r: 74,
            run_k: 1.0,
            flee: None,
            eco: None,
            events: &mut events,
        };
        ctx.target.view = hunter_view(c.pos[0] - gap(t), c.pos[2]);
        if flee::step(
            &mut c,
            &mut ctx,
            limits,
            flee::Habitat::default(),
            (1.0, 0.0),
            gap(t),
            aware,
        )
        .is_none()
        {
            return Some(t);
        }
    }
    None
}

#[test]
fn a_pursuer_extends_a_flight_by_a_bounded_amount_then_it_ends() {
    let l = FleeLimits {
        max_distance: 100_000,
        max_duration: 5000,
        chase_extension: 3000,
        retrigger_lockout: 0,
    };
    // A distant or lost hunter: it ends at the duration cap.
    let far = flight_length(l, true, |_| 5500.0, 30_000).expect("ends");
    assert!((4900..=5100).contains(&far), "{far}");
    // A hunter who does not know it (the animal is unaware): same.
    let unaware = flight_length(l, false, |t| 5000.0 - t as f32 * 0.5, 30_000).expect("ends");
    assert!((4900..=5100).contains(&unaware), "{unaware}");
    // A hunter closing in for ever: the cap plus exactly the extension.
    let chased = flight_length(l, true, |t| 5900.0 - t as f32 * 0.15, 30_000).expect("ends");
    assert!((7900..=8100).contains(&chased), "{chased}");
}

#[test]
fn the_launchers_vanilla_table_matches_the_ai_code() {
    use c2_formats::flee::{vanilla, LEG_UNITS, NEAR_UNITS};
    assert_eq!(LEG_UNITS as f32, flee::LEG);
    assert_eq!(NEAR_UNITS as f32, flee::PROXIMITY_UNITS);
    for ai_id in 0..40 {
        let v = vanilla(ai_id);
        let herbivore = HerbivoreParams::for_ai(ai_id);
        let small = SmallParams::for_ai(ai_id);
        let carnivore = CarnivoreParams::for_ai(ai_id).is_some() || ai_id == ai::TREX;
        assert_eq!(v.affected, !carnivore, "AI {ai_id}");
        if let Some(h) = herbivore {
            // A defensive herbivore re-arms its timer near the hunter; the
            // others are alarmed by a close approach.
            assert_eq!(v.refresh_ms.is_some(), h.defensive, "AI {ai_id}");
            assert_eq!(v.alarm_ms.is_some(), !h.defensive, "AI {ai_id}");
        }
        if small.is_some() {
            assert_eq!(v.alarm_ms, Some((5 * 1024, 9 * 1024)), "AI {ai_id}");
        }
        if v.refresh_ms.is_some() {
            assert_eq!(v.refresh_ms, Some(flee::PROXIMITY_FEAR_MS), "AI {ai_id}");
        }
    }
}

/// A stock carnivore that has lost the hunter (he is beyond its give-up
/// distance) walks off: bounded, the retreat aims inside the map.
#[test]
fn a_retreating_carnivore_keeps_to_the_map() {
    let area = Area::flat(10);
    for ai_id in [ai::ALLO, ai::VELO] {
        let aims = |bounded: bool| {
            let h = hunter_view(cell(42 + 80), cell(500));
            let mut herd = herd_with(&area, ai_id, 12, [cell(42), 0.0, cell(500)]);
            if bounded {
                herd.flee = table(FleeLimits::default());
            }
            let mut west = f32::MAX;
            for _ in 0..10 {
                herd.chars[0].state = 1;
                herd.chars[0].afraid_time = 20_000;
                run_bounded(&mut herd, &area, h, 500);
                if herd.chars[0].state != 0 {
                    west = west.min(herd.chars[0].tgx);
                }
            }
            west
        };
        assert!(aims(false) < cell(40), "ai {ai_id}: the original aims out");
        assert!(aims(true) >= 40.0 * 256.0, "ai {ai_id}: {}", aims(true));
    }
}

/// Each medium takes its own ground: a fish stays in water of its depth,
/// a wader in water, a flyer ignores what is below.
#[test]
fn a_flight_keeps_to_what_the_animal_moves_through() {
    use crate::area::cell_flags;
    use c2_formats::map::MAP_SIZE;
    use flee::{pick_target_kept, Keep, Medium};
    // A lake over cells 480..=520 both ways, 320 units deep.
    let mut area = Area::flat(10);
    area.rsc.waters = vec![c2_formats::rsc::Water {
        texture_index: 0,
        level: 20,
        transp: 1.0,
        fog_rgb: 0,
    }];
    for cz in 480..=520 {
        for cx in 480..=520 {
            area.map.flags[cz * MAP_SIZE + cx] |= cell_flags::WATER;
            area.map.water[cz * MAP_SIZE + cx] = 0;
        }
    }
    let wet = |p: (f32, f32)| area.land_up_h(p.0, p.1) > area.land_height(p.0, p.1);
    // By the lake's east shore, the hunter to the west: "away" leaves it.
    let c = at(cell(516), cell(500));
    let hunter = (cell(510), cell(500));
    let aim = |medium| {
        let keep = Keep {
            medium,
            ..Keep::default()
        };
        pick_target_kept(&c, &area, keep, (1.0, 0.0), hunter, 2048.0, 1.0, None)
    };
    let depth = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
    let band = Medium::Water {
        lo: 100.0,
        hi: depth + 100.0,
    };
    assert!(wet(aim(band)), "fish: {:?}", aim(band));
    assert!(wet(aim(Medium::Wading)), "wader: {:?}", aim(Medium::Wading));
    assert!(!wet(aim(Medium::Air)), "flyer: straight on");
    assert_eq!(aim(Medium::Air), (c.pos[0] + 2048.0, c.pos[2]));
    // A dry-land animal in the same spot would be refused the lake.
    let dry = Medium::Shore {
        dry: true,
        level: 0.0,
    };
    let shore = at(cell(524), cell(500));
    let keep = Keep {
        medium: dry,
        ..Keep::default()
    };
    let t = pick_target_kept(
        &shore,
        &area,
        keep,
        (-1.0, 0.0),
        (cell(530), cell(500)),
        2048.0,
        1.0,
        None,
    );
    assert!(!wet(t), "land brachiosaur: {t:?}");
}
