//! Spawn ecosystems ([`super::eco`]).

use super::eco::{Spawn, State};
use super::flee::{pick_target_kept, Fence, Keep};
use super::tests::{run, species};
use super::*;
use crate::area::TimeOfDay;
use c2_formats::bits::Bits;
use c2_formats::world::World;

const CELL: f32 = 256.0;

/// The hunter, the five small ambient animals, then the seven kinds of the
/// main population, named `a0`..`a12` by index.
fn stock_herd() -> Herd {
    let ais = [0, 1, 2, 3, 4, 5, 10, 11, 12, 13, 14, 15, 16];
    let mut herd = Herd {
        species: ais.iter().map(|&a| species(a, 8)).collect(),
        ..Default::default()
    };
    for (i, s) in herd.species.iter_mut().enumerate() {
        s.info.name = format!("a{i}");
    }
    herd
}

fn eco(herd: &Herd, text: &str) -> Option<State> {
    State::load(
        &World::parse(text.as_bytes()),
        "area1",
        &herd.species,
        TimeOfDay::Day,
    )
}

fn options() -> PlaceOptions {
    PlaceOptions {
        density: 128,
        opt_agres: 128,
        targets: Bits::NONE,
        max_ambient: None,
    }
}

const HUNTER: (f32, f32) = (512.0 * CELL, 512.0 * CELL);

fn placed(area: &Area, text: &str, seed: u32) -> Herd {
    let mut herd = stock_herd();
    herd.eco = eco(&herd, text);
    herd.place(area, HUNTER, &mut MsRand::new(seed), options());
    herd
}

fn in_circle(c: &Character, x: f32, z: f32, r: f32) -> bool {
    ((c.pos[0] / CELL - x).powi(2) + (c.pos[2] / CELL - z).powi(2)).sqrt() <= r
}

fn main_kinds(h: &Herd) -> impl Iterator<Item = &Character> {
    h.chars.iter().filter(|c| c.ctype >= 6)
}

#[test]
fn rules_for_another_map_or_none_at_all_load_nothing() {
    let herd = stock_herd();
    assert!(eco(&herd, "").is_none());
    assert!(eco(&herd, "area area2\nzone a circle 500 500 50\neco cap 1\n").is_none());
    assert!(eco(&herd, "area area1\nzone a circle 500 500 50\ntags wet\n").is_none());
    assert!(eco(&herd, "species_max 2 nobody\n").is_none());
    assert!(eco(&herd, "area AREA1\nzone a circle 500 500 50\neco cap 1\n").is_some());
    assert!(eco(&herd, "max_animals 9\n").is_some());
}

#[test]
fn rules_that_refuse_nothing_leave_the_placement_as_it_was() {
    let area = Area::flat(10);
    let plain = placed(&area, "", 7);
    // A cap never reached, a weight of 1 and containment: no spot is
    // refused and nothing is drawn from the random stream.
    let ruled = placed(
        &area,
        "max_animals 200\nspecies_max 99 a8\narea area1\n\
         zone all circle 512 512 900\neco cap 200\neco weight 1 a8\n",
        7,
    );
    assert!(plain.chars.len() > 8);
    assert_eq!(plain.chars.len(), ruled.chars.len());
    for (a, b) in plain.chars.iter().zip(&ruled.chars) {
        assert_eq!((a.ctype, a.pos), (b.ctype, b.pos));
    }
}

#[test]
fn nothing_is_placed_in_a_no_spawn_zone() {
    let area = Area::flat(10);
    // The right half of the map.
    let text = "area area1\nzone east polygon 512 0 1024 0 1024 1024 512 1024\neco no_spawn 1\n";
    for seed in 1..12 {
        let h = placed(&area, text, seed);
        assert!(main_kinds(&h).count() > 4, "seed {seed}");
        for c in &h.chars {
            assert!(c.pos[0] / CELL < 512.0, "seed {seed}: {:?}", c.pos);
        }
    }
}

#[test]
fn an_allow_list_keeps_the_others_out() {
    let area = Area::flat(10);
    let text = "area area1\nzone east polygon 512 0 1024 0 1024 1024 512 1024\n\
                eco allow a8\neco allow A9\n";
    let mut inside = 0;
    for seed in 1..12 {
        let h = placed(&area, text, seed);
        for c in h.chars.iter().filter(|c| c.pos[0] / CELL > 512.0) {
            assert!(matches!(c.ctype, 8 | 9), "seed {seed}: kind {}", c.ctype);
            inside += 1;
        }
    }
    assert!(inside > 0, "the allowed kinds still appear there");
}

