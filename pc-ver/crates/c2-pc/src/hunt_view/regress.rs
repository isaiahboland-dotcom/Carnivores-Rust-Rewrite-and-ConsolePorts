//! Regression scenarios on the PC renderer (`c2-pc regress`): the hunt is
//! replaced by the scenario's and advanced by the script, one tick per
//! frame, instead of by keys. Not reachable from normal play.

use c2_core::scenario::{self, Arrange, Cast, Hold, Scenario, Screen, Script, SetupError, TICK_MS};
use glam::Vec3;

use super::{HuntView, Pick};
use crate::audio::Audio;
use crate::gpu::Gpu;
use crate::input::Input;
use crate::scene::Scene;

impl HuntView {
    /// Swaps in the scenario's hunt (same species, weapons and models as
    /// loaded) with a fresh profile and silent audio; returns the layout
    /// to hold, the weapon names in order and the placed animals.
    pub(crate) fn regress_setup(
        &mut self,
        s: &Scenario,
    ) -> Result<(Hold, Vec<String>, Vec<Cast>), SetupError> {
        let built = scenario::build(
            self.view.area(),
            self.hunt.herd.species.clone(),
            self.weapon_infos.clone(),
            self.weapon_anims.clone(),
            s,
            c2_core::profile::new_player("regress", 0, 0),
        )?;
        self.hunt = built.hunt;
        self.audio = Audio::silent();
        self.prev_cam = self.hunt.camera().0;
        let names = self.weapon_infos.iter().map(|w| w.name.clone()).collect();
        Ok((built.hold, names, built.cast))
    }

    /// The kill script's change to the hunt. A revived hunter's death
    /// message goes too, so each kill's pictures stand alone.
    pub(crate) fn regress_arrange(&mut self, hold: &mut Hold, a: Arrange, script: &Script) {
        let Script::Kills(k) = script else { return };
        hold.arrange(&mut self.hunt, self.view.area(), a, k);
        if matches!(a, Arrange::Revive(_)) {
            self.message = None;
            self.prev_cam = self.hunt.camera().0;
        }
    }

    pub(crate) fn regress_hunt(&self) -> &c2_core::game::Hunt {
        &self.hunt
    }

    pub(crate) fn regress_hunt_mut(&mut self) -> &mut c2_core::game::Hunt {
        &mut self.hunt
    }

    pub(crate) fn regress_hold(&mut self, hold: &Hold) -> Vec<usize> {
        hold.apply(&mut self.hunt, self.view.area())
    }

    /// Opens or closes the weapon or call picker.
    pub(crate) fn regress_picker(&mut self, s: Screen, open: bool) {
        let kind = match s {
            Screen::WeaponPicker => Pick::Weapon,
            Screen::CallPicker => Pick::Call,
            _ => return,
        };
        if open {
            self.open_pick(kind, false);
        } else if self.picker.is_some() && self.picker_kind == kind {
            self.picker = None;
        }
    }

    /// The picker showing, if any.
    pub(crate) fn regress_open_picker(&self) -> Option<Screen> {
        self.picker.map(|_| match self.picker_kind {
            Pick::Weapon => Screen::WeaponPicker,
            Pick::Call => Screen::CallPicker,
        })
    }

    /// One scripted tick (none while a picker is open: the hunt waits, as
    /// in play), then what `Scene::update` does after its ticks.
    pub(crate) fn regress_tick(&mut self, gpu: &Gpu, input: &c2_core::game::HuntInput) {
        if self.picker.is_none() {
            self.step(input);
        }
        self.handle_events();
        let (pos, alpha, beta) = self.hunt.camera();
        self.prev_cam = pos;
        self.acc_ms = 0.0;
        self.view.cam.pos = Vec3::from(pos);
        self.view.cam.yaw = -alpha;
        self.view.cam.pitch = -beta;
        self.view.zoom = self.hunt.zoom();
        self.view
            .update(gpu, TICK_MS as f32 / 1000.0, &Input::default());
    }
}
