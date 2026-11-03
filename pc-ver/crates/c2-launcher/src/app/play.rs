//! PC and VR launch actions.
use super::*;

impl Launcher {
    pub(super) fn play(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let Some(pc) = find_tool("c2-pc", self.cfg.tools.as_deref()) else {
            self.ui.note = Some((
                "c2-pc not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let steps = vec![Step::Launch {
            program: pc,
            args: self.play_args(),
        }];
        self.start_job("Starting the game", steps);
    }

    /// The same game in a VR headset (`c2-vr`, README M17): it checks the
    /// OpenXR runtime and the game, its findings in the log. A first draft:
    /// it shows a test scene in the headset, not the game yet; run and
    /// waited for rather than launched, so its report lands in the log.
    pub(super) fn play_vr(&mut self) {
        if !self.can_start_operation() {
            return;
        }
        let Some(vr) = find_tool("c2-vr", self.cfg.tools.as_deref()) else {
            self.ui.note = Some((
                "c2-vr not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let steps = vec![Step::Run {
            program: vr,
            args: self.play_args(),
        }];
        self.start_job("Starting the game in VR", steps);
    }

    pub(super) fn play_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Play on PC");
        let ready = self
            .cfg
            .base
            .as_deref()
            .is_some_and(|b| describe_game(b).is_some());
        panel(ui, |ui| {
            match &self.cfg.base {
                Some(b) => ui.label(format!("Game: {}", b.display())),
                None => {
                    ui.label(RichText::new("Set the game folder first (The game tab).").color(RED))
                }
            };
            ui.label(format!("Overlays: {}", self.cfg.overlays.len()));
            let on = self.cfg.enabled_addons().len();
            ui.label(format!("Add-ons on: {on} of {}", self.cfg.addons.len()));
            if self.ui.balance.file().is_some() {
                ui.label(RichText::new("Your balance changes apply.").color(GOLD));
            }
            help(
                ui,
                "With add-ons, the combined game is built first (a few seconds; kept until the \
                 list changes).",
            );
            ui.add_space(4.0);
            if self.menu_style_ui(ui, false) | self.survival_ui(ui) {
                self.save();
            }
        });
        ui.add_space(12.0);
        let busy = self.busy();
        ui.horizontal(|ui| {
            if big_button(ui, "PLAY", ready && !busy) {
                self.play();
            }
            ui.add_space(12.0);
            if big_button(ui, "PLAY IN VR", ready && !busy) {
                self.play_vr();
            }
        });
        help(
            ui,
            "PLAY IN VR: the same game in a headset, through OpenXR (start SteamVR, Monado or \
             WiVRn first). A first draft: for now it checks your headset runtime and the game, \
             then shows a test scene in the headset (floor grid, boxes, your controllers); \
             the left menu button quits. Its report is in the log below.",
        );
    }
}
