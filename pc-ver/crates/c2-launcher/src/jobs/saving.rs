//! Hash the current setup into an instance off the UI thread.
use super::Completion;
use crate::config::{describe_game, Config, RecipeUse};
use c2_mods::recipe::{self, RecipeMod};
use std::path::PathBuf;
pub(crate) fn from_setup(cfg: Config, out: PathBuf, id: String) -> Result<Completion, String> {
    let base = cfg.base.clone();
    let overlays = cfg.overlays.clone();
    let addons = cfg.enabled_addons();
    let standalones = cfg.enabled_standalones();
    let settings = cfg.settings(&Default::default());
    let base = base
        .filter(|b| describe_game(b).is_some())
        .ok_or("no game folder in the old setup")?;
    let games: Vec<PathBuf> = std::iter::once(base.clone())
        .chain(overlays.clone())
        .collect();
    let v = c2_mods::vanilla(&games).map_err(|e| e.to_string())?;
    let sources = addons
        .iter()
        .map(|m| c2_mods::load_mod(&v, m).map_err(|e| format!("{}: {e}", m.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let base_id = crate::instances::match_base(&out, &base).unwrap_or_default();
    let mut r = recipe::new_recipe(&id, &id, &base_id, &sources).map_err(|e| e.to_string())?;
    let mut folders: Vec<(String, PathBuf)> = r
        .mods
        .iter()
        .map(|m| m.id.clone())
        .zip(addons.iter().cloned())
        .collect();
    for (list, dirs) in [
        (&mut r.overlays, &overlays),
        (&mut r.standalones, &standalones),
    ] {
        for d in dirs {
            let m: RecipeMod = recipe::pin_game(d).map_err(|e| e.to_string())?;
            folders.push((m.id.clone(), d.clone()));
            list.push(m);
        }
    }
    // Each standalone's "Pick parts" goes with it.
    let skips = cfg
        .standalones
        .iter()
        .filter(|a| a.enabled)
        .map(|a| a.skip.clone());
    for (m, skip) in r.standalones.iter_mut().zip(skips) {
        m.skip = skip;
    }
    if !r.standalones.is_empty() {
        r.combine = Some(recipe::Combine {
            ambients: "tied".into(),
        });
    }
    r.settings = settings;
    let u = RecipeUse {
        path: out.clone(),
        game: Some(base),
        overlays: Some(Vec::new()),
        folders: folders.into_iter().collect(),
        on: true,
    };
    Ok(Completion::Saved(Box::new((r, u))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_pinning_preserves_an_existing_instance() {
        let temp = crate::publish::tests::Temp::new();
        let out = temp.0.join("instance.toml");
        std::fs::write(&out, b"previous recipe").unwrap();
        assert!(from_setup(Config::default(), out.clone(), "test".into()).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), b"previous recipe");
    }
}
