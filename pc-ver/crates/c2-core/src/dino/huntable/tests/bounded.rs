//! Bounded fleeing ([`flee`](crate::dino::flee)) in the dialect's own
//! AIs: ambient animals, wary flyers, brachiosaurs and fish at the map's
//! west edge with the hunter just inside it.

use super::*;
use crate::dino::flee::{FleeLimits, FleeTable};

/// The first cell a bounded flight may aim at (the walkable square's edge
/// plus the aim margin).
const MARGIN: f32 = 40.0 * 256.0;

/// The westmost point the animal aimed at while alarmed over 15 s, standing
/// at cell (42, 500) with the hunter three cells east.
fn westmost_aim(area: &Area, m: MeeInfo, bounded: bool) -> f32 {
    let (x, z) = (42.5 * 256.0, 500.5 * 256.0);
    let mut h = hunter();
    (h.x, h.z) = (x + 3.0 * 256.0, z);
    // In the water when there is any (a fish only minds a swimmer).
    h.y = area.land_up_h(h.x, h.z).max(area.land_height(h.x, h.z)) - 100.0;
    let mut herd = Herd {
        species: vec![species(m)],
        ..Default::default()
    };
    if bounded {
        herd.flee = Some(Box::new(FleeTable {
            per_species: vec![FleeLimits::default()],
            ..Default::default()
        }));
    }
    let i = add(&mut herd, 0, area, x, z);
    let mid = (area.land_up_h(x, z) + area.land_height(x, z)) / 2.0;
    (herd.chars[i].depth, herd.chars[i].tdepth) = (mid, mid);
    let mut west = f32::MAX;
    for _ in 0..15 {
        let c = &mut herd.chars[i];
        (c.state, c.afraid_time) = (1, 20_000);
        for _ in 0..10 {
            run(&mut herd, area, h, 100);
            let c = &herd.chars[i];
            if c.state != 0 {
                west = west.min(c.tgx);
            }
        }
    }
    west
}

/// Off, the animal aims off the map; on, never past the margin.
fn check(area: &Area, m: MeeInfo, what: &str) {
    let off = westmost_aim(area, m.clone(), false);
    let on = westmost_aim(area, m, true);
    assert!(off < MARGIN, "{what}: the original aims outside ({off})");
    assert!(on >= MARGIN, "{what}: bounded aims inside ({on})");
}

#[test]
fn an_ambient_animal_keeps_to_the_map() {
    check(&Area::flat(10), info(ai::MOSH), "ambient");
}

#[test]
fn a_wary_flyer_keeps_to_the_map() {
    let mut m = info(mee_ai::WARYDIMOR);
    (m.fly, m.glide) = (IDLE, IDLE + 1);
    (m.fly_speed, m.glide_speed) = (0.6, 0.6);
    (m.min_depth, m.max_depth) = (300, 1200);
    check(&Area::flat(10), m, "flyer");
}

#[test]
fn a_fleeing_land_brachiosaur_keeps_to_the_map() {
    let mut m = info(mee_ai::LANDBRACH);
    // No aggression: it always backs off.
    (m.aggress, m.collision_dist) = (0, 128);
    m.rot_speed_multi = 1.5;
    m.run_speed = 0.4;
    check(&Area::flat(10), m, "land brachiosaur");
}

#[test]
fn a_shy_fish_keeps_to_the_map() {
    let mut m = info(mee_ai::FISH);
    (m.min_depth, m.max_depth, m.spacing_depth) = (200, 8000, 400);
    m.aggress = 80;
    check(&sea(), m, "fish");
}
