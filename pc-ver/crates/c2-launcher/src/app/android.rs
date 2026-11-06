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
                .add_enabled(ready, egui::Button::new("Build and install over adb"))
                .on_hover_text(
                    "Installs the APK on the connected device and replaces the app's game \
                     folder. Close the game on the device first.",
                )
                .clicked()
            {
                self.install_android();
            }
        });
        help(
            ui,
            &format!(
                "The output contains a folder named after the instance, with c2rs.apk and files/. \
                 Rebuilding replaces that generated folder. By hand: install c2rs.apk, start it \
                 once, then copy the contents of files/ to Android/data/{}/files/ on the device. \
                 Over adb (USB debugging on), the game is pushed beside the old one and swapped \
                 in when complete; saves and other folders there are kept.",
                crate::android::PACKAGE
            ),
        );
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

    pub(super) fn build_android(&mut self) {
        self.start_android(false);
    }

    pub(super) fn install_android(&mut self) {
        self.start_android(true);
    }

    fn start_android(&mut self, install: bool) {
        if !self.can_start_operation() {
            return;
        }
        match self.android_steps(install) {
            Ok(steps) => {
                let title = if install {
                    "Building and installing for Android"
                } else {
                    "Building Android package"
                };
                self.start_job(title, steps);
            }
            Err(e) => self.ui.note = Some((e, RED)),
        }
    }

    fn android_steps(&self, install: bool) -> Result<Vec<Step>, String> {
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
        let adb = if install {
            Some(crate::android::find_adb(tools).ok_or(
                "adb not found: install the Android SDK platform tools or put adb in the tools \
                 folder.",
            )?)
        } else {
            None
        };
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
        let package = out.clone();
        let mut steps = vec![
            Step::Run { program: pc, args },
            Step::Task(Box::new(move |job| {
                crate::android::package(&work, &base, &apk, balance.as_deref(), &out)
                    .map_err(|e| e.to_string())?;
                job.log(format!("Android package ready: {}", out.display()));
                Ok(())
            })),
        ];
        if let Some(adb) = adb {
            steps.extend(crate::android::install_steps(&adb, &package));
        }
        Ok(steps)
    }
}
