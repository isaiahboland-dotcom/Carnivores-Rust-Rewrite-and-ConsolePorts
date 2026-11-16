//! Android packaging and install options.
use super::*;

impl Launcher {
    pub(super) fn android_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Android package");
        self.instance_build_heading(ui);
        help(
            ui,
            "Builds a saved instance for the Android app: the engine APK and the instance's game, \
             laid out as the app's files folder. The instance's base game must be built from your \
             own mobile APK (Dinosaur Hunter or Ice Age). Supply c2rs.apk (android-ver/build-apk.sh) \
             in the tools folder (Settings); c2-pc is also needed to merge the instance.",
        );
        ui.add_space(6.0);
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Output folder:");
                let shown = self
                    .cfg
                    .android_out
                    .as_ref()
                    .map_or("not set".to_string(), |p| p.display().to_string());
                ui.label(RichText::new(shown).color(SAND));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.android_out = Some(p);
                        self.save();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Name on the phone:");
                let edit = egui::TextEdit::singleline(&mut self.cfg.android_name)
                    .hint_text(crate::android::LABEL)
                    .desired_width(200.0);
                if ui.add(edit).changed() {
                    self.save();
                }
                let package = crate::android::app_package(&self.cfg.android_name);
                ui.label(RichText::new(format!("-> {package}")).color(DIM));
            });
            if let Some(other) = self.android_clash() {
                ui.label(RichText::new(other).color(RED));
            }
            help(
                ui,
                "Empty: the shared c2rs app (each install replaces the last). A name makes the \
                 instance an app of its own, installed beside the others with its own saves; \
                 changing the name later makes another app. Needs the Android SDK's build \
                 tools (the APK is relabelled and signed with the debug key).",
            );
        });
        ui.add_space(8.0);
        let problem = self
            .active_recipe()
            .and_then(|r| self.android_base_problem(r));
        if let Some(p) = &problem {
            let mut change = false;
            panel(ui, |ui| {
                ui.label(RichText::new(p).color(RED));
                change = ui.button("Change the base game…").clicked();
            });
            if change {
                if let Some(path) = self.active_recipe().map(|r| r.path.clone()) {
                    self.ui.build_for = None;
                    self.edit_recipe(&path);
                }
                return;
            }
            ui.add_space(8.0);
        }
        let ready = !self.busy()
            && self.active_recipe().is_some()
            && self.cfg.android_out.is_some()
            && problem.is_none();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ready, egui::Button::new("Build Android package"))
                .clicked()
            {
                self.build_android();
            }
            if ui
                .add_enabled(ready, egui::Button::new("Install over adb"))
                .on_hover_text(
                    "Installs the last Android package built for this instance on the connected \
                     device. It does not rebuild the package.",
                )
                .clicked()
            {
                self.ui.adb_confirm = true;
            }
        });
        self.adb_confirm(ui.ctx());
        self.engine_confirm(ui.ctx());
        help(
            ui,
            &format!(
                "The output contains a folder named after the instance, with c2rs.apk and files/. \
                 Rebuilding replaces that generated folder. By hand: install c2rs.apk, start it \
                 once, then copy the contents of files/ to Android/data/{}/files/ on the device. \
                 Install over adb (USB debugging on) installs that folder as last built: build \
                 the Android package first after changing the instance. The game is pushed beside \
                 the old one and swapped in when complete; saves and other folders there are kept.",
                crate::android::app_package(&self.cfg.android_name)
            ),
        );
    }

    /// The confirmation before installing: the last built package goes to
    /// the device as it is.
    fn adb_confirm(&mut self, ctx: &egui::Context) {
        if !self.ui.adb_confirm {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("android-install-modal")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new("Install the last built Android package?")
                    .color(SAND)
                    .strong(),
            );
            help(
                ui,
                "This installs the package already built for this instance. It is not rebuilt: \
                 if the instance, its base game, add-ons or balance changed since, use Build \
                 Android package first. Saves and other folders on the device are kept.",
            );
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Install").color(GOLD)).clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(go) = answer {
            self.ui.adb_confirm = false;
            if go {
                self.install_android();
            }
        }
    }

    /// Why `r` can't be built for Android (its base game isn't one built
    /// from a mobile APK), if it can't.
    pub(super) fn android_base_problem(&self, r: &RecipeUse) -> Option<String> {
        let games = self.recipe_game_with_overlays(r);
        let Some(base) = games.first() else {
            return Some(
                "This instance has no base game yet. Set one built from your mobile \
                         APK on its Base game page."
                    .into(),
            );
        };
        crate::android::validate_base(base).err().map(|_| {
            let name = base.file_name().map_or_else(
                || base.display().to_string(),
                |n| n.to_string_lossy().into(),
            );
            format!(
                "The base game ({name}) is not built from a mobile APK, so the Android app \
                 can't use it. Change the instance's base game to your Dinosaur Hunter or Ice \
                 Age APK (Base game > Use my APK as the game), or build it for another platform."
            )
        })
    }

    /// Why installing would replace another instance's app: its name on
    /// the phone gives the same package as this one's.
    fn android_clash(&self) -> Option<String> {
        let own = &self.active_recipe()?.path;
        let package = crate::android::app_package(&self.cfg.android_name);
        let other = crate::android::package_clash(&package, own, &self.cfg.instance_builds)?;
        let name = other.file_stem().map_or_else(
            || other.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        Some(format!(
            "The instance {name} is also the app {package}: installing this one would replace \
             it. Give this one a different name on the phone."
        ))
    }

    pub(super) fn build_android(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        if let Some(root) = self.stale_engine() {
            self.ui.engine_confirm = Some(root);
            return;
        }
        self.start_android_build(None);
    }

    /// The source tree to rebuild the engine APK from, when the one the
    /// package would carry is older than the engine source.
    fn stale_engine(&self) -> Option<PathBuf> {
        if !cfg!(unix) {
            return None;
        }
        let apk = find_tool(crate::android::APK, self.cfg.tools.as_deref())?;
        let root = crate::android::engine_source_root(&apk)?;
        crate::android::engine_is_stale(&root, &apk).then_some(root)
    }

    /// Builds the package; with `rebuild` (a source tree) the engine APK is
    /// rebuilt first by `android-ver/build-apk.sh`.
    fn start_android_build(&mut self, rebuild: Option<PathBuf>) {
        match self.android_steps() {
            Ok(mut steps) => {
                if let Some(root) = &rebuild {
                    steps.insert(
                        0,
                        Step::Run {
                            program: "bash".into(),
                            args: vec![root.join("android-ver/build-apk.sh").display().to_string()],
                        },
                    );
                }
                self.start_job("Building Android package", steps);
            }
            Err(e) => self.ui.note = Some((e, RED)),
        }
    }

    /// The question before building with an engine APK older than its
    /// source: rebuild it first, build with it as it is, or stop.
    fn engine_confirm(&mut self, ctx: &egui::Context) {
        let Some(root) = self.ui.engine_confirm.clone() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("android-engine-modal")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(
                RichText::new("The engine APK is older than the engine source")
                    .color(SAND)
                    .strong(),
            );
            help(
                ui,
                "Engine code changed after c2rs.apk was built, so the app would not have those \
                 changes (an old engine with a new game folder behaves like a bug in the new \
                 code). Rebuilding takes a few minutes and needs the Android SDK and NDK.",
            );
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Rebuild engine, then build").color(GOLD))
                    .clicked()
                {
                    answer = Some(Some(true));
                }
                if ui.button("Build with the old engine").clicked() {
                    answer = Some(Some(false));
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(None);
                }
            });
        });
        if let Some(choice) = answer {
            self.ui.engine_confirm = None;
            if let Some(rebuild) = choice {
                self.start_android_build(rebuild.then_some(root));
            }
        }
    }

    /// Installs the package built last (it is not rebuilt here).
    pub(super) fn install_android(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        match self.android_install_steps() {
            Ok(steps) => self.start_job("Installing for Android", steps),
            Err(e) => self.ui.note = Some((e, RED)),
        }
    }

    fn android_install_steps(&self) -> Result<Vec<Step>, String> {
        let r = self
            .active_recipe()
            .ok_or("Choose a saved instance first.")?;
        let tools = self.cfg.tools.as_deref();
        if let Some(clash) = self.android_clash() {
            return Err(clash);
        }
        let adb = crate::android::find_adb(tools).ok_or(
            "adb not found: install the Android SDK platform tools or put adb in the tools \
             folder.",
        )?;
        let out = self
            .cfg
            .android_out
            .as_ref()
            .ok_or("Choose an output folder.")?
            .join(install_name(&r.id()));
        if !out.join(crate::android::APK).is_file() {
            return Err(
                "No Android package for this instance yet: use Build Android package first.".into(),
            );
        }
        let app = crate::android::app_package(&self.cfg.android_name);
        Ok(crate::android::install_steps(&adb, &out, &app))
    }

    fn android_steps(&self) -> Result<Vec<Step>, String> {
        let r = self
            .active_recipe()
            .ok_or("Choose a saved instance first.")?;
        let tools = self.cfg.tools.as_deref();
        let pc = find_tool("c2-pc", tools).ok_or("c2-pc not found in the tools folder.")?;
        let apk = find_tool(crate::android::APK, tools).ok_or(
            "c2rs.apk not found. Build it with android-ver/build-apk.sh, then set the tools \
             folder in Settings.",
        )?;
        crate::android::validate_apk(&apk)?;
        let games = self.recipe_game_with_overlays(r);
        let base = games
            .first()
            .ok_or("Set the instance's base game first.")?
            .clone();
        crate::android::validate_base(&base)?;
        let work = crate::config::data_dir()
            .join("android")
            .join(install_name(&r.id()));
        let mut args = vec![
            "recipe".into(),
            "build".into(),
            r.path.display().to_string(),
            "--base".into(),
            base.display().to_string(),
        ];
        for overlay in games.iter().skip(1) {
            args.extend(["--overlay".into(), overlay.display().to_string()]);
        }
        args.extend(r.mods_args());
        args.extend(self.cfg.accept_base_args(r));
        args.extend(["--out".into(), work.display().to_string()]);
        let out = self
            .cfg
            .android_out
            .as_ref()
            .ok_or("Choose an output folder.")?
            .join(install_name(&r.id()));
        // The app takes no arguments: the balance goes beside the game.
        let balance = self
            .balance_args()
            .windows(2)
            .find(|p| p[0] == "--balance")
            .map(|p| PathBuf::from(&p[1]));
        // The APK is relabelled and given the game's launcher icon: as an
        // app of its own when named, else as the shared app. Without the
        // SDK's tools only a name needs them.
        let name = self.cfg.android_name.trim().to_string();
        let named = !name.is_empty();
        let app = crate::android::app_package(&name);
        let label = if named {
            name
        } else {
            crate::android::LABEL.to_string()
        };
        let sdk = match crate::android::rename::find_sdk() {
            Ok(sdk) => Some(sdk),
            Err(e) if named => return Err(e),
            Err(_) => None,
        };
        let renamed = work.with_extension("apk-build");
        let app_id = app.clone();
        let steps = vec![
            Step::Run { program: pc, args },
            Step::Task(Box::new(move |job| {
                let apk = match &sdk {
                    Some(sdk) => {
                        let icon_path = work.join("HUNTDAT").join("MOBILE").join("ICON.PNG");
                        let icon = std::fs::read(&icon_path).ok();
                        if named {
                            job.log(format!("Naming the app \"{label}\" ({app_id})"));
                        }
                        if icon.is_none() {
                            job.log("No launcher icon in the game: the app keeps the default");
                        }
                        crate::android::rename::renamed_apk(
                            sdk,
                            &apk,
                            &label,
                            &app_id,
                            icon.as_deref(),
                            &renamed,
                        )?
                    }
                    None => {
                        job.log("Android SDK not found: the APK is used as it is (no icon)");
                        apk
                    }
                };
                crate::android::package(&work, &base, &apk, balance.as_deref(), &app_id, &out)
                    .map_err(|e| e.to_string())?;
                job.log(format!("Android package ready: {}", out.display()));
                Ok(())
            })),
        ];
        Ok(steps)
    }
}
