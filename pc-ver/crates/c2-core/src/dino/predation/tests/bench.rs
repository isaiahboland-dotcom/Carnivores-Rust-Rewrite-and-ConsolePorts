//! The bench helpers (`bench.rs`) and the one-pass scan (`scan.rs`).

use super::{fights, herd, hunter, ALLO, RAPTOR, STEGO};
use crate::area::Area;
use crate::dino::predation::bench;
use crate::dino::predation::scan::{Scan, Wants};
use crate::rng::MsRand;

const MID: f32 = 512.0 * 256.0;

#[test]
fn fill_grows_with_the_placed_mix_and_trims() {
    let area = Area::flat(10);
    let mut h = herd(&fights(), &area, &[(RAPTOR, 0.0), (STEGO, 2000.0)]);
    let mut rng = MsRand::new(3);
    // The hunter far from the centre, so the 40-cell rule leaves room.
    let at = (MID + 200.0 * 256.0, MID);
    bench::fill(&mut h, &area, at, &mut rng, 30);
    assert_eq!(bench::population(&h), 30);
    assert!(h.chars.iter().all(|c| c.ctype == RAPTOR || c.ctype == STEGO));
    let raptors = h.chars.iter().filter(|c| c.ctype == RAPTOR).count();
    assert!((14..=16).contains(&raptors), "{raptors} raptors of 30");
    for c in &h.chars[2..] {
        let d = (c.pos[0] - at.0).abs() + (c.pos[2] - at.1).abs();
        assert!(d >= 40.0 * 256.0, "placed {d} from the hunter");
    }
    bench::fill(&mut h, &area, at, &mut rng, 5);
    assert_eq!(bench::population(&h), 5);
}

#[test]
fn gather_brings_the_living_close() {
    let area = Area::flat(10);
    let mut h = herd(&fights(), &area, &[(RAPTOR, 0.0), (STEGO, 2000.0), (ALLO, 4000.0)]);
    h.chars[2].health = 0;
    let dead_at = h.chars[2].pos;
    let at = (MID + 100.0 * 256.0, MID + 100.0 * 256.0);
    bench::gather(&mut h, &area, at, &mut MsRand::new(4), 8.0);
    for c in &h.chars[..2] {
        assert!((c.pos[0] - at.0).abs() <= 8.0 * 256.0 + 1.0);
        assert!((c.pos[2] - at.1).abs() <= 8.0 * 256.0 + 1.0);
        assert_eq!([c.tgx, c.tgz], [c.pos[0], c.pos[2]]);
    }
    assert_eq!(h.chars[2].pos, dead_at, "a body stays");
}

#[test]
fn measure_counts_steps_and_thinks() {
    let area = Area::flat(10);
    let mut h = herd(&fights(), &area, &[(RAPTOR, 0.0), (STEGO, 1500.0)]);
    let mut t = 0u64;
    let mut clock = || {
        t += 7;
        t
    };
    let s = bench::measure(&mut h, &area, hunter(&area), &mut MsRand::new(5), 3, &mut clock);
    assert_eq!((s.animals, s.steps, s.thinks), (2, 150, 10));
    assert_eq!((s.step_mean_us, s.step_worst_us), (7, 7));
    assert_eq!((s.think_mean_us, s.think_worst_us), (7, 7));
    h.predation = None;
    let s = bench::measure(&mut h, &area, hunter(&area), &mut MsRand::new(5), 1, &mut clock);
    assert_eq!((s.thinks, s.think_worst_us), (0, 0));
}

#[test]
fn the_scan_finds_the_nearer_prey_and_a_tie_keeps_the_first() {
    let area = Area::flat(10);
    let wants = Wants {
        charged: false,
        prey: true,
        body: false,
    };
    for (a, b, want) in [(1800.0, 1200.0, 2), (1200.0, 1800.0, 1), (1500.0, 1500.0, 1)] {
        let h = herd(&fights(), &area, &[(RAPTOR, 0.0), (STEGO, a), (STEGO, -b)]);
        let rules = &h.predation.as_ref().unwrap().rules;
        let s = Scan::new(0, &h.chars, &h.species, rules, 74, wants);
        assert_eq!(s.prey(), Some(want), "stegos at {a} and -{b}");
        assert_eq!(s.predator(), None);
    }
}
