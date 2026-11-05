//! The home screen's grid of instance boxes: as many columns as fit the
//! window (one on a narrow one), every box the same size, so none is cut
//! off. Each box's own area is laid down before its buttons, so the
//! buttons take the clicks and the rest of the box takes the right click
//! (owner's report 2026-11-05: Play, Build and Options did nothing while
//! the box's area was laid over them).

use super::*;

/// The narrowest a box gets, and every box's height (points).
const MIN_W: f32 = 250.0;
const H: f32 = 132.0;
const GAP: f32 = 8.0;

pub(super) fn show(
    ui: &mut egui::Ui,
    items: &[Item],
    in_use: Option<&Path>,
    mode: crate::Mode,
    busy: bool,
    ask: &mut Ask,
) {
    let avail = ui.available_width().max(MIN_W);
    let cols = (((avail + GAP) / (MIN_W + GAP)).floor() as usize).max(1);
    let w = ((avail - GAP * (cols - 1) as f32) / cols as f32).floor();
    let n = items.len() + 1;
    for row in 0..n.div_ceil(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for i in row * cols..((row + 1) * cols).min(n) {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::click());
                let using = items
                    .get(i)
                    .is_some_and(|it| Some(it.path.as_path()) == in_use);
                let stroke = if using { GOLD } else { PANEL_HI };
                ui.painter().rect(
                    rect,
                    6.0,
                    PANEL,
                    egui::Stroke::new(1.5, stroke),
                    egui::StrokeKind::Inside,
                );
                let mut inner = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink(10.0))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                inner.set_clip_rect(rect.shrink(2.0));
                match items.get(i) {
                    Some(it) => {
                        instance_box(&mut inner, it, using, mode, busy, ask);
                        if !busy {
                            resp.context_menu(|ui| options_menu(ui, &it.path, mode, ask));
                        }
                    }
                    None => {
                        new_box(&mut inner, busy, ask);
                        if resp.clicked() && !busy {
                            *ask = Ask::New;
                        }
                    }
                }
            }
        });
        ui.add_space(GAP);
    }
}

