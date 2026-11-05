//! Roster eligibility for the rewritten survival modes.

use super::{Arena, SurvivalKind, SurvivalOptions};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{
    dino::{ai, mee, Species},
    game::Hunt,
};

/// Exact stock identity, including its root model path. Imported standalone
/// animals use namespaced paths and must not qualify merely by a similar name.
pub fn classic_identity(name: &str, file: &str) -> bool {
    let pairs = [
        ("parasaurolophus", "par2.car"),
        ("ankylosaurus", "ankylo1.car"),
        ("stegosaurus", "stego.car"),
        ("allosaurus", "allo.car"),
        ("chasmosaurus", "chasmosa.car"),
        ("velociraptor", "velo2.car"),
        ("spinosaurus", "spino.car"),
        ("ceratosaurus", "cerato1.car"),
        ("t-rex", "tirex.car"),
        ("brontoteriy", "bronto.car"),
        ("wild boar", "hog.car"),
        ("wolf", "wolf.car"),
        ("wooly rhinoceros", "rhino.car"),
        ("diatryma", "diatr.car"),
        ("giant deer", "deer.car"),
        ("smilodon", "smilo.car"),
        ("mammoth", "mamm.car"),
        ("bear", "bear.car"),
        ("yeti", "bigfoot.car"),
    ];
    pairs
        .iter()
        .any(|(n, f)| name.trim().eq_ignore_ascii_case(n) && file.trim().eq_ignore_ascii_case(f))
}

fn can_attack(sp: &Species) -> bool {
    if sp.ani_time.is_empty() {
        return false;
    }
    match &sp.mee {
        Some(m) => {
            super::dangerous(sp)
                && (mee::is_huntable(m.ai) || m.ai == ai::TREX)
                && m.kill_dist > 0
                && m.run_speed.is_finite()
                && m.run_speed > 0.0
        }
        None => matches!(
            sp.runs_as(),
            ai::ALLO | ai::VELO | ai::SPINO | ai::CERAT | ai::TREX | ai::CHASM
        ),
    }
}

/// Apply the same slot prices and roaming-animal donor prices as the menus.
/// Call after dialect tuning has resolved menu slots.
pub fn price_species(species: &mut [Species], menu: &c2_formats::menu_txt::MenuData) {
    for sp in species {
        let slot = sp
            .tuning
            .menu_slot
            .or_else(|| sp.mee.as_ref().map(|m| m.call_no))
            .or_else(|| sp.mee.is_none().then_some(sp.info.ai - 10));
        let entry = slot
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| menu.huntables.get(i))
            .or_else(|| {
                menu.huntables
                    .iter()
                    .find(|h| h.ai == sp.tuning.mee_ai.unwrap_or(sp.info.ai))
            });
        sp.hunt_price = entry
            .map(|h| h.price)
            .or_else(|| (sp.tuning.menu_slot == Some(-1)).then_some(0));
    }
}

impl SurvivalOptions {
    pub fn initial_budget(&self) -> u32 {
        if self.starting_budget != 0 {
            return self.starting_budget.max(10);
        }
        self.roster
            .iter()
            .map(|(_, c)| *c)
            .min()
            .unwrap_or(0)
            .saturating_mul(2)
            .max(10)
    }

    pub fn wave_budget(&self, round: u32) -> u32 {
        super::wave_budget(self.initial_budget(), round)
    }

    /// Whether the initial budget can buy at least one loaded animal.
    pub fn budget_ready(&self) -> bool {
        self.max_wave > 0 && self.roster.iter().any(|(_, c)| *c <= self.initial_budget())
    }

    /// The survival `kind` for a hunt: every loaded animal that can attack
    /// and has a menu price (Classic: the stock animals only), from Mount
    /// Ravan's ravine, with unlimited ammunition.
    pub fn pick(hunt: &Hunt, kind: SurvivalKind, max_wave: usize) -> Self {
        Self {
            kind,
            roster: roster(&hunt.herd.species, kind),
            max_wave,
            unlimited: true,
            arena: Arena::ravine(),
            starting_budget: 0,
        }
    }
}

/// The animals survival `kind` may send (character types) with their prices.
pub fn roster(species: &[Species], kind: SurvivalKind) -> Vec<(usize, u32)> {
    species
        .iter()
        .enumerate()
        .filter_map(|(i, sp)| {
            let cost = u32::try_from(sp.hunt_price?).ok()?;
            (can_attack(sp)
                && (kind != SurvivalKind::Classic
                    || classic_identity(&sp.info.name, &sp.info.file)))
            .then_some((i, cost))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::classic_identity;

    #[test]
    fn stock_identity_requires_exact_name_and_root_model() {
        assert!(classic_identity("Chasmosaurus", "CHASMOSA.CAR"));
        assert!(classic_identity("Wild Boar", "hog.car"));
        assert!(!classic_identity("Mod Chasmosaurus", "chasmosa.car"));
        assert!(!classic_identity("Chasmosaurus", "MODS/game/chasmosa.car"));
        assert!(!classic_identity("Chasmosaurus", "other.car"));
    }
}
