//! The hardcore hunt ruleset: zones and extraction, the forfeit, equipment
//! limits, ammunition drops, the knife and its dodge, and hesitation in a
//! whole hunt.

use super::support::{info, setup};
use crate::area::Area;
use crate::dino::{ai, Character, Species};
use crate::game::hardcore::{test_rules, Deploy, Pickup};
use crate::game::{GameEvent, Hunt, HuntInput, Outcome};
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
use crate::CELL;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use c2_formats::hardcore::{self as fmt, Reaction};

fn ticks(h: &mut Hunt, area: &Area, ms: i32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ms / 10 {
        h.tick(area, &[], &HuntInput::default(), 10, &mut ev);
    }
    ev
}

/// A hunt with `rules`, deployed: dropped where the hunter stands, the
/// pickup zone 60 cells east of it. The Stegosaurus of the fixture is put
/// far off.
fn hunt_with(rules: &fmt::Hunt) -> (Area, Hunt) {
    let (area, mut h, _) = setup(false);
    h.herd.chars[0].pos = [900.0 * CELL, 0.0, 900.0 * CELL];
    h.start_hardcore(&area, rules);
    if h.deploying().is_some() {
        let (x, z) = (h.player.x, h.player.z);
        assert_eq!(h.pick_zone(&area, x, z), Ok(()));
        assert_eq!(h.pick_zone(&area, x + 60.0 * CELL, z), Ok(()));
    }
    (area, h)
}

fn only(f: impl FnOnce(&mut fmt::Hunt)) -> fmt::Hunt {
    let mut r = fmt::Hunt::default();
    f(&mut r);
    r
}

fn extraction() -> fmt::Hunt {
    only(|r| r.extraction = Some(fmt::Extraction::default()))
}

fn pickup(h: &Hunt) -> Pickup {
    h.hardcore.as_deref().map(|x| x.pickup).unwrap_or_default()
}

fn enter_zone(h: &mut Hunt) {
    let (x, z) = h
        .hardcore
        .as_deref()
        .and_then(|x| x.zone)
        .unwrap_or_default();
    (h.player.x, h.player.z) = (x + 100.0, z);
}

#[test]
fn no_rules_is_a_regular_hunt() {
    let (area, mut h, _) = setup(false);
    h.start_hardcore(&area, &fmt::Hunt::default());
    assert!(h.hardcore.is_none() && h.herd.hesitation.is_none());
    assert!(h.deploying().is_none() && !h.exit_forfeits() && !h.knife_ready());
    let mut ev = Vec::new();
    assert!(h.call_ship(&mut ev).is_err());
    assert!(!h.dodge(1.0));
}

#[test]
fn the_ruleset_stays_out_of_observer_hunts() {
    let (area, mut h, _) = setup(false);
    h.opts.observer = true;
    h.start_hardcore(&area, &test_rules::all());
    assert!(h.hardcore.is_none());
}

#[test]
fn the_zones_are_picked_before_anything_moves() {
    let (area, mut h, _) = setup(false);
    h.start_hardcore(&area, &extraction());
    assert_eq!(h.deploying(), Some(Deploy::DropOff));
    ticks(&mut h, &area, 1000);
    assert_eq!(h.real_time, 0, "the hunt waits for the zones");
    let (x, z) = (h.player.x + 100.0 * CELL, h.player.z);
    assert_eq!(h.pick_zone(&area, x, z), Ok(()));
    assert_eq!(h.deploying(), Some(Deploy::Pickup));
    assert!((h.player.x - x).abs() < CELL, "the hunter is dropped there");
    // Too close to the drop-off (the default gap is 48 cells).
    assert!(h.pick_zone(&area, x + 20.0 * CELL, z).is_err());
    assert_eq!(h.deploying(), Some(Deploy::Pickup));
    assert_eq!(h.pick_zone(&area, x + 50.0 * CELL, z), Ok(()));
    assert_eq!(h.deploying(), None);
    assert!(h.pick_zone(&area, x, z).is_err());
    ticks(&mut h, &area, 100);
    assert_eq!(h.real_time, 100);
}