fn instance_box(
    ui: &mut egui::Ui,
    it: &Item,
    using: bool,
    mode: crate::Mode,
    busy: bool,
    ask: &mut Ask,
) {
    let r = &it.recipe;
    ui.add(
        egui::Label::new(
            RichText::new(display_name(r))
                .size(17.0)
                .color(if using { GOLD } else { SAND })
                .strong(),
        )
        .truncate(),
    )
    .on_hover_text(&it.shown);
    let addons = r.mods.len();
    let mut line = format!(
        "{} · {addons} add-on{}",
        it.base,
        if addons == 1 { "" } else { "s" }
    );
    if !r.standalones.is_empty() {
        line += &format!(" · {} standalone", r.standalones.len());
    }
    ui.add(egui::Label::new(RichText::new(line).color(TEXT)).truncate());
    ui.horizontal(|ui| {
        if it.missing > 0 {
            ui.label(RichText::new(format!("⚠ {} missing", it.missing)).color(GOLD))
                .on_hover_text(
                    "Its base game or downloads have no folder here (or it moved): Fix missing \
                     in Options looks for them.",
                );
        }
        if it.played == 0 {
            ui.label(RichText::new("Unplayed").color(DIM));
        } else {
            ui.label(RichText::new(played_text(it.played)).color(DIM))
                .on_hover_text("Played from this launcher");
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if mode == crate::Mode::Full
            && ui
                .add_enabled(
                    !busy,
                    egui::Button::new(RichText::new("Play").color(GOLD).strong()).fill(GREEN),
                )
                .clicked()
        {
            *ask = Ask::Play(it.path.clone());
        }
        if ui.add_enabled(!busy, egui::Button::new("Build")).clicked() {
            *ask = Ask::Build(it.path.clone());
        }
        ui.add_enabled_ui(!busy, |ui| {
            ui.menu_button("Options", |ui| options_menu(ui, &it.path, mode, ask));
        });
    });
}

/// An instance's Options: the same on its button and on a right click.
fn options_menu(ui: &mut egui::Ui, path: &Path, mode: crate::Mode, ask: &mut Ask) {
    let p = || path.to_path_buf();
    if ui.button("Edit").clicked() {
        *ask = Ask::Edit(p());
    }
    if ui.button("Clone").clicked() {
        *ask = Ask::Clone(p());
    }
    if ui
        .button("Fix missing")
        .on_hover_text(
            "Looks for its base game and downloads in the mod and add-on folders (Settings) \
             and keeps what is found.",
        )
        .clicked()
    {
        *ask = Ask::FixMissing(p());
    }
    if ui
        .button("Export...")
        .on_hover_text(
            "The recipe to share (credits, download pages and file hashes; no game or mod \
             files), with its base game's hash list and patches.",
        )
        .clicked()
    {
        *ask = Ask::Export(p());
    }
    if mode == crate::Mode::Full
        && ui
            .button("Play in VR")
            .on_hover_text(
                "A first draft, through OpenXR (start SteamVR, Monado or WiVRn first): it \
                 checks your headset runtime and the game, then shows a test scene. Its report \
                 is in the log.",
            )
            .clicked()
    {
        *ask = Ask::PlayVr(p());
    }
    ui.separator();
    if ui.button(RichText::new("Delete").color(RED)).clicked() {
        *ask = Ask::Delete(p());
    }
}

fn new_box(ui: &mut egui::Ui, busy: bool, ask: &mut Ask) {
    ui.label(
        RichText::new("New instance")
            .size(17.0)
            .color(SAND)
            .strong(),
    );
    help(ui, "Start from a base game, then add add-ons.");
    ui.add_space(4.0);
    if ui
        .add_enabled(
            !busy,
            egui::Button::new(RichText::new("+ New").color(GOLD)).fill(GREEN),
        )
        .clicked()
    {
        *ask = Ask::New;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> Item {
        let mut recipe = Recipe::default();
        recipe.recipe.name = "Test".into();
        Item {
            path: "test.toml".into(),
            shown: "test.toml".into(),
            recipe,
            base: "c2".into(),
            missing: 2,
            played: 0,
        }
    }

    /// One click at `pos` (press, then release a frame later): what the
    /// home screen asked for, and whether a menu opened.
    fn click(pos: egui::Pos2, button: egui::PointerButton) -> (Option<String>, bool) {
        let ctx = egui::Context::default();
        let items = [item()];
        let mut got = None;
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut ask = Ask::Nothing;
                    show(ui, &items, None, crate::Mode::Full, false, &mut ask);
                    match ask {
                        Ask::Nothing => {}
                        Ask::Play(_) => got = Some("play".to_string()),
                        Ask::Build(_) => got = Some("build".to_string()),
                        Ask::New => got = Some("new".to_string()),
                        _ => got = Some("other".to_string()),
                    }
                });
            });
            out.textures_delta.clear();
        };
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![egui::Event::PointerMoved(pos)]);
        frame(vec![press(true)]);
        frame(vec![press(false)]);
        frame(vec![]);
        let menu = egui::Popup::is_any_open(&ctx);
        (got, menu)
    }

    /// Owner's report 2026-11-05: Play, Build and Options did nothing.
    #[test]
    fn the_box_buttons_take_clicks() {
        let mut found = std::collections::BTreeSet::new();
        let mut menu = false;
        for y in (0..150).step_by(8) {
            for x in (0..260).step_by(12) {
                let pos = egui::pos2(x as f32 + 8.0, y as f32 + 8.0);
                if let (Some(a), _) = click(pos, egui::PointerButton::Primary) {
                    found.insert(a);
                }
            }
        }
        // A right click on the box (its title row) opens Options.
        for y in (0..40).step_by(4) {
            menu |= click(
                egui::pos2(200.0, y as f32 + 8.0),
                egui::PointerButton::Secondary,
            )
            .1;
        }
        assert!(found.contains("play"), "{found:?}");
        assert!(found.contains("build"), "{found:?}");
        assert!(menu, "right click opens Options");
    }
}