#[test]
fn caps_hold_at_placement() {
    let area = Area::flat(10);
    for seed in 1..12 {
        let h = placed(
            &area,
            "max_animals 9\nspecies_max 1 a7\narea area1\n\
             zone west polygon 0 0 512 0 512 1024 0 1024\neco cap 2\n",
            seed,
        );
        assert!(h.chars.len() <= 9, "seed {seed}: {}", h.chars.len());
        assert!(h.chars.iter().filter(|c| c.ctype == 7).count() <= 1);
        let west = h.chars.iter().filter(|c| c.pos[0] / CELL < 512.0).count();
        assert!(west <= 2, "seed {seed}: {west} in the capped zone");
    }
}

#[test]
fn bodies_do_not_count_towards_a_cap() {
    let mut h = stock_herd();
    h.eco = eco(&h, "species_max 2 a6\n");
    let mut rng = MsRand::new(3);
    let p = [HUNTER.0, 0.0, HUNTER.1];
    assert!(h.spawn(6, p, &mut rng, 128) && h.spawn(6, p, &mut rng, 128));
    assert!(!h.spawn(6, p, &mut rng, 128), "at its cap");
    assert!(h.spawn(7, p, &mut rng, 128), "another kind is free");
    h.chars[0].health = 0;
    assert!(h.spawn(6, p, &mut rng, 128), "one died: one more may come");
}

#[test]
fn weights_set_how_the_animals_spread_over_the_zones() {
    let area = Area::flat(10);
    // Two zones of the same size either side of the hunter, one weighted
    // four times the other.
    let text = "area area1\nzone a circle 300 512 150\neco weight 4 a6\neco weight 4 a7\n\
                eco weight 4 a8\neco weight 4 a9\neco weight 4 a10\neco weight 4 a11\n\
                eco weight 4 a12\nzone b circle 724 512 150\n";
    let (mut a, mut b) = (0, 0);
    for seed in 1..=60 {
        let h = placed(&area, text, seed);
        a += main_kinds(&h)
            .filter(|c| in_circle(c, 300.0, 512.0, 150.0))
            .count();
        b += main_kinds(&h)
            .filter(|c| in_circle(c, 724.0, 512.0, 150.0))
            .count();
    }
    let ratio = a as f32 / b.max(1) as f32;
    assert!(b > 10 && (2.8..5.6).contains(&ratio), "{a} against {b}");
}

#[test]
fn an_impossible_rule_leaves_animals_out_and_returns() {
    let area = Area::flat(10);
    // Nowhere to spawn at all.
    let h = placed(
        &area,
        "area area1\nzone all polygon 0 0 1024 0 1024 1024 0 1024\neco no_spawn 1\n",
        4,
    );
    assert!(h.chars.is_empty(), "{}", h.chars.len());
    // One kind allowed nowhere: the others still come.
    let h = placed(
        &area,
        "area area1\nzone all polygon 0 0 1024 0 1024 1024 0 1024\neco weight 0 a6\n",
        4,
    );
    assert!(main_kinds(&h).count() > 4);
    assert!(h.chars.iter().all(|c| c.ctype != 6));
}

fn hunter_at(x: f32, z: f32) -> HunterView {
    HunterView {
        x,
        z,
        y: 640.0,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    }
}

/// One Parasaurolophus at cell (500, 500), in a zone of radius 12 there.
fn contained(contain: &str) -> (Area, Herd) {
    let area = Area::flat(10);
    let mut herd = Herd {
        species: vec![species(ai::PARA, 8)],
        ..Default::default()
    };
    let text = format!("area area1\nzone pen circle 500 500 12\neco contain {contain}\n");
    herd.eco = eco(&herd, &text);
    let p = [
        500.0 * CELL,
        area.land_height(500.0 * CELL, 500.0 * CELL),
        500.0 * CELL,
    ];
    assert!(herd.spawn(0, p, &mut MsRand::new(2), 128));
    (area, herd)
}

