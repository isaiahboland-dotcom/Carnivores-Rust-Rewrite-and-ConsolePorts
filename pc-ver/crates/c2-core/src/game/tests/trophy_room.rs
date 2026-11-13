use super::support::{blank_save, info};
use crate::area::Area;
use crate::dino::{ai, Herd, Species};
use crate::game::trophy_room::{
    displayed_species, plan, plate_lines, Budget, ModelCost, RoomMode, PLATE_COLS, PORTRAIT_KB,
};
use crate::rng::MsRand;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use c2_formats::sav::TrophyItem;

const BUDGET: Budget = Budget {
    linear_kb: 22 * 1024,
    heap_kb: 34 * 1024,
    base_linear_kb: 4 * 1024,
    base_heap_kb: 20 * 1024,
};

fn stands(ctypes: &[i32]) -> [TrophyItem; 24] {
    let mut bodies = blank_save().bodies;
    for (slot, &c) in ctypes.iter().enumerate() {
        bodies[slot] = TrophyItem {
            ctype: c,
            scale: 1.0,
            ..Default::default()
        };
    }
    bodies
}

fn cost(linear_kb: usize, heap_kb: usize) -> Option<ModelCost> {
    Some(ModelCost { linear_kb, heap_kb })
}

#[test]
fn small_species_stay_models_even_past_five() {
    // Eight small animals: the old fixed limit of five would have fallen
    // back; the real cost fits, so the 3D room stays.
    let bodies = stands(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let p = plan(&bodies, 20, BUDGET, &|_| cost(300, 500), &|_| true);
    assert_eq!(p.mode, RoomMode::Models);
    assert_eq!(p.load, p.species);
    assert_eq!(p.species.len(), 8);
    assert!(!p.low_memory());
}

#[test]
fn few_huge_species_fall_back_below_five() {
    // Four big add-ons don't fit: a species count never decides.
    let bodies = stands(&[1, 2, 3, 4]);
    let p = plan(&bodies, 20, BUDGET, &|_| cost(2000, 6000), &|_| true);
    assert_eq!(p.mode, RoomMode::Portraits);
    assert!(p.load.is_empty());
    assert_eq!(p.species, [1, 2, 3, 4]);
    assert!(p.low_memory());
    assert_eq!(p.linear_kb, BUDGET.base_linear_kb + 4 * PORTRAIT_KB);
}

#[test]
fn five_or_fewer_species_that_fit_keep_the_original_room() {
    let bodies = stands(&[1, 1, 2, 3, 4, 5]);
    let p = plan(&bodies, 20, BUDGET, &|_| cost(1000, 2000), &|_| true);
    assert_eq!(p.mode, RoomMode::Models);
    assert_eq!(p.species, [1, 2, 3, 4, 5]);
}

#[test]
fn the_budget_edge_is_inclusive() {
    let bodies = stands(&[1, 2]);
    let exact = BUDGET.linear_kb - BUDGET.base_linear_kb;
    let fit = plan(&bodies, 5, BUDGET, &|_| cost(exact / 2, 0), &|_| true);
    assert_eq!(fit.mode, RoomMode::Models);
    let over = plan(&bodies, 5, BUDGET, &|_| cost(exact / 2 + 1, 0), &|_| true);
    assert_eq!(over.mode, RoomMode::Portraits);
    // Main memory decides the same way.
    let heap = BUDGET.heap_kb - BUDGET.base_heap_kb;
    let over = plan(&bodies, 5, BUDGET, &|_| cost(0, heap / 2 + 1), &|_| true);
    assert_eq!(over.mode, RoomMode::Portraits);
}

#[test]
fn an_uncooked_model_cannot_be_a_statue() {
    let bodies = stands(&[1, 2]);
    let p = plan(
        &bodies,
        5,
        BUDGET,
        &|s| if s == 2 { None } else { cost(10, 10) },
        &|_| true,
    );
    assert_eq!(p.mode, RoomMode::Portraits);
}

#[test]
fn modded_species_and_variants_are_listed_by_their_own_index() {
    // Index 12 is a modded animal; 13 is a second skin of the same species
    // (a variant has its own character, model texture and portrait).
    let bodies = stands(&[12, 13, 12, 13]);
    assert_eq!(displayed_species(&bodies, 20), [12, 13]);
    // Records of animals the pack lacks (ctype -1 for a missing variant,
    // an index past the table) are skipped, not dropped from the save.
    let bodies = stands(&[12, -1, 99, 0, 13]);
    assert_eq!(displayed_species(&bodies, 20), [12, 13]);
    assert_eq!(bodies[1].ctype, -1);
    assert_eq!(bodies[2].ctype, 99);
}

#[test]
fn a_missing_portrait_leaves_a_plate_only_stand() {
    let bodies = stands(&[1, 2, 3]);
    let p = plan(&bodies, 5, BUDGET, &|_| cost(9000, 9000), &|s| s != 2);
    assert_eq!(p.mode, RoomMode::Portraits);
    assert_eq!(p.no_portrait, [2]);
    assert_eq!(p.species, [1, 2, 3]);
    assert_eq!(p.linear_kb, BUDGET.base_linear_kb + 2 * PORTRAIT_KB);
}

#[test]
fn a_full_room_of_distinct_species_is_bounded() {
    let all: Vec<i32> = (1..=24).collect();
    let bodies = stands(&all);
    let p = plan(&bodies, 40, BUDGET, &|_| cost(700, 900), &|_| true);
    assert_eq!(p.species.len(), 24);
    // 24 models at this size don't fit, the 24 portraits do.
    assert_eq!(p.mode, RoomMode::Portraits);
    assert!(p.linear_kb <= BUDGET.linear_kb);
    // Stands past the room's 24 are never read.
    let mut long = bodies.to_vec();
    long.push(TrophyItem {
        ctype: 30,
        ..Default::default()
    });
    assert_eq!(displayed_species(&long, 40).len(), 24);
}

#[test]
fn planning_leaves_the_records_alone() {
    let bodies = stands(&[3, 4, 3]);
    let before = bodies;
    let _ = plan(&bodies, 9, BUDGET, &|_| cost(9999, 9999), &|_| false);
    assert_eq!(bodies, before);
}

#[test]
fn portrait_stands_place_trophies_whose_animals_are_not_loaded() {
    let mut area = Area::flat(10);
    area.set_trophy_mode();
    let room = {
        let mut r = blank_save();
        r.bodies = stands(&[1, 1, 1]);
        r.bodies[2].scale = 0.8;
        r
    };
    // Species without a model: no animations.
    let empty = || Species::new(info(ai::STEGO), &super::support::target_car(true));
    let mut bare = empty();
    bare.ani_time.clear();
    let species = vec![empty(), bare];
    let mut herd = Herd {
        species,
        ..Default::default()
    };
    herd.place_trophies(&area, &room, &mut MsRand::new(1));
    assert!(herd.chars.is_empty(), "unloaded animals aren't statues");
    herd.portrait_stands = true;
    herd.place_trophies(&area, &room, &mut MsRand::new(1));
    assert_eq!(herd.chars.len(), 3);
    assert_eq!(herd.chars[2].state, 2);
    assert_eq!(herd.chars[2].scale, 0.8);
}

#[test]
fn plates_fit_the_label_and_follow_the_unit_setting() {
    let b = TrophyItem {
        scale: 1.0,
        score: 1234,
        ..Default::default()
    };
    let m = plate_lines("Parasaurolophus walkeri", 2.0, 9.0, &b, false);
    assert_eq!(m[0].chars().count(), PLATE_COLS);
    assert_eq!(m[1], "Score 1234");
    assert_eq!(m[2], "2.0T 9.0m");
    let us = plate_lines("Rex", 2.0, 9.0, &b, true);
    assert_eq!(us[0], "Rex");
    assert_eq!(us[2], "2.2t 30ft");
}
