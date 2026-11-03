//! Balance, survival, and menu choices.
use super::*;

impl Launcher {
    /// `--balance FILE`: the balance editor's changes, over the phone
    /// game's rules where they are taken (`balance-tuned.toml` in the data
    /// folder then).
    pub(super) fn balance_args(&self) -> Vec<String> {
        let edits = self
            .ui
            .balance
            .file()
            .map(|f| (f, self.ui.balance.edits.clone()));
        let tuned = self.phone_rules().map(|b| match &edits {
            Some((_, e)) => b.with(e),
            None => b,
        });
        let file = match tuned {
            Some(b) if !b.is_empty() => {
                let f = crate::config::data_dir().join("balance-tuned.toml");
                std::fs::write(&f, b.to_toml()).ok().map(|()| f)
            }
            _ => edits.map(|(f, _)| f),
        };
        match file {
            Some(f) => vec!["--balance".into(), f.display().to_string()],
            None => Vec::new(),
        }
    }

    /// The phone game's rules chosen for a PC game (`cfg.tuning`), from
    /// the APK import's values; `None` on a game built from the APK (its
    /// rules are the phone's already) or with none chosen.
    pub(super) fn phone_rules(&self) -> Option<c2_mods::balance::Balance> {
        let t = self.cfg.tuning;
        let (s, c, a) = (
            t.shotgun.mobile(false),
            t.scoring.mobile(false),
            t.animals.mobile(false),
        );
        if !(s || c || a) || self.game_is_phone() {
            return None;
        }
        Some(self.ui.balance.mobile.as_ref()?.phone_tuning(s, c, a))
    }