#[test]
fn dangerous_animals_are_moved_out_of_the_chosen_drop_off() {
    let (area, mut h, car) = setup(false);
    h.herd.species.push(Species::new(info(ai::VELO), &car));
    let (x, z) = (h.player.x + 200.0 * CELL, h.player.z);
    let mut near = Character {
        ctype: 2,
        ai: ai::VELO,
        health: 5,
        scale: 1.0,
        pos: [x + 5.0 * CELL, 0.0, z],
        ..Default::default()
    };
    h.herd.chars.push(near.clone());
    near.ctype = 1; // The Stegosaurus kind: harmless, it stays.
    h.herd.chars.push(near);
    h.start_hardcore(&area, &extraction());
    assert_eq!(h.pick_zone(&area, x, z), Ok(()));
    let d = |c: &Character| ((c.pos[0] - x).powi(2) + (c.pos[2] - z).powi(2)).sqrt() / CELL;
    assert!(d(&h.herd.chars[1]) > 30.0, "{}", d(&h.herd.chars[1]));
    assert!(d(&h.herd.chars[2]) < 6.0);
}

#[test]
fn the_ship_arrives_hovers_leaves_and_cools_down() {
    let (area, mut h) = hunt_with(&extraction());
    let mut ev = Vec::new();
    assert_eq!(h.call_ship(&mut ev), Ok(()));
    assert_eq!(pickup(&h), Pickup::Arriving(5000));
    assert!(h.call_ship(&mut ev).is_err(), "already on its way");
    ticks(&mut h, &area, 4990);
    assert!(matches!(pickup(&h), Pickup::Arriving(_)));
    assert!(h.extraction_flies_ship());
    ticks(&mut h, &area, 10);
    assert_eq!(pickup(&h), Pickup::Hovering(30_000));
    assert!(h.call_ship(&mut ev).is_err());
    // 30 s with nobody in the zone: it leaves, and the cooldown starts then.
    ticks(&mut h, &area, 29_990);
    assert!(matches!(pickup(&h), Pickup::Hovering(_)));
    ticks(&mut h, &area, 10);
    assert_eq!(pickup(&h), Pickup::Cooldown(30_000));
    assert_eq!(h.outcome(), None);
    let e = h.call_ship(&mut ev).err().unwrap_or_default();
    assert!(e.contains("30s"), "{e}");
    // In the zone after it left: no escape.
    enter_zone(&mut h);
    ticks(&mut h, &area, 29_990);
    assert!(matches!(pickup(&h), Pickup::Cooldown(_)));
    assert_eq!(h.outcome(), None);
    ticks(&mut h, &area, 10);
    assert_eq!(pickup(&h), Pickup::Idle);
    assert!(
        !h.extraction_flies_ship(),
        "the ship is the trophies' again"
    );
    assert_eq!(h.call_ship(&mut ev), Ok(()));
}

#[test]
fn being_in_the_zone_at_any_moment_of_the_hover_is_an_escape() {
    for at_ms in [0, 15_000, 29_980] {
        let (area, mut h) = hunt_with(&extraction());
        h.room.score = 40;
        h.successes = 2;
        let mut ev = Vec::new();
        assert_eq!(h.call_ship(&mut ev), Ok(()));
        ticks(&mut h, &area, 5000 + at_ms);
        assert_eq!(h.outcome(), None, "{at_ms}");
        enter_zone(&mut h);
        let ev = ticks(&mut h, &area, 10);
        assert_eq!(h.outcome(), Some(Outcome::Exited), "{at_ms}");
        assert_eq!((h.room.score, h.room.total.success), (40, 2), "kept");
        assert!(ev.contains(&GameEvent::Message("Extracted".into())));
    }
}

