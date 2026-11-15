//! The DinoHunt Corp terminal ("DHC OS"): an optional in-universe front end
//! for the hunt setup. This is its shared model, the same on every
//! platform: what the pack offers ([`Catalog`]), what the recipe's rules
//! allow on each island ([`Rules`], from `_DHC.TXT`), and the terminal's
//! screens and actions ([`terminal::Terminal`]). No rendering, no I/O: each
//! platform draws the state and turns its input into [`terminal::Action`]s.
//!
//! Without `_DHC.TXT` the terminal offers what the original menu offers.

use alloc::string::String;
use alloc::vec::Vec;

pub use c2_formats::dhc::{Dhc, Discovery};

pub mod terminal;

#[cfg(test)]
mod tests;

/// The hunt setup's lists, in the order the original menu has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Area,
    Targets,
    Arms,
    Gear,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Area, Tab::Targets, Tab::Arms, Tab::Gear];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Area => "AREA",
            Tab::Targets => "TARGETS",
            Tab::Arms => "ARMS",
            Tab::Gear => "GEAR",
        }
    }
}

/// One thing the menu lists: its name and price.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub price: i32,
    /// Areas: the map's name (`AREA1`), which islands are keyed by.
    pub stem: String,
}

impl Item {
    pub fn new(name: &str, price: i32) -> Self {
        Item {
            name: name.into(),
            price,
            stem: String::new(),
        }
    }
}

/// What the pack offers, as the platform's own menu loaded it. Indices
/// into these lists are the menu's, so a loadout maps straight back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub areas: Vec<Item>,
    pub animals: Vec<Item>,
    pub weapons: Vec<Item>,
    pub equipment: Vec<Item>,
}

impl Catalog {
    pub fn list(&self, tab: Tab) -> &[Item] {
        match tab {
            Tab::Area => &self.areas,
            Tab::Targets => &self.animals,
            Tab::Arms => &self.weapons,
            Tab::Gear => &self.equipment,
        }
    }
}

/// Limits the platform's menu applies beside the recipe's rules (the
/// instance's times of day, `--max-picks`, the hardcore hunt ruleset), so
/// the terminal refuses what the original menu refuses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// Bit per time of day (0 dawn, 1 day, 2 night); 0: every time.
    pub times: u8,
    pub max_animals: Option<usize>,
    pub max_weapons: Option<usize>,
    /// Equipment the hunt leaves out (indices into the catalog's list).
    pub barred_equipment: Vec<usize>,
}

/// The rules in force: the recipe's `_DHC.TXT`, if any, over the catalog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    pub dhc: Option<Dhc>,
    pub limits: Limits,
}

impl Rules {
    /// Whether item `i` of `tab` may be picked on area `area`.
    pub fn allows(&self, cat: &Catalog, area: usize, tab: Tab, i: usize) -> bool {
        let Some(item) = cat.list(tab).get(i) else {
            return false;
        };
        if tab == Tab::Gear && self.limits.barred_equipment.contains(&i) {
            return false;
        }
        let island = self.island(cat, area);
        match (tab, island) {
            (Tab::Area, _) => true,
            (_, None) => true,
            (Tab::Targets, Some(isl)) => isl.allows_animal(&item.name),
            (Tab::Arms, Some(isl)) => isl.allows_weapon(&item.name),
            (Tab::Gear, Some(isl)) => isl.allows_equipment(&item.name),
        }
    }

    /// Whether time `t` (0 dawn, 1 day, 2 night) is offered on `area`.
    pub fn allows_time(&self, cat: &Catalog, area: usize, t: usize) -> bool {
        let inst = self.limits.times == 0 || (t < 8 && self.limits.times & (1 << t) != 0);
        inst && self.island(cat, area).is_none_or(|i| i.allows_time(t))
    }

    /// The island of area `area`, when the rules name one.
    pub fn island(&self, cat: &Catalog, area: usize) -> Option<&c2_formats::dhc::Island> {
        let a = cat.areas.get(area)?;
        let dhc = self.dhc.as_ref()?;
        dhc.island(&a.stem).or_else(|| dhc.island(&a.name))
    }
}

/// The original's kill-score multipliers for the accessories picked
/// (`Game.cpp`, `AddScore`: tranquilizer ×1.25, radar ×0.70, cover scent
/// ×0.80, camouflage ×0.85), matched by the equipment's name.
pub fn earnings_factor<'a>(equipment: impl IntoIterator<Item = &'a str>, tranq: bool) -> f32 {
    let mut k = if tranq { 1.25 } else { 1.0 };
    for name in equipment {
        let n = name.trim();
        for (key, f) in [("radar", 0.70), ("cover scent", 0.80), ("camouflage", 0.85)] {
            if n.eq_ignore_ascii_case(key) {
                k *= f;
            }
        }
    }
    k
}

/// What the terminal sends the hunt: indices into the [`Catalog`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loadout {
    pub area: usize,
    pub animals: Vec<usize>,
    pub weapons: Vec<usize>,
    pub equipment: Vec<usize>,
    /// 0 dawn, 1 day, 2 night.
    pub time: usize,
    /// Observer mode: watch the animals, unarmed and free.
    pub observer: bool,
}
