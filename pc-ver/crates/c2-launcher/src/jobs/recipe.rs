//! Recipe verification requests executed as one launcher job.
use crate::recipe_page::{Msg, Status};
use c2_mods::recipe::{self, Base, Recipe, RecipeMod};
use std::path::{Path, PathBuf};

pub(crate) enum Check {
    Row {
        index: usize,
        dir: PathBuf,
        entry: RecipeMod,
        game: bool,
    },
    Base {
        base: Base,
        first: PathBuf,
        dirs: Vec<PathBuf>,
        others: Vec<PathBuf>,
    },
    Scan {
        mods: Box<Recipe>,
        games: Vec<RecipeMod>,
        index: Vec<usize>,
        dirs: Vec<PathBuf>,
    },
}

/// Whether the game in `dir` is `base`, every listed file as it pins it
/// (stops at the first that is not, so other games are quick to pass).
fn is_base(base: &Base, dir: &Path) -> bool {
    if base.files.is_empty() || c2_mods::pack::find_root(dir).is_none() {
        return false;
    }
    let Ok(v) = c2_mods::pack::vanilla_files(&[dir.to_path_buf()]) else {
        return false;
    };
    base.files.iter().all(|(path, want)| {
        v.read(path)
            .is_ok_and(|d| recipe::sha256(&d).eq_ignore_ascii_case(want))
    })
}

impl Check {
    pub fn run(self) -> Vec<Msg> {
        match self {
            Self::Row {
                index,
                dir,
                entry,
                game,
            } => {
                let located = recipe::check_folder(&entry, &dir, game);
                vec![Msg::Row(index, dir, located)]
            }
            Self::Base {
                base,
                first,
                dirs,
                others,
            } => {
                let s = match recipe::check_apk(&base, &first) {
                    Some(true) => Status::Ok("built from that APK".into()),
                    Some(false) => {
                        if let Some(g) = others
                            .iter()
                            .flat_map(|d| recipe::dir_and_subs(d, false))
                            .find(|g| recipe::check_apk(&base, g) == Some(true))
                        {
                            return vec![Msg::Found(g, Status::Ok("built from that APK".into()))];
                        }
                        Status::Bad(
                            "Select or build this recipe's APK base; the current game is different"
                                .into(),
                        )
                    }
                    None => match c2_mods::pack::vanilla_files(&dirs) {
                        Err(e) => Status::Bad(e.to_string()),
                        Ok(v) => {
                            let (missing, bad) = recipe::check_base(&base, &v);
                            if missing.is_empty() && bad.is_empty() {
                                Status::Ok(format!("ok ({} files)", base.files.len()))
                            } else if let Some(g) = others
                                .iter()
                                .flat_map(|d| recipe::dir_and_subs(d, false))
                                .find(|g| !dirs.contains(g) && is_base(&base, g))
                            {
                                return vec![Msg::Found(
                                    g.clone(),
                                    Status::Ok(format!("ok ({} files)", base.files.len())),
                                )];
                            } else {
                                // As `c2-pc recipe verify`: a warning, not a stop.
                                Status::Bad(format!(
                                    "{} missing, {} different (another version?)",
                                    missing.len(),
                                    bad.len()
                                ))
                            }
                        }
                    },
                };
                vec![Msg::Base(s)]
            }
            Self::Scan {
                mods,
                games,
                index,
                dirs,
            } => {
                let mut messages = Vec::new();
                let found = recipe::locate(&mods, &dirs)
                    .into_iter()
                    .chain(recipe::locate_games(&games, &dirs));
                let mut n = 0;
                for (i, l) in index.into_iter().zip(found) {
                    if let (true, Some(root)) = (l.complete(), l.root.clone()) {
                        n += 1;
                        messages.push(Msg::Row(i, root, l));
                    }
                }
                messages.push(Msg::Scanned(n));
                messages
            }
        }
    }
}