#[test]
fn waiting_in_the_zone_escapes_when_the_hover_starts_not_before() {
    let (area, mut h) = hunt_with(&extraction());
    enter_zone(&mut h);
    let mut ev = Vec::new();
    assert_eq!(h.call_ship(&mut ev), Ok(()));
    ticks(&mut h, &area, 5000);
    assert_eq!(h.outcome(), None, "still arriving");
    ticks(&mut h, &area, 10);
    assert_eq!(h.outcome(), Some(Outcome::Exited));
}

#[test]
fn the_ship_does_not_come_while_it_collects_a_trophy() {
    let (area, mut h) = hunt_with(&extraction());
    h.ship.tasks.push_back(0);
    let mut ev = Vec::new();
    assert_eq!(h.call_ship(&mut ev), Err("The ship is busy".into()));
    h.ship.tasks.clear();
    assert_eq!(h.call_ship(&mut ev), Ok(()));
    let _ = area;
}

#[test]
fn leaving_by_the_prompt_forfeits_only_with_extraction() {
    for (rules, want) in [
        (extraction(), Outcome::Died),
        (only(|r| r.knife = true), Outcome::Exited),
    ] {
        let (area, mut h) = hunt_with(&rules);
        h.room.score = 40;
        h.press_escape();
        h.confirm_exit();
        ticks(&mut h, &area, 4000);
        assert_eq!(h.outcome(), Some(want));
    }
}

#[test]
fn equipment_rules_take_the_radar_scent_and_extra_firearms() {
    let (area, mut h, _) = setup(false);
    let mut w = crate::game::Weapons::new(
        vec![super::support::rifle(); 3],
        vec![crate::game::WeaponAnims::uniform(100); 3],
        c2_formats::bits::Bits::from(0b111u128),
        false,
    );
    w.current = 2;
    h.weapons = w;
    h.opts.radar = true;
    h.opts.scent = true;
    h.opts.camo = true;
    h.start_hardcore(
        &area,
        &only(|r| {
            r.no_radar = true;
            r.no_cover_scent = true;
            r.max_firearms = Some(2);
        }),
    );
    assert!(!h.opts.radar && !h.opts.scent && h.opts.camo);
    let carried: Vec<bool> = (0..3).map(|i| h.weapons.carried(i)).collect();
    assert_eq!(carried, [true, true, false]);
    assert_eq!(h.weapons.rounds(2), 0);
    assert_eq!(h.weapons.current, 0, "the weapon in hand was left behind");
    assert!(h.deploying().is_none(), "no extraction: no zones to pick");
}

#[test]
fn drops_come_from_the_seed_away_from_the_drop_off_and_add_rounds() {
    let rules = only(|r| r.ammo_drops = 3);
    let (area, mut h) = hunt_with(&rules);
    let (_, h2) = hunt_with(&rules);
    let places = |h: &Hunt| h.ammo_drops().map(|d| d.pos).collect::<Vec<_>>();
    assert_eq!(places(&h).len(), 3);
    assert_eq!(places(&h), places(&h2), "the same seed: the same places");
    let (px, pz) = (h.player.x, h.player.z);
    for p in places(&h) {
        let d = ((p[0] - px).powi(2) + (p[2] - pz).powi(2)).sqrt() / CELL;
        assert!((23.0..=200.0).contains(&d), "{d}");
    }
    let before = h.weapons.rounds(0);
    let p = places(&h)[0];
    (h.player.x, h.player.z) = (p[0] + 50.0, p[2]);
    let ev = ticks(&mut h, &area, 10);
    assert_eq!(h.weapons.rounds(0), before + 14, "one more load");
    assert_eq!(h.ammo_drops().count(), 2);
    assert!(ev.contains(&GameEvent::Message("Ammunition collected".into())));
    ticks(&mut h, &area, 100);
    assert_eq!(h.weapons.rounds(0), before + 14, "taken once");
}

