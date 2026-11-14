//! Ammunition drops: supply crates at places drawn from the hunt's seed,
//! away from the drop-off. Walking over one adds a load of rounds to every
//! carried weapon.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{string::ToString, vec::Vec};

use super::super::{GameEvent, Hunt};
use super::Hardcore;
use crate::area::Area;
use crate::rng::MsRand;
use crate::CELL;

/// A drop is at least this far from the drop-off and at most this far,
/// cells.
const NEAREST: i32 = 24;
const FARTHEST: i32 = 140;
/// The hunter collects a drop within this distance.
const REACH: f32 = 1.25 * CELL;

/// One supply crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drop {
    pub pos: [f32; 3],
    pub taken: bool,
}

impl Hardcore {
    /// Places the drops round the drop-off, from the hunt's seed alone (the
    /// same seed and drop-off: the same places).
    pub(super) fn place_drops(&mut self, area: &Area) {
        self.drops.clear();
        let mut rng = MsRand::new(self.seed);
        let (cx, cz) = (
            (self.drop_off.0 / CELL) as i32,
            (self.drop_off.1 / CELL) as i32,
        );
        for _ in 0..self.rules.ammo_drops {
            let spot = (0..400).find_map(|_| {
                let (dx, dz) = (rng.si_rand(FARTHEST), rng.si_rand(FARTHEST));
                let d2 = dx * dx + dz * dz;
                let (x, z) = (cx + dx, cz + dz);
                ((NEAREST * NEAREST..=FARTHEST * FARTHEST).contains(&d2)
                    && super::super::teleport::dry(area, x, z))
                .then_some((x, z))
            });
            if let Some((x, z)) = spot {
                let (x, z) = (x as f32 * CELL + CELL / 2.0, z as f32 * CELL + CELL / 2.0);
                self.drops.push(Drop {
                    pos: [x, area.land_height(x, z), z],
                    taken: false,
                });
            }
        }
    }
}

impl Hunt {
    /// The drops still lying about.
    pub fn ammo_drops(&self) -> impl Iterator<Item = &Drop> {
        self.hardcore
            .as_deref()
            .into_iter()
            .flat_map(|h| h.drops.iter())
            .filter(|d| !d.taken)
    }

    pub(super) fn drops_step(&mut self, out: &mut Vec<GameEvent>) {
        if self.health <= 0 {
            return;
        }
        let (px, pz) = (self.player.x, self.player.z);
        let Some(h) = self.hardcore.as_deref_mut() else {
            return;
        };
        for d in h.drops.iter_mut().filter(|d| !d.taken) {
            if (d.pos[0] - px).powi(2) + (d.pos[2] - pz).powi(2) <= REACH * REACH {
                d.taken = true;
                self.weapons.add_load();
                out.push(GameEvent::Message("Ammunition collected".to_string()));
            }
        }
    }
}
