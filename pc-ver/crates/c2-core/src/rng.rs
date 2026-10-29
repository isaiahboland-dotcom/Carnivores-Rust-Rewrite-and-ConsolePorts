//! The MSVC C runtime `rand()` the original was built with, and the
//! game's helpers on top of it (`mathematics.cpp: rRand, siRand`).

pub const RAND_MAX: i32 = 0x7fff;

/// `rand()`: `seed = seed * 214013 + 2531011; return (seed >> 16) & 0x7fff`.
#[derive(Debug, Clone)]
pub struct MsRand {
    seed: u32,
}

impl MsRand {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub fn rand(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.seed >> 16) & 0x7fff) as i32
    }

    /// `rRand(r)`: 0..=r.
    pub fn r_rand(&mut self, r: i32) -> i32 {
        if r <= 0 {
            return 0;
        }
        self.rand() % (r + 1)
    }

    /// `siRand(R)`: -R..=R.
    pub fn si_rand(&mut self, r: i32) -> i32 {
        ((i64::from(self.rand()) * i64::from(r * 2 + 1)) / i64::from(RAND_MAX) - i64::from(r))
            as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_msvc_sequence() {
        // srand(1): the well-known first values of MSVC rand().
        let mut r = MsRand::new(1);
        assert_eq!(
            [r.rand(), r.rand(), r.rand(), r.rand()],
            [41, 18467, 6334, 26500]
        );
    }

    #[test]
    fn signed_and_unsigned_checkpoints() {
        let mut r = MsRand::new(7);
        assert_eq!([r.r_rand(5), r.r_rand(5), r.r_rand(5)], [1, 4, 5]);
        let mut r = MsRand::new(7);
        assert_eq!([r.si_rand(3), r.si_rand(3), r.si_rand(3)], [-3, 0, 0]);
    }

    #[test]
    fn ranges() {
        let mut r = MsRand::new(7);
        for _ in 0..1000 {
            assert!((0..=5).contains(&r.r_rand(5)));
            assert!((-3..=3).contains(&r.si_rand(3)));
        }
        assert_eq!(r.r_rand(0), 0);
    }
}
