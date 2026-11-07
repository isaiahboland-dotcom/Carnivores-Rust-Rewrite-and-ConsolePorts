//! An instance's spawn rules (`_RULES.TXT`, not in the original game; see
//! [`c2_formats::rules`]), applied once the area is populated: at most so
//! many of the picked animals on a map, and extra unpicked ones of a kind
//! placed at random.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::Hunt;
use crate::area::Area;
use crate::dino::{ai, check_place_collision_p, Character, Species};
use c2_formats::rules::Rules;

/// Tries per animal before giving up on a crowded or watery map.
const TRIES: u32 = 2048;

/// The species called `name` (`Raptor #2`: the second one so named).
pub fn species_named(species: &[Species], name: &str) -> Option<usize> {
    index_named(species.iter().map(|s| s.info.name.as_str()), name)
}

/// The entry of `names` called `name`, case ignored (`Raptor #2`: the
/// second one so named).
pub fn index_named<'a>(names: impl Iterator<Item = &'a str> + Clone, name: &str) -> Option<usize> {
    let name = name.trim();
    let (base, nth) = match name.rsplit_once(" #").map(|(a, k)| (a, k.parse::<usize>())) {
        Some((a, Ok(k))) if k >= 1 => (a.trim(), k),
        _ => (name, 1),
    };
    let named = |s: &str, n: &str| s.trim().eq_ignore_ascii_case(n);
    names
        .clone()
        .enumerate()
        .filter(|(_, s)| named(s, base))
        .nth(nth - 1)
        .map(|(i, _)| i)
        // A name that itself ends in " #2".
        .or_else(|| names.clone().position(|s| named(s, name)))
}

impl Hunt {
    /// Applies `rules` for the map named `stem` (`area3`, a `.c2map`'s):
    /// the picked animals capped, then the extra ones placed anywhere on
    /// the map at least 40 cells from the hunter. Returns how many extra
    /// animals were placed. Call right after placing the animals (before
    /// the first tick); survival and the trophy room leave it out.
    pub fn apply_spawn_rules(&mut self, area: &Area, rules: &Rules, stem: &str) -> usize {
        if let Some(max) = rules.max_huntable(stem) {
            let picked: Vec<bool> = (0..self.herd.species.len())
                .map(|ctype| {
                    let c = Character {
                        ctype,
                        ai: self.herd.species[ctype].runs_as(),
                        ..Default::default()
                    };
                    self.is_target(&c)
                })
                .collect();
            self.herd
                .cap_population(max as usize, |ct| picked.get(ct).copied().unwrap_or(false));
        }
        let mut placed = 0;
        let secrets: Vec<(usize, u32)> = rules
            .secrets_on(stem)
            .filter_map(|s| {
                let ct = species_named(&self.herd.species, &s.name);
                #[cfg(feature = "std")]
                if ct.is_none() {
                    tracing::warn!(animal = %s.name, "rules: no such animal in this game");
                }
                Some((ct?, s.count))
            })
            .collect();
        for (ctype, count) in secrets {
            placed += self.place_extra(area, ctype, count);
        }
        placed
    }

    /// Up to `count` animals of `ctype` at random spots: anywhere the
    /// original's main population may stand (`PlaceCharacters`), at least
    /// 40 cells from the hunter.
    fn place_extra(&mut self, area: &Area, ctype: usize, count: u32) -> usize {
        let (px, pz) = (self.player.x, self.player.z);
        let opt_agres = self.opts.opt_agres;
        let mut placed = 0;
        let mut tries = 0;
        while placed < count as usize && tries < TRIES * count {
            tries += 1;
            let mut p = [
                512.0 * 256.0 + (self.rng.si_rand(50 * 256) * 10) as f32,
                0.0,
                512.0 * 256.0 + (self.rng.si_rand(50 * 256) * 10) as f32,
            ];
            p[1] = area.land_height(p[0], p[2]);
            if (p[0] - px).abs() + (p[2] - pz).abs() < 256.0 * 40.0
                || check_place_collision_p(area, &mut p)
            {
                continue;
            }
            if !self.herd.spawn(ctype, p, &mut self.rng, opt_agres) {
                // No model (or an AI not implemented): none will spawn.
                break;
            }
            if let Some(c) = self.herd.chars.last_mut() {
                if matches!(c.ai, ai::DIMET | ai::PTERA) {
                    c.pos[1] += 2048.0;
                }
            }
            placed += 1;
        }
        placed
    }
}
