//! Multi ammo (not in the original): a weapon's load split between lethal
//! rounds and tranquilizer darts, one kind loaded at a time. Swapping puts
//! the loaded kind aside and loads the other; when the loaded kind runs out
//! the other is loaded on its own. A dart's hit puts the animal to sleep as
//! the hunt's tranquilizer option does (`Character::darted`).

use super::{Hunt, Weapons};
use c2_formats::bits::Bits;

/// The rounds of the kind not loaded (the fields [`Weapons`] keeps per
/// weapon for the loaded one).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stash {
    shots_left: i32,
    ammo_mag: i32,
    chambered: i32,
    mag_shots: i32,
}

impl Stash {
    fn rounds(&self) -> i32 {
        let mag = if self.ammo_mag > 0 { self.mag_shots } else { 0 };
        self.shots_left + self.chambered + mag
    }
}

impl Weapons {
    /// "Half tranq": every carried weapon keeps half its rounds as lethal
    /// (loaded) and puts the other half aside as darts.
    pub fn split_tranq(&mut self) {
        self.multi = true;
        for i in 0..self.info.len().min(Bits::LEN) {
            if !self.present.has(i) {
                continue;
            }
            let chambers = self.info[i].chambers();
            // Classic weapons: every round counted, the spare magazines too.
            let per_mag = self.info[i].shots.max(0);
            let total = if chambers {
                self.rounds(i)
            } else {
                self.shots_left[i] + self.ammo_mag[i].max(0) * per_mag
            };
            let darts = total / 2;
            let mut keep = total - darts;
            if chambers {
                // The loaded kind keeps what is chambered first, then the
                // reserve; the spare magazine goes into the reserve.
                let c = self.chambered[i].min(keep);
                self.chambered[i] = c;
                keep -= c;
                self.shots_left[i] = keep;
                self.ammo_mag[i] = 0;
                self.mag_shots[i] = 0;
            } else {
                self.shots_left[i] = keep;
                self.ammo_mag[i] = 0;
            }
            self.darts[i] = false;
            self.stash[i] = Stash {
                shots_left: darts,
                ..Stash::default()
            };
        }
    }

    /// Back to one kind (lethal), dropping what was put aside.
    pub(super) fn unsplit(&mut self) {
        self.multi = false;
        self.darts.iter_mut().for_each(|d| *d = false);
        self.stash.iter_mut().for_each(|s| *s = Stash::default());
    }

    /// Whether this hunt splits ammo between lethal rounds and darts.
    pub fn multi_ammo(&self) -> bool {
        self.multi
    }

    /// Weapon `i`'s rounds as (lethal, tranquilizer).
    pub fn kinds(&self, i: usize) -> (i32, i32) {
        let loaded = self.rounds(i);
        let aside = self.stash.get(i).map_or(0, Stash::rounds);
        if self.darts.get(i).copied().unwrap_or(false) {
            (aside, loaded)
        } else {
            (loaded, aside)
        }
    }

    /// Loads weapon `i`'s other kind; false if it has none.
    pub fn swap_kind(&mut self, i: usize) -> bool {
        if !self.multi || self.stash.get(i).is_none_or(|s| s.rounds() == 0) {
            return false;
        }
        let s = &mut self.stash[i];
        core::mem::swap(&mut self.shots_left[i], &mut s.shots_left);
        core::mem::swap(&mut self.ammo_mag[i], &mut s.ammo_mag);
        core::mem::swap(&mut self.chambered[i], &mut s.chambered);
        core::mem::swap(&mut self.mag_shots[i], &mut s.mag_shots);
        // A dialect weapon's reserve is chambered at once: the swap is meant
        // to be seamless.
        if self.info[i].chambers() && self.chambered[i] == 0 && self.shots_left[i] > 0 {
            let n = self.info[i].reload.max(1).min(self.shots_left[i]);
            self.chambered[i] = n;
            self.shots_left[i] -= n;
        }
        self.darts[i] = !self.darts[i];
        true
    }

    /// Whether weapon `i` fires darts now.
    pub fn fires_darts(&self, i: usize) -> bool {
        self.darts.get(i).copied().unwrap_or(false)
    }
}

impl Hunt {
    /// The ammo-switch control: the current weapon's other kind. A message
    /// for the HUD, if there is anything to say.
    pub fn swap_ammo(&mut self) -> Option<&'static str> {
        if !self.weapons.multi || self.opts.observer {
            return None;
        }
        let cur = self.weapons.current;
        if self.weapons.swap_kind(cur) {
            Some(if self.weapons.fires_darts(cur) {
                "Tranquilizer darts loaded"
            } else {
                "Lethal rounds loaded"
            })
        } else {
            Some(if self.weapons.fires_darts(cur) {
                "No lethal rounds left"
            } else {
                "No darts left"
            })
        }
    }

    /// After a shot: the loaded kind used up, the other is loaded.
    pub(super) fn auto_swap_kind(&mut self) {
        let cur = self.weapons.current;
        if self.weapons.multi && self.weapons.rounds(cur) == 0 {
            self.weapons.swap_kind(cur);
        }
    }
}