#[test]
fn hard_containment_holds_a_fleeing_animal_at_the_edge() {
    let (area, mut herd) = contained("hard");
    assert_eq!(herd.chars[0].home_zone, Some(0));
    // The hunter beside it: it runs, and without the zone would be far off.
    let hunter = hunter_at(500.0 * CELL + 800.0, 500.0 * CELL);
    for _ in 0..40 {
        run(&mut herd, &area, hunter, 1000);
        let c = &herd.chars[0];
        assert!(
            in_circle(c, 500.0, 500.0, 12.01),
            "left the zone: {:?}",
            c.pos
        );
    }
    let c = &herd.chars[0];
    assert!(!in_circle(c, 500.0, 500.0, 6.0), "it did run to the edge");
}

#[test]
fn soft_containment_lets_it_flee_and_keeps_its_wandering_inside() {
    let (area, mut herd) = contained("soft");
    let hunter = hunter_at(500.0 * CELL + 800.0, 500.0 * CELL);
    run(&mut herd, &area, hunter, 20_000);
    assert!(
        !in_circle(&herd.chars[0], 500.0, 500.0, 12.0),
        "a soft zone does not stop a fleeing animal"
    );
    // Calm, far from any hunter, it wanders: mostly in its zone.
    let (area, mut herd) = contained("soft");
    let far = HunterView {
        alive: false,
        ..hunter_at(100.0 * CELL, 100.0 * CELL)
    };
    let (mut inside, mut all) = (0, 0);
    for _ in 0..240 {
        run(&mut herd, &area, far, 1000);
        all += 1;
        inside += usize::from(in_circle(&herd.chars[0], 500.0, 500.0, 13.0));
    }
    assert!(inside * 10 >= all * 8, "{inside} of {all} s inside");
}

#[test]
fn a_free_animal_wanders_off_where_a_contained_one_stays() {
    // The control for the test above: the same walk without containment.
    let area = Area::flat(10);
    let mut herd = Herd {
        species: vec![species(ai::PARA, 8)],
        ..Default::default()
    };
    let p = [
        500.0 * CELL,
        area.land_height(500.0 * CELL, 500.0 * CELL),
        500.0 * CELL,
    ];
    assert!(herd.spawn(0, p, &mut MsRand::new(2), 128));
    let far = HunterView {
        alive: false,
        ..hunter_at(100.0 * CELL, 100.0 * CELL)
    };
    let mut inside = 0;
    for _ in 0..240 {
        run(&mut herd, &area, far, 1000);
        inside += usize::from(in_circle(&herd.chars[0], 500.0, 500.0, 13.0));
    }
    assert!(inside < 160, "{inside} of 240 s inside");
}

#[test]
fn a_forward_replacement_into_a_refused_spot_is_undone() {
    let area = Area::flat(10);
    let mut herd = stock_herd();
    herd.eco = eco(
        &herd,
        "area area1\nzone marsh circle 700 500 20\neco no_spawn 1\n\
         zone pen circle 300 500 20\neco contain soft\n",
    );
    let e = herd.eco.clone().unwrap_or_else(|| panic!("no rules"));
    let at = |x: f32| {
        [
            x * CELL,
            area.land_height(x * CELL, 500.0 * CELL),
            500.0 * CELL,
        ]
    };
    let mut rng = MsRand::new(1);
    assert!(herd.spawn(1, at(500.0), &mut rng, 128));
    // Moved into the no-spawn zone: put back.
    let before = e.before(&herd.chars[0]);
    herd.chars[0].pos = at(700.0);
    e.after_step(0, before, &mut herd.chars, &mut rng);
    assert_eq!(herd.chars[0].pos, at(500.0));
    // Moved into the containing zone: it stays, and that is its home now.
    let before = e.before(&herd.chars[0]);
    herd.chars[0].pos = at(300.0);
    e.after_step(0, before, &mut herd.chars, &mut rng);
    assert_eq!(herd.chars[0].pos, at(300.0));
    assert_eq!(herd.chars[0].home_zone, Some(1));
}

fn manhattan(c: &Character) -> f32 {
    ((c.pos[0] - HUNTER.0).abs() + (c.pos[2] - HUNTER.1).abs()) / CELL
}

