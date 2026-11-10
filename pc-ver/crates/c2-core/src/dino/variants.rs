//! Resolved per-map variants. Authoring and asset loading live outside core.
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::Herd;
use crate::rng::MsRand;

/// Preserve the saved model identity across pack reorderings. Missing members
/// stay recorded with ctype -1, and are shown again if their identity returns.
/// Legacy trophies (token zero) retain their existing index interpretation.
pub fn remap_trophies(species: &[super::Species], room: &mut c2_formats::sav::Save) {
    for trophy in &mut room.bodies {
        let token = trophy.variant_token();
        if token == 0 {
            continue;
        }
        trophy.ctype = species
            .iter()
            .position(|sp| sp.variant.as_ref().is_some_and(|v| v.token == token))
            .map_or(-1, |i| i as i32);
    }
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub owner: usize,
    /// Stable group/member/model identity stored in trophy spare fields.
    pub token: u64,
    /// Only the owner holds the normalized, eligible candidates.
    pub choices: Vec<(usize, f64)>,
}

impl Herd {
    /// Every spawn path uses this selector. Ordinary species consume no RNG.
    pub(crate) fn select_variant(&self, ct: usize, rng: &mut MsRand) -> Option<usize> {
        let sp = self.species.get(ct)?;
        let Some(member) = &sp.variant else {
            return Some(ct);
        };
        let group = self.species.get(member.owner)?.variant.as_ref()?;
        let positive = || group.choices.iter().filter(|(_, p)| *p > 0.0);
        let mut candidates = positive();
        let first = candidates.next()?;
        if candidates.next().is_none() {
            return Some(first.0);
        }
        // Combine two MSVC draws into a half-open 30-bit uniform sample.
        let draw = ((rng.rand() as u32) << 15) | rng.rand() as u32;
        let mut pick = f64::from(draw) / 1073741824.0;
        let mut last = first.0;
        for &(index, probability) in positive() {
            last = index;
            if pick < probability {
                return Some(index);
            }
            pick -= probability;
        }
        Some(last) // rounding at the cumulative upper boundary
    }

    pub(crate) fn variant_owner_enabled(&self, ct: usize) -> bool {
        self.species.get(ct).is_some_and(|sp| {
            sp.variant
                .as_ref()
                .is_none_or(|v| v.owner == ct && v.choices.iter().any(|(_, p)| *p > 0.0))
        })
    }
}
