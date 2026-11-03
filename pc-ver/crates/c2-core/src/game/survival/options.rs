//! Mode configuration, species selection and data-driven wave rosters.

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::dino::{ai, Character};
use crate::game::Hunt;

/// A wave animal's health at most: the original's shotgun is 6 pellets of 1.
pub const WAVE_HEALTH: i32 = 3;

/// How a survival is set up.
#[derive(Debug, Clone, Default)]
pub struct SurvivalOptions {
    /// The animals of the waves (character types; hunters only).
    pub roster: Vec<usize>,
    /// Hardcore: 1-3 of these join each wave (empty: off).
    pub hardcore: Vec<usize>,
    /// The weapons never run out (every survival, the owner's call).
    pub unlimited: bool,
    /// The T-Rex (its type), from wave 15.
    pub trex: Option<usize>,
    /// At most this many animals in a wave (the 3DS: fewer).
    pub max_wave: usize,
    /// The mobile game's waves instead.
    pub mobile: Option<MobileWaves>,
}

/// The mobile game's Survive mode for this hunt's species.
#[derive(Debug, Clone, Default)]
pub struct MobileWaves {
    /// Character types of each wave in turn, then of every later one.
    pub waves: Vec<Vec<usize>>,
    pub after: Vec<usize>,
    /// Where they come: centre `x`, `z`, half sizes.
    pub spawn: [f32; 4],
    pub hunter: [f32; 2],
    pub first_ms: i32,
    pub rest_ms: i32,
    /// Ice Age stand-ins, away from Mount Ravan: the hunter keeps his start
    /// and the waves come round him as in our survival (no box).
    pub stand_ins: bool,
    /// Which way the hunter faces at his spot (rad; the dialect's
    /// `survivalPlayerA`), if set.
    pub hunter_alpha: Option<f32>,
}

impl MobileWaves {
    /// `s`'s animals found among `species` by name, or their stand-ins
    /// (`Survive::for_game`); `None` if one isn't.
    pub fn new(
        s: &c2_formats::mobile_arcade::Survive,
        species: &[crate::dino::Species],
    ) -> Option<Self> {
        let names: Vec<String> = species.iter().map(|sp| sp.info.name.clone()).collect();
        let s = &s.for_game(&names)?;
        let norm = |n: &str| -> String {
            n.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect()
        };
        let find = |n: &String| species.iter().position(|sp| norm(&sp.info.name) == norm(n));
        let all = |w: &Vec<String>| w.iter().map(find).collect::<Option<Vec<_>>>();
        Some(Self {
            waves: s.waves.iter().map(all).collect::<Option<_>>()?,
            after: all(&s.after)?,
            spawn: s.spawn,
            hunter: s.hunter,
            first_ms: s.first_ms,
            rest_ms: s.rest_ms,
            stand_ins: s.stand_ins,
            hunter_alpha: None,
        })
    }

    /// The animals of wave `n` (from 1).
    pub(super) fn wave(&self, n: u32) -> &[usize] {
        self.waves.get(n as usize - 1).unwrap_or(&self.after)
    }
}
/// Which survival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurvivalKind {
    #[default]
    Normal,
    /// 1-3 modded animals join each wave.
    Hardcore,
    /// Unlimited ammunition; the T-Rex from wave 15.
    Custom,
    /// The mobile game's Survive mode (from the player's APK).
    Mobile,
    /// The Modder's Edition's own (`-survival`), from the game's data.
    Mee,
}

impl SurvivalKind {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Hardcore, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Survival",
            Self::Hardcore => "Survival: hardcore",
            Self::Custom => "Survival: custom",
            Self::Mobile => "Classic survival",
            Self::Mee => "Modded survival",
        }
    }
}

/// The original game's hunting animals (by name): the others are a mod's.
const ORIGINAL: [&str; 6] = [
    "allosaurus",
    "velociraptor",
    "spinosaurus",
    "ceratosaurus",
    "t-rex",
    "tyrannosaurus",
];

/// Whether a species is the T-Rex.
fn trex(sp: &crate::dino::Species) -> bool {
    is_trex(&sp.info.name, sp.info.ai, sp.mee.is_some())
}

/// Whether an animal is the T-Rex: the original's AI, or by name.
pub fn is_trex(name: &str, ai_id: i32, dialect: bool) -> bool {
    let n = name.to_ascii_lowercase();
    (!dialect && ai_id == ai::TREX) || n.contains("t-rex") || n.contains("tyrannosaur")
}