    /// The game is one built from the APK (the recipe's or the Game tab's).
    pub(super) fn game_is_phone(&self) -> bool {
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

    /// PC or phone rules (shotgun, scoring, animals' health).
    pub(super) fn tuning_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use c2_mods::recipe::Source;
        let phone = self.game_is_phone();
        let have = self.ui.balance.mobile.is_some() && !phone;
        let mut changed = false;
        let t = &mut self.cfg.tuning;
        ui.label(RichText::new("PC or phone rules").color(GOLD));
        for (name, s) in [
            ("Shotguns", &mut t.shotgun),
            ("Scores and prices", &mut t.scoring),
            ("Animals' health", &mut t.animals),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                ui.add_enabled_ui(have, |ui| {
                    changed |= ui.radio_value(s, Source::Auto, "Auto (PC)").changed();
                    changed |= ui.radio_value(s, Source::Pc, "PC").changed();
                    changed |= ui.radio_value(s, Source::Mobile, "Phone").changed();
                });
            });
        }
        help(
            ui,
            if phone {
                "This game is built from the APK: its rules are the phone's already."
            } else if have {
                "The phone's values, from your APK import, under any changes you make below. \
                 Applies to playing on PC, the 3DS and the Wii U."
            } else {
                "The phone's rules come with importing your APK (Add-ons tab)."
            },
        );
        changed
    }

    /// `--survival-modes ..`, and `--mobile-survival FILE` once an APK
    /// import has made the file.
    pub(super) fn survival_args(&self) -> Vec<String> {
        let mut v = vec!["--survival-modes".into(), self.cfg.survival_modes.arg()];
        if let Some(f) = self.cfg.mobile_survival.as_ref().filter(|f| f.exists()) {
            v.extend(["--mobile-survival".into(), f.display().to_string()]);
        }
        v
    }

    /// `c2-cook`'s survival arguments, and the phone's pictures where
    /// chosen (`--phone-game`, `--phone-pictures`; the 3DS and Wii U).
    pub(super) fn cook_extras(&self) -> Vec<String> {
        let mut v = self.survival_args();
        let Some(phone) = self.phone_game() else {
            return v;
        };
        let p = self.cfg.pictures;
        let want: Vec<&str> = [
            ("weapons", p.weapons),
            ("animals", p.animals),
            ("scope", p.scope),
        ]
        .iter()
        .filter(|(_, s)| s.mobile(true))
        .map(|(k, _)| *k)
        .collect();
        if !want.is_empty() {
            v.extend([
                "--phone-game".to_string(),
                phone.display().to_string(),
                "--phone-pictures".into(),
                want.join(","),
            ]);
        }
        v
    }

    /// The game built from the player's APK whose pictures the consoles
    /// can take: the recipe's or the Game tab's own game if it is one, else
    /// one built before (Ice Age's for an Ice Age game, else Dinosaur
    /// Hunter's).
    pub(super) fn phone_game(&self) -> Option<PathBuf> {
        let own = self
            .active_recipe()
            .and_then(|r| r.game.clone())
            .into_iter()
            .chain(self.cfg.base.clone());
        let built = crate::config::phone_games();
        let ice = |p: &PathBuf| p.to_string_lossy().to_ascii_lowercase().contains("ice");
        let wants_ice = self
            .cfg
            .base
            .as_deref()
            .is_some_and(c2_mods::ice_age::is_ice_age_dir);
        own.into_iter()
            .find(|g| crate::config::is_phone_game(g))
            .or_else(|| built.iter().find(|g| ice(g) == wants_ice).cloned())
            .or_else(|| built.first().cloned())
    }

    /// The 3DS and Wii U hunt pictures: the phone's or the game's own.
    pub(super) fn pictures_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use c2_mods::recipe::Source;
        let mut changed = false;
        let phone = self.phone_game();
        let p = &mut self.cfg.pictures;
        for (name, s) in [
            ("Weapon pictures", &mut p.weapons),
            ("Animal pictures and call icons", &mut p.animals),
            ("Sniper scope", &mut p.scope),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                ui.add_enabled_ui(phone.is_some(), |ui| {
                    changed |= ui.radio_value(s, Source::Auto, "Auto (phone)").changed();
                    changed |= ui.radio_value(s, Source::Mobile, "Phone").changed();
                });
                changed |= ui.radio_value(s, Source::Pc, "PC").changed();
            });
        }
        help(
            ui,
            &match &phone {
                Some(g) => format!(
                    "The phone's pictures come from {} (matched by model: animals and weapons \
                     both games have). The ammo counter stays as it is. On a game built from the \
                     APK, its pictures are the phone's already.",
                    g.display()
                ),
                None => "The phone's pictures need a game built from your APK (\"Use my APK as \
                         the game...\" on the Game tab, or an Android recipe)."
                    .into(),
            },
        );
        changed
    }

    /// Survival choice (PC and 3DS alike).
    pub(super) fn survival_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let have = self
            .cfg
            .mobile_survival
            .as_ref()
            .is_some_and(|f| f.exists());
        let mut changed = false;
        let m = &mut self.cfg.survival_modes;
        ui.horizontal(|ui| {
            ui.label("Survival:");
            changed |= ui
                .add_enabled(
                    have,
                    egui::Checkbox::new(&mut m.classic, "Classic (Android)"),
                )
                .changed();
            changed |= ui.checkbox(&mut m.modded, "Modded").changed();
        });
        help(
            ui,
            if have {
                "Classic: the mobile game's own Survive mode, read from your APK (its waves on \
                 Mount Ravan, straight in with the shotgun; on Ice Age, Ice Age animals stand \
                 in). Modded: a Modder's Edition game's own survival, where its data has one. \
                 With both, the game asks which when you pick Survival."
            } else {
                "Modded: a Modder's Edition game's own survival, where its data has one. The \
                 mobile game's classic Survive mode comes with importing your Carnivores 2 \
                 (Dinosaur Hunter) APK (Add-ons tab)."
            },
        );
        changed
    }

    /// `play <base> --overlay .. --mod .. [--balance ..]`
    pub(super) fn play_args(&self) -> Vec<String> {
        let mut args = vec!["play".to_string()];
        if let Some(r) = self.active_recipe() {
            let game = self.recipe_game_with_overlays(r);
            args.extend(game.first().map(|g| g.display().to_string()));
            for o in game.iter().skip(1) {
                args.extend(["--overlay".to_string(), o.display().to_string()]);
            }
            args.extend(["--recipe".to_string(), r.path.display().to_string()]);
            args.extend(r.mods_args());
        } else {
            if let Some(b) = &self.cfg.base {
                args.push(b.display().to_string());
            }
            for o in &self.cfg.overlays {
                args.push("--overlay".into());
                args.push(o.display().to_string());
            }
            for s in self.cfg.enabled_standalones() {
                args.push("--standalone".into());
                args.push(s.display().to_string());
            }
            for a in self.cfg.enabled_addons() {
                args.push("--mod".into());
                args.push(a.display().to_string());
            }
        }
        args.extend(self.balance_args());
        args.extend(self.survival_args());
        let art = match self.cfg.menu_style {
            MenuStyle::Original => self.cfg.base.clone(),
            MenuStyle::IceAge => self.cfg.ice_age.clone(),
            MenuStyle::Mod => None,
            MenuStyle::Mobile => {
                args.push("--mobile-menu".into());
                None
            }
        };
        if let Some(a) = art {
            args.push("--menu-art".into());
            args.push(a.display().to_string());
        }
        args
    }

    /// Menu style choice (PC: four; the 3DS: the title art).
    pub(super) fn menu_style_ui(&mut self, ui: &mut egui::Ui, three_ds: bool) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Menu:");
            if three_ds {
                let t = &mut self.cfg.three_ds;
                changed |= ui
                    .radio_value(&mut t.ice_age_title, false, "Carnivores 2")
                    .changed();
                changed |= ui
                    .radio_value(&mut t.ice_age_title, true, "Ice Age")
                    .changed();
            } else {
                for (s, name) in [
                    (MenuStyle::Original, "Original"),
                    (MenuStyle::Mod, "Mod"),
                    (MenuStyle::IceAge, "Ice Age"),
                    (MenuStyle::Mobile, "Experimental mobile layout"),
                ] {
                    changed |= ui.radio_value(&mut self.cfg.menu_style, s, name).changed();
                }
            }
        });
        let wants_ia = if three_ds {
            self.cfg.three_ds.ice_age_title
        } else {
            self.cfg.menu_style == MenuStyle::IceAge
        };
        if wants_ia {
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
        }
        changed
    }

    pub(super) fn balance_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Balance");
        help(
            ui,
            "Prices and scores of the game as set up (base, overlays and the add-ons that are \
             on). Change any of them, to taste or to make add-ons fit together; the game's own \
             files are never changed. Applies to playing on PC and to the 3DS add-ons pack (or \
             the original game pack when there are no add-ons).",
        );
        ui.add_space(6.0);
        if panel(ui, |ui| self.tuning_ui(ui)) {
            self.save();
        }
        ui.add_space(6.0);
        let (bases, mods) = self.game_folders();
        if let Some(e) = self.ui.balance.ui(ui, &bases, &mods, &mut self.job) {
            self.ui.note = Some((e, RED));
        }
    }
}
