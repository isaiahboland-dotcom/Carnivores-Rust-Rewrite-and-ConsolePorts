//! Recipe and instance navigation.
use super::*;

impl Launcher {
    /// The recipe page for `path` (a dropped or opened `.toml`, or an
    /// instance): the folders it was last given, the rest looked for in
    /// the search folders.
    pub(super) fn open_recipe(&mut self, path: PathBuf) {
        if !self.can_start_operation() {
            return;
        }
        let dirs = self.game_folders().0;
        let candidates = self.base_candidates();
        match crate::recipe_page::Page::open(&path, self.cfg.remembered(&path), dirs, candidates) {
            Ok(mut p) => {
                p.scan(self.cfg.search_dirs());
                p.dispatch(&mut self.job);
                self.ui.recipe_page = Some(p);
                if self.ui.tab != Tab::Instances {
                    self.ui.tab = Tab::AddOns;
                }
            }
            Err(e) => self.ui.note = Some((format!("Not a recipe: {e}"), RED)),
        }
    }

    /// The recipe page's recipe in use (its choices taken in), and played.
    pub(super) fn use_recipe(&mut self, play: bool) {
        if !self.can_start_operation() {
            return;
        }
        let Some(p) = self.ui.recipe_page.take() else {
            return;
        };
        let mut u = p.to_use();
        // Freeze the resolved base/overlays before the Game tab changes.
        u.overlays = self
            .cfg
            .remembered(&u.path)
            .and_then(|r| r.overlays.clone());
        let games = self.recipe_game_with_overlays(&u);
        u.game = games.first().cloned();
        u.overlays = Some(games.into_iter().skip(1).collect());
        let s = &p.recipe.settings;
        self.cfg.remember_builds();
        self.cfg.apply(s);
        self.cfg.pictures = s.pictures;
        self.cfg.tuning = s.tuning;
        self.cfg.restore_builds(&u.path, &p.recipe.recipe.id);
        self.ui.balance.edits = s.balance.clone().unwrap_or_default();
        let _ = self.ui.balance.store();
        self.cfg.remember(&u);
        self.ui.note = Some((
            format!("Recipe '{}' in use: play and the builds go by it.", u.id()),
            TEXT,
        ));
        self.cfg.recipe = Some(u);
        self.save();
        if play {
            self.play();
        }
    }

    /// "Save as instance": the recipe in use with the choices made now, or
    /// the setup (game, add-ons, standalones) as a new recipe, saved under
    /// the recipes folder by default.
    pub(super) fn save_instance(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let dir = crate::config::recipes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let current = self.active_recipe().cloned();
        let name = current.as_ref().map_or("my-instance".into(), |u| u.id());
        let Some(out) = rfd::FileDialog::new()
            .set_directory(&dir)
            .set_file_name(format!("{name}.toml"))
            .add_filter("Recipe", &["toml"])
            .save_file()
        else {
            return;
        };
        let stem = out
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let Some(u) = current else {
            let cfg = self.cfg.clone();
            let id = crate::instances::id_from(&stem);
            if let Err(e) = self.job.start_task("Saving instance", move |_| {
                crate::jobs::saving::from_setup(cfg, out, id)
            }) {
                self.ui.note = Some((e, RED));
                return;
            }
            self.ui.note = Some(("Saving the instance (hashing its files)...".into(), TEXT));
            return;
        };
        let mut r = match c2_mods::recipe::Recipe::load(&u.path) {
            Ok(r) => r,
            Err(e) => {
                self.ui.note = Some((format!("Not saved: {e}"), RED));
                return;
            }
        };
        // A recipe's patches are found beside it.
        if !r.patches.is_empty() && out.parent() != u.path.parent() {
            self.ui.note = Some((
                "Not saved: the recipe has patches beside it; save it in the same folder.".into(),
                RED,
            ));
            return;
        }
        if out != u.path {
            r.recipe.id = crate::instances::id_from(&stem);
            r.recipe.name = stem;
        }
        r.settings = self.cfg.settings(&r.settings);
        r.settings.balance = self
            .ui
            .balance
            .file()
            .map(|_| self.ui.balance.edits.clone());
        self.write_instance(r, RecipeUse { path: out, ..u });
    }

