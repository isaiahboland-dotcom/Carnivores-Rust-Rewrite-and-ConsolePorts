use super::support::setup;
use crate::area::Area;
use crate::dino::ai;
use crate::game::{GameEvent, HuntInput};
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, format, vec, vec::Vec};
use c2_formats::bits::Bits;

#[test]
fn explicit_menu_identity_overrides_authored_roaming_radar() {
    let (_, mut h, _) = setup(false);
    let c = h.herd.chars[0].clone();
    h.opts.targets = Bits::bit(30);
    h.herd.species[c.ctype].tuning.radar = Some(true);
    h.herd.species[c.ctype].tuning.menu_slot = Some(-1);
    assert!(!h.is_target(&c));
    h.herd.species[c.ctype].tuning.menu_slot = Some(21);
    assert!(!h.is_target(&c));
    h.herd.species[c.ctype].tuning.menu_slot = Some(20);
    assert!(h.is_target(&c));
}

#[test]
fn original_ai_in_a_mixed_pack_uses_its_menu_slot() {
    let (_, mut hunt, _) = setup(false);
    let original_ai = hunt.herd.species[1].info.ai;
    let res = c2_formats::res_txt::ResTxt::parse(&format!(
        "characters {{\n{{\n ai = {original_ai}\n nativeAI = TRUE\n menuSlot = 71\n}}\n}}\n"
    ))
    .expect("script");
    let tables = c2_formats::mee::Tables::read(
        &res,
        &c2_formats::mee::Hunt {
            area: Some(1),
            time: 1,
            picked: Bits::bit(71),
        },
    );
    crate::mee::tune(&mut hunt.herd.species[1..2], &tables);
    assert!(
        hunt.herd.species[1].mee.is_none(),
        "retains stock animation layout"
    );
    assert_eq!(hunt.herd.species[1].info.ai, original_ai);
    assert_eq!(hunt.call_bit(&hunt.herd.chars[0]), Some(81));
}

#[test]
fn no_blood_leaves_no_trail() {
    let (area, mut hunt, _) = setup(false);
    hunt.opts.blood = false;
    hunt.herd.chars[0].health = 3;
    hunt.herd.chars[0].blood_ttime = 90_000;
    let mut out = Vec::new();
    for _ in 0..500 {
        hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
    }
    assert!(hunt.effects.blood.is_empty());
    hunt.opts.blood = true;
    for _ in 0..500 {
        hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
    }
    assert!(!hunt.effects.blood.is_empty());
}

#[test]
fn animals_keep_to_where_the_hunter_can_go() {
    let area = Area::flat(10);
    let at = |cell: f32| {
        let mut p = [cell * 256.0 + 128.0, 0.0, 500.0 * 256.0];
        crate::dino::check_place_collision_p(&area, &mut p)
    };
    assert!(!at(500.0) && !at(40.0));
    assert!(at(20.0) && at(990.0), "outside the hunter's 36..980");
}

#[test]
fn friendly_and_stealth_shorten_noticing() {
    let normal = notice_range(1.0, false, false);
    assert!(normal > 0.0);
    let friendly = notice_range(0.6, false, false);
    assert!(
        friendly < normal && friendly > 0.0,
        "{friendly} vs {normal}"
    );
    // Crouched, stealth halves how visible the hunter is (sight; smell
    // still finds him).
    let crouched = notice_range_by(1.0, false, true, 0.0);
    assert!(crouched > 0.0);
    assert!(notice_range_by(1.0, true, true, 0.0) < crouched);
    // Standing, stealth changes nothing.
    assert_eq!(
        notice_range_by(1.0, true, false, 0.0),
        notice_range_by(1.0, false, false, 0.0)
    );
}

#[test]
fn dialect_calls_go_by_menu_slot() {
    let (area, mut hunt, car) = setup(false);
    let cars = [None, Some(&car)];
    let mut m = crate::dino::mee::MeeInfo::new(ai::STEGO);
    (m.call_no, m.fear_calls) = (3, vec![5]);
    hunt.herd.species[1].mee = Some(Box::new(m));
    // Can't see or smell the hunter: only calls move it.
    hunt.herd.species[1].info.look_k = 0.0;
    hunt.herd.species[1].info.smell_k = 0.0;
    hunt.opts.targets = Bits::bit(13) | Bits::bit(15);
    let call = HuntInput {
        call: true,
        ..Default::default()
    };
    let mut out = Vec::new();
    // Slot 3's call: it comes over (heads near the hunter).
    hunt.target_call = 13;
    hunt.tick(&area, &cars, &call, 10, &mut out);
    let c = &hunt.herd.chars[0];
    assert!((c.tgx - hunt.player.x).abs() <= 1800.0 && (c.tgz - hunt.player.z).abs() <= 1800.0);
    assert_eq!(c.state, 0);
    assert!(out
        .iter()
        .any(|e| matches!(e, GameEvent::Call { call: 13, .. })));
    // Slot 5's call: it fears that one.
    hunt.call_lock = 0;
    hunt.target_call = 15;
    hunt.tick(&area, &cars, &call, 10, &mut out);
    assert_ne!(hunt.herd.chars[0].state, 0);
}

#[test]
fn dialect_ambients_without_a_call_are_not_the_first_menu_animal() {
    let (_, mut hunt, _) = setup(false);
    hunt.opts.targets = Bits::bit(10);
    let c = hunt.herd.chars[0].clone();
    // `callNo` left out: 0, as the engine reads it.
    hunt.herd.species[1].mee = Some(Box::new(crate::dino::mee::MeeInfo::new(ai::STEGO)));
    assert!(hunt.is_target(&c));
    hunt.herd.species[1].mee = Some(Box::new(crate::dino::mee::MeeInfo::new(2)));
    assert_eq!(hunt.call_bit(&c), None);
    assert!(!hunt.is_target(&c));
    // Unless its data puts it on the radar.
    hunt.herd.species[1].tuning.radar = Some(true);
    assert!(hunt.is_target(&c));
}

/// The farthest an animal ahead notices the hunter, in steps of 500.
fn notice_range(notice_k: f32, stealth: bool, crouched: bool) -> f32 {
    notice_range_by(notice_k, stealth, crouched, 1.0)
}

/// [`notice_range`] for an animal with `smell` (0: sight only).
fn notice_range_by(notice_k: f32, stealth: bool, crouched: bool, smell: f32) -> f32 {
    let mut last = 0.0;
    for k in 1..80 {
        let d = k as f32 * 500.0;
        let (area, mut hunt, _) = setup(false);
        (hunt.opts.notice_k, hunt.opts.stealth) = (notice_k, stealth);
        if crouched {
            hunt.player.head_y = 110.0;
        }
        hunt.wind.speed = 0.0;
        hunt.herd.species[1].info.smell_k = smell.max(1e-4);
        let c = &mut hunt.herd.chars[0];
        c.pos = [hunt.player.x, c.pos[1], hunt.player.z - d];
        (c.lookx, c.lookz) = (0.0, 1.0);
        hunt.check_afraid(&area);
        if hunt.herd.chars[0].afraid_time != 0 {
            last = d;
        }
    }
    last
}
