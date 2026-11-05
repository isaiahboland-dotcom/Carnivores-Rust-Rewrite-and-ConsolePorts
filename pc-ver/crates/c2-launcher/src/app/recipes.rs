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
        // No game is lent to an instance: one without its own looks for
        // its base in the candidates (the old Game tab's game was used, and
        // instances played it: owner's reports 2026-11-05).
        let dirs = Vec::new();
        let candidates = self.base_candidates();
        match crate::recipe_page::Page::open(&path, self.cfg.remembered(&path), dirs, candidates) {
            Ok(mut p) => {
                p.scan(self.cfg.search_dirs());
                p.dispatch(&mut self.job);
                self.ui.recipe_page = Some(p);
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
        self.take_settings(&p.recipe.settings, &u.path, &p.recipe.recipe.id);
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

    /// An instance from the old Game and Add-ons tabs' setup (game,
    /// overlays, add-ons and standalone games, hashed off the UI thread),
    /// saved under the recipes folder by default.
    pub(super) fn save_instance(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let dir = crate::config::recipes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let Some(out) = rfd::FileDialog::new()
            .set_directory(&dir)
            .set_file_name("my-instance.toml")
            .add_filter("Recipe", &["toml"])
            .save_file()
        else {
            return;
        };
        let stem = out
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let cfg = self.cfg.clone();
        let id = crate::instances::id_from(&stem);
        if let Err(e) = self.job.start_task("Saving instance", move |_| {
            crate::jobs::saving::from_setup(cfg, out, id)
        }) {
            self.ui.note = Some((e, RED));
            return;
        }
        self.ui.note = Some(("Saving the instance (hashing its files)...".into(), TEXT));
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

    /// An instance from the old setup, hashed.
    pub(super) fn saving_done(&mut self, mut r: c2_mods::recipe::Recipe, u: RecipeUse) {
        r.settings.balance = self
            .ui
            .balance
            .file()
            .map(|_| self.ui.balance.edits.clone());
        self.write_instance(r, u);
    }

    /// Clone: the recipe saved beside it under `name` (with the folders
    /// given for it), then opened to edit.
    pub(super) fn clone_recipe(&mut self, from: &Path, name: &str) {
        if !self.can_start_operation() {
            return;
        }
        let id = crate::instances::id_from(name);
        let to = from.with_file_name(format!("{id}.toml"));
        if to.exists() {
            self.ui.note = Some((format!("Not cloned: {} exists already", to.display()), RED));
            return;
        }
        let made = c2_mods::recipe::Recipe::load(from)
            .map_err(|e| e.to_string())
            .and_then(|r| {
                let mut c = crate::recipe_edit::renamed_for(&r, &to);
                c.recipe.name = name.trim().to_string();
                let old = std::fs::read_to_string(from).unwrap_or_default();
                std::fs::write(&to, crate::recipe_edit::with_header(&old, &c.to_toml()))
                    .map_err(|e| e.to_string())
            });
        match made {
            Ok(()) => {
                self.recipe_saved(from, &to);
                self.ui.note = Some((format!("Cloned to {}", to.display()), TEXT));
                self.edit_recipe(&to);
            }
            Err(e) => self.ui.note = Some((format!("Not cloned: {e}"), RED)),
        }
    }

    /// A recipe copied to `to` from the one at `from`: the folders given
    /// for it are kept for `to`, and the list shows it.
    pub(super) fn recipe_saved(&mut self, from: &Path, to: &Path) {
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
        }
        u.on = false;
        self.cfg.remember(&u);
        self.save();
        self.ui.instances.refresh(&self.cfg);
    }

    /// The recipe page: the instance's downloads and base game being
    /// checked before Play, a build or Fix missing; what is missing and
    /// why it can't go on yet.
    pub(super) fn recipe_ui(&mut self, ui: &mut egui::Ui) {
        let dirs = self.game_folders().0;
        let busy = self.busy();
        let Some(p) = &mut self.ui.recipe_page else {
            return;
        };
        let path = p.path.clone();
        if self.ui.fixing.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Fix missing: looking for the base game and downloads...");
            });
        }
        let mut ask = crate::recipe_page::Ask::Nothing;
        let play = self.ui.mode == Mode::Full;
        panel(ui, |ui| {
            ui.add_enabled_ui(!busy, |ui| ask = p.ui(ui, &dirs, busy, play, &mut self.job));
        });
        let mut edit = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Edit the instance"))
                .on_hover_text("Choose its base game and folders yourself.")
                .clicked()
            {
                edit = true;
            }
        });
        match ask {
            crate::recipe_page::Ask::Nothing => {}
            crate::recipe_page::Ask::Use => self.use_recipe(false),
            crate::recipe_page::Ask::Play => self.use_recipe(true),
            crate::recipe_page::Ask::Close => self.go_home(),
            crate::recipe_page::Ask::ChooseApk => {
                if let Some(apk) = rfd::FileDialog::new()
                    .add_filter("Android app", &["apk"])
                    .pick_file()
                {
                    self.base_from_apk(apk, true);
                }
            }
        }
        if edit {
            self.go_home();
            self.edit_recipe(&path);
        }
    }
}
