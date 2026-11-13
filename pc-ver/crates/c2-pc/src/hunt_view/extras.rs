//! In-hunt toggles not in the original (each also a setting or bought):
//! - K: the flashlight's beam, wide (the original) or narrow (reaching
//!   farther, lighting less); remembered in `c2rs.cfg`.
//! - T: multi ammo's swap between lethal rounds and tranquilizer darts.
//! - Advanced observer mode: F flies, T stops time (P still pauses all).
//!
//! Keys bound to something in the hunter's key map are left to that.
use super::*;

impl HuntView {
    /// Whether `key` is free of the hunter's bindings.
    fn unbound(&self, key: KeyCode) -> bool {
        let k = &self.hunt.room.options.keys;
        let Some(vk) = keys::vk_of(key) else {
            return true;
        };
        ![
            k.forward,
            k.backward,
            k.up,
            k.down,
            k.left,
            k.right,
            k.fire,
            k.show,
            k.strafe_left,
            k.strafe_right,
            k.strafe,
            k.jump,
            k.run,
            k.crouch,
            k.call,
            k.change_call,
            k.binoculars,
        ]
        .contains(&vk)
    }

    fn tap(&self, input: &Input, key: KeyCode) -> bool {
        input.pressed(key) && self.unbound(key)
    }

    /// The toggles' keys, once a frame while the hunt is live.
    pub(super) fn update_extras(&mut self, input: &Input) {
        let h = &self.hunt;
        if h.show_exit() || h.paused || h.death.is_some() || h.opts.trophy {
            return;
        }
        if self.setup.flashlight && self.view.area.cycle.is_some() && self.tap(input, KeyCode::KeyK)
        {
            self.toggle_beam();
        }
        if self.hunt.free_observer() {
            let now = std::time::Instant::now();
            let jump = input.pad.pressed(c2_pad::Action::Jump);
            let double_jump = jump
                && self
                    .last_jump_tap
                    .is_some_and(|t| now.duration_since(t).as_secs_f32() <= 0.3);
            if jump {
                self.last_jump_tap = if double_jump { None } else { Some(now) };
            }
            if self.tap(input, KeyCode::KeyF) || double_jump {
                let on = self.hunt.toggle_fly();
                self.message = Some((if on { "Flying" } else { "Walking" }.into(), 2.0));
            }
            if self.tap(input, KeyCode::KeyT) || input.pad.pressed(c2_pad::Action::Call) {
                let on = self.hunt.toggle_time_stop();
                self.message = Some((if on { "Time stopped" } else { "Time going" }.into(), 2.0));
            }
        } else if self.hunt.weapons.multi_ammo() && self.tap(input, KeyCode::KeyT) {
            if let Some(m) = self.hunt.swap_ammo() {
                self.message = Some((m.into(), 2.0));
            }
        }
    }

    pub(super) fn toggle_beam(&mut self) {
        if !self.setup.flashlight || self.view.area.cycle.is_none() {
            return;
        }
        self.view.narrow_flashlight = !self.view.narrow_flashlight;
        let narrow = self.view.narrow_flashlight;
        self.message = Some((
            if narrow {
                "Flashlight: narrow beam"
            } else {
                "Flashlight: wide beam"
            }
            .into(),
            2.0,
        ));
        // Kept for the next hunt.
        let mut pc = crate::settings::PcSettings::load();
        pc.narrow_flashlight = i32::from(narrow);
        pc.store();
        self.setup.pc.narrow_flashlight = pc.narrow_flashlight;
    }

    /// The hint bar's entries for these toggles.
    pub(super) fn extra_hints(&self, pad: bool) -> Vec<(String, &'static str)> {
        if pad {
            let mut hints = Vec::new();
            if self.hunt.free_observer() {
                hints.push(("A twice".into(), "Fly"));
                hints.push(("X".into(), "Stop / resume time"));
            } else if self
                .hunt
                .weapons
                .mee()
                .is_some_and(|m| m.semi_auto && m.full_auto)
                && self.hunt.weapons.cur_anims().ms[c2_core::game::weapon_anim::MODE] > 0
            {
                hints.push(("B".into(), "Firing mode"));
            }
            return hints;
        }
        let mut v = Vec::new();
        if self.setup.flashlight && self.view.area.cycle.is_some() {
            v.push(("K".into(), "Beam"));
        }
        if self.hunt.free_observer() {
            v.push(("F".into(), "Fly"));
            v.push(("T".into(), "Stop time"));
        } else if self.hunt.weapons.multi_ammo() {
            v.push(("T".into(), "Lethal / tranq"));
        }
        v
    }
}