    pub(super) fn write_instance(&mut self, r: c2_mods::recipe::Recipe, u: RecipeUse) {
        match std::fs::write(&u.path, r.to_toml()) {
            Ok(()) => {
                self.ui.note = Some((format!("Instance saved: {}", u.path.display()), TEXT));
                self.cfg.remember_builds();
                self.cfg.remember(&u);
                self.cfg.restore_builds(&u.path, &r.recipe.id);
                self.cfg.recipe = Some(u);
                self.save();
                self.ui.instances.refresh(&self.cfg);
            }
            Err(e) => self.ui.note = Some((format!("Not saved: {e}"), RED)),
        }
    }

    /// A "Save as instance" of the setup that finished.
    pub(super) fn saving_done(&mut self, mut r: c2_mods::recipe::Recipe, u: RecipeUse) {
        r.settings.balance = self
            .ui
            .balance
            .file()
            .map(|_| self.ui.balance.edits.clone());
        self.write_instance(r, u);
    }

    pub(super) fn instances_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Instances");
        if self.ui.recipe_edit.is_some() {
            self.recipe_edit_ui(ui);
            return;
        }
        if self.ui.recipe_page.is_some() {
            self.recipe_ui(ui);
            return;
        }
        let busy = self.busy();
        match self.ui.instances.ui(ui, &self.cfg, self.ui.mode, busy) {
            crate::instances::Ask::Nothing => {}
            crate::instances::Ask::Open(p) => self.open_recipe(p),
            crate::instances::Ask::Start(p, destination) => {
                self.open_recipe(p);
                if let Some(page) = &mut self.ui.recipe_page {
                    page.next = Some(destination);
                }
            }
            crate::instances::Ask::Edit(p) => self.edit_recipe(&p),
            crate::instances::Ask::Clone(p) => self.clone_recipe(&p),
            crate::instances::Ask::Save => self.save_instance(),
        }
    }

    pub(super) fn edit_recipe(&mut self, path: &Path) {
        if !self.can_start_operation() {
            return;
        }
        match crate::recipe_edit::Editor::open(path) {
            Ok(e) => self.ui.recipe_edit = Some(e),
            Err(e) => self.ui.note = Some((format!("Not a recipe: {e}"), RED)),
        }
    }

    /// "Clone...": the recipe saved under a new name (with the folders
    /// given for it), then opened to edit.
    pub(super) fn clone_recipe(&mut self, from: &Path) {
        if !self.can_start_operation() {
            return;
        }
        let Some(to) = crate::recipe_edit::pick_copy_path(from) else {
            return;
        };
        let made = c2_mods::recipe::Recipe::load(from)
            .map_err(|e| e.to_string())
            .and_then(|r| {
                let c = crate::recipe_edit::renamed_for(&r, &to);
                if let Some(p) = crate::recipe_edit::save_problem(&c, from, &to) {
                    return Err(p);
                }
                let old = std::fs::read_to_string(from).unwrap_or_default();
                std::fs::write(&to, crate::recipe_edit::with_header(&old, &c.to_toml()))
                    .map_err(|e| e.to_string())
            });
        match made {
            Ok(()) => {
                self.recipe_saved(from, &to, &[]);
                self.ui.note = Some((format!("Cloned to {}", to.display()), TEXT));
                self.edit_recipe(&to);
            }
            Err(e) => self.ui.note = Some((format!("Not cloned: {e}"), RED)),
        }
    }

    /// A recipe written to `to` from the one at `from` (the same path when
    /// saved over it): the folders given for it (and `added`) are kept for
    /// `to`, and the list shows it.
    pub(super) fn recipe_saved(&mut self, from: &Path, to: &Path, added: &[(String, PathBuf)]) {
        let mut u = self.cfg.remembered(from).cloned().unwrap_or(RecipeUse {
            path: to.to_path_buf(),
            game: None,
            overlays: None,
            folders: Default::default(),
            on: false,
        });
        u.path = to.to_path_buf();
        // Only entries the recipe still has.
        if let Ok(r) = c2_mods::recipe::Recipe::load(to) {
            let ids: std::collections::HashSet<String> = c2_mods::recipe::entries(&r)
                .map(|(_, m, _)| m.id.clone())
                .collect();
            u.folders.retain(|id, _| ids.contains(id));
            for (id, dir) in added {
                if ids.contains(id) {
                    u.folders.insert(id.clone(), dir.clone());
                }
            }
        }
        if let Some(cur) = &mut self.cfg.recipe {
            if cur.path == to {
                cur.folders = u.folders.clone();
            }
        }
        u.on = false;
        self.cfg.remember(&u);
        self.save();
        self.ui.instances.refresh(&self.cfg);
    }

    /// The recipe editor; add-ons added there are read against the
    /// recipe's game (as last given), else the Game tab's.
    pub(super) fn recipe_edit_ui(&mut self, ui: &mut egui::Ui) {
        let Some(e) = &self.ui.recipe_edit else {
            return;
        };
        let game = match self.cfg.remembered(&e.path) {
            Some(u) => self.recipe_game_with_overlays(&u.clone()),
            None => self.game_folders().0,
        };
        let Some(e) = &mut self.ui.recipe_edit else {
            return;
        };
        if e.working() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        let mut ask = crate::recipe_edit::Ask::Nothing;
        let other_busy =
            self.job.busy() || self.ui.recipe_page.as_ref().is_some_and(|p| p.working());
        panel(ui, |ui| {
            ui.add_enabled_ui(!other_busy && !e.working(), |ui| {
                ask = e.ui(ui, &game, &mut self.job)
            });
        });
        match ask {
            crate::recipe_edit::Ask::Nothing => {}
            crate::recipe_edit::Ask::Close => self.ui.recipe_edit = None,
            crate::recipe_edit::Ask::Saved { from, to } => {
                let added = std::mem::take(&mut e.added);
                self.recipe_saved(&from, &to, &added);
            }
        }
    }

    /// The recipe page while one is set up, else the recipe in use (or a
    /// button to open one).
    pub(super) fn recipe_ui(&mut self, ui: &mut egui::Ui) {
        let dirs = self.game_folders().0;
        let busy = self.busy();
        if let Some(p) = &mut self.ui.recipe_page {
            let mut ask = crate::recipe_page::Ask::Nothing;
            let play = self.ui.mode == Mode::Full;
            panel(ui, |ui| {
                ui.add_enabled_ui(!busy, |ui| ask = p.ui(ui, &dirs, busy, play, &mut self.job));
            });
            match ask {
                crate::recipe_page::Ask::Nothing => {}
                crate::recipe_page::Ask::Use => self.use_recipe(false),
                crate::recipe_page::Ask::Play => self.use_recipe(true),
                crate::recipe_page::Ask::Close => self.ui.recipe_page = None,
                crate::recipe_page::Ask::ChooseApk => {
                    if let Some(apk) = rfd::FileDialog::new()
                        .add_filter("Android app", &["apk"])
                        .pick_file()
                    {
                        self.base_from_apk(apk, true);
                    }
                }
            }
            return;
        }
        let mut open = None;
        let mut stop = false;
        let mut changed = false;
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                match &mut self.cfg.recipe {
                    Some(r) => {
                        changed |= ui
                            .checkbox(&mut r.on, RichText::new("Recipe").color(SAND).strong())
                            .on_hover_text(
                                "On: play and the builds use the recipe instead of the lists                                  below.",
                            )
                            .changed();
                        ui.label(r.id()).on_hover_text(r.path.display().to_string());
                        if ui.button("Edit").clicked() {
                            open = Some(r.path.clone());
                        }
                        if ui.button("Stop using").clicked() {
                            stop = true;
                        }
                    }
                    None => {
                        ui.label(RichText::new("Recipe").color(SAND).strong());
                        help(ui, "none: drop a recipe (.toml) on the window, or");
                    }
                }
                if ui.button("Open a recipe...").clicked() {
                    open = rfd::FileDialog::new()
                        .add_filter("Recipe", &["toml"])
                        .pick_file();
                }
            });
            if self.active_recipe().is_some() {
                help(
                    ui,
                    "While the recipe is on, the add-on list and the standalone games below                      aren't used.",
                );
            }
        });
        if stop {
            self.cfg.recipe = None;
            changed = true;
        }
        if changed {
            self.save();
        }
        if let Some(p) = open {
            self.open_recipe(p);
        }
    }
}
