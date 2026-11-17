//! Scuba gear: drowning at half the rate, and the score.

use super::binoculars::kill_score;
use super::support::setup;
use crate::area::{cell_flags, Area};
use crate::game::{HuntInput, MAX_HEALTH, SCUBA_SCORE};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use c2_formats::map::MAP_SIZE;

/// Deep water over the hunter's cells of `area`.
fn flood(area: &mut Area) {
    area.rsc.waters = vec![c2_formats::rsc::Water {
        texture_index: 0,
        level: 40,
        transp: 1.0,
        fog_rgb: 0,
    }];
    for cz in 490..=510 {
        for cx in 490..=510 {
            let i = cz * MAP_SIZE + cx;
            area.map.flags[i] |= cell_flags::WATER;
            area.map.water[i] = 0;
        }
    }
}

/// Health lost over one second under water, from half health.
fn drowned(scuba: bool) -> i32 {
    let (mut area, mut h, _) = setup(false);
    flood(&mut area);
    h.opts.scuba = scuba;
    let mut out = Vec::new();
    let mut tick = |h: &mut crate::game::Hunt| {
        h.tick(&area, &[], &HuntInput::default(), 10, &mut out);
    };
    tick(&mut h);
    assert!(h.player.underwater, "scuba {scuba}");
    h.health = MAX_HEALTH / 2;
    for _ in 0..100 {
        tick(&mut h);
    }
    MAX_HEALTH / 2 - h.health
}

#[test]
fn the_oxygen_tank_halves_drowning() {
    let plain = drowned(false);
    let scuba = drowned(true);
    // The original's net 8 a millisecond (12 lost, 4 regained).
    assert_eq!(plain, 8000);
    assert_eq!(plain, 2 * scuba, "plain {plain}, scuba {scuba}");
}

#[test]
fn the_gear_takes_fifteen_percent_of_a_kill() {
    let (mut area, mut geared, _) = setup(false);
    geared.opts.scuba = true;
    assert_eq!(kill_score(&mut area, &mut geared), 850);
    assert_eq!(SCUBA_SCORE, 0.85);
}

#[test]
fn the_terminal_prices_the_gear_into_its_earnings() {
    let k = crate::dhc::earnings_factor(["Scuba gear"], false);
    assert!((k - 0.85).abs() < 1e-6, "{k}");
    let k = crate::dhc::earnings_factor(["Radar", "scuba gear"], false);
    assert!((k - 0.70 * 0.85).abs() < 1e-6, "{k}");
}
