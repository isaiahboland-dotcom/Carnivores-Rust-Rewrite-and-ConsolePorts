//! A recipe's `[settings.dhc]`: the DinoHunt Corp terminal's rules
//! (discovery of secret animals, licence costs, what each island offers).
//! Written into the built game as `HUNTDAT/_DHC.TXT` ([`c2_formats::dhc`]),
//! which the menus read when they load the game. Opt-in: absent or
//! `enabled = false`, nothing is written and the build is unchanged.
//!
//! ```toml
//! [settings.dhc]
//! enabled = true
//! discovery = "sighting"       # off (default), sighting, scan or kill
//! licence_cost = 200           # score points (default 200)
//!
//! [[settings.dhc.animals]]
//! name = "Velociraptor"
//! discovery = "kill"
//! licence_cost = 500
//!
//! [[settings.dhc.islands]]
//! area = "Vengar Peninsula"    # as the balance editor names it
//! animals = ["Parasaurolophus", "Pachycephalosaurus"]   # empty: all
//! weapons = ["Pistol", "Shotgun"]
//! equipment = ["Radar"]
//! times = ["dawn", "day"]
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::dhc as fmt;
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dhc {
    pub enabled: bool,
    /// `off` (default), `sighting`, `scan` or `kill`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub discovery: String,
    /// Score points; unset: 200.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_cost: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<Animal>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub islands: Vec<Island>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animal {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub discovery: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_cost: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Island {
    /// The area's name, as the balance editor lists it.
    pub area: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub equipment: Vec<String>,
    /// `dawn`, `day`, `night`; empty: all.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub times: Vec<String>,
}

/// The equipment names an island can list (the last two with dynamic time).
pub const EQUIPMENT: [&str; 6] = [
    "Camouflage",
    "Radar",
    "Cover Scent",
    "Double Ammo",
    "Night Vision",
    "Flashlight",
];

fn clean(list: &[String]) -> Vec<String> {
    list.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Dhc {
    /// The file's form; `maps` gives each area name's map (islands whose
    /// area it lacks are skipped). Unknown modes fall back to the default.
    pub fn to_format(&self, maps: &[(String, String)]) -> fmt::Dhc {
        let map = |name: &str| {
            maps.iter()
                .find(|(n, m)| {
                    n.eq_ignore_ascii_case(name.trim()) || m.eq_ignore_ascii_case(name.trim())
                })
                .map(|(_, m)| m.clone())
        };
        let cost = |c: u32| c.clamp(fmt::LICENCE_RANGE.0, fmt::LICENCE_RANGE.1);
        fmt::Dhc {
            discovery: fmt::Discovery::from_key(&self.discovery).unwrap_or_default(),
            licence_cost: self.licence_cost.map_or(fmt::DEFAULT_LICENCE_COST, cost),
            animals: self
                .animals
                .iter()
                .filter(|a| !a.name.trim().is_empty())
                .map(|a| fmt::Animal {
                    name: a.name.trim().into(),
                    discovery: fmt::Discovery::from_key(&a.discovery),
                    licence_cost: a.licence_cost.map(cost),
                })
                .filter(|a| a.discovery.is_some() || a.licence_cost.is_some())
                .collect(),
            islands: self
                .islands
                .iter()
                .filter_map(|i| {
                    let mut times = 0u8;
                    for t in &i.times {
                        if let Some(b) = fmt::TIMES
                            .iter()
                            .position(|x| x.eq_ignore_ascii_case(t.trim()))
                        {
                            times |= 1 << b;
                        }
                    }
                    Some(fmt::Island {
                        area: map(&i.area)?,
                        animals: clean(&i.animals),
                        weapons: clean(&i.weapons),
                        equipment: clean(&i.equipment),
                        times: if times == 7 { 0 } else { times },
                    })
                })
                .collect(),
        }
    }

    /// The layer to put over the built game in `vfs`, when the section is
    /// on.
    pub fn layer(&self, vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
        if !self.enabled {
            return Ok(None);
        }
        let maps = if self.islands.is_empty() {
            Vec::new()
        } else {
            crate::balance::area_maps(vfs)?
        };
        let mut m = MemBackend::new();
        m.insert(fmt::PATH, self.to_format(&maps).to_text().into_bytes());
        Ok(Some(m))
    }

    /// Puts the layer over `vfs` (nothing when off). True when it did.
    pub fn push(&self, vfs: &mut Vfs) -> Result<bool, ModError> {
        Ok(match self.layer(vfs)? {
            Some(l) => {
                vfs.push("dhc", Box::new(l));
                true
            }
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> Dhc {
        toml::from_str(
            r#"
            enabled = true
            discovery = "scan"
            licence_cost = 300
            [[animals]]
            name = "Raptor"
            discovery = "kill"
            [[animals]]
            name = "Plain"
            [[animals]]
            name = "Rex"
            licence_cost = 900000
            [[islands]]
            area = "Ravan"
            animals = ["Trike", " "]
            times = ["Dawn", "day", "night"]
            [[islands]]
            area = "Gone"
            weapons = ["Pistol"]
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn a_recipe_section_becomes_the_settings_file() {
        let maps = [("Ravan".to_string(), "area2".to_string())];
        let f = section().to_format(&maps);
        assert_eq!(f.discovery, fmt::Discovery::Scan);
        assert_eq!(f.licence_cost, 300);
        assert_eq!(f.animals.len(), 2, "animals without settings are left out");
        assert_eq!(f.licence_cost_of("rex"), fmt::LICENCE_RANGE.1);
        assert_eq!(f.islands.len(), 1, "an island on a missing area is dropped");
        assert_eq!(f.islands[0].area, "area2");
        assert_eq!(f.islands[0].animals, ["Trike"]);
        assert_eq!(f.islands[0].times, 0, "every time is no restriction");
        assert_eq!(fmt::Dhc::parse(f.to_text().as_bytes()), f);
    }

    #[test]
    fn off_or_absent_writes_nothing_and_unset_cost_is_200() {
        let vfs = Vfs::new();
        let mut d = section();
        d.enabled = false;
        assert!(matches!(d.layer(&vfs), Ok(None)));
        let d = Dhc {
            enabled: true,
            ..Dhc::default()
        };
        assert_eq!(d.to_format(&[]).licence_cost, 200);
        let mut vfs = Vfs::new();
        assert!(d.push(&mut vfs).unwrap_or(false));
        let back = fmt::Dhc::parse(&vfs.read(fmt::PATH).unwrap_or_default());
        assert_eq!(back, fmt::Dhc::default());
    }
}
