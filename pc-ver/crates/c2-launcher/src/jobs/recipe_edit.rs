//! Pin a recipe entry using the shared launcher worker.
use super::Completion;
use crate::recipe_edit::Kind;
use c2_mods::recipe;
use std::path::PathBuf;
pub(crate) fn pin(kind: Kind, dir: PathBuf, game_dirs: Vec<PathBuf>) -> Result<Completion, String> {
    let m = match kind {
        Kind::Overlay | Kind::Standalone => recipe::pin_game(&dir).map_err(|e| e.to_string())?,
        Kind::AddOn => {
            if game_dirs.is_empty() {
                return Err(
                    "an add-on is read against the game: choose the base game first".into(),
                );
            }
            let v = c2_mods::vanilla(&game_dirs).map_err(|e| e.to_string())?;
            let src = c2_mods::load_mod(&v, &dir).map_err(|e| e.to_string())?;
            recipe::new_recipe("x", "x", "", &[src])
                .map_err(|e| e.to_string())?
                .mods
                .pop()
                .ok_or("nothing to pin in it")?
        }
    };
    Ok(Completion::Pinned(Box::new((kind, m, dir))))
}

/// Which base profile beside the recipe `game` is (by its hashes).
pub(crate) fn match_base(recipe: PathBuf, game: PathBuf) -> Result<Completion, String> {
    let id = crate::instances::match_base(&recipe, &game);
    Ok(Completion::BaseMatch(Box::new((game, id))))
}