/// The fixture's Stegosaurus 200 units in front of the hunter.
fn knife_hunt() -> (Area, Hunt) {
    let (area, mut h) = hunt_with(&only(|r| {
        r.knife = true;
        r.knife_dodge = true;
    }));
    h.herd.chars[0].pos = [h.player.x, h.player.y, h.player.z - 200.0];
    (area, h)
}

fn stab(h: &mut Hunt, area: &Area) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    let fire = HuntInput {
        fire: true,
        ..Default::default()
    };
    h.tick(area, &[], &fire, 10, &mut ev);
    for _ in 0..60 {
        h.tick(area, &[], &HuntInput::default(), 10, &mut ev);
    }
    ev
}

#[test]
fn the_knife_is_offered_only_when_every_firearm_is_empty() {
    let (area, mut h) = knife_hunt();
    assert!(!h.knife_ready() && h.knife().is_none());
    let health = h.herd.chars[0].health;
    stab(&mut h, &area);
    assert_eq!(
        h.herd.chars[0].health, health,
        "the rifle is in hand, not the knife"
    );
    h.weapons.shots_left[0] = 0;
    assert!(h.knife_ready());
    // Without the rule, an empty rifle is just empty.
    let (_, mut plain) = hunt_with(&only(|r| r.no_radar = true));
    plain.weapons.shots_left[0] = 0;
    assert!(!plain.knife_ready());
}

#[test]
fn a_knife_kill_scores_nothing_and_leaves_no_trophy() {
    let (area, mut h) = knife_hunt();
    h.weapons.shots_left[0] = 0;
    let health = h.herd.chars[0].health;
    let ev = stab(&mut h, &area);
    assert_eq!(h.herd.chars[0].health, health - 2);
    assert!(h.herd.chars[0].aware_hunter, "a wound alarms it");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Killed { .. })));
    let mut all = Vec::new();
    for _ in 0..3 {
        // The wounded animal runs: keep it at the knife.
        h.herd.chars[0].pos = [h.player.x, h.player.y, h.player.z - 200.0];
        all.extend(stab(&mut h, &area));
    }
    assert_eq!(h.herd.chars[0].health, 0);
    assert!(all.contains(&GameEvent::Killed {
        index: 0,
        trophy: false
    }));
    assert!(!all
        .iter()
        .any(|e| matches!(e, GameEvent::Trophy { .. } | GameEvent::Scored { .. })));
    assert_eq!((h.room.score, h.successes), (0, 0));
    assert!(h.kills.is_empty() && h.ship.tasks.is_empty());
    assert!(h.room.bodies.iter().all(|b| b.ctype == 0));
}

#[test]
fn the_knife_misses_what_is_behind_or_out_of_reach() {
    for dz in [200.0, -900.0] {
        let (area, mut h) = knife_hunt();
        h.weapons.shots_left[0] = 0;
        h.herd.chars[0].pos = [h.player.x, h.player.y, h.player.z + dz];
        let health = h.herd.chars[0].health;
        stab(&mut h, &area);
        assert_eq!(h.herd.chars[0].health, health, "{dz}");
    }
}

#[test]
fn the_dodge_steps_aside_spares_the_hunter_and_cools_down() {
    let (area, mut h) = knife_hunt();
    assert!(!h.dodge(1.0), "not with a loaded rifle");
    h.weapons.shots_left[0] = 0;
    h.herd.chars[0].pos = [900.0 * CELL, 0.0, 900.0 * CELL];
    let x = h.player.x;
    assert!(h.dodge(1.0));
    assert!(h.dodging() && !h.dodge(1.0));
    assert!(h.herd.hesitation.as_deref().is_some_and(|x| x.hunter_safe));
    ticks(&mut h, &area, 300);
    assert!(h.player.x - x > 400.0, "stepped right: {}", h.player.x - x);
    assert!(h.dodging());
    ticks(&mut h, &area, 300);
    assert!(!h.dodging());
    assert!(h.herd.hesitation.as_deref().is_some_and(|x| !x.hunter_safe));
    assert!(!h.dodge(-1.0), "cooling down");
    ticks(&mut h, &area, 2400);
    assert!(h.dodge(-1.0));
    // Without the dodge rule the knife has none.
    let (_, mut plain) = hunt_with(&only(|r| r.knife = true));
    plain.weapons.shots_left[0] = 0;
    assert!(!plain.dodge(1.0) && plain.herd.hesitation.is_none());
}