#[test]
fn the_time_of_day_picks_the_rules_and_never_removes_an_animal() {
    let area = Area::flat(10);
    // By day anything anywhere; at night the east is closed and one kind
    // is capped.
    let text = "at night species_max 0 a7\narea area1\n\
                zone east polygon 512 0 1024 0 1024 1024 512 1024\neco cap 200\n\
                at night eco no_spawn 1\n";
    let mut day = stock_herd();
    day.eco = eco(&day, text);
    day.place(&area, HUNTER, &mut MsRand::new(5), options());
    let east = |h: &Herd| h.chars.iter().filter(|c| c.pos[0] / CELL > 512.0).count();
    assert!(east(&day) > 0, "by day the east is open");
    let mut night = stock_herd();
    night.eco = eco(&night, text);
    night.set_eco_time(TimeOfDay::Night);
    for seed in 1..8 {
        night.place(&area, HUNTER, &mut MsRand::new(seed), options());
        assert_eq!(east(&night), 0, "seed {seed}");
        assert!(night.chars.iter().all(|c| c.ctype != 7), "seed {seed}");
        assert!(main_kinds(&night).count() > 4, "seed {seed}");
    }
    // Night falls on the day's herd: nobody goes, nobody new comes east.
    let before = day.chars.len();
    day.set_eco_time(TimeOfDay::Night);
    let mut rng = MsRand::new(9);
    let spot = [700.0 * CELL, 0.0, 512.0 * CELL];
    assert_eq!(day.chars.len(), before);
    assert!(!day.eco_spot(6, &spot, &mut rng));
    assert!(!day.eco_room(7) && day.eco_room(6));
    day.set_eco_time(TimeOfDay::Dusk);
    assert!(
        day.eco_spot(6, &spot, &mut rng) && day.eco_room(7),
        "dawn inherits the base"
    );
}

#[test]
fn safety_distances_replace_the_originals_forty_cells() {
    let area = Area::flat(10);
    // The original's distance without the lines: some land well inside
    // the 150 cells asked for below.
    let nearest = (1..10)
        .flat_map(|seed| {
            let h = placed(&area, "max_animals 200\n", seed);
            main_kinds(&h).map(manhattan).collect::<Vec<_>>()
        })
        .fold(f32::MAX, f32::min);
    assert!((40.0..150.0).contains(&nearest), "{nearest}");
    for seed in 1..6 {
        let h = placed(&area, "safety initial 150\nsafety species 300 a8\n", seed);
        assert!(main_kinds(&h).count() > 4, "seed {seed}");
        for c in main_kinds(&h) {
            let least = if c.ctype == 8 { 300.0 } else { 150.0 };
            assert!(
                manhattan(c) >= least,
                "seed {seed}: kind {} at {}",
                c.ctype,
                manhattan(c)
            );
        }
    }
    // Each kind of spawn has its own distance; a smaller one is allowed.
    let mut h = stock_herd();
    h.eco = eco(&h, "safety respawn 10\nsafety extra 90\n");
    let at = |cells: f32| [HUNTER.0 + cells * CELL, 0.0, HUNTER.1];
    assert!(h.hunter_clear(Spawn::Respawn, Some(6), &at(12.0), HUNTER));
    assert!(!h.hunter_clear(Spawn::Initial, Some(6), &at(12.0), HUNTER));
    assert!(!h.hunter_clear(Spawn::Extra, Some(6), &at(80.0), HUNTER));
    assert!(h.hunter_clear(Spawn::Extra, Some(6), &at(95.0), HUNTER));
    h.eco = None;
    assert!(!h.hunter_clear(Spawn::Respawn, Some(6), &at(12.0), HUNTER));
    assert!(h.hunter_clear(Spawn::Respawn, Some(6), &at(40.0), HUNTER));
}

#[test]
fn unseen_replacements_keep_out_of_the_hunters_view() {
    let mut h = stock_herd();
    h.eco = eco(&h, "safety unseen 1\nsafety respawn 5\n");
    // He faces -z (alpha 0) and sees 60 cells.
    let hunter = hunter_at(HUNTER.0, HUNTER.1);
    let at = |dx: f32, dz: f32| [HUNTER.0 + dx * CELL, 0.0, HUNTER.1 + dz * CELL];
    let clear = |h: &Herd, kind, p: [f32; 3]| h.hunter_clear(kind, Some(6), &p, HUNTER);
    // Before any step he has no view: nothing is refused for it.
    assert!(clear(&h, Spawn::Respawn, at(0.0, -30.0)));
    if let Some(e) = &mut h.eco {
        e.set_view(hunter, 60);
    }
    assert!(!clear(&h, Spawn::Respawn, at(0.0, -30.0)), "straight ahead");
    assert!(
        !clear(&h, Spawn::Respawn, at(10.0, -30.0)),
        "within 60 degrees"
    );
    assert!(clear(&h, Spawn::Respawn, at(0.0, 30.0)), "behind him");
    assert!(clear(&h, Spawn::Respawn, at(30.0, 0.0)), "to his side");
    assert!(clear(&h, Spawn::Respawn, at(0.0, -70.0)), "past his sight");
    assert!(
        clear(&h, Spawn::Initial, at(0.0, -45.0)),
        "only replacements"
    );
}

