//! Mode configuration: which survival, its priced roster and its arena.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

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

/// The survival arena: the hunter's spot and the box the waves come from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arena {
    /// Where they come: centre `x`, `z`, half sizes (map units).
    pub spawn: [f32; 4],
    pub hunter: [f32; 2],
    /// Which way the hunter faces at his spot (rad).
    pub hunter_alpha: f32,
    /// Before the first wave, and between waves (ms).
    pub first_ms: i32,
    pub rest_ms: i32,
}

impl Arena {
    /// Mount Ravan's mobile survival ravine, in the original PC map units.
    /// Placement is independent of the wave roster and needs no APK at runtime.
    pub fn ravine() -> Self {
        let hunter = [89330.13, 65388.617];
        let spawn = [84069.125, 45230.77, 3000.0, 2000.0];
        Self {
            hunter,
            spawn,
            hunter_alpha: libm::atan2f(spawn[0] - hunter[0], hunter[1] - spawn[1]),
            first_ms: 500,
            rest_ms: 2000,
        }
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::ravine()
    }
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
