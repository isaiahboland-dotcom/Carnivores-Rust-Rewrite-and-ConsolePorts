//! Points-funded waves, independent of the hunter's saved score.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::rng::MsRand;

/// Starting points compounded by ten percent per round.
/// Round only the final result; bound both work and conversion at late rounds.
pub fn wave_budget(start: u32, round: u32) -> u32 {
    let mut points = start.max(10) as f64;
    for _ in 1..round.min(256) {
        points *= 1.1;
        if points >= u32::MAX as f64 {
            return u32::MAX;
        }
    }
    points as u32
}

/// Randomly buy affordable animals until none fit or the platform cap is hit.
/// Free animals still count toward the wave cap; unspent points do not carry over.
pub(super) fn buy_wave(
    roster: &[(usize, u32)],
    budget: u32,
    cap: usize,
    rng: &mut MsRand,
) -> Vec<usize> {
    let mut left = budget;
    let mut wave = Vec::new();
    while wave.len() < cap {
        let count = roster.iter().filter(|(_, cost)| *cost <= left).count();
        if count == 0 {
            break;
        }
        let pick = rng.r_rand(count.saturating_sub(1).min(i32::MAX as usize - 1) as i32) as usize;
        let Some(&(ctype, cost)) = roster.iter().filter(|(_, cost)| *cost <= left).nth(pick) else {
            break;
        };
        wave.push(ctype);
        left -= cost;
    }
    wave
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compounds_without_rounding_each_step_and_saturates() {
        assert_eq!(
            (wave_budget(30, 1), wave_budget(30, 2), wave_budget(30, 3)),
            (30, 33, 36)
        );
        assert_eq!((wave_budget(30, 7), wave_budget(30, 18)), (53, 151));
        assert_eq!(wave_budget(30, u32::MAX), u32::MAX);
    }

    #[test]
    fn spending_is_bounded_random_and_respects_cap() {
        let roster = [(1, 18), (2, 25), (3, 500), (4, 500)];
        let mut seen = [false; 2];
        for seed in 0..100 {
            let mut rng = MsRand::new(seed);
            let wave = buy_wave(&roster, 30, 6, &mut rng);
            assert_eq!(wave.len(), 1);
            seen[wave[0] - 1] = true;
            let wave = buy_wave(&roster, 53, 6, &mut rng);
            assert!(wave.iter().map(|&i| roster[i - 1].1).sum::<u32>() <= 53);
            assert!(wave.iter().all(|&i| i == 1 || i == 2));
            assert!(buy_wave(&roster, 100, 6, &mut rng).len() <= 6);
        }
        assert_eq!(seen, [true, true]);
        assert!(buy_wave(&[(0, 31)], 30, 6, &mut MsRand::new(1)).is_empty());
        assert!(buy_wave(&roster, 30, 0, &mut MsRand::new(1)).is_empty());
    }
}
