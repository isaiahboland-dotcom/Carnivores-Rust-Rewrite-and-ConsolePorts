//! Mode configuration: which survival, its priced roster and its arena.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::arena::Arena;
use crate::dino::ai;

/// A wave animal's health at most: the original's shotgun is 6 pellets of 1.
pub const WAVE_HEALTH: i32 = 3;

/// How a survival is set up (the mobile game's Survive mode: Mount Ravan's
/// ravine, the shotgun, waves bought with points).
#[derive(Debug, Clone, Default)]
pub struct SurvivalOptions {
    pub kind: SurvivalKind,
    /// Eligible animals (character types) and their menu prices.
    pub roster: Vec<(usize, u32)>,
    /// Zero selects twice the cheapest eligible price (minimum ten).
    pub starting_budget: u32,
    /// The weapons never run out (every survival, the owner's call).
    pub unlimited: bool,
    /// At most this many animals in a wave (the 3DS: fewer).
    pub max_wave: usize,
    /// Where the hunter stands and the waves come from.
    pub arena: Arena,
}

/// Which survival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurvivalKind {
    /// Base-game animals, independent of APK data.
    #[default]
    Classic,
    /// All eligible installed animals.
    Modded,
    /// Modded's waves at night that never ends, with the radar and the
    /// flashlight (not in the original or the mobile game; a setting, off by
    /// default, unlocks it).
    Hardcore,
}

impl SurvivalKind {
    /// The modes always offered (Hardcore is unlocked by a setting).
    pub const ALL: [Self; 2] = [Self::Classic, Self::Modded];

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "Classic survival",
            Self::Modded => "Modded survival",
            Self::Hardcore => "Hardcore survival",
        }
    }

    /// Played at night with the dynamic darkness held there.
    pub fn endless_night(self) -> bool {
        self == Self::Hardcore
    }
}

/// The listed area (map stems, menu order, no trophy room) a survival is
/// held on: Mount Ravan (`AREA5`) in a game with the stock animals (the
/// original, or a pack built on it), else the last listed area, so any
/// standalone game gets a survival.
pub fn survival_area<S: AsRef<str>>(stems: &[S], stock: bool) -> Option<usize> {
    stock
        .then(|| {
            stems
                .iter()
                .position(|s| s.as_ref().trim().eq_ignore_ascii_case("AREA5"))
        })
        .flatten()
        .or_else(|| stems.len().checked_sub(1))
}

/// The weapon a survival hunts with, from the listed `(name, model file)`:
/// the shotgun (by its model, else its name), else the third listed, else
/// the last.
pub fn survival_weapon(weapons: &[(&str, &str)]) -> Option<usize> {
    fn model(f: &str) -> &str {
        f.rsplit(['/', '\\']).next().unwrap_or(f).trim()
    }
    weapons
        .iter()
        .position(|(_, f)| model(f).eq_ignore_ascii_case("shotgun.car"))
        .or_else(|| {
            weapons
                .iter()
                .position(|(n, _)| n.trim().eq_ignore_ascii_case("shotgun"))
        })
        .or_else(|| (weapons.len() > 2).then_some(2))
        .or_else(|| weapons.len().checked_sub(1))
}

/// Whether an animal is the T-Rex: the original's AI, or by name.
pub fn is_trex(name: &str, ai_id: i32, dialect: bool) -> bool {
    let n = name.to_ascii_lowercase();
    (!dialect && ai_id == ai::TREX) || n.contains("t-rex") || n.contains("tyrannosaur")
}

/// Whether a species is dangerous to the hunter (a survival animal): the
/// original's hunters and its Chasmosaurus (which charges); in a dialect
/// game, a carnivore AI or any huntable with an attack (`aggress`).
pub fn dangerous(sp: &crate::dino::Species) -> bool {
    dangerous_ai(sp.info.ai, sp.mee.as_ref().map(|m| m.aggress))
}

/// [`dangerous`] by AI and, in a dialect game, `aggress` (`None`: the
/// original's rules).
pub fn dangerous_ai(ai_id: i32, aggress: Option<i32>) -> bool {
    use crate::dino::mee;
    match aggress {
        Some(a) => {
            (mee::is_huntable(ai_id) || ai_id == ai::TREX)
                && (mee::ai_info(ai_id).carnivore || a > 0)
        }
        None => matches!(
            ai_id,
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX | ai::CHASM
        ),
    }
}
