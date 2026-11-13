//! A recipe's `[settings.flee]` (the launcher's Advanced tab, "Custom flee
//! limits"): written into the built game as `HUNTDAT/_FLEE.TXT`
//! ([`c2_formats::flee`]). The limits apply only when the player has
//! "Bounded fleeing" switched on in the game's options; without the file the
//! built-in defaults apply.
//!
//! ```toml
//! [settings.flee]
//! enabled = true
//! max_distance = 24        # cells a flight may cover
//! max_duration = 20        # seconds a flight may last
//! chase_extension = 10     # seconds a still-pursuing hunter adds
//! retrigger_lockout = 4    # seconds a noise cannot re-arm fear
//!
//! [[settings.flee.animals]]
//! name = "Parasaurolophus"
//! max_distance = 40        # unset keys inherit the global value
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::flee as fmt;
use c2_vfs::{MemBackend, Vfs};

/// The four limits as a recipe holds them; absent keys inherit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chase_extension: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrigger_lockout: Option<u32>,
}

impl Limits {
    fn to_format(self) -> fmt::Limits {
        fmt::Limits {
            max_distance: self.max_distance,
            max_duration: self.max_duration,
            chase_extension: self.chase_extension,
            retrigger_lockout: self.retrigger_lockout,
        }
        .clamped()
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animal {
    /// The animal's resource name (as `_RES.TXT` gives it).
    pub name: String,
    #[serde(flatten)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flee {
    pub enabled: bool,
    #[serde(flatten)]
    pub limits: Limits,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<Animal>,
}

impl Flee {
    /// The settings file's form (animals that change nothing left out).
    pub fn to_format(&self) -> fmt::Flee {
        fmt::Flee {
            global: self.limits.to_format(),
            animals: self
                .animals
                .iter()
                .filter(|a| !a.name.trim().is_empty() && !a.limits.is_empty())
                .map(|a| (a.name.trim().into(), a.limits.to_format()))
                .collect(),
        }
    }

    /// The layer to put over a built game, when custom limits are on.
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
                vfs.push("flee limits", Box::new(l));
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
        let f: Flee = toml::from_str(
            r#"
            enabled = true
            max_distance = 30
            retrigger_lockout = 2
            [[animals]]
            name = "Hadro"
            max_duration = 45
            [[animals]]
            name = "Plain"
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let want = f.to_format();
        assert_eq!(want.animals.len(), 1, "unchanged animals are left out");
        let mut vfs = Vfs::new();
        assert!(f.push(&mut vfs));
        let back = fmt::Flee::parse(&vfs.read(fmt::PATH).unwrap_or_default());
        assert_eq!(back, want);
        assert_eq!(back.for_animal("hadro"), [30, 45, 10, 2]);
    }

    #[test]
    fn a_recipe_keeps_its_flee_section_through_save_and_load() {
        let text = "[recipe]\nname = \"t\"\n[settings.flee]\nenabled = true\nmax_duration = 15\n\
                    [[settings.flee.animals]]\nname = \"Hadro\"\nmax_distance = 40\n";
        let r: crate::recipe::Recipe = toml::from_str(text).unwrap_or_else(|e| panic!("{e}"));
        let flee = r.settings.flee.clone().unwrap_or_default();
        assert_eq!(flee.limits.max_duration, Some(15));
        assert_eq!(flee.animals[0].limits.max_distance, Some(40));
        let back: crate::recipe::Recipe = toml::from_str(&toml::to_string(&r).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.settings.flee, r.settings.flee);
    }

    #[test]
    fn off_writes_nothing_and_the_recipe_round_trips() {
        let mut f = Flee {
            limits: Limits {
                max_distance: Some(12),
                ..Limits::default()
            },
            ..Flee::default()
        };
        assert!(f.layer().is_none());
        f.enabled = true;
        let text = toml::to_string(&f).unwrap_or_default();
        assert_eq!(toml::from_str::<Flee>(&text).ok(), Some(f));
    }
}
