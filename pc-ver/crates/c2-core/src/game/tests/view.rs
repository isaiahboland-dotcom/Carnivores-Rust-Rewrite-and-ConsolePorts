//! Binocular range and selection regressions using synthetic terrain/animals.

use super::support::setup;
use crate::game::Hunt;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
use crate::CELL;

fn target_at(hunt: &mut Hunt, distance: f32) {
    let (eye, alpha, _) = hunt.camera();
    hunt.herd.chars[0].pos = [
        eye[0] + alpha.sin() * distance,
        eye[1] - 220.0 * hunt.herd.chars[0].scale,
        eye[2] - alpha.cos() * distance,
    ];
}

#[test]
fn binocular_range_follows_render_distance_with_inclusive_boundary() {
    let (area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.view_r = 74;
    for cells in [42, 74, 104, 160, 250] {
        hunt.opts.view_r = cells.min(74);
        hunt.opts.render_r = cells;
        for power in [1.5, 3.0] {
            hunt.bin_power = power;
            for (offset, visible) in [(-1.0, true), (0.0, true), (1.0, false)] {
                target_at(&mut hunt, cells as f32 * CELL + offset);
                assert_eq!(
                    hunt.life_info(&area).is_some(),
                    visible,
                    "cells={cells}, zoom={power}, offset={offset}"
                );
            }
        }
    }
    // A legacy caller with a smaller render radius retains its view range.
    hunt.opts.view_r = 74;
    hunt.opts.render_r = 42;
    target_at(&mut hunt, 74.0 * CELL);
    assert!(hunt.life_info(&area).is_some());
}

#[test]
fn binoculars_identify_beyond_300m_only_when_the_view_reaches_it() {
    let (area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.view_r = 74;
    hunt.opts.render_r = 384;
    for metres in [295.0, 296.0, 299.0, 300.0, 301.0, 1000.0] {
        target_at(&mut hunt, metres * 64.0);
        assert!(hunt.life_info(&area).is_some(), "{metres}m");
        assert_eq!(
            hunt.life_info_with_range(&area, 74).is_some(),
            metres <= 296.0
        );
    }
}

#[test]
fn binoculars_cap_standard_and_high_end_views_at_1000m() {
    let (area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.render_r = 384;
    for cells in [250, 384, 1024] {
        for power in [1.5, 3.0] {
            hunt.bin_power = power;
            for (metres, visible) in [(999.0, true), (1000.0, true), (1001.0, false)] {
                target_at(&mut hunt, metres * 64.0);
                assert_eq!(hunt.life_info_with_range(&area, cells).is_some(), visible);
            }
        }
    }
}

#[test]
fn extended_binocular_range_preserves_selection_and_terrain_occlusion() {
    let (mut area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.render_r = 384;
    target_at(&mut hunt, 400.0 * 64.0);
    assert!(hunt.life_info(&area).is_some());
    hunt.bin_mode = false;
    assert!(hunt.life_info(&area).is_none());
    hunt.bin_mode = true;
    hunt.herd.chars[0].health = 0;
    assert!(hunt.life_info(&area).is_none());
    hunt.herd.chars[0].health = 5;
    hunt.herd.chars[0].pos[0] += 10000.0;
    assert!(hunt.life_info(&area).is_none());
    target_at(&mut hunt, 400.0 * 64.0);
    let mut nearer = hunt.herd.chars[0].clone();
    nearer.pos[2] += 1000.0;
    hunt.herd.chars.push(nearer);
    assert_eq!(hunt.life_info(&area).unwrap().index, 1);
    // A ridge between the hunter and both otherwise eligible targets.
    for z in 449..452 {
        for x in 498..503 {
            area.map.height[crate::area::idx(x, z)] = 200;
        }
    }
    assert!(hunt.life_info(&area).is_none());
}

#[test]
fn binoculars_preserve_near_and_behind_camera_limits() {
    let (area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.render_r = 384;
    for (distance, visible) in [(-1000.0, false), (511.0, false), (512.0, true)] {
        target_at(&mut hunt, distance);
        assert_eq!(hunt.life_info(&area).is_some(), visible);
    }
}

#[test]
fn binoculars_require_precise_aim_at_both_zoom_levels() {
    let (area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.render_r = 384;
    for distance in [300.0 * 64.0, 900.0 * 64.0] {
        for power in [1.5, 3.0] {
            hunt.bin_power = power;
            for (screen_offset, visible) in [(0.04, true), (0.06, false), (0.14, false)] {
                for axis in [0, 1] {
                    target_at(&mut hunt, distance);
                    hunt.herd.chars[0].pos[axis] += distance * screen_offset / power;
                    assert_eq!(
                        hunt.life_info(&area).is_some(),
                        visible,
                        "distance={distance}, zoom={power}, axis={axis}, offset={screen_offset}"
                    );
                }
            }
        }
    }
}

#[test]
fn distant_mountain_blocks_centered_animals_for_both_terrain_diagonals() {
    let (mut area, mut hunt, _) = setup(false);
    hunt.bin_mode = true;
    hunt.opts.render_r = 384;
    // Aim diagonally across many cells at 900 m; the broad aim cone is not
    // involved. Repeat with each terrain triangle orientation.
    hunt.player.alpha = -core::f32::consts::FRAC_PI_4;
    for reverse in [0, crate::area::cell_flags::REVERSE] {
        area.map.height.fill(10);
        area.map.flags.fill(reverse);
        target_at(&mut hunt, 900.0 * 64.0);
        assert!(hunt.life_info(&area).is_some());
        for z in 415..425 {
            for x in 400..440 {
                area.map.height[crate::area::idx(x, z)] = 100;
            }
        }
        for power in [1.5, 3.0] {
            hunt.bin_power = power;
            assert!(hunt.life_info(&area).is_none());
        }
    }
}
