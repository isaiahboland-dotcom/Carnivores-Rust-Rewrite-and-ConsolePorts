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
        let step = Step::Launch {
            program: pc,
            args: self.play_args(),
        };
        // The time played goes to the instance in use.
        let instance = self.active_recipe().map(|r| r.path.clone());
        if let Err(e) = self.job.start_task("Starting the game", move |job| {
            let t = std::time::Instant::now();
            let result = job.run(step).err();
            let secs = t.elapsed().as_secs();
            Ok(crate::jobs::Completion::Played(Box::new((
                instance, secs, result,
            ))))
        }) {
            self.ui.note = Some((e, RED));
        }
    }

    /// The game closed: its time added to the instance's.
    pub(super) fn played(&mut self, instance: Option<PathBuf>, secs: u64, error: Option<String>) {
        if let Some(p) = instance {
            *self.cfg.playtime.entry(p).or_default() += secs;
            self.save();
            self.ui.instances.refresh(&self.cfg);
        }
        if let Some(e) = error {
            self.ui.note = Some((e, RED));
        }
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
