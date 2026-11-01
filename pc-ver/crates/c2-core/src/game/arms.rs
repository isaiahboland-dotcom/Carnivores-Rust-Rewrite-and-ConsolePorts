//! The Modder's Edition's ammo model for its weapons (`Hunt.cpp`:
//! `ProcessReload`, `ProcessPump`, `ProcessFireMode` and the weapon states
//! in `DrawPostObjects`): rounds are chambered and fired from there, the
//! reserve goes in by a reload (all, or part when some are left) and,
//! for a weapon with a rack animation, by racking. Reloading and racking
//! are by hand unless `autoReload` / `autoRack`. Under water every
//! animation runs at half speed.

use super::{weapon_anim as wa, GameEvent, Hunt, WeaponState};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

impl Hunt {
    /// The dialect's weapon states, one step (`DrawPostObjects`).
    pub(super) fn chamber_step(&mut self, dt: i32, out: &mut Vec<GameEvent>) {
        let d = if self.player.underwater { dt / 2 } else { dt };
        let unlimited = self.survival.as_ref().is_some_and(|s| s.opts.unlimited);
        let under = self.player.underwater;
        let w = &mut self.weapons;
        w.shakel = (w.shakel + dt as f32 / 10000.0).min(4.0);
        let cur = w.current;
        let Some(info) = w.info.get(cur).cloned() else {
            return;
        };
        let mut m = info.mee.clone().unwrap_or_default();
        if self.opts.auto_chamber {
            m.auto_reload = true;
            m.auto_rack = true;
        }
        let a = w.cur_anims();
        match w.state {
            WeaponState::Drawing => {
                w.ftime += d;
                let slot = if w.chambered[cur] == 0 && a.ms[wa::DRAW_EMPTY] > 0 {
                    wa::DRAW_EMPTY
                } else {
                    wa::DRAW
                };
                if w.ftime >= a.ms[slot] {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Reloading | WeaponState::PartReload => {
                let full = w.state == WeaponState::Reloading;
                w.ftime += d;
                if w.ftime >= a.ms[if full { wa::RELOAD } else { wa::PART }] {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                    self.reloaded(full, unlimited, out);
                }
            }
            WeaponState::Ready if w.ftime > 0 => {
                w.ftime += d;
                // The muzzle flash lasts its model's animation (`MUZZ4.CAR`).
                if w.muzzle.is_some() && (under || w.ftime > w.muzzle_ms) {
                    w.muzzle = None;
                }
                if w.ftime >= a.ms[wa::FIRE] {
                    w.ftime = 0;
                    w.muzzle = None;
                    if info.reload == 0 && !m.must_rack && w.shots_left[cur] > 0 {
                        w.chambered[cur] = 1;
                        if !unlimited {
                            w.shots_left[cur] -= 1;
                        }
                    }
                    let left = w.shots_left[cur];
                    let empty = w.chambered[cur] == 0;
                    if m.must_rack && m.auto_rack && left > 0 {
                        self.rack(out);
                    }
                    if m.auto_reload && empty && (info.reload != 0 || left == 0) {
                        self.reload(out);
                    }
                }
            }
            WeaponState::Racking => {
                w.ftime += d;
                if w.ftime >= a.ms[wa::RACK] {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                    if w.shots_left[cur] > 0 {
                        w.chambered[cur] = 1;
                        if !unlimited {
                            w.shots_left[cur] -= 1;
                        }
                    }
                }
            }
            WeaponState::Switching => {
                w.ftime += d;
                if w.ftime >= a.ms[wa::MODE] {
                    w.fire_mode[cur] = u8::from(w.fire_mode[cur] == 0);
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Hiding => {
                w.ftime += d;
                let slot = if w.chambered[cur] == 0 && a.ms[wa::HIDE_EMPTY] > 0 {
                    wa::HIDE_EMPTY
                } else {
                    wa::HIDE
                };
                if w.ftime >= a.ms[slot] {
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

    /// The end of a reload: the rounds in (a magazine weapon swaps its
    /// magazine for the spare and, with nothing chambered after a full
    /// one, chambers a round or racks).
    fn reloaded(&mut self, full: bool, unlimited: bool, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        let cur = w.current;
        let Some(info) = w.info.get(cur) else { return };
        if info.reload != 0 {
            if !unlimited {
                w.shots_left[cur] -= w.ammo_in;
            }
            w.chambered[cur] += w.ammo_in;
            return;
        }
        core::mem::swap(&mut w.mag_shots[cur], &mut w.shots_left[cur]);
        if w.mag_shots[cur] == 0 {
            w.ammo_mag[cur] -= 1;
        }
        if full && w.chambered[cur] == 0 {
            if w.cur_anims().ms[wa::RACK] == 0 {
                w.chambered[cur] = 1;
                w.shots_left[cur] -= 1;
            } else if self.opts.auto_chamber || w.mee().is_some_and(|m| m.auto_rack) {
                self.rack(out);
            }
        }
    }

    /// `ProcessReload`: rounds into a weapon that loads them one by one
    /// (`reload`, a part reload when some are left or too few remain), or
    /// the spare magazine into one that takes magazines.
    pub(super) fn reload(&mut self, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        if w.state != WeaponState::Ready || w.ftime != 0 {
            return;
        }
        let cur = w.current;
        let Some(info) = w.info.get(cur) else { return };
        let a = w.cur_anims();
        let part = if info.reload != 0 {
            let (ch, left) = (w.chambered[cur], w.shots_left[cur]);
            if ch >= info.reload || left <= 0 {
                return;
            }
            // As the dialect has it: all that fit, or what is left when that
            // is fewer than a full load.
            w.ammo_in = if left < info.reload {
                left
            } else {
                info.reload - ch
            };
            ch > 0 || left < info.reload
        } else if w.ammo_mag[cur] > 0 {
            w.chambered[cur] > 0
        } else {
            return;
        };
        let slot = if part && a.ms[wa::PART] > 0 {
            wa::PART
        } else {
            wa::RELOAD
        };
        if a.ms[slot] == 0 {
            // No reload animation at all (the dialect would read slot -1):
            // the rounds go straight in.
            let unlimited = self.survival.as_ref().is_some_and(|s| s.opts.unlimited);
            self.reloaded(true, unlimited, out);
            return;
        }
        w.state = if slot == wa::PART {
            WeaponState::PartReload
        } else {
            WeaponState::Reloading
        };
        w.ftime = 1;
        self.weapon_sound(slot, out);
    }

    /// `ProcessPump`: racking a magazine weapon that has a rack animation
    /// (a round left in the chamber is thrown out).
    pub(super) fn rack(&mut self, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        let cur = w.current;
        if w.cur_anims().ms[wa::RACK] == 0
            || w.state != WeaponState::Ready
            || w.ftime != 0
            || w.info.get(cur).is_none_or(|i| i.reload != 0)
        {
            return;
        }
        w.chambered[cur] = 0;
        w.state = WeaponState::Racking;
        w.ftime = 1;
        self.weapon_sound(wa::RACK, out);
    }

    /// `ProcessFireMode`: a weapon both `semiAuto` and `fullAuto`, with a
    /// `modAnim`, switches between them.
    pub(super) fn switch_mode(&mut self, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        let both = w.mee().is_some_and(|m| m.semi_auto && m.full_auto);
        if !both || w.cur_anims().ms[wa::MODE] == 0 || w.state != WeaponState::Ready || w.ftime != 0
        {
            return;
        }
        w.state = WeaponState::Switching;
        w.ftime = 1;
        self.weapon_sound(wa::MODE, out);
    }

    /// `AnimateBullets`, a retrievable round lying where it stopped: picked
    /// up within 300 units, when there is room for it (`fxCollect`).
    pub(super) fn pick_up(&mut self, b: &super::Bullet, out: &mut Vec<GameEvent>) -> bool {
        let p = &self.player;
        let d = [b.pos[0] - p.x, b.pos[1] - p.y, b.pos[2] - p.z];
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= 300.0 * 300.0 {
            return false;
        }
        let w = &mut self.weapons;
        let i = b.weapon;
        let Some(info) = w.info.get(i) else {
            return true;
        };
        let a = w.anims.get(i).copied().unwrap_or_default();
        let mut most = info.shots;
        if w.double_ammo && (info.reload != 0 || a.ms[wa::RELOAD] == 0) {
            most *= 2;
        }
        if w.shots_left[i] >= most {
            return false;
        }
        out.push(GameEvent::Collect(self.rng.r_rand(2) as usize));
        let by_hand = if info.reload == 0 {
            a.ms[wa::RACK] == 0
        } else {
            a.ms[wa::RELOAD] == 0
        };
        if w.chambered[i] == 0 && by_hand {
            w.chambered[i] += 1;
        } else {
            w.shots_left[i] += 1;
        }
        true
    }
}

/// The muzzle flash's length (ms) until a platform reads its model's.
pub const MUZZLE_MS: i32 = 50;
