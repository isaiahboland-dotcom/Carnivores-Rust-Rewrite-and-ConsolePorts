//! Game inputs and import completion handling.
use super::*;

impl Launcher {
    pub(super) fn import_apk(&mut self, apk: PathBuf) {
        if !self.can_start_operation() {
            return;
        }
        let bases = self.import_bases();
        if bases.is_empty() {
            self.ui.note = Some(("Choose the base game first (Base game page).".into(), RED));
            return;
        }
        let stem = apk
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mobile".into());
        let out = crate::config::data_dir()
            .join("mobile")
            .join(crate::config::install_name(&stem));
        if let Err(e) = self
            .job
            .start_task("Importing from the mobile game", move |job| {
                crate::jobs::imports::mobile(job, apk, bases, out)
            })
        {
            self.ui.note = Some((e, RED));
        }
    }

    pub(super) fn import_ice_age(&mut self, ia: PathBuf) {
        if !self.can_start_operation() {
            return;
        }
        let bases = self.import_bases();
        if bases.is_empty() {
            self.ui.note = Some(("Choose the base game first (Base game page).".into(), RED));
            return;
        }
        self.cfg.ice_age = Some(ia.clone());
        self.save();
        let out = crate::config::data_dir().join("ice-age");
        if let Err(e) = self
            .job
            .start_task("Importing from Carnivores: Ice Age", move |job| {
                crate::jobs::imports::ice_age(job, ia, bases, out)
            })
        {
            self.ui.note = Some((e, RED));
        }
    }

    pub(super) fn base_from_apk(&mut self, apk: PathBuf, for_recipe: bool) {
        if !self.can_start_operation() {
            return;
        }
        let stem = apk
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mobile".into());
        let out = crate::config::data_dir()
            .join("apk-base")
            .join(crate::config::install_name(&stem));
        if let Err(e) = self
            .job
            .start_task("Building the game from your APK", move |job| {
                crate::jobs::apk_base::build(job, apk, out, for_recipe)
            })
        {
            self.ui.note = Some((e, RED));
        }
    }

    /// The game imports are read against: the edited instance's, else the
    /// launcher's.
    fn import_bases(&self) -> Vec<PathBuf> {
        match &self.ui.recipe_edit {
            Some(e) => e.draft.game_dirs(),
            None => self.game_folders().0,
        }
    }

    pub(super) fn apk_base_done(&mut self, out: PathBuf, for_recipe: bool) {
        if let Some(e) = self.ui.recipe_edit.as_mut().filter(|_| for_recipe) {
            e.set_game(out);
            self.ui.note = Some((
                "The game built from your APK is the base game.".into(),
                TEXT,
            ));
        } else if for_recipe {
            let dirs = self.game_folders().0;
            if let Some(p) = &mut self.ui.recipe_page {
                p.game = Some(out);
                p.check_base(dirs);
                p.dispatch(&mut self.job);
            }
        } else {
            self.set_base(out);
        }
    }