#[test]
fn a_flight_keeps_out_of_avoided_zones_and_inside_a_hard_one() {
    let area = Area::flat(10);
    let mut herd = stock_herd();
    herd.eco = eco(
        &herd,
        "area area1\nzone swamp circle 520 500 8\neco avoid 1\n\
         zone pen circle 500 500 6\neco contain hard\n",
    );
    let rules = herd.eco.as_ref().map(|e| e.rules.clone());
    let rules = rules.unwrap_or_else(|| panic!("no rules"));
    let c = Character {
        pos: [505.0 * CELL, 0.0, 500.0 * CELL],
        ..Default::default()
    };
    let hunter = (495.0 * CELL, 500.0 * CELL);
    let pick = |keep: Keep<'_>| {
        let t = pick_target_kept(&c, &area, keep, (1.0, 0.0), hunter, 20.0 * CELL, 1.0, None);
        (t.0 / CELL, t.1 / CELL)
    };
    let dist = |p: (f32, f32), x: f32, z: f32| ((p.0 - x).powi(2) + (p.1 - z).powi(2)).sqrt();
    // Unfenced, straight away from the hunter runs into the swamp.
    let free = pick(Keep::default());
    assert!(dist(free, 520.0, 500.0) < 8.0, "{free:?}");
    let fence = |home| Keep {
        fence: Some(Fence {
            eco: &rules,
            home,
            from: (c.pos[0], c.pos[2]),
        }),
        ..Keep::default()
    };
    let around = pick(fence(None));
    assert!(dist(around, 520.0, 500.0) > 8.0, "{around:?}");
    assert!(
        dist(around, 505.0, 500.0) > 2.0,
        "it still runs: {around:?}"
    );
    // At home in the pen (zone 1), the flight stays in it.
    let penned = pick(fence(Some(1)));
    assert!(dist(penned, 500.0, 500.0) <= 6.0, "{penned:?}");
    // In an avoided zone already, it may run out of (and through) it.
    assert!(rules.may_aim(
        None,
        (520.0 * CELL, 500.0 * CELL),
        522.0 * CELL,
        500.0 * CELL
    ));
    assert!(!rules.may_aim(
        None,
        (505.0 * CELL, 500.0 * CELL),
        522.0 * CELL,
        500.0 * CELL
    ));
}

#[test]
fn a_calm_animals_target_is_pulled_out_of_an_avoided_zone() {
    let mut herd = stock_herd();
    herd.eco = eco(
        &herd,
        "area area1\nzone swamp circle 520 500 8\neco avoid 1\n",
    );
    let e = herd.eco.clone().unwrap_or_else(|| panic!("no rules"));
    let mut rng = MsRand::new(1);
    let p = [505.0 * CELL, 0.0, 500.0 * CELL];
    assert!(herd.spawn(6, p, &mut rng, 128));
    herd.chars[0].state = 0;
    let before = e.before(&herd.chars[0]);
    herd.chars[0].tgx = 520.0 * CELL;
    herd.chars[0].tgz = 500.0 * CELL;
    e.after_step(0, before, &mut herd.chars, &mut rng);
    let c = &herd.chars[0];
    assert_eq!(c.tgz, 500.0 * CELL);
    assert!((505.0..512.0).contains(&(c.tgx / CELL)), "{}", c.tgx / CELL);
    // A target clear of the zone is left alone.
    let before = e.before(&herd.chars[0]);
    herd.chars[0].tgx = 490.0 * CELL;
    e.after_step(0, before, &mut herd.chars, &mut rng);
    assert_eq!(herd.chars[0].tgx, 490.0 * CELL);
}
