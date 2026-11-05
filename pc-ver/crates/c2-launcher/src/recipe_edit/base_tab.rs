//! The editor's Base game page: the instance's name, the game it is built
//! on (any Carnivores 2 game folder, one built from the player's APK, or a
//! standalone mod on its own), checked against the base profiles' hashes
//! so a wrong game is shown before it is played; the pinned overlays and
//! the base game's parts.

use super::*;
use crate::app::{help, panel};

/// What the Base game page asks of the launcher.
pub enum BaseAsk {
    Nothing,
    /// Build the game from the player's APK, for this instance.
    ChooseApk,
}

impl Editor {
    pub fn base_ui(&mut self, ui: &mut egui::Ui, busy: bool) -> BaseAsk {
        let mut ask = BaseAsk::Nothing;
        let r = &mut self.recipe;
        panel(ui, |ui| {
            ui.label(RichText::new("Instance").color(GOLD));
            egui::Grid::new("recipe-edit-info")
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut r.recipe.name);
                    ui.end_row();
                    ui.label("Id");
                    ui.text_edit_singleline(&mut r.recipe.id).on_hover_text(
                        "Lower case letters, digits and '-': the built game's folder name.",
                    );
                    ui.end_row();
                    ui.label("Version");
                    ui.text_edit_singleline(&mut r.recipe.version);
                    ui.end_row();
                });
        });
        ui.add_space(6.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Base game").color(GOLD));
            help(
                ui,
                "The game this instance is built on: your Carnivores 2 folder (the one with \
                 HUNTDAT in it), a game built from your Android APK, or a standalone mod (a \
                 whole game made from Carnivores 2) used as the base game on its own. Drop a \
                 game folder or an APK on the window, or:",
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Choose game folder...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.set_game(p);
                    }
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Build from my APK..."))
                    .on_hover_text(
                        "Carnivores: Dinosaur Hunter or Ice Age for Android: the game is built \
                         from your own APK into the data folder. It all stays on this computer.",
                    )
                    .clicked()
                {
                    ask = BaseAsk::ChooseApk;
                }
            });
            self.game_status_ui(ui);
        });
        ui.add_space(6.0);
        self.list_ui(ui, Kind::Overlay);
        if !self.draft.overlays.is_empty() {
            ui.add_space(6.0);
            self.loose_overlays_ui(ui);
        }
        let game = self.draft.game_dirs();
        if !game.is_empty() && self.recipe.standalones.is_empty() {
            ui.add_space(6.0);
            let (r, extras) = (&mut self.recipe, &mut self.extras);
            panel(ui, |ui| extras.base_parts_ui(ui, r, &game));
        }
        ask
    }

    /// The game folder, what it is, and whether it is the base the recipe
    /// expects (its hashes).
    fn game_status_ui(&mut self, ui: &mut egui::Ui) {
        let Some(game) = self.draft.game.clone() else {
            ui.label(RichText::new("No game folder chosen yet.").color(RED));
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(game.display().to_string()).color(SAND));
            match crate::config::describe_game(&game) {
                Some(kind) => ui.label(RichText::new(kind).color(TEXT)),
                None => ui.label(RichText::new("no HUNTDAT folder there").color(RED)),
            };
        });
        let expected = self.recipe.recipe.base.clone();
        let mut take: Option<String> = None;
        match &self.base_match {
            BaseMatch::Unchecked | BaseMatch::Checking(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    help(ui, "checking which game it is...");
                });
            }
            BaseMatch::Done(_, found) => match (found, expected.is_empty()) {
                (Some(id), _) if *id == expected => {
                    ui.label(RichText::new(format!("Matches the base game '{id}'.")).color(TEXT));
                }
                (Some(id), true) => {
                    ui.label(format!("This folder is the base game '{id}'."));
                    take = ui
                        .button(format!("Expect '{id}'"))
                        .on_hover_text("Play then checks the game is this one.")
                        .clicked()
                        .then(|| id.clone());
                }
                (Some(id), false) => {
                    ui.label(
                        RichText::new(format!(
                            "This folder is '{id}', but the instance expects '{expected}'. \
                             Choose the right folder, or make '{id}' the base."
                        ))
                        .color(RED),
                    );
                    take = ui
                        .button(format!("Make '{id}' the base"))
                        .clicked()
                        .then(|| id.clone());
                }
                (None, true) => help(ui, "Not a known base game: any game is accepted."),
                (None, false) => {
                    ui.label(
                        RichText::new(format!(
                            "This folder doesn't match the base game '{expected}' (another \
                             version, or another game). Play will ask before using it."
                        ))
                        .color(RED),
                    );
                }
            },
        }
        if let Some(id) = take {
            self.recipe.recipe.base = id;
        }
        ui.horizontal(|ui| {
            ui.label("Expected base game:");
            let r = &mut self.recipe;
            egui::ComboBox::from_id_salt("recipe-edit-base")
                .selected_text(if r.recipe.base.is_empty() {
                    "any game".to_string()
                } else {
                    r.recipe.base.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.recipe.base, String::new(), "any game");
                    for b in &self.bases {
                        ui.selectable_value(&mut r.recipe.base, b.clone(), b);
                    }
                });
        });
    }

    /// Overlays the instance was given as plain folders (the old Game
    /// tab's), not pinned in the recipe.
    fn loose_overlays_ui(&mut self, ui: &mut egui::Ui) {
        let mut remove = None;
        panel(ui, |ui| {
            ui.label(RichText::new("Unpinned overlays (from the old Game tab)").color(SAND));
            help(
                ui,
                "Copied over the base game for this computer only; add them as pinned overlays \
                 above to share them with the recipe.",
            );
            for (i, o) in self.draft.overlays.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(o.display().to_string());
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            self.draft.overlays.remove(i);
            self.refresh_list();
        }
    }
}