    pub(super) fn import_done(&mut self, out: PathBuf) {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&out)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        let values = out.join(c2_mods::mobile::MOBILE_VALUES);
        self.cfg.mobile_values = values.exists().then_some(values);
        self.ui.balance.mobile = cfg_mobile(&self.cfg);
        let n = dirs.len();
        if let Some(e) = &mut self.ui.recipe_edit {
            e.queue_pins(crate::recipe_edit::Kind::AddOn, dirs);
            self.ui.note = Some((format!("{n} imported add-ons being added."), TEXT));
            return;
        }
        self.add_addons(dirs);
        self.ui.note = Some((format!("{n} imported add-ons added."), TEXT));
    }

    pub(super) fn set_base(&mut self, p: PathBuf) {
        match describe_game(&p) {
            Some(kind) => {
                self.ui.note = Some((format!("{kind} found."), TEXT));
                self.cfg.base = Some(p);
                self.save();
            }
            None => {
                self.ui.note = Some((
                    format!(
                        "{}: no HUNTDAT folder: that isn't a game folder.",
                        p.display()
                    ),
                    RED,
                ));
            }
        }
    }

    /// Folders are added as they are; archives are unpacked into the data
    /// folder first.
    pub(super) fn add_addons(&mut self, paths: Vec<PathBuf>) {
        if !self.can_start_operation() {
            return;
        }
        let mut unpack = Vec::new();
        for p in paths {
            let dest = if is_archive(&p) {
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "addon".into());
                let to = crate::config::data_dir()
                    .join("addons")
                    .join(crate::config::install_name(&stem));
                unpack.push((p, to));
                continue;
            } else if p.is_dir() {
                p
            } else {
                self.ui.note = Some((
                    format!(
                        "{}: not a folder or an archive (.zip, .rar, .7z)",
                        p.display()
                    ),
                    RED,
                ));
                continue;
            };
            if !self.cfg.addons.iter().any(|a| a.path == dest) {
                self.cfg.addons.push(AddOn {
                    path: dest,
                    enabled: true,
                    skip: Default::default(),
                });
            }
        }
        if !unpack.is_empty() {
            if let Err(e) = self.job.start_task("Unpacking add-ons", move |job| {
                crate::jobs::addons::unpack(job, unpack)
            }) {
                self.ui.note = Some((e, RED));
            }
        }
        self.save();
    }

    pub(super) fn game_folders(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let bases = match &self.cfg.base {
            Some(b) if describe_game(b).is_some() => std::iter::once(b.clone())
                .chain(self.cfg.overlays.iter().cloned())
                .collect(),
            _ => Vec::new(),
        };
        let mods = self.cfg.enabled_addons();
        (bases, mods)
    }

    /// The game played is one built from an APK, with no standalone game
    /// beside it: it has the phone's menus only (the PC's would be black,
    /// its folder has no PC frames), whatever the Menu choice says.
    pub(super) fn plays_phone_game(&self) -> bool {
        match self.active_recipe() {
            Some(r) => self
                .recipe_game(r)
                .first()
                .is_some_and(|g| crate::config::is_phone_game(g)),
            None => {
                self.cfg.enabled_standalones().is_empty()
                    && self
                        .cfg
                        .base
                        .as_deref()
                        .is_some_and(crate::config::is_phone_game)
            }
        }
    }

    /// The recipe in use, if it is on.
    pub(super) fn active_recipe(&self) -> Option<&RecipeUse> {
        self.cfg.recipe.as_ref().filter(|r| r.on)
    }

    /// A recipe's game folders: its own game and overlays (none if it has
    /// no game yet).
    pub(super) fn recipe_game(&self, r: &RecipeUse) -> Vec<PathBuf> {
        match &r.game {
            Some(g) => std::iter::once(g.clone())
                .chain(r.overlays.clone().unwrap_or_default())
                .collect(),
            // Never the old Game tab's game: an instance has its own.
            None => Vec::new(),
        }
    }

    /// [`Self::recipe_game`] to play or build with: a recipe with its own
    /// game that combines standalones and names no overlay of its own takes
    /// the Game tab's overlays over that game (combining needs the Modder's
    /// Edition; the recipe page only finds the base game). Reads the recipe.
    ///
    /// The Game tab's overlays are Carnivores 2 releases (the Modder's
    /// Edition), so a recipe on another base never takes them: copied over
    /// a standalone such as Triassic, their `_RES.TXT` replaced the game's
    /// animals with Carnivores 2's while its menu pictures stayed. Copies of
    /// them saved into such an instance by earlier versions are dropped too.
    pub(super) fn recipe_game_with_overlays(&self, r: &RecipeUse) -> Vec<PathBuf> {
        let mut g = self.recipe_game(r);
        if let Ok(rc) = c2_mods::recipe::Recipe::load(&r.path) {
            let apk = c2_mods::recipe::load_base(&r.path, &rc)
                .is_some_and(|base| !base.base.apk.is_empty());
            let c2_base = matches!(rc.recipe.base.as_str(), "" | "c2-vanilla");
            let inherited =
                !self.cfg.overlays.is_empty() && r.overlays.as_ref() == Some(&self.cfg.overlays);
            if apk {
                // A phone game's own converted rules must not be replaced
                // by an unrelated desktop overlay from the Game tab.
                g.truncate(1);
            } else if !rc.overlays.is_empty() || (!c2_base && inherited) {
                g.truncate(1);
            } else if !c2_base {
                // Its own game (and any overlays it was given) as it is.
            } else if r.overlays.is_none()
                && r.game.is_some()
                && (r.game == self.cfg.base || !rc.standalones.is_empty())
            {
                g.extend(self.cfg.overlays.iter().cloned());
            }
        }
        g
    }

    /// Where a recipe's base may be, when it is not the Game tab's: the
    /// Standalones tab's games (those on first), then the search folders
    /// (looked through off the UI thread, `c2_mods::recipe::dir_and_subs`).
    pub(super) fn base_candidates(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.cfg.enabled_standalones();
        out.extend(self.cfg.standalones.iter().map(|a| a.path.clone()));
        out.extend(self.cfg.search_dirs());
        out.push(crate::config::data_dir().join("apk-base"));
        let mut seen = std::collections::HashSet::new();
        out.retain(|p| seen.insert(p.clone()));
        out
    }
}
