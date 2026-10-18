//! `mod.toml` (README §5).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    #[serde(rename = "mod")]
    pub info: ModInfo,
    pub files: Files,
    pub res: Res,
    pub deps: Deps,
    pub engine: Engine,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub source_url: String,
}

/// Paths relative to the mod root (`HUNTDAT/...`). Auto-detected when both
/// lists are empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Files {
    pub adds: Vec<String>,
    pub replaces: Vec<String>,
}

impl Files {
    pub fn is_empty(&self) -> bool {
        self.adds.is_empty() && self.replaces.is_empty()
    }
}

/// `_RES.TXT` entries this mod adds or modifies, by entry id (the entry's
/// `name`, lower case).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Res {
    pub adds_dinos: Vec<String>,
    pub modifies_dinos: Vec<String>,
    pub adds_weapons: Vec<String>,
    pub modifies_weapons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Deps {
    pub requires: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Engine {
    pub features: Vec<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn to_toml(&self) -> String {
        // Serialising plain structs of strings cannot fail.
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// A mod id from a folder name: lower case, alphanumerics and `-`.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    s.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_example_parses_and_round_trips() {
        let m = Manifest::parse(
            r#"
[mod]
id = "example-raptors"
name = "Example Raptor Pack"
version = "1.0"
authors = ["Original Author"]
source_url = "https://example.com/mod-page"

[files]
adds = ["HUNTDAT/DINO/NEWRAPTOR.CAR"]
replaces = ["HUNTDAT/MENU/MENUM.TGA"]

[res]
adds_dinos = ["newraptor"]
modifies_dinos = []

[deps]
requires = []
conflicts = []

[engine]
features = []
"#,
        );
        let m = m.unwrap_or_default();
        assert_eq!(m.info.id, "example-raptors");
        assert_eq!(m.files.adds, vec!["HUNTDAT/DINO/NEWRAPTOR.CAR"]);
        assert_eq!(m.res.adds_dinos, vec!["newraptor"]);
        assert_eq!(Manifest::parse(&m.to_toml()).ok(), Some(m));
        assert_eq!(
            Manifest::parse("[mod]\nid = \"x\"").map(|m| m.info.id).ok(),
            Some("x".into())
        );
        assert_eq!(slug("My Raptor Pack (v2)"), "my-raptor-pack-v2");
    }
}
