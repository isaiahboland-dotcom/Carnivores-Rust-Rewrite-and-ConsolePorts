//! The recipe file format's version (`[recipe] schema`) and the keys a
//! recipe holds that this build does not read.
//!
//! `schema` versions the file format only. A missing one is 1; a newer one
//! than [`SCHEMA`] is refused, naming both, rather than half-read. The
//! author's `[recipe] version` is free text and is never compared.
//! Unknown keys load (an older build reading a newer recipe's additions)
//! and are reported as warnings, except inside `[settings.flee]` and its
//! animals, whose flattened limits hide them from the check.

use super::Recipe;

/// The recipe format this build reads and writes.
pub const SCHEMA: u32 = 1;

/// Top-level sections set aside for later formats: accepted, reported as
/// not implemented, and ignored.
pub const RESERVED: &[&str] = &["missions"];

fn invalid(msg: String) -> toml::de::Error {
    <toml::de::Error as serde::de::Error>::custom(msg)
}

impl Recipe {
    /// Parses a recipe, with a warning for each key this build ignores
    /// (unknown or reserved). Errors on bad TOML and on a `schema` this build
    /// can't read.
    pub fn parse_checked(text: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let de = toml::Deserializer::parse(text)?;
        let mut ignored = Vec::new();
        let r: Recipe = serde_ignored::deserialize(de, |p| ignored.push(p.to_string()))?;
        match r.recipe.schema {
            0 => {
                return Err(invalid(format!(
                    "[recipe] schema = 0 is not a recipe format (this build reads schema {SCHEMA})"
                )))
            }
            n if n > SCHEMA => {
                return Err(invalid(format!(
                    "[recipe] schema = {n} is newer than this build reads (schema {SCHEMA}); \
                     update c2rs to load this recipe"
                )))
            }
            _ => {}
        }
        let warnings = ignored
            .into_iter()
            .map(|p| {
                let top = p.split('.').next().unwrap_or_default();
                if RESERVED.contains(&top) {
                    format!("[{top}] is reserved for a later version and not implemented; ignored")
                } else {
                    format!("unknown key `{p}` ignored")
                }
            })
            .collect();
        Ok((r, warnings))
    }

    /// [`Recipe::load`] with the warnings of [`Recipe::parse_checked`].
    pub fn load_checked(path: &std::path::Path) -> Result<(Self, Vec<String>), crate::ModError> {
        let text = std::fs::read_to_string(path).map_err(|source| crate::ModError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_checked(&text).map_err(|e| crate::ModError::Manifest {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: &str =
        "[recipe]\nid = \"r\"\nname = \"R\"\nversion = \"1\"\nbase = \"c2-vanilla\"\n";

    #[test]
    fn a_missing_schema_is_1_and_is_written() {
        let (r, w) = Recipe::parse_checked(MIN).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.recipe.schema, 1);
        assert!(w.is_empty(), "{w:?}");
        assert!(Recipe::default().to_toml().contains("schema = 1"));
        assert_eq!(Recipe::parse(&r.to_toml()).ok(), Some(r));
    }

    #[test]
    fn a_newer_schema_is_refused_naming_both() {
        let e = Recipe::parse(&format!("{MIN}schema = 2\n"))
            .err()
            .map(|e| e.to_string());
        let e = e.unwrap_or_default();
        assert!(e.contains("schema = 2") && e.contains("schema 1"), "{e}");
        assert!(Recipe::parse(&format!("{MIN}schema = 0\n")).is_err());
        assert!(Recipe::parse(&format!("{MIN}schema = 1\n")).is_ok());
    }

    #[test]
    fn unknown_keys_load_with_a_warning_each() {
        let text = format!(
            "{MIN}colour = \"red\"\n[settings]\nmenu = \"mod\"\nfuture = 3\n\
             [[mods]]\nid = \"m\"\ncredit = \"c\"\nurl = \"u\"\nextra = true\n"
        );
        let (r, w) = Recipe::parse_checked(&text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.settings.menu, "mod");
        assert_eq!(r.mods.len(), 1);
        assert_eq!(w.len(), 3, "{w:?}");
        for k in ["recipe.colour", "settings.future", "extra"] {
            assert!(w.iter().any(|x| x.contains(k)), "{k}: {w:?}");
        }
    }

    /// Flattened sections (`[settings.flee]`'s limits) are read keys, not
    /// unknown ones. A serde limitation: an unknown key inside a table with
    /// flattened fields is dropped without being reported.
    #[test]
    fn flattened_keys_are_not_unknown() {
        let text = format!(
            "{MIN}[settings.flee]\nenabled = true\nmax_distance = 30\ntypo = 1\n\
             [[settings.flee.animals]]\nname = \"Velociraptor\"\nmax_duration = 9\n"
        );
        let (r, w) = Recipe::parse_checked(&text).unwrap_or_else(|e| panic!("{e}"));
        let f = r.settings.flee.unwrap_or_default();
        assert_eq!(f.limits.max_distance, Some(30));
        assert_eq!(f.animals[0].limits.max_duration, Some(9));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn missions_are_reserved_not_implemented() {
        let text = format!("{MIN}[missions]\nfirst = \"hunt a rex\"\n[[missions.list]]\nid = 1\n");
        let (r, w) = Recipe::parse_checked(&text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("[missions] is reserved") && w[0].contains("not implemented"));
        // Nothing of it is kept: saved again, the section is gone.
        assert!(!r.to_toml().contains("missions"));
    }
}
