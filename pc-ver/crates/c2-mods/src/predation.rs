//! A recipe's `[settings.predation]` (the launcher's advanced instance
//! setting "Carnivores hunt herbivores"): written into the built game as
//! `HUNTDAT/_PREDATION.TXT` ([`c2_formats::predation`]), which every
//! platform's engine reads at the start of a hunt.
//!
//! ```toml
//! [settings.predation]
//! enabled = true
//! priority = "prey"            # or "hunter" (default)
//! carnivore_fights = true
//! apex = "Tyrannosaurus"       # one animal above all; or:
//! chain = ["Tyrannosaurus", "Spinosaurus", "Velociraptor"]  # top first
//!
//! [[settings.predation.animals]]
//! name = "Velociraptor"
//! scavenger = true
//!
//! [[settings.predation.animals]]
//! name = "Triceratops"
//! diet = "herbivore"           # or "carnivore"; omitted: from its AI
//! defensive = true
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::predation as fmt;
use c2_vfs::{MemBackend, Vfs};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Predation {
    pub enabled: bool,
    /// `hunter` (default) or `prey`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub priority: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub carnivore_fights: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub apex: String,
    /// The food chain, top first; replaces prices (and `apex`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<Animal>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animal {
    pub name: String,
    /// `herbivore`, `carnivore`, or empty (from its AI).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub diet: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub scavenger: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub defensive: bool,
}

impl Animal {
    /// Whether it changes anything (else it need not be kept).
    pub fn is_tagged(&self) -> bool {
        !self.diet.is_empty() || self.scavenger || self.defensive
    }
}

impl Predation {
    /// The settings file's form.
    pub fn to_format(&self) -> fmt::Predation {
        fmt::Predation {
            priority: if self.priority.eq_ignore_ascii_case("prey") {
                fmt::Priority::Prey
            } else {
                fmt::Priority::Hunter
            },
            carnivore_fights: self.carnivore_fights,
            apex: (!self.apex.trim().is_empty() && self.chain.is_empty())
                .then(|| self.apex.trim().into()),
            chain: self.chain.clone(),
            animals: self
                .animals
                .iter()
                .filter(|a| a.is_tagged())
                .map(|a| fmt::Animal {
                    name: a.name.clone(),
                    diet: match a.diet.to_ascii_lowercase().as_str() {
                        "herbivore" => Some(fmt::Diet::Herbivore),
                        "carnivore" => Some(fmt::Diet::Carnivore),
                        _ => None,
                    },
                    scavenger: a.scavenger,
                    defensive: a.defensive,
                })
                .collect(),
        }
    }

    /// The layer to put over a built game, when predation is on.
    pub fn layer(&self) -> Option<MemBackend> {
        if !self.enabled {
            return None;
        }
        let mut m = MemBackend::new();
        m.insert(fmt::PATH, self.to_format().to_text().into_bytes());
        Some(m)
    }

    /// Puts the layer over `vfs` (nothing when off). True when it did.
    pub fn push(&self, vfs: &mut Vfs) -> bool {
        match self.layer() {
            Some(l) => {
                vfs.push("predation", Box::new(l));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_section_becomes_the_settings_file() {
        let p: Predation = toml::from_str(
            r#"
            enabled = true
            priority = "prey"
            carnivore_fights = true
            apex = "Rex"
            [[animals]]
            name = "Raptor"
            scavenger = true
            [[animals]]
            name = "Trike"
            diet = "herbivore"
            defensive = true
            [[animals]]
            name = "Plain"
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let f = p.to_format();
        assert_eq!(f.priority, fmt::Priority::Prey);
        assert_eq!(f.apex.as_deref(), Some("Rex"));
        assert_eq!(f.animals.len(), 2, "untagged animals are left out");
        let mut vfs = Vfs::new();
        assert!(p.push(&mut vfs));
        let back = fmt::Predation::parse(&vfs.read(fmt::PATH).unwrap_or_default());
        assert_eq!(back, f);
    }

    #[test]
    fn off_writes_nothing_and_a_chain_wins_over_the_apex() {
        let mut p = Predation {
            apex: "Rex".into(),
            chain: vec!["Raptor".into()],
            ..Predation::default()
        };
        assert!(p.layer().is_none());
        p.enabled = true;
        assert_eq!(p.to_format().apex, None);
    }
}
