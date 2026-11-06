//! The launcher's screens and entry points, without game data.
use super::*;

fn launcher() -> Launcher {
    Launcher {
        cfg: Config::default(),
        ui: View {
            mode: Mode::Full,
            tab: Tab::Instances,
            note: None,
            balance: Default::default(),
            recipe_page: None,
            recipe_edit: None,
            instances: Default::default(),
            build_for: None,
            fixing: None,
            dialog: None,
        },
        job: Default::default(),
    }
}

#[test]
fn all_entry_points_keep_the_current_job_and_pending_result() {
    let mut app = launcher();
    app.job
        .start_task("pending", |_| Ok(crate::jobs::Completion::Recipe(vec![])))
        .unwrap();
    app.import_apk("input.apk".into());
    app.import_ice_age("input".into());
    app.base_from_apk("input.apk".into(), true);
    app.add_addons(vec!["input.zip".into()]);
    app.play();
    app.play_vr();
    app.build_3ds();
    app.build_wiiu();
    app.build_windows();
    app.save_instance();
    app.open_recipe("missing.toml".into());
    app.edit_recipe(Path::new("missing.toml"));
    app.clone_recipe(Path::new("missing.toml"), "copy");
    app.dropped(vec!["missing.toml".into()]);
    assert_eq!(app.job.current().unwrap().title, "pending");
    assert!(app.busy());
    assert!(app.cfg.base.is_none());
    assert!(app.cfg.recipe.is_none());
    assert!(app.ui.recipe_page.is_none());
    assert!(app.ui.recipe_edit.is_none());
    assert!(app.ui.note.is_some());
}

#[test]
fn all_tabs_render_without_game_data() {
    let mut app = launcher();
    let ctx = egui::Context::default();
    for tab in [
        Tab::Instances,
        Tab::Settings,
        Tab::BaseGame,
        Tab::AddOns,
        Tab::Balance,
        Tab::Advanced,
        Tab::Build,
        Tab::Windows,
        Tab::ThreeDs,
        Tab::WiiU,
    ] {
        app.ui.tab = tab;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                app.top_bar(ui);
                app.page(ui);
            });
        });
        output.textures_delta.clear();
    }
}

#[test]
fn editing_an_instance_shows_its_pages_and_cancel_restores_it() {
    let t = crate::publish::tests::Temp::new();
    let path = t.0.join("inst.toml");
    std::fs::write(&path, "[recipe]\nid = 'inst'\nname = 'Inst'\n").unwrap();
    let mut app = launcher();
    app.edit_recipe(&path);
    assert_eq!(app.ui.tab, Tab::BaseGame);
    assert!(
        !app.ui.recipe_edit.as_ref().expect("editing").changed(),
        "opening is not a change"
    );
    // No game yet: nothing is read (no job writes to the data folder).
    let ctx = egui::Context::default();
    for tab in [Tab::BaseGame, Tab::AddOns, Tab::Balance, Tab::Advanced] {
        app.ui.tab = tab;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.page(ui));
        });
        output.textures_delta.clear();
    }
    let e = app.ui.recipe_edit.as_mut().expect("editing");
    e.recipe.recipe.name = "Renamed".into();
    e.set_game(t.0.join("game"));
    assert!(e.changed());
    app.discard_edit();
    let e = app.ui.recipe_edit.as_ref().expect("editing");
    assert!(!e.changed());
    assert_eq!(e.recipe.recipe.name, "Inst");
    assert_eq!(app.cfg.recipe.as_ref().and_then(|u| u.game.clone()), None);
}

/// Owner's report 2026-11-05: every instance showed JPHL, the folder left
/// on the old Game tab. Nothing an instance plays comes from it.
#[test]
fn an_instance_never_takes_the_old_game_tabs_folder() {
    let t = crate::publish::tests::Temp::new();
    let game = |name: &str| {
        let g = t.0.join(name);
        std::fs::create_dir_all(g.join("HUNTDAT")).unwrap();
        std::fs::write(g.join("HUNTDAT/_RES.TXT"), "characters {\n}\n.\n").unwrap();
        g
    };
    let (old, own, retail) = (game("old-tab"), game("own"), game("retail"));
    let recipe = |name: &str, base: &str| {
        let p = t.0.join(format!("{name}.toml"));
        std::fs::write(&p, format!("[recipe]\nid = '{name}'\nbase = '{base}'\n")).unwrap();
        p
    };
    let mut app = launcher();
    app.cfg.base = Some(old.clone());
    app.cfg.menu_style = MenuStyle::Original;
    let used = |path: PathBuf, game: Option<PathBuf>| RecipeUse {
        path,
        game,
        overlays: Some(Vec::new()),
        on: true,
        ..Default::default()
    };
    // Its own game, and no Carnivores 2 known: the game's own menus.
    app.cfg.recipe = Some(used(recipe("sa", "sa"), Some(own.clone())));
    let args = app.play_args();
    let shown = |p: &Path| args.iter().any(|a| a == &p.display().to_string());
    assert!(shown(&own) && !shown(&old), "{args:?}");
    // The original's menus come from an instance on the retail game.
    app.cfg
        .remembered
        .push(used(recipe("vanilla", "c2-vanilla"), Some(retail.clone())));
    let args = app.play_args();
    assert!(
        args.iter().any(|a| a == &retail.display().to_string()),
        "{args:?}"
    );
    assert!(
        !args.iter().any(|a| a == &old.display().to_string()),
        "{args:?}"
    );
    // No game of its own: none lent.
    app.cfg.recipe = Some(used(recipe("none", ""), None));
    assert!(app.recipe_game(app.active_recipe().unwrap()).is_empty());
}

#[test]
fn the_advanced_tab_is_hidden_until_enabled_in_settings() {
    let t = crate::publish::tests::Temp::new();
    let path = t.0.join("inst.toml");
    std::fs::write(&path, "[recipe]\nid = 'inst'\nname = 'Inst'\n").unwrap();
    let mut app = launcher();
    assert!(!app.cfg.show_advanced, "off by default");
    app.edit_recipe(&path);
    let ctx = egui::Context::default();
    let show = |app: &mut Launcher| {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.page(ui));
        });
        output.textures_delta.clear();
    };
    // Hidden: the editor goes back to its first page.
    app.ui.tab = Tab::Advanced;
    show(&mut app);
    assert_eq!(app.ui.tab, Tab::BaseGame);
    app.cfg.show_advanced = true;
    app.ui.tab = Tab::Advanced;
    show(&mut app);
    assert_eq!(app.ui.tab, Tab::Advanced);
}
