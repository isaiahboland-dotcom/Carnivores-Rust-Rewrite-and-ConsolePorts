//! A recipe's `[settings.rules]` (the launcher's Advanced tab): the times
//! of day offered, settings fixed by the instance and spawn rules per area.
//! Written into the built game as `HUNTDAT/_RULES.TXT`
//! ([`c2_formats::rules`]), which every platform's menu and hunt read.
//! Nothing set: no file, and the game is as it was.
//!
//! Areas and animals are named as the balance editor lists them (area
//! titles, `_RES.TXT` names), so the rules survive add-ons being reordered;
//! names the built game doesn't have are skipped.
//!
//! ```toml
//! [settings.rules]
//! times = ["day"]                 # empty: dawn, day and night
//!
//! [settings.rules.locks]          # fixed, and hidden from the in-game menus
//! dynamic_time = false
//! secret_dinos = 4
//!
//! [[settings.rules.areas]]
//! area = "Vengar Peninsula"
//! max_huntable = 3                # at most 3 of the picked animals
//!
//! [[settings.rules.secrets]]
//! animal = "Tyrannosaurus Rex"
//! count = 2
//! areas = ["Vengar Peninsula"]    # empty: every area
//!
//! [settings.rules]
//! hidden = ["Tyrannosaurus Rex"]  # not in the hunt menu; still a secret
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::rules::{self as fmt, Lock};
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// `dawn`, `day`, `night`: the ones offered (empty: all three).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub times: Vec<String>,
    #[serde(skip_serializing_if = "Locks::is_empty")]
    pub locks: Locks,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<AreaRule>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretRule>,
    /// Animals kept off the hunt menu (they can still spawn as secrets).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
}

/// Settings fixed by the instance; `None`: the player's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Locks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced_observer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_ammo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile_radar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleport: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_huntable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_dinos: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survival_budget: Option<u32>,
}

