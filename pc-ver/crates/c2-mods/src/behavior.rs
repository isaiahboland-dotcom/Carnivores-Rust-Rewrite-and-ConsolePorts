//! A recipe's `[settings.behavior]` (the launcher's Advanced tab, "Advanced
//! Dinosaur Behavior and AI"): written into the built game as
//! `HUNTDAT/_BEHAVIOR.TXT` ([`c2_formats::behavior`]). Design:
//! `docs/BEHAVIOR.md`. So far it holds one part, awareness.
//!
//! ```toml
//! [settings.behavior.awareness]
//! enabled = true
//! suspicion = 160          # half-notices up to this percent of the alarm threshold
//! investigate_after = 2    # seconds of suspicion before it goes to look
//! search = 20              # seconds a predator searches where it lost the hunter
//!
//! [[settings.behavior.awareness.animals]]
//! name = "Velociraptor"
//! search = 40              # unset keys inherit the global value
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::behavior as fmt;
use c2_vfs::{MemBackend, Vfs};

/// One animal's awareness values; absent keys inherit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animal {
    /// The animal's resource name (as `_RES.TXT` gives it).
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicion: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigate_after: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<u32>,
}

impl Animal {
    pub fn values(&self) -> fmt::Awareness {
        fmt::Awareness {
            suspicion: self.suspicion,
            investigate_after: self.investigate_after,
            search: self.search,
        }
    }
}

/// `[settings.behavior.awareness]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Awareness {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicion: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigate_after: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<Animal>,
}

impl Awareness {
    pub fn values(&self) -> fmt::Awareness {
        fmt::Awareness {
            suspicion: self.suspicion,
            investigate_after: self.investigate_after,
            search: self.search,
        }
    }
}

/// `[settings.behavior]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awareness: Option<Awareness>,
}

impl Behavior {
    /// Whether any part is on.
    pub fn is_active(&self) -> bool {
        self.awareness.as_ref().is_some_and(|a| a.enabled)
    }

    /// The settings file's form (animals that change nothing left out).
    pub fn to_format(&self) -> fmt::Behavior {
        let Some(a) = self.awareness.as_ref().filter(|a| a.enabled) else {
            return fmt::Behavior::default();
        };
        fmt::Behavior {
            awareness: true,
            global: a.values().clamped(),
            animals: a
                .animals
                .iter()
                .filter(|x| !x.name.trim().is_empty() && !x.values().is_empty())
                .map(|x| (x.name.trim().into(), x.values().clamped()))
                .collect(),
        }
    }

    /// The layer to put over a built game, when a part is on.
    pub fn layer(&self) -> Option<MemBackend> {
        if !self.is_active() {
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
                vfs.push("behavior settings", Box::new(l));
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
        let b: Behavior = toml::from_str(
            r#"
            [awareness]
            enabled = true
            search = 30
            [[awareness.animals]]
            name = "Raptor"
            suspicion = 250
            [[awareness.animals]]
            name = "Plain"
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let want = b.to_format();
        assert!(want.awareness);
        assert_eq!(want.animals.len(), 1, "unchanged animals are left out");
        let mut vfs = Vfs::new();
        assert!(b.push(&mut vfs));
        let back = fmt::Behavior::parse(&vfs.read(fmt::PATH).unwrap_or_default());
        assert_eq!(back, want);
        assert_eq!(back.for_animal("raptor"), [250, 2, 30]);
    }

    #[test]
    fn off_writes_nothing() {
        let b: Behavior =
            toml::from_str("[awareness]\nenabled = false\nsearch = 30\n").unwrap_or_default();
        assert!(!b.is_active());
        assert!(!b.push(&mut Vfs::new()));
        assert!(!Behavior::default().push(&mut Vfs::new()));
    }

    #[test]
    fn a_recipe_keeps_its_section_through_save_and_load() {
        let text = "[recipe]\nname = \"t\"\n[settings.behavior.awareness]\nenabled = true\n\
                    search = 15\n[[settings.behavior.awareness.animals]]\nname = \"Rex\"\n\
                    investigate_after = 5\n";
        let r: crate::recipe::Recipe = toml::from_str(text).unwrap_or_else(|e| panic!("{e}"));
        let a = r.settings.behavior.clone().unwrap_or_default().awareness;
        let a = a.unwrap_or_default();
        assert_eq!(a.search, Some(15));
        assert_eq!(a.animals[0].investigate_after, Some(5));
        let back: crate::recipe::Recipe = toml::from_str(&toml::to_string(&r).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.settings.behavior, r.settings.behavior);
    }
}
