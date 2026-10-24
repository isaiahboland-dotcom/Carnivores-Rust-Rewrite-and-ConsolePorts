//! A set of small numbers (bits 0..512): picked menu slots and hunt targets.
//! The original packs these into a 32-bit word (AI numbers 10..); combined
//! games list far more animals than a `u128` holds once slot + 10 is taken.

use core::ops::{BitAnd, BitOr, BitOrAssign};

/// Bits `0..Bits::LEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bits([u64; 8]);

impl Bits {
    pub const LEN: usize = 512;
    pub const NONE: Bits = Bits([0; 8]);
    pub const ALL: Bits = Bits([u64::MAX; 8]);

    /// Just bit `i` (none past [`Bits::LEN`]).
    pub fn bit(i: usize) -> Self {
        let mut b = Self::NONE;
        b.set(i);
        b
    }

    pub fn has(&self, i: usize) -> bool {
        i < Self::LEN && self.0[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn set(&mut self, i: usize) {
        if i < Self::LEN {
            self.0[i / 64] |= 1 << (i % 64);
        }
    }

    pub fn clear(&mut self, i: usize) {
        if i < Self::LEN {
            self.0[i / 64] &= !(1 << (i % 64));
        }
    }

    pub fn toggle(&mut self, i: usize) {
        if i < Self::LEN {
            self.0[i / 64] ^= 1 << (i % 64);
        }
    }

    /// How many bits are set.
    pub fn count(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// The bits set, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..Self::LEN).filter(|&i| self.has(i))
    }

    /// Bit `i` becomes bit `i - n` (lower ones dropped).
    pub fn down(&self, n: usize) -> Self {
        let mut b = Self::NONE;
        for i in self.iter().filter(|&i| i >= n) {
            b.set(i - n);
        }
        b
    }

    /// Bit `i` becomes bit `i + n` (those past the end dropped).
    pub fn up(&self, n: usize) -> Self {
        let mut b = Self::NONE;
        for i in self.iter() {
            b.set(i + n);
        }
        b
    }

    /// Bits `0..n`.
    pub fn first(n: usize) -> Self {
        let mut b = Self::NONE;
        for i in 0..n.min(Self::LEN) {
            b.set(i);
        }
        b
    }
}

impl From<u128> for Bits {
    fn from(v: u128) -> Self {
        Bits([v as u64, (v >> 64) as u64, 0, 0, 0, 0, 0, 0])
    }
}

impl BitOr for Bits {
    type Output = Bits;
    fn bitor(mut self, o: Bits) -> Bits {
        for (a, b) in self.0.iter_mut().zip(o.0) {
            *a |= b;
        }
        self
    }
}

impl BitOrAssign for Bits {
    fn bitor_assign(&mut self, o: Bits) {
        *self = *self | o;
    }
}

impl BitAnd for Bits {
    type Output = Bits;
    fn bitand(mut self, o: Bits) -> Bits {
        for (a, b) in self.0.iter_mut().zip(o.0) {
            *a &= b;
        }
        self
    }
}

impl FromIterator<usize> for Bits {
    fn from_iter<I: IntoIterator<Item = usize>>(it: I) -> Self {
        let mut b = Self::NONE;
        for i in it {
            b.set(i);
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_past_128() {
        let mut b = Bits::bit(10) | Bits::bit(300);
        assert!(b.has(10) && b.has(300) && !b.has(11));
        assert_eq!(b.down(10).iter().collect::<Vec<_>>(), [0, 290]);
        assert_eq!(b.up(5).iter().collect::<Vec<_>>(), [15, 305]);
        b.clear(10);
        assert_eq!(b, Bits::bit(300));
        assert!(!Bits::bit(600).has(600));
        assert_eq!(Bits::from(1u128 << 100), Bits::bit(100));
        assert_eq!(Bits::first(3).iter().count(), 3);
    }
}