/// Whether an animal is a mod's (not one of the original's hunters), by name.
pub fn modded(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !ORIGINAL.iter().any(|o| n.contains(o))
}

/// The first wave a T-Rex may come in.
pub const TREX_WAVE: u32 = 15;

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

impl SurvivalOptions {
    /// The waves for a hunt: the dangerous animals among the targets (the
    /// frontends make every loaded animal one), at random each wave, but the
    /// T-Rex (none: every dangerous animal but it); hardcore extras from the
    /// mods' dangerous animals, those not in the waves first (the game's own
    /// if there are none).
    pub fn pick(hunt: &Hunt, kind: SurvivalKind, max_wave: usize) -> Self {
        let sp = &hunt.herd.species;
        let hunting: Vec<usize> = (0..sp.len()).filter(|&i| dangerous(&sp[i])).collect();
        let target = |i: &usize| {
            hunt.is_target(&Character {
                ctype: *i,
                ai: sp[*i].info.ai,
                ..Default::default()
            })
        };
        let mut roster: Vec<usize> = hunting.iter().copied().filter(target).collect();
        if roster.is_empty() {
            roster = hunting.iter().copied().filter(|&i| !trex(&sp[i])).collect();
        }
        let hardcore = if kind == SurvivalKind::Hardcore {
            let mods = |i: &usize| modded(&sp[*i].info.name);
            let fresh: Vec<usize> = hunting
                .iter()
                .copied()
                .filter(|i| mods(i) && !roster.contains(i))
                .collect();
            let any: Vec<usize> = hunting.iter().copied().filter(mods).collect();
            [fresh, any, hunting.clone()]
                .into_iter()
                .find(|v| !v.is_empty())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let custom = kind == SurvivalKind::Custom;
        Self {
            roster,
            hardcore,
            unlimited: true,
            trex: hunting.iter().copied().find(|&i| custom && trex(&sp[i])),
            max_wave,
            mobile: None,
        }
    }
}

/// Waves the dialect's survival makes (`PlaceCharactersSurvival`) up to its
/// limit of 26: wave `n` has `2^n` to spend; the `k`-th animal of the roster
/// (0 first) costs `3^k`; the dearest that fits is bought first, as often as
/// it fits, then the next cheaper.
pub fn mee_waves(roster: &[usize]) -> Vec<Vec<usize>> {
    (1..=26u32)
        .map(|n| {
            let mut left = 1u64 << n;
            let mut wave = Vec::new();
            for (k, &ctype) in roster.iter().enumerate().rev() {
                let cost = 3u64.saturating_pow(k as u32);
                while left >= cost && wave.len() < 255 {
                    wave.push(ctype);
                    left -= cost;
                }
            }
            wave
        })
        .collect()
}

impl SurvivalOptions {
    /// The dialect's survival: its `roster` (character types in
    /// `survivalIndex` order) from the area's box, the hunter at its spot
    /// facing its way, unlimited ammunition; the next wave 2 s after one
    /// falls (`WaveNoteTime`). `None` without a roster.
    pub fn mee(
        roster: &[usize],
        spot: &c2_formats::mee::SurvivalSpot,
        max_wave: usize,
    ) -> Option<Self> {
        if roster.is_empty() {
            return None;
        }
        let cell = |c: i32| c as f32 * 256.0 + 128.0;
        let [x0, x1, y0, y1] = spot.spawn;
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let mut waves = mee_waves(roster);
        let after = waves.pop().unwrap_or_default();
        Some(Self {
            mobile: Some(MobileWaves {
                waves,
                after,
                spawn: [
                    (cell(x0) + cell(x1)) / 2.0,
                    (cell(y0) + cell(y1)) / 2.0,
                    (x1 - x0) as f32 * 128.0 + 128.0,
                    (y1 - y0) as f32 * 128.0 + 128.0,
                ],
                hunter: [cell(spot.hunter.0), cell(spot.hunter.1)],
                first_ms: 500,
                rest_ms: 2000,
                stand_ins: false,
                hunter_alpha: Some(spot.angle.to_radians()),
            }),
            unlimited: true,
            max_wave,
            ..Default::default()
        })
    }

    /// The mobile game's Survive mode (`s`) with this hunt's species, or
    /// `None` if one of its animals is missing.
    pub fn mobile(
        hunt: &Hunt,
        s: &c2_formats::mobile_arcade::Survive,
        max_wave: usize,
    ) -> Option<Self> {
        Some(Self {
            mobile: Some(MobileWaves::new(s, &hunt.herd.species)?),
            max_wave,
            unlimited: true,
            ..Default::default()
        })
    }
}
