//! Play Vanilla (`c2-pc play --recipe R --classic`) and the review of an
//! imported recipe's advanced sections. Playing vanilla never writes the
//! recipe; Remove and Restore defaults are separate, explicit edits of it.
//! Both keep the recipe's comments (`Recipe::save`). What a classic
//! session keeps and leaves out: `docs/RECIPE-SETTINGS.md`.

use c2_mods::classic::{advanced_summary, Advanced};
use c2_mods::recipe::Recipe;

use super::*;

/// One edit of a recipe's advanced section, from the review dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SectionEdit {
    Remove,
    RestoreDefaults,
}

/// The hover text of the Play Vanilla button: what it leaves out, or why
/// there is nothing to leave out.
pub(crate) fn play_vanilla_hint(r: &Recipe) -> String {
    let left_out = advanced_summary(r);
    if left_out.is_empty() {
        return "This instance has no enhanced rules: Play already plays it as the original does."
            .into();
    }
    let names: Vec<String> = left_out.iter().map(|a| a.label()).collect();
    format!(
        "The same game, prices and saves, without: {}. Options such as teleport and dynamic \
         time are off for the session. The instance is not changed.",
        names.join("; ")
    )
}

impl Launcher {
    /// The instance in use, played without its enhanced rules.
    pub(super) fn play_vanilla(&mut self) {
        self.launch_game(true);
    }

    /// Changes one advanced section of the recipe at `path` and saves it.
    pub(super) fn edit_section(
        &mut self,
        path: &Path,
        a: Advanced,
        edit: SectionEdit,
    ) -> Result<(), String> {
        let mut r = Recipe::load(path).map_err(|e| e.to_string())?;
        match edit {
            SectionEdit::Remove => c2_mods::classic::remove(&mut r, a),
            SectionEdit::RestoreDefaults => {
                if !c2_mods::classic::restore_defaults(&mut r, a) {
                    return Err(format!("{} has no defaults to restore", a.label()));
                }
            }
        }
        r.save(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.ui.instances.refresh(&self.cfg);
        Ok(())
    }

    /// The review of an imported recipe: its advanced sections, each with
    /// Remove (and Restore defaults where it has them). True when closed.
    pub(super) fn review_ui(&mut self, ui: &mut egui::Ui, path: &Path) -> bool {
        let r = match Recipe::load_checked(path) {
            Ok(r) => r,
            Err(e) => {
                ui.label(RichText::new(format!("Can't read it: {e}")).color(theme::red()));
                return ui.button("Close").clicked();
            }
        };
        let (recipe, warnings) = r;
        let name = crate::instances::display_name(&recipe);
        ui.label(
            RichText::new(format!("'{name}' has advanced rules"))
                .color(theme::sand())
                .strong(),
        );
        help(
            ui,
            "They change how the game plays. Keep them, or take them out of this instance. \
             Play Vanilla plays it without them either way.",
        );
        let mut edit = None;
        egui::Grid::new("advanced-sections")
            .num_columns(3)
            .show(ui, |ui| {
                for a in advanced_summary(&recipe) {
                    ui.label(RichText::new(a.label()).color(theme::text()))
                        .on_hover_text(a.key());
                    if ui.button("Remove").clicked() {
                        edit = Some((a, SectionEdit::Remove));
                    }
                    if a.has_defaults() {
                        if ui
                            .button("Restore defaults")
                            .on_hover_text("Keep it on, with the built-in values.")
                            .clicked()
                        {
                            edit = Some((a, SectionEdit::RestoreDefaults));
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        for w in &warnings {
            ui.label(RichText::new(w).color(theme::gold()));
        }
        if let Some((a, e)) = edit {
            if let Err(err) = self.edit_section(path, a, e) {
                self.ui.note = Some((format!("Not changed: {err}"), theme::red()));
            }
        }
        let close = ui.button("Done").clicked();
        let empty = Recipe::load(path).is_ok_and(|r| advanced_summary(&r).is_empty());
        close || (empty && edit.is_some())
    }
}
