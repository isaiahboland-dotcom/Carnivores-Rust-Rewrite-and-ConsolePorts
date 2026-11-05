//! The PC menu choice: the original's, the mod's, Ice Age's, Android's
//! (a game built from an APK) or any standalone game's own (owner request
//! 2026-11-05: an instance with several standalones and/or an APK picks
//! whose menu screens it shows). The 3DS takes the title art only.
use super::*;

impl Launcher {
    /// The game played has a base built from an APK (with or without
    /// standalones beside it).
    pub(super) fn has_phone_base(&self) -> bool {
        match self.active_recipe() {
            Some(r) => self
                .recipe_game(r)
                .first()
                .is_some_and(|g| crate::config::is_phone_game(g)),
            None => self
                .cfg
                .base
                .as_deref()
                .is_some_and(crate::config::is_phone_game),
        }
    }

    /// The standalone games played, each `(name, game folder)`: the
    /// recipe's (by entry id) or the Add-ons tab's that are on.
    pub(super) fn menu_games(&self) -> Vec<(String, PathBuf)> {
        let root = |p: &Path| c2_mods::pack::find_game_root(p).unwrap_or_else(|| p.to_path_buf());
        match self.active_recipe() {
            Some(u) => c2_mods::recipe::Recipe::load(&u.path)
                .map(|r| {
                    r.standalones
                        .iter()
                        .filter_map(|m| Some((m.id.clone(), root(u.folders.get(&m.id)?))))
                        .collect()
                })
                .unwrap_or_default(),
            None => self
                .cfg
                .standalones
                .iter()
                .filter(|a| a.enabled)
                .map(|a| (a.name(), root(&a.path)))
                .collect(),
        }
    }

    /// The menu the game is played with. A phone game on its own has the
    /// phone's menus only; beside standalones it has Android's, Ice Age's
    /// or a standalone's (its own folder has no PC frames). Android needs
    /// the APK's pictures: without a phone base it is the mod's.
    pub(super) fn menu_style(&self) -> MenuStyle {
        if self.plays_phone_game() {
            return MenuStyle::Mobile;
        }
        let phone = self.has_phone_base();
        let fallback = if phone {
            MenuStyle::Mobile
        } else {
            MenuStyle::Mod
        };
        match self.cfg.menu_style {
            MenuStyle::Mobile if !phone => MenuStyle::Mod,
            MenuStyle::Original | MenuStyle::Mod if phone => MenuStyle::Mobile,
            MenuStyle::Game if self.menu_game_dir().is_none() => fallback,
            s => s,
        }
    }

    /// The standalone whose menu is picked, if it is played.
    fn menu_game_dir(&self) -> Option<PathBuf> {
        let want = self.cfg.menu_game.as_deref()?;
        self.menu_games()
            .into_iter()
            .find(|(n, _)| n == want)
            .map(|(_, d)| d)
    }

    /// `c2-pc` arguments for the menu (`--menu-art DIR`, `--mobile-menu`).
    pub(super) fn menu_args(&self) -> Vec<String> {
        let art = match self.menu_style() {
            MenuStyle::Original => self.cfg.base.clone(),
            MenuStyle::IceAge => self.cfg.ice_age.clone(),
            MenuStyle::Game => self.menu_game_dir(),
            MenuStyle::Mod => None,
            MenuStyle::Mobile => return vec!["--mobile-menu".into()],
        };
        art.map(|a| vec!["--menu-art".into(), a.display().to_string()])
            .unwrap_or_default()
    }

    /// Menu style choice (PC: the game's menus; the 3DS: the title art).
    pub(super) fn menu_style_ui(&mut self, ui: &mut egui::Ui, three_ds: bool) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Menu:");
            if three_ds {
                let t = &mut self.cfg.three_ds;
                changed |= ui
                    .radio_value(&mut t.ice_age_title, false, "Carnivores 2")
                    .changed();
                changed |= ui
                    .radio_value(&mut t.ice_age_title, true, "Ice Age")
                    .changed();
            } else if self.plays_phone_game() {
                ui.label(
                    RichText::new("Android (a game built from an APK has the phone's menus only)")
                        .color(SAND),
                );
            } else {
                changed |= self.pc_menu_choices(ui);
            }
        });
        let wants_ia = if three_ds {
            self.cfg.three_ds.ice_age_title
        } else {
            self.menu_style() == MenuStyle::IceAge
        };
        if wants_ia {
            changed |= self.ice_age_folder_ui(ui);
        }
        changed
    }

    fn pc_menu_choices(&mut self, ui: &mut egui::Ui) -> bool {
        let phone = self.has_phone_base();
        let mut choices: Vec<(MenuStyle, Option<String>, String)> = Vec::new();
        if phone {
            choices.push((MenuStyle::Mobile, None, "Android".into()));
        } else {
            choices.push((MenuStyle::Original, None, "Original".into()));
            choices.push((MenuStyle::Mod, None, "Mod".into()));
        }
        choices.push((MenuStyle::IceAge, None, "Ice Age".into()));
        for (name, _) in self.menu_games() {
            choices.push((MenuStyle::Game, Some(name.clone()), name));
        }
        let now = self.menu_style();
        let mut changed = false;
        for (style, game, label) in choices {
            let on = now == style && (style != MenuStyle::Game || self.cfg.menu_game == game);
            let hover = match style {
                MenuStyle::Game => "This standalone game's own menu screens.",
                MenuStyle::Mobile => "The phone game's menus (from your APK).",
                _ => "",
            };
            let r = ui.radio(on, label);
            let r = if hover.is_empty() {
                r
            } else {
                r.on_hover_text(hover)
            };
            if r.clicked() && !on {
                self.cfg.menu_style = style;
                if game.is_some() {
                    self.cfg.menu_game = game;
                }
                changed = true;
            }
        }
        if !phone {
            ui.add_enabled(false, egui::RadioButton::new(false, "Android"))
                .on_disabled_hover_text(
                    "The Android menus need a game built from your APK as the base \
                     (its call, binocular, map and menu pictures).",
                );
        }
        changed
    }

    fn ice_age_folder_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let shown = self
                .cfg
                .ice_age
                .as_ref()
                .map_or("Ice Age folder: not set".to_string(), |p| {
                    format!("Ice Age folder: {}", p.display())
                });
            let ok = self
                .cfg
                .ice_age
                .as_deref()
                .is_some_and(|p| describe_game(p).is_some());
            ui.label(RichText::new(shown).color(if ok { SAND } else { RED }));
            if ui.button("Browse...").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.cfg.ice_age = Some(p);
                    changed = true;
                }
            }
        });
        help(
            ui,
            "Your own Carnivores: Ice Age folder (the one with HUNTDAT); its menu art only.",
        );
        changed
    }
}
