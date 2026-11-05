//! Game folders as the launcher keeps them: the folder with `HUNTDAT` in
//! it. An instance's game was kept as given (the folder picked, or the
//! search folder its base was found under: `source/`, the download's
//! parent). Play searched below it and worked; the 3DS and Wii U build
//! pages looked for `HUNTDAT` in it and stayed greyed out (owner's report
//! 2026-11-05: jphl and others).

use std::path::{Path, PathBuf};

use super::Config;

/// The game in `dir` (searched a few folders down, as `c2-pc` does); `dir`
/// itself if none is found.
pub fn game_root(dir: &Path) -> PathBuf {
    c2_mods::pack::find_game_root(dir).unwrap_or_else(|| dir.to_path_buf())
}

/// A folder's game as compared between the add-on and standalone lists:
/// its game root, resolved on disk.
pub(super) fn same_game(p: &Path) -> PathBuf {
    let r = game_root(p);
    std::fs::canonicalize(&r).unwrap_or(r)
}

impl Config {
    /// Every instance's game made its game folder (configs from before).
    /// Whether one changed.
    pub(super) fn fix_instance_games(&mut self) -> bool {
        let mut changed = false;
        let uses = self.recipe.iter_mut().chain(self.remembered.iter_mut());
        for g in uses.filter_map(|u| u.game.as_mut()) {
            let root = game_root(g);
            if root != *g {
                *g = root;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RecipeUse;

    #[test]
    fn an_instance_keeps_the_game_folder_not_its_parent() {
        let t = crate::publish::tests::Temp::new();
        let game = t.0.join("jphl-se/JPHL Special Edition");
        std::fs::create_dir_all(game.join("HUNTDAT")).unwrap();
        std::fs::write(game.join("HUNTDAT/_RES.TXT"), "characters {\n}\n.\n").unwrap();
        let u = |g: PathBuf| RecipeUse {
            game: Some(g),
            ..Default::default()
        };
        let mut c = Config {
            recipe: Some(u(t.0.join("jphl-se"))),
            remembered: vec![u(t.0.clone()), u(game.clone())],
            ..Default::default()
        };
        assert!(c.fix_instance_games());
        assert_eq!(c.recipe.unwrap().game, Some(game.clone()));
        assert!(c.remembered.iter().all(|r| r.game == Some(game.clone())));
        // Not a game: kept as it is.
        assert_eq!(game_root(&t.0.join("nothing")), t.0.join("nothing"));
    }
}
