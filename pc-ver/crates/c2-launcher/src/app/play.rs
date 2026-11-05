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
}
