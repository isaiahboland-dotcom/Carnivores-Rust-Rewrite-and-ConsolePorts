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
                return Err("an add-on is read against the game: set the Game tab's game".into());
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
