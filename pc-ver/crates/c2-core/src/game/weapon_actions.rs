//! Weapon input, draw/hide animations, trackers and recoil.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{weapon_anim, GameEvent, Hunt, HuntInput, WeaponInfo, WeaponState};
use crate::area::Area;
use c2_formats::car::Car;

impl Hunt {
    pub(super) fn weapon_input(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        input: &HuntInput,
        out: &mut Vec<GameEvent>,
    ) {
        if let Some(w) = input.select_weapon {
            if self.weapons.ftime == 0 {
                if self.weapons.rounds(w) == 0 {
                    out.push(GameEvent::Message("No weapon".into()));
                } else {
                    self.weapons.target = w;
                    if self.weapons.state == WeaponState::Hidden {
                        self.weapons.current = w;
                    }
                    self.hide_weapon(out);
                }
            }
        }
        if input.toggle_weapon {
            self.hide_weapon(out);
        }
        // A dialect weapon fires once per pull unless in its full-auto mode;
        // the original's fire whenever ready while the trigger is held.
        if input.fire {
            self.process_shoot(area, cars, out);
        }
        self.fire_held = input.fire;
        if self
            .weapons
            .info
            .get(self.weapons.current)
            .is_some_and(WeaponInfo::chambers)
        {
            if input.rack {
                self.rack(out);
            }
            if input.switch_mode {
                self.switch_mode(out);
            }
            if input.reload {
                self.reload(out);
            }
        }
    }

    /// The current weapon's sound for a [`weapon_anim`] slot: its
    /// animation's (the dialect's under water: its `*AqSnd`, else none).
    pub(super) fn weapon_sound(&self, slot: usize, out: &mut Vec<GameEvent>) {
        let w = &self.weapons;
        let mee = w.mee();
        let sound = match mee {
            Some(m) if self.player.underwater => m.aq_sounds.get(slot).copied().flatten(),
            _ => usize::try_from(w.cur_anims().fx[slot]).ok(),
        };
        if let Some(sound) = sound {
            out.push(GameEvent::WeaponSound {
                weapon: w.current,
                sound,
            });
        }
    }

    /// `HideWeapon`: draw if hidden, put away if ready.
    pub fn hide_weapon(&mut self, out: &mut Vec<GameEvent>) {
        let harpoon = self.weapons.mee().is_some_and(|m| m.harpoon);
        if self.player.underwater && self.weapons.state == WeaponState::Hidden && !harpoon {
            return;
        }
        if self.opts.observer || self.opts.trophy {
            return;
        }
        let w = &mut self.weapons;
        if w.state == WeaponState::Hidden {
            if w.rounds(w.current) == 0 {
                return;
            }
            w.optic_mode = w.info.get(w.current).is_some_and(|i| i.optic);
            self.bin_mode = false;
            w.ftime = 0;
            w.state = WeaponState::Drawing;
            w.shakel = w.mee().and_then(|m| m.shake).map_or(0.2, |s| s * 4.0);
            let empty = w.info.get(w.current).is_some_and(WeaponInfo::chambers)
                && w.chambered.get(w.current) == Some(&0)
                && w.anim(weapon_anim::DRAW_EMPTY) > 0;
            self.weapon_sound(
                if empty {
                    weapon_anim::DRAW_EMPTY
                } else {
                    weapon_anim::DRAW
                },
                out,
            );
            return;
        }
        if w.state != WeaponState::Ready || w.ftime != 0 {
            return;
        }
        w.state = WeaponState::Hiding;
        w.ftime = 0;
        w.optic_mode = false;
        let empty = w.info.get(w.current).is_some_and(WeaponInfo::chambers)
            && w.chambered.get(w.current) == Some(&0)
            && w.anim(weapon_anim::HIDE_EMPTY) > 0;
        self.weapon_sound(
            if empty {
                weapon_anim::HIDE_EMPTY
            } else {
                weapon_anim::HIDE
            },
            out,
        );
    }

    /// Weapon animation state machine (from `DrawPostObjects`; the
    /// dialect's in [`arms`]).
    pub(super) fn weapon_step(&mut self, dt: i32, out: &mut Vec<GameEvent>) {
        // The HUD's round flash: four pictures, a step each (`FlashP`).
        let w = &mut self.weapons;
        if w.flash > 0 {
            w.flash = if w.flash >= 4 { 0 } else { w.flash + 1 };
        }
        if self.weapons.state == WeaponState::Hidden {
            return;
        }
        if self
            .weapons
            .info
            .get(self.weapons.current)
            .is_some_and(WeaponInfo::chambers)
        {
            self.chamber_step(dt, out);
        } else {
            self.original_step(dt, out);
        }
        // Out of ammo: put it away and pick another weapon that has some.
        let w = &mut self.weapons;
        if w.state != WeaponState::Hidden && w.rounds(w.current) == 0 {
            if let Some(next) = (0..w.info.len()).find(|&i| w.rounds(i) > 0) {
                w.target = next;
            }
            self.hide_weapon(out);
        }
    }

    /// The original's weapon states: reloads come on their own.
    fn original_step(&mut self, dt: i32, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        w.shakel = (w.shakel + dt as f32 / 10000.0).min(4.0);
        let cur = w.current;
        match w.state {
            WeaponState::Drawing => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::DRAW) {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Reloading => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::RELOAD) {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Ready if w.ftime > 0 => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::FIRE) {
                    w.ftime = 0;
                    let info = &w.info[cur];
                    let left = w.shots_left[cur];
                    let mut reload = false;
                    if info.reload != 0
                        && left % info.reload == 0
                        && (left > 0 || w.ammo_mag[cur] > 0)
                    {
                        w.state = WeaponState::Reloading;
                        w.ftime = 1;
                        reload = true;
                    }
                    if w.shots_left[cur] == 0 && w.ammo_mag[cur] > 0 {
                        w.ammo_mag[cur] -= 1;
                        w.shots_left[cur] = w.info[cur].shots;
                        if w.anim(weapon_anim::RELOAD) != 0 {
                            w.state = WeaponState::Reloading;
                            w.ftime = 1;
                            reload = true;
                        }
                    }
                    if reload {
                        self.weapon_sound(weapon_anim::RELOAD, out);
                    }
                }
            }
            WeaponState::Hiding => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::HIDE) {
                    w.ftime = 0;
                    w.state = WeaponState::Hidden;
                    if w.current != w.target {
                        w.current = w.target;
                        self.hide_weapon(out);
                    }
                }
            }
            _ => {}
        }
    }

    /// Trackers run out (`RTime -= TimeDt`).
    pub(super) fn track_step(&mut self, dt: i32) {
        for c in &mut self.herd.chars {
            if let Some((_, Some(ms))) = &mut c.tracked {
                *ms -= dt;
                if *ms <= 0 {
                    c.tracked = None;
                }
            }
        }
    }

    /// The recoil moves the view each step and eases off, 0.005 rad per
    /// 17 ms (`Recoil` in the dialect's camera code).
    pub(super) fn apply_recoil(&mut self, dt: i32) {
        if self.recoil == [0.0; 2] {
            return;
        }
        let k = dt as f32 / 17.0;
        let p = &mut self.player;
        p.alpha += self.recoil[0] * k;
        p.beta = (p.beta + self.recoil[1] * k).clamp(-1.26, 1.46);
        let ease = 0.005 * k;
        self.recoil[1] = (self.recoil[1] + ease).min(0.0);
        let x = self.recoil[0];
        self.recoil[0] = if x.abs() <= ease {
            0.0
        } else {
            x - ease * x.signum()
        };
    }
}
