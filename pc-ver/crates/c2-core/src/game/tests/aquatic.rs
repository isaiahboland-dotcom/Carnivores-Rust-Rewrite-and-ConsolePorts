//! Aquatic animals kept near the hunter (`game::aquatic`), on a flat map
//! with two lakes.

use super::support::{info, setup, target_car};
use crate::area::{cell_flags, Area};
use crate::dino::mee::{mee_ai, MeeInfo};
use crate::dino::Species;
use crate::game::{Hunt, AQUATIC_FAR_M, AQUATIC_NEAR_M};
use crate::CELL;
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec, vec::Vec};
use c2_formats::map::MAP_SIZE;

/// The near lake, 60 to 80 cells east of the hunter's start (500, 500),
/// and the far one, by the map's east edge.
const NEAR_LAKE: (usize, usize) = (560, 580);
const FAR_LAKE: (usize, usize) = (940, 970);

/// Water over cells `x.0..=x.1`, 20 cells either side of row 500.
fn flood(area: &mut Area, x: (usize, usize)) {
    for cz in 480..=520 {
        for cx in x.0..=x.1 {
            let i = cz * MAP_SIZE + cx;
            area.map.flags[i] |= cell_flags::WATER;
            area.map.water[i] = 0;
        }
    }
}

/// The hunt's species 2 is a fish; `area` has the two lakes.
fn lakes() -> (Area, Hunt, usize) {
    let (mut area, mut h, _) = setup(false);
    area.rsc.waters = vec![c2_formats::rsc::Water {
        texture_index: 0,
        // Ten height steps over the flat ground.
        level: 20,
        transp: 1.0,
        fog_rgb: 0,
    }];
    flood(&mut area, NEAR_LAKE);
    flood(&mut area, FAR_LAKE);
    let mut fish = Species::new(info(mee_ai::FISH), &target_car(false));
    fish.mee = Some(Box::new(MeeInfo {
        min_depth: 100,
        max_depth: 2000,
        ..MeeInfo::new(mee_ai::FISH)
    }));
    h.herd.species.push(fish);
    (area, h, 2)
}

fn fish_cells(h: &Hunt, fish: usize) -> Vec<usize> {
    let of_kind = h.herd.chars.iter().filter(|c| c.ctype == fish);
    of_kind.map(|c| (c.pos[0] / CELL) as usize).collect()
}

fn metres_away(h: &Hunt, fish: usize) -> Vec<f32> {
    let (px, pz) = (h.player.x, h.player.z);
    let of_kind = h.herd.chars.iter().filter(|c| c.ctype == fish);
    of_kind
        .map(|c| ((c.pos[0] - px).powi(2) + (c.pos[2] - pz).powi(2)).sqrt() / 64.0)
        .collect()
}

#[test]
fn aquatic_animals_start_in_the_closest_water_past_200_metres() {
    let (area, mut h, fish) = lakes();
    assert!(crate::game::has_water(&area));
    assert_eq!(h.roam_aquatics(&area, &[(fish, 2)]), 2);
    let cells = fish_cells(&h, fish);
    assert_eq!(cells.len(), 2);
    for (cell, m) in cells.iter().zip(metres_away(&h, fish)) {
        assert!((NEAR_LAKE.0..=NEAR_LAKE.1).contains(cell), "{cell}");
        assert!(m >= AQUATIC_NEAR_M - 1.0, "{m} m");
        // The closest water, not just any: the lake starts at 240 m.
        assert!(m < 300.0, "{m} m");
    }
    // In the water, not on its bed.
    for c in h.herd.chars.iter().filter(|c| c.ctype == fish) {
        assert!(c.pos[1] > area.land_height(c.pos[0], c.pos[2]));
        assert!(c.pos[1] < area.land_up_h(c.pos[0], c.pos[2]));
    }
}

#[test]
fn one_left_400_metres_behind_comes_back_to_the_closest_water() {
    let (area, mut h, fish) = lakes();
    h.roam_aquatics(&area, &[(fish, 2)]);
    // The hunter crosses the map to the far lake's shore.
    h.player.x = 880.0 * CELL;
    assert!(metres_away(&h, fish).iter().all(|&m| m > AQUATIC_FAR_M));
    // One animal a second.
    h.step_aquatics(&area, 999);
    assert!(fish_cells(&h, fish).iter().all(|c| *c <= NEAR_LAKE.1));
    for _ in 0..3 {
        h.step_aquatics(&area, 1000);
    }
    for (cell, m) in fish_cells(&h, fish).iter().zip(metres_away(&h, fish)) {
        assert!((FAR_LAKE.0..=FAR_LAKE.1).contains(cell), "{cell}");
        assert!((AQUATIC_NEAR_M - 1.0..AQUATIC_FAR_M).contains(&m), "{m} m");
    }
    assert_eq!(fish_cells(&h, fish).len(), 2);
}

#[test]
fn far_water_keeps_its_animals_when_none_is_closer() {
    let (area, mut h, fish) = lakes();
    h.roam_aquatics(&area, &[(fish, 1)]);
    let before = fish_cells(&h, fish);
    // West of everything: the near lake is 260 cells (1,040 m) away and
    // still the closest water.
    h.player.x = 300.0 * CELL;
    for _ in 0..5 {
        h.step_aquatics(&area, 1000);
    }
    let after = fish_cells(&h, fish);
    assert_eq!(after.len(), 1);
    assert!((NEAR_LAKE.0..=NEAR_LAKE.1).contains(&after[0]));
    // It was not taken out and put back in the same lake every second.
    assert_eq!(before, after);
}

#[test]
fn a_dead_one_stays_where_it_died() {
    let (area, mut h, fish) = lakes();
    h.roam_aquatics(&area, &[(fish, 1)]);
    let i = h.herd.chars.iter().position(|c| c.ctype == fish);
    let Some(i) = i else {
        panic!("a fish was placed");
    };
    h.herd.chars[i].health = 0;
    let at = h.herd.chars[i].pos;
    h.player.x = 880.0 * CELL;
    h.step_aquatics(&area, 1000);
    assert_eq!(h.herd.chars[i].pos, at);
}

#[test]
fn no_water_of_its_kind_places_none_and_land_animals_are_left_alone() {
    let (_, mut h, fish) = lakes();
    // A dry map.
    let dry = Area::flat(10);
    assert!(!crate::game::has_water(&dry));
    assert_eq!(h.roam_aquatics(&dry, &[(fish, 2)]), 0);
    assert!(h.aquatic.is_none());
    // A fish of deeper water than the lakes have.
    let (area, mut h, fish) = lakes();
    if let Some(m) = h.herd.species[fish].mee.as_mut() {
        m.min_depth = 5000;
        m.max_depth = 9000;
    }
    assert_eq!(h.roam_aquatics(&area, &[(fish, 2)]), 0);
    // A land animal is not an aquatic one: nothing is placed, and the one
    // on the map stays.
    let (area, mut h, _) = lakes();
    assert_eq!(h.roam_aquatics(&area, &[]), 0);
    assert_eq!(h.herd.chars.len(), 1);
    assert!(h.aquatic.is_none());
    // Without the roaming the tick does nothing.
    h.player.x = 880.0 * CELL;
    h.step_aquatics(&area, 5000);
    assert_eq!(h.herd.chars.len(), 1);
}
