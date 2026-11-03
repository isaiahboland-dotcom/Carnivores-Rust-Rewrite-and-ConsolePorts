use super::support::{setup, weapon_ticks};
use crate::game::{GameEvent, HuntInput};
use crate::trace::Hit;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[test]
fn vital_hit_kills_and_noise_alarms() {
    let (area, mut h, car) = setup(true);
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            toggle_weapon: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            fire: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    assert!(
        ev.iter().any(|e| matches!(
            e,
            GameEvent::Impact {
                hit: Hit::Character {
                    index: 0,
                    mortal: true
                },
                ..
            }
        )),
        "{ev:?}"
    );
    assert_eq!(h.herd.chars[0].health, 0);
    assert!(ev.contains(&GameEvent::Killed {
        index: 0,
        trophy: true
    }));
    assert_eq!(h.successes, 1);
    assert!(h
        .effects
        .bursts
        .iter()
        .any(|b| b.kind == crate::effects::Part::Blood && !b.parts.is_empty()));
}

#[test]
fn body_hit_wounds_and_alarms() {
    let (area, mut h, car) = setup(false);
    let cars = [Some(&car), Some(&car)];
    let mut ev = Vec::new();
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            toggle_weapon: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
    weapon_ticks(
        &mut h,
        &area,
        &cars,
        HuntInput {
            fire: true,
            ..Default::default()
        },
        1,
        &mut ev,
    );
    let c = &h.herd.chars[0];
    assert_eq!(c.health, 1, "5 - power 4");
    assert_eq!(c.state, 2, "alarmed");
    assert!(c.afraid_time > 0 && c.blood_ttime > 0);
    assert!(h
        .effects
        .bursts
        .iter()
        .any(|b| b.kind == crate::effects::Part::Blood && !b.parts.is_empty()));
}
