//! Keeping the population up: an animal killed by another one comes back.
//!
//! Not in the original game, where nothing respawns; there only the hunter
//! kills, and the ship takes the body. Under predation the carnivores would
//! otherwise empty the map in one hunt (2026-11-07 audit P3). A body left by
//! an animal lies [`BODY_MS`](super::run::BODY_MS) for scavengers, then, once the hunter can't
//! see it, becomes a new animal of its species placed out of his sight, as
//! a hunt places them: anywhere at least 40 cells off for the original's
//! animals, in its spawn group's regions for the dialect's. The character
//! keeps its index, so no goal, pack or ship task needs renumbering.
//!
//! The hunter's own kills never come back.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::run::Goal;
use crate::area::Area;
use crate::dino::eco::Spawn;
use crate::dino::place::Habitat;
use crate::dino::{check_place_collision_p, Character, Ctx, Herd, HunterView};
use crate::rng::MsRand;

/// Spot tries per body; a body with no spot tries again [`RETRY_MS`] later.
const TRIES: usize = 64;
const RETRY_MS: i32 = 1_000;

impl Herd {
    /// Replaces each animal's kill that has lain long enough out of the
    /// hunter's sight with a new animal of its species.
    pub(crate) fn respawn_kills(&mut self, ctx: &mut Ctx<'_>) {
        let sight = (ctx.render_r * 256) as f32;
        for i in 0..self.chars.len() {
            let c = &self.chars[i];
            let t = &c.predation;
            if c.health != 0
                || !t.killed_by_animal
                || t.respawn > 0
                || seen(c.pos, ctx.hunter, sight)
                || !self.eco_room(c.ctype)
            {
                continue;
            }
            let Some(mut pos) = self.spot(i, ctx.area, ctx.hunter, ctx.rng, sight) else {
                self.chars[i].predation.respawn = RETRY_MS;
                continue;
            };
            self.swim_height(ctx.area, self.chars[i].ctype, &mut pos);
            let old = &self.chars[i];
            let Some(ctype) = self.select_variant(old.ctype, ctx.rng) else {
                continue;
            };
            let mut c = Character {
                ctype,
                pos,
                tgx: pos[0],
                tgz: pos[2],
                spawn_group: old.spawn_group,
                pack: old.pack,
                pack_density: old.pack_density,
                home_zone: self.eco_home(&pos),
                ..Default::default()
            };
            if let Some(sp) = self.species.get(c.ctype) {
                c.reset(sp, ctx.rng, ctx.opt_agres);
            }
            self.chars[i] = c;
            for c in &mut self.chars {
                if c.predation.goal == Goal::Scavenge(i) {
                    c.predation.goal = Goal::None;
                }
            }
        }
    }

    /// A spot for character `i` to come back at, out of `sight` of the
    /// hunter. `None` when none was found this time.
    fn spot(
        &self,
        i: usize,
        area: &Area,
        hunter: HunterView,
        rng: &mut MsRand,
        sight: f32,
    ) -> Option<[f32; 3]> {
        let c = &self.chars[i];
        let from = (hunter.x, hunter.z);
        let group = c
            .spawn_group
            .and_then(|g| self.groups.get(g))
            .filter(|g| !g.regions.is_empty());
        // `mee_spot` gives up after 10240 tries in all.
        let mut tr = 0;
        for _ in 0..TRIES {
            let p = match group {
                Some(g) => self.mee_spot(area, from, rng, &mut tr, g, c.ctype, None)?,
                None => match stock_spot(area, rng) {
                    // Dry ground: an animal of the water waits for a
                    // group with a region to come back in.
                    Some(_) if self.habitat(c.ctype) != Habitat::Land => return None,
                    // (`mee_spot` asks the ecosystem itself.)
                    Some(p)
                        if !self.hunter_clear(Spawn::Respawn, Some(c.ctype), &p, from)
                            || !self.eco_spot(c.ctype, &p, rng) =>
                    {
                        continue
                    }
                    Some(p) => p,
                    None => continue,
                },
            };
            if !seen(p, hunter, sight) {
                return Some(p);
            }
        }
        None
    }
}

/// One try at `PlaceCharacters`' spot for the main population: anywhere
/// within 500 cells of the map centre (the caller checks the distance from
/// the hunter).
fn stock_spot(area: &Area, rng: &mut MsRand) -> Option<[f32; 3]> {
    let mut p = [
        512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
        0.0,
        512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
    ];
    p[1] = area.land_height(p[0], p[2]);
    (!check_place_collision_p(area, &mut p)).then_some(p)
}

/// Whether the hunter can see `pos`: within his render distance.
fn seen(pos: [f32; 3], h: HunterView, sight: f32) -> bool {
    (pos[0] - h.x).powi(2) + (pos[2] - h.z).powi(2) < sight * sight
}