/// One locked setting's value, as the file keeps it.
pub enum LockValue<'a> {
    Switch(&'a mut Option<bool>),
    Number(&'a mut Option<u32>),
}

impl Locks {
    pub fn is_empty(&self) -> bool {
        *self == Locks::default()
    }

    /// The field kept for `l`.
    pub fn field(&mut self, l: Lock) -> LockValue<'_> {
        match l {
            Lock::DynamicTime => LockValue::Switch(&mut self.dynamic_time),
            Lock::AdvancedObserver => LockValue::Switch(&mut self.advanced_observer),
            Lock::MultiAmmo => LockValue::Switch(&mut self.multi_ammo),
            Lock::MobileRadar => LockValue::Switch(&mut self.mobile_radar),
            Lock::Teleport => LockValue::Switch(&mut self.teleport),
            Lock::AllHuntable => LockValue::Switch(&mut self.all_huntable),
            Lock::SecretDinos => LockValue::Number(&mut self.secret_dinos),
            Lock::SurvivalBudget => LockValue::Number(&mut self.survival_budget),
        }
    }

    /// Every locked setting and its value.
    pub fn list(&self) -> Vec<(Lock, i32)> {
        let mut me = self.clone();
        Lock::ALL
            .into_iter()
            .filter_map(|l| match me.field(l) {
                LockValue::Switch(v) => v.map(|b| (l, i32::from(b))),
                LockValue::Number(v) => v.map(|n| (l, n.min(i32::MAX as u32) as i32)),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AreaRule {
    pub area: String,
    /// At most this many of the picked animals spawn there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_huntable: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretRule {
    pub animal: String,
    pub count: u32,
    /// Area names; empty: every area.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<String>,
}

impl Rules {
    pub fn is_empty(&self) -> bool {
        self.to_format(&[]).is_empty() && self.areas.is_empty() && self.secrets.is_empty()
    }

    /// The file's form; `maps` gives each area name's map (names it lacks
    /// are skipped).
    pub fn to_format(&self, maps: &[(String, String)]) -> fmt::Rules {
        let map = |name: &str| {
            maps.iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
                .map(|(_, m)| m.clone())
        };
        let mut times = 0u8;
        for t in &self.times {
            if let Some(i) = fmt::TIMES.iter().position(|x| x.eq_ignore_ascii_case(t)) {
                times |= 1 << i;
            }
        }
        fmt::Rules {
            times: if times == 7 { 0 } else { times },
            locks: self.locks.list(),
            huntable: self
                .areas
                .iter()
                .filter_map(|a| Some((map(&a.area)?, a.max_huntable?)))
                .collect(),
            secrets: self
                .secrets
                .iter()
                .filter(|s| s.count > 0 && !s.animal.trim().is_empty())
                .filter_map(|s| {
                    let areas: Vec<String> = s.areas.iter().filter_map(|a| map(a)).collect();
                    // Every area it named is gone: it spawns nowhere.
                    (s.areas.is_empty() || !areas.is_empty()).then(|| fmt::Secret {
                        name: s.animal.trim().into(),
                        count: s.count,
                        areas,
                    })
                })
                .collect(),
            hidden: self
                .hidden
                .iter()
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty())
                .collect(),
        }
    }

    /// The layer to put over the built game in `vfs`; `None` when the
    /// rules change nothing there.
    pub fn layer(&self, vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
        let maps = if self.areas.is_empty() && self.secrets.is_empty() {
            Vec::new()
        } else {
            crate::balance::area_maps(vfs)?
        };
        let f = self.to_format(&maps);
        if f.is_empty() {
            return Ok(None);
        }
        let mut m = MemBackend::new();
        m.insert(fmt::PATH, f.to_text().into_bytes());
        Ok(Some(m))
    }

    /// Puts the layer over `vfs` (nothing when it changes nothing). True
    /// when it did.
    pub fn push(&self, vfs: &mut Vfs) -> Result<bool, ModError> {
        Ok(match self.layer(vfs)? {
            Some(l) => {
                vfs.push("rules", Box::new(l));
                true
            }
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_section_becomes_the_rules_file() {
        let r: Rules = toml::from_str(
            r#"
            times = ["night", "dawn"]
            [locks]
            dynamic_time = false
            secret_dinos = 4
            [[areas]]
            area = "Ravan"
            max_huntable = 3
            [[areas]]
            area = "Gone"
            max_huntable = 1
            [[secrets]]
            animal = "Rex"
            count = 2
            areas = ["Ravan"]
            [[secrets]]
            animal = "Raptor"
            count = 1
            [[secrets]]
            animal = "Lost"
            count = 1
            areas = ["Gone"]
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let maps = [("Ravan".to_string(), "area2".to_string())];
        let f = r.to_format(&maps);
        assert_eq!(f.times, 0b101);
        assert_eq!(f.locks, [(Lock::DynamicTime, 0), (Lock::SecretDinos, 4)]);
        assert_eq!(f.huntable, [("area2".to_string(), 3)]);
        assert_eq!(f.secrets.len(), 2, "a secret only on gone areas is dropped");
        assert_eq!(f.secrets[0].areas, ["area2"]);
        assert!(f.secrets[1].areas.is_empty());
        assert_eq!(
            toml::from_str::<Rules>(&toml::to_string(&r).unwrap_or_default()).ok(),
            Some(r)
        );
    }

    #[test]
    fn area_titles_become_map_names_in_the_built_game() {
        let mut m = MemBackend::new();
        m.insert(
            "HUNTDAT/_RES.TXT",
            b"prices {\n area = 10\n area = 20\n}\n.\n".to_vec(),
        );
        m.insert("HUNTDAT/MENU/TXT/AREA2.TXT", b"Fern Valley\r\n".to_vec());
        let mut vfs = Vfs::new();
        vfs.push("game", Box::new(m));
        let r = Rules {
            areas: vec![AreaRule {
                area: "fern valley".into(),
                max_huntable: Some(2),
            }],
            hidden: vec!["Rex".into()],
            ..Rules::default()
        };
        assert!(r.push(&mut vfs).unwrap_or(false));
        let f = fmt::Rules::parse(&vfs.read(fmt::PATH).unwrap_or_default());
        assert_eq!(f.max_huntable("AREA2"), Some(2));
        assert_eq!(f.hidden, ["Rex"]);
    }

    #[test]
    fn nothing_set_writes_nothing() {
        let r = Rules {
            times: vec!["dawn".into(), "day".into(), "night".into()],
            ..Rules::default()
        };
        assert!(r.is_empty());
        let vfs = Vfs::new();
        assert!(r.layer(&vfs).ok().flatten().is_none());
    }
}
