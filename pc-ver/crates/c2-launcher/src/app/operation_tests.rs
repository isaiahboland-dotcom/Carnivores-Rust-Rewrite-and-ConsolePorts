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
    for tab in [Tab::BaseGame, Tab::AddOns, Tab::Balance] {
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
