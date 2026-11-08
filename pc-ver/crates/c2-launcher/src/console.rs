//! Shared recipe inputs for console rebuilds. Combined games are rebuilt
//! before cooking, so a saved instance never relies on an old merged pack.

use std::path::{Path, PathBuf};

use crate::config::{install_name, RecipeUse};
use crate::jobs::Step;

pub struct RecipeInput {
    pub game: PathBuf,
    pub extra: Vec<String>,
    pub merge_args: Option<Vec<String>>,
}

impl RecipeInput {
    /// `accept_base`: the player chose to use this game although it
    /// differs from the recipe's base (`--accept-base` for the c2-pc
    /// merge; the console cookers take the game as given).
    pub fn new(
        recipe: &RecipeUse,
        games: &[PathBuf],
        work: &Path,
        accept_base: bool,
    ) -> Result<Self, String> {
        let source = c2_mods::recipe::Recipe::load(&recipe.path).map_err(|e| e.to_string())?;
        let base = games.first().ok_or("Set the instance's base game first.")?;
        let overlays: Vec<String> = games
            .iter()
            .skip(1)
            .flat_map(|p| ["--overlay".to_string(), p.display().to_string()])
            .collect();
        if source.standalones.is_empty() {
            let mut extra = overlays;
            extra.extend(["--recipe".to_string(), recipe.path.display().to_string()]);
            extra.extend(recipe.mods_args());
            Ok(Self {
                game: base.clone(),
                extra,
                merge_args: None,
            })
        } else {
            let game = work.join(install_name(&source.recipe.id));
            let mut args = vec![
                "recipe".into(),
                "build".into(),
                recipe.path.display().to_string(),
                "--base".into(),
                base.display().to_string(),
            ];
            args.extend(overlays);
            args.extend(recipe.mods_args());
            if accept_base {
                args.push("--accept-base".into());
            }
            args.extend(["--out".into(), game.display().to_string()]);
            Ok(Self {
                game,
                extra: Vec::new(),
                merge_args: Some(args),
            })
        }
    }

    pub fn cook_args(&self) -> Vec<String> {
        let mut args = vec!["--game".into(), self.game.display().to_string()];
        args.extend(self.extra.clone());
        args
    }

    pub fn prepare(&self, pc: Option<PathBuf>) -> Result<Vec<Step>, String> {
        match &self.merge_args {
            Some(args) => Ok(vec![Step::Run {
                program: pc.ok_or("c2-pc is needed to rebuild this combined instance. Set the tools folder in Settings.")?,
                args: args.clone(),
            }]),
            None => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_preserve_inputs_and_rebuild_combined_games() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../(testing)/launcher-tests")
            .join(format!("console-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fixture folder");
        let path = dir.join("saved instance.toml");
        let mut source = c2_mods::recipe::Recipe::default();
        source.recipe.id = "test-instance".into();
        std::fs::write(&path, source.to_toml()).expect("recipe fixture");
        let recipe = RecipeUse {
            path,
            folders: [("addon".into(), PathBuf::from("downloads/mod with spaces"))].into(),
            ..Default::default()
        };
        let games = [PathBuf::from("base game"), PathBuf::from("engine overlay")];
        let direct = RecipeInput::new(&recipe, &games, &dir, false).expect("direct input");
        assert_eq!(direct.game, games[0]);
        assert!(direct.prepare(None).expect("no merge needed").is_empty());
        assert!(direct
            .cook_args()
            .windows(2)
            .any(|a| a == ["--overlay", "engine overlay"]));
        assert!(direct
            .extra
            .windows(2)
            .any(|a| a == ["--mods", "addon=downloads/mod with spaces"]));
        source.standalones.push(c2_mods::recipe::RecipeMod {
            id: "standalone".into(),
            ..Default::default()
        });
        std::fs::write(&recipe.path, source.to_toml()).expect("combined fixture");
        let combined = RecipeInput::new(&recipe, &games, &dir, false).expect("combined input");
        assert_eq!(combined.game, dir.join("test-instance"));
        assert!(combined.extra.is_empty());
        assert!(combined.prepare(None).is_err());
        let steps = combined.prepare(Some("c2-pc".into())).expect("merge step");
        assert!(
            matches!(&steps[..], [Step::Run { args, .. }] if args.windows(2).any(|a| a == ["--overlay", "engine overlay"]))
        );
        assert!(RecipeInput::new(&recipe, &[], &dir, false).is_err());
        std::fs::remove_dir_all(dir).expect("clean fixture");
    }
}
