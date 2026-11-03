//! Windows packaging options.
use super::*;

impl Launcher {
    pub(super) fn windows_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Windows package");
        help(ui, "Build a saved instance as a portable folder with c2-pc.exe and its game data. Select Build Windows on Instances first. Supply the Windows release c2-pc.exe in the tools folder (Settings); on Linux, c2-pc is also needed to merge the instance.");
        ui.label(self.cfg.windows_out.as_ref().map_or_else(
            || "Output folder: not set".into(),
            |p| p.display().to_string(),
        ));
        if ui.button("Choose output folder…").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                self.cfg.windows_out = Some(p);
                self.save();
            }
        }
        let ready =
            !self.busy() && self.active_recipe().is_some() && self.cfg.windows_out.is_some();
        if ui
            .add_enabled(ready, egui::Button::new("Build Windows package"))
            .clicked()
        {
            self.build_windows();
        }
        help(ui, "The output contains a folder named after the instance. Rebuilding replaces that generated folder, so keep personal files elsewhere. Copy the entire folder to Windows and double-click c2-pc.exe. Game data is supplied by you, separately from the engine download.");
    }
}
