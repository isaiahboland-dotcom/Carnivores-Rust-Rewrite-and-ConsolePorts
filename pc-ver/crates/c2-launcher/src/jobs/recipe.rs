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
        /// The game to check; `None`: only look in `others`.
        first: Option<PathBuf>,
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
                // A game in `others` that is the base (built from its APK,
                // or with every listed file as pinned).
                let found = || {
                    others
                        .iter()
                        .flat_map(|d| recipe::dir_and_subs(d, false))
                        .find(|g| {
                            !dirs.contains(g)
                                && match recipe::check_apk(&base, g) {
                                    Some(ok) => ok,
                                    None => base.base.apk.is_empty() && is_base(&base, g),
                                }
                        })
                };
                let ok = || {
                    if base.base.apk.is_empty() {
                        Status::Ok(format!("ok ({} files)", base.files.len()))
                    } else {
                        Status::Ok("built from that APK".into())
                    }
                };
                let Some(first) = first else {
                    return vec![match found() {
                        Some(g) => Msg::Found(g, ok()),
                        None => Msg::Base(Status::Bad(
                            "no game folder: its base game wasn't found in the mod and add-on \
                             folders; pick it with Edit, Base game"
                                .into(),
                        )),
                    }];
                };
                let s = match recipe::check_apk(&base, &first) {
                    Some(true) => ok(),
                    Some(false) => {
                        if let Some(g) = found() {
                            return vec![Msg::Found(g, ok())];
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
                                ok()
                            } else if let Some(g) = found() {
                                return vec![Msg::Found(g, ok())];
                            } else {
                                // Play waits for "Use this game anyway".
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
