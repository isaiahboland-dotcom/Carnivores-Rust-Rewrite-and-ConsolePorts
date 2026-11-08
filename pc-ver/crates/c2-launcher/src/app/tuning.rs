//! Balance and menu choices.
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
                "The phone's rules come with importing animals from your APK (Add-ons page)."
            },
        );
        changed
    }

    /// `c2-cook`'s phone pictures where chosen (`--phone-game`,
    /// `--phone-pictures`; the 3DS and Wii U).
    pub(super) fn cook_extras(&self) -> Vec<String> {
        let mut v = Vec::new();
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
            .into_iter();
        let built = crate::config::phone_games();
        let ice = |p: &PathBuf| p.to_string_lossy().to_ascii_lowercase().contains("ice");
        let wants_ice = self
            .active_recipe()
            .and_then(|r| r.game.as_deref())
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
                None => "The phone's pictures need a game built from your APK (\"Build from my \
                         APK...\" on an instance's Base game page, or an Android recipe)."
                    .into(),
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
            args.extend(self.cfg.accept_base_args(r));
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
            args.extend(self.cfg.standalone_skips());
            for a in self.cfg.enabled_addons() {
                args.push("--mod".into());
                args.push(a.display().to_string());
            }
        }
        args.extend(self.balance_args());
        args.extend(self.menu_args());
        args
    }
}