/// A hunting Velociraptor 12 cells ahead of the hunter; `face`: the hunter
/// keeps turning to look at it. The ms until the hunter died (within 30 s).
fn raptor_kills_after(rules: Option<&fmt::Hunt>, face: bool) -> Option<i32> {
    let (area, mut h, car) = setup(false);
    h.herd.chars[0].pos = [900.0 * CELL, 0.0, 900.0 * CELL];
    let mut sp = Species::new(info(ai::VELO), &car);
    sp.hunt_price = Some(100);
    h.herd.species.push(sp);
    let mut c = Character {
        ctype: 2,
        pos: [h.player.x, h.player.y, h.player.z - 12.0 * CELL],
        ..Default::default()
    };
    c.reset(&h.herd.species[2], &mut h.rng, 128);
    c.state = 1;
    c.afraid_time = 60_000;
    h.herd.chars.push(c);
    if let Some(r) = rules {
        h.start_hardcore(&area, r);
    }
    let mut ev = Vec::new();
    for t in 0..3000 {
        let c = &h.herd.chars[1];
        let (dx, dz) = (c.pos[0] - h.player.x, c.pos[2] - h.player.z);
        // Forward is (sin a, -cos a).
        let toward = dx.atan2(-dz);
        h.player.alpha = if face {
            toward
        } else {
            toward + crate::dino::PI
        };
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        if h.death.is_some() {
            return Some(t * 10);
        }
    }
    None
}

#[test]
fn a_faced_raptor_circles_for_its_time_then_attacks() {
    let plain = raptor_kills_after(None, true).unwrap_or(i32::MAX);
    assert!(plain < 6_000, "the original attack: {plain}");
    let rules = only(|r| r.hesitation_s = 10);
    let faced = raptor_kills_after(Some(&rules), true).unwrap_or(i32::MAX);
    assert!((10_000..20_000).contains(&faced), "{faced}");
}

#[test]
fn a_turned_back_brings_the_lunge() {
    let rules = only(|r| r.hesitation_s = 10);
    let t = raptor_kills_after(Some(&rules), false).unwrap_or(i32::MAX);
    assert!(t < 6_000, "{t}");
}

#[test]
fn an_ignoring_species_attacks_as_the_original() {
    let rules = only(|r| {
        r.hesitation_s = 10;
        r.predators = vec![("t".into(), Reaction::Ignore)];
    });
    let t = raptor_kills_after(Some(&rules), true).unwrap_or(i32::MAX);
    assert_eq!(Some(t), raptor_kills_after(None, true));
}

#[test]
fn unlisted_species_react_by_diet_and_size() {
    use crate::game::hardcore::default_reaction;
    let car = super::support::target_car(false);
    let of = |a: i32| default_reaction(&Species::new(info(a), &car));
    assert_eq!(of(ai::VELO), Reaction::CircleLunge);
    assert_eq!(of(ai::ALLO), Reaction::CircleLunge);
    assert_eq!(of(ai::TREX), Reaction::CircleEat);
    assert_eq!(of(ai::STEGO), Reaction::Ignore);
    assert_eq!(of(ai::CHASM), Reaction::Ignore);
    let mut big = Species::new(info(ai::SPINO), &car);
    big.size = 1200.0;
    assert_eq!(
        default_reaction(&big),
        Reaction::CircleEat,
        "too big to lunge"
    );
}
