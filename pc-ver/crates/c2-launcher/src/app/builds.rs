//! Construct platform build steps from the selected configuration.
use super::*;

impl Launcher {
    pub(super) fn build_3ds(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let tools = self.cfg.tools.clone();
        let (Some(cook), Some(app)) = (
            find_tool("c2-cook", tools.as_deref()),
            find_tool("c2-3ds.3dsx", tools.as_deref()),
        ) else {
            self.ui.note = Some((
                "c2-cook or c2-3ds.3dsx not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let recipe = self.active_recipe().cloned();
        let recipe_game = recipe.as_ref().map(|r| self.recipe_game_with_overlays(r));
        let base = match &recipe_game {
            Some(g) => g.first().cloned(),
            None => None,
        };
        let (Some(base), Some(out)) = (base, self.cfg.three_ds.out.clone()) else {
            self.ui.note = Some(("Set the game folder and the SD card first.".into(), RED));
            return;
        };
        let t = &self.cfg.three_ds;
        let name = install_name(&t.name);
        let dir = out.join("3ds").join(&name);
        let mut steps = Vec::new();
        let title_art = t.ice_age_title.then(|| self.cfg.ice_age.clone()).flatten();
        let extras = self.cook_extras();
        let cook_args = |extra: Vec<String>, pack: &str| -> Vec<String> {
            let mut a = vec!["--game".to_string(), base.display().to_string()];
            a.extend(extra);
            a.extend(extras.clone());
            if let Some(ia) = &title_art {
                a.extend(["--menu-art".to_string(), ia.display().to_string()]);
            }
            a.extend([
                "--target".into(),
                t.target.arg().into(),
                "--out".into(),
                dir.join(pack).display().to_string(),
            ]);
            a
        };
        let addons = self.cfg.enabled_addons();
        let modded = !addons.is_empty() || !self.cfg.overlays.is_empty();
        // The balance goes with the game it was made for: the add-ons pack,
        // or the original game's when there are no add-ons.
        if t.vanilla_pack {
            let extra = if modded {
                Vec::new()
            } else {
                self.balance_args()
            };
            steps.push(Step::Run {
                program: cook.clone(),
                args: cook_args(extra, "c2rs.c2pk"),
            });
        }
        if let (Some(r), Some(game)) = (&recipe, &recipe_game) {
            // The recipe's pack in place of the add-ons and standalone ones.
            if t.addons_pack {
                let id = install_name(&r.id());
                let input = match crate::console::RecipeInput::new(
                    r,
                    game,
                    &crate::config::data_dir().join("recipes"),
                    self.cfg.base_accepted(&r.path, r.game.as_deref()),
                ) {
                    Ok(input) => input,
                    Err(e) => {
                        self.ui.note = Some((e, RED));
                        return;
                    }
                };
                match input.prepare(find_tool("c2-pc", tools.as_deref())) {
                    Ok(prepare) => steps.extend(prepare),
                    Err(e) => {
                        self.ui.note = Some((e, RED));
                        return;
                    }
                }
                let mut extra = input.extra.clone();
                extra.extend(self.balance_args());
                let mut a = cook_args(extra, &format!("{id}.c2pk"));
                a[1] = input.game.display().to_string();
                steps.push(Step::Run {
                    program: cook.clone(),
                    args: a,
                });
            }
        } else if t.addons_pack && modded {
            let mut extra = self.balance_args();
            for o in &self.cfg.overlays {
                extra.push("--overlay".into());
                extra.push(o.display().to_string());
            }
            for a in addons {
                extra.push("--mod".into());
                extra.push(a.display().to_string());
            }
            steps.push(Step::Run {
                program: cook.clone(),
                args: cook_args(extra, "modded.c2pk"),
            });
        }
        // A pack per standalone, each its own game (as on PC played alone).
        if t.standalone_packs && recipe.is_none() {
            for s in self.cfg.enabled_standalones() {
                let Some(root) = c2_mods::pack::find_game_root(&s) else {
                    continue;
                };
                let pack = format!(
                    "{}.c2pk",
                    c2_mods::manifest::slug(
                        &s.file_name()
                            .map_or_else(|| "game".into(), |n| n.to_string_lossy().into_owned())
                    )
                );
                let mut a = cook_args(Vec::new(), &pack);
                if let Some(i) = a.iter().position(|x| x == "--game") {
                    a[i + 1] = root.display().to_string();
                }
                steps.push(Step::Run {
                    program: cook.clone(),
                    args: a,
                });
            }
        }
        steps.push(Step::Copy {
            from: app,
            to: dir.join(format!("{name}.3dsx")),
        });
        if t.cia {
            let (Some(elf), Some(makerom), Some(bannertool)) = (
                find_tool("c2-3ds.elf", tools.as_deref()),
                find_tool("makerom", tools.as_deref()),
                find_tool("bannertool", tools.as_deref()),
            ) else {
                self.ui.note = Some((
                    "For a .cia: c2-3ds.elf, makerom and bannertool are needed (see the 3DS tab)."
                        .into(),
                    RED,
                ));
                return;
            };
            steps.extend(crate::cia::steps(&crate::cia::Cia {
                name: &name,
                target: t.target,
                elf,
                makerom,
                bannertool,
                icon: t.icon.as_deref(),
                banner: t.banner.as_deref(),
                sound: t.banner_sound.as_deref(),
                art_from: match &t.art_from {
                    crate::config::ArtFrom::Base => Some(base.as_path()),
                    crate::config::ArtFrom::Game(g) => Some(g.as_path()),
                    crate::config::ArtFrom::Plain => None,
                },
                work: crate::config::data_dir().join("cia").join(&name),
                out: out.join("cia").join(format!("{name}.cia")),
            }));
        }
        self.start_job(format!("Building the 3DS install '{name}'"), steps);
    }

    /// The base game's files and the Wii U app onto the SD card
    /// (`crate::wiiu`).
    /// The Wii U installs the tab's choices make: (folder, title, `c2-cook`
    /// arguments). Both formats support separate named installs.
    pub(super) fn wiiu_installs(&self) -> Vec<(String, String, Vec<String>)> {
        self.wiiu_default_installs()
            .into_iter()
            .map(|(key, title, args)| {
                let title = self
                    .cfg
                    .wiiu
                    .names
                    .get(&key)
                    .filter(|n| !n.trim().is_empty())
                    .map(|n| n.trim().to_string())
                    .unwrap_or(title);
                let name = crate::wiiu::folder_name(
                    &key,
                    self.cfg.wiiu.names.get(&key).map(String::as_str),
                );
                (name, title, args)
            })
            .collect()
    }

    pub(super) fn wiiu_default_installs(&self) -> Vec<(String, String, Vec<String>)> {
        if let Some(r) = self.active_recipe() {
            if !self.cfg.wiiu.addons {
                return Vec::new();
            }
            let games = self.recipe_game_with_overlays(r);
            let Ok(input) = crate::console::RecipeInput::new(
                r,
                &games,
                &crate::config::data_dir().join("recipes"),
                self.cfg.base_accepted(&r.path, r.game.as_deref()),
            ) else {
                return Vec::new();
            };
            let mut args = input.cook_args();
            args.extend(self.balance_args());
            args.extend(self.cook_extras());
            let title = c2_mods::recipe::Recipe::load(&r.path)
                .map(|r| r.recipe.name)
                .unwrap_or_else(|_| r.id());
            return vec![(install_name(&r.id()), title, args)];
        }
        let Some(base) = self.cfg.base.clone() else {
            return Vec::new();
        };
        let w = &self.cfg.wiiu;
        let game = |root: &std::path::Path| vec!["--game".to_string(), root.display().to_string()];
        let extras = self.cook_extras();
        let addons = self.cfg.enabled_addons();
        let modded = !addons.is_empty() || !self.cfg.overlays.is_empty();
        let (folder, title) = crate::config::base_install(&base);
        let mut v = Vec::new();
        if w.original {
            let mut a = game(&base);
            a.extend(extras.clone());
            if !modded {
                a.extend(self.balance_args());
            }
            v.push((folder.to_string(), title.to_string(), a));
        }
        if w.addons && modded {
            let mut a = game(&base);
            for o in &self.cfg.overlays {
                a.extend(["--overlay".to_string(), o.display().to_string()]);
            }
            for m in addons {
                a.extend(["--mod".to_string(), m.display().to_string()]);
            }
            a.extend(self.balance_args());
            a.extend(extras.clone());
            v.push((format!("{folder}-addons"), format!("{title} Add-ons"), a));
        }
        if w.standalones {
            for s in self.cfg.enabled_standalones() {
                let Some(root) = c2_mods::pack::find_game_root(&s) else {
                    continue;
                };
                let title = s
                    .file_name()
                    .map_or_else(|| "Carnivores".into(), |n| n.to_string_lossy().into_owned());
                let mut a = game(&root);
                a.extend(extras.clone());
                v.push((install_name(&c2_mods::manifest::slug(&title)), title, a));
            }
        }
        v
    }

    pub(super) fn build_wiiu(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let tools = self.cfg.tools.clone();
        let (Some(cook), Some(app)) = (
            find_tool("c2-cook", tools.as_deref()),
            find_tool(crate::wiiu::APP, tools.as_deref()),
        ) else {
            self.ui.note = Some((
                format!(
                    "c2-cook or {} not found: set the tools folder in Settings.",
                    crate::wiiu::APP
                ),
                RED,
            ));
            return;
        };
        let Some(out) = self.cfg.wiiu.out.clone() else {
            return;
        };
        let wuhbtool = if self.cfg.wiiu.format == crate::config::WiiUFormat::Wuhb {
            match find_tool(crate::wiiu::WUHB_TOOL, tools.as_deref()) {
                Some(tool) => Some(tool),
                None => {
                    self.ui.note = Some((
                        "WUHB output needs wuhbtool from devkitPro wut-tools (install it or set the tools folder in Settings).".into(),
                        RED,
                    ));
                    return;
                }
            }
        } else {
            None
        };
        let mut steps = Vec::new();
        if let Some(r) = self.active_recipe().filter(|_| self.cfg.wiiu.addons) {
            let games = self.recipe_game_with_overlays(r);
            let input = match crate::console::RecipeInput::new(
                r,
                &games,
                &crate::config::data_dir().join("recipes"),
                self.cfg.base_accepted(&r.path, r.game.as_deref()),
            ) {
                Ok(input) => input,
                Err(e) => {
                    self.ui.note = Some((e, RED));
                    return;
                }
            };
            match input.prepare(find_tool("c2-pc", tools.as_deref())) {
                Ok(prepare) => steps.extend(prepare),
                Err(e) => {
                    self.ui.note = Some((e, RED));
                    return;
                }
            }
        }
        for (name, title, cook_args) in self.wiiu_installs() {
            let art = crate::wiiu::art_source(&cook_args);
            let own = self.cfg.wiiu.art.get(&name).cloned().unwrap_or_default();
            let cook = cook.clone();
            let app = app.clone();
            let wuhbtool = wuhbtool.clone();
            let out = out.clone();
            // Read generated menu art only after the recipe merge has finished.
            steps.push(Step::Task(Box::new(move |job| {
                for step in crate::wiiu::steps(
                    &crate::wiiu::Install {
                        name: &name,
                        title: &title,
                        cook_args,
                        art_from: art.as_deref(),
                        own_art: [own.icon.as_deref(), own.tv.as_deref(), own.drc.as_deref()],
                    },
                    &crate::wiiu::Tools {
                        cook: &cook,
                        app: &app,
                        wuhbtool: wuhbtool.as_deref(),
                    },
                    &out,
                    &crate::config::data_dir().join("wiiu").join(&name),
                ) {
                    job.run(step)?;
                }
                Ok(())
            })));
        }
        self.start_job("Building the Wii U installs", steps);
    }

    pub(super) fn build_windows(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let result = (|| -> Result<Vec<Step>, String> {
            let r = self
                .active_recipe()
                .ok_or("Choose a saved instance first.")?;
            let pc = find_tool("c2-pc", self.cfg.tools.as_deref())
                .ok_or("c2-pc not found in the tools folder.")?;
            let exe = find_tool("c2-pc.exe", self.cfg.tools.as_deref()).ok_or("Windows c2-pc.exe not found. Download a Windows engine build or build it on Windows, then set the tools folder in Settings.")?;
            let games = self.recipe_game_with_overlays(r);
            let base = games.first().ok_or("Set the instance's base game first.")?;
            let work = crate::config::data_dir()
                .join("windows")
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
                .windows_out
                .as_ref()
                .ok_or("Choose an output folder.")?
                .join(install_name(&r.id()));
            let mut launch = self.balance_args();
            launch.extend(["--save-instance".into(), r.id()]);
            launch.extend(self.menu_args());
            crate::windows::validate_exe(&exe)?;
            Ok(vec![
                Step::Run { program: pc, args },
                Step::Task(Box::new(move |job| {
                    crate::windows::package(&work, &exe, &out, launch)
                        .map_err(|e| e.to_string())?;
                    job.log(format!("Windows package ready: {}", out.display()));
                    Ok(())
                })),
            ])
        })();
        match result {
            Ok(steps) => self.start_job("Building Windows package", steps),
            Err(e) => self.ui.note = Some((e, RED)),
        }
    }
}
