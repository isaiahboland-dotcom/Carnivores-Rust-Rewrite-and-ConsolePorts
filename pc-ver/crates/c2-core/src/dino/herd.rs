//! The herd: all characters stepped together (`AnimateCharacters`).

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

use c2_formats::mee::SpawnGroup;

use super::{
    ai, brachi, carnivore, dead, flyer, herbivore, huntable, mee, mee_ambient, mee_brachi,
    mee_fish, mee_flyer, mee_icth, mee_trex, predation, small, trex, CarnivoreParams, Character,
    Ctx, DinoEvent, HerbivoreParams, HunterView, SmallParams, Species, Target, TargetKind,
};

/// A pack of a dialect hunt (`Packs[]`): members follow the leader and
/// share alarm and attack.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pack {
    /// Index into `Herd::chars`; a dead leader is replaced by a living
    /// member.
    pub leader: usize,
    /// Raised by members this step / seen by them (last step's, `_alert`).
    pub alert: bool,
    pub attack: bool,
    pub alert_prev: bool,
    pub attack_prev: bool,
}

/// All characters plus the species table, stepped together
/// (`AnimateCharacters`).
#[derive(Default)]
pub struct Herd {
    pub species: Vec<Species>,
    pub chars: Vec<Character>,
    /// `DemoPoint.CIndex`: who the death camera watches (the hunter's body,
    /// or the T-Rex carrying it).
    pub demo_index: Option<usize>,
    /// Dialect hunts: packs, and the spawn groups characters keep to.
    pub packs: Vec<Pack>,
    pub groups: Vec<SpawnGroup>,
    /// Dialect games: the trophy room's spots (trophy slot n = spot n);
    /// empty: the original's stands.
    pub trophy_spots: Vec<c2_formats::mee::TrophySpot>,
    /// Predation, when the game turns it on (`None`: the original's herd).
    pub predation: Option<Box<predation::run::Predators>>,
}

impl Herd {
    /// Character `index` was removed: renumber what refers past it.
    pub fn forget(&mut self, index: usize) {
        if self.predation.is_some() {
            predation::run::Predators::forget(&mut self.chars, index);
        }
    }

    /// `AnimateCharacters`.
    pub fn step(&mut self, ctx: &mut Ctx<'_>) {
        self.step_scheduled(ctx, false);
    }

    /// Optional four-way staggering of calm, distant stock hunt targets.
    /// Near/alerted/wounded/dead animals and dialect packs remain full-rate.
    pub fn step_scheduled(&mut self, ctx: &mut Ctx<'_>, distant: bool) {
        let hunter = ctx.hunter;
        let base_dt = ctx.dt;
        for p in &mut self.packs {
            p.alert_prev = core::mem::take(&mut p.alert);
            p.attack_prev = core::mem::take(&mut p.attack);
        }
        if let Some(p) = self.predation.as_deref_mut() {
            p.think(&mut self.chars, &self.species, ctx);
        }
        for i in 0..self.chars.len() {
            ctx.dt = base_dt;
            self.chars[i].predation.tick(base_dt);
            let steer = self
                .predation
                .as_deref()
                .and_then(|p| p.steer(i, &mut self.chars, &self.species));
            // A pack whose leader died follows the next living member.
            if let Some(p) = self.chars[i].pack.and_then(|p| self.packs.get_mut(p)) {
                if self.chars[i].health != 0
                    && self.chars.get(p.leader).is_none_or(|l| l.health == 0)
                {
                    p.leader = i;
                }
            }
            let leader = self.chars[i]
                .pack
                .and_then(|p| self.packs.get(p))
                .and_then(|p| self.chars.get(p.leader))
                .map(|l| l.pos);
            let carrier = self
                .demo_index
                .and_then(|d| self.chars.get(d))
                .filter(|t| t.ai == ai::TREX)
                .map(|t| dead::Carrier {
                    pos: t.pos,
                    ftime: t.ftime,
                    beta: t.beta,
                    gamma: t.gamma,
                });
            let c = &mut self.chars[i];
            if c.state_f == 0xff {
                continue;
            }
            let Some(sp) = self.species.get(c.ctype) else {
                continue;
            };
            let dx = c.pos[0] - hunter.x;
            let dz = c.pos[2] - hunter.z;
            let range = ((ctx.view_r + 16) * 256) as f32;
            let defer = distant
                && base_dt > 0
                && base_dt <= 16
                && hunter.alive
                && sp.mee.is_none()
                && c.ai >= 10
                && c.health > 0
                && c.state == 0
                && c.afraid_time == 0
                && c.blood_ttime == 0
                && c.lure.is_none()
                && dx * dx + dz * dz > range * range;
            c.pending_ai_ms += base_dt;
            if defer && !((ctx.real_time / 16) as usize + i).is_multiple_of(4) {
                continue;
            }
            ctx.dt = core::mem::take(&mut c.pending_ai_ms);
            c.tgtime += ctx.dt;
            let group = c.spawn_group.and_then(|g| self.groups.get(g));
            // The dialect's Ichthyornis: a minute on the ground ends in a
            // take-off, one in the air in a new heading.
            if let Some(m) = sp.mee.as_ref().filter(|m| m.ai == mee::mee_ai::ICTH) {
                if c.tgtime > 50 * 1000 {
                    if [m.fly, m.glide, m.takeoff, m.land].contains(&c.phase) {
                        c.set_new_target_place_icth(ctx.area, ctx.rng, 2048.0, group);
                    } else {
                        c.state = 2;
                        c.afraid_time = (50 + ctx.rng.r_rand(8)) * 1024;
                        c.not_flushed = true;
                    }
                }
            }
            if c.tgtime > 30 * 1000 {
                if sp.mee.is_some() {
                    c.retarget_default(sp, ctx.area, ctx.rng, group);
                } else {
                    c.set_new_target_place(ctx.area, ctx.rng, 2048.0);
                }
            }
            if c.health != 0 && c.blood_ttime != 0 {
                c.blood_ttime = (c.blood_ttime - ctx.dt).max(0);
                let k = ((20000.0 + c.blood_ttime as f32) / 90000.0).min(1.5);
                c.blood_time += (ctx.dt as f32 * k) as i32;
                if c.blood_time > 600 {
                    c.blood_time = ctx.rng.r_rand(228);
                    ctx.events.push(DinoEvent::BloodTrail { pos: c.pos });
                    if ctx.rng.r_rand(128) > 96 {
                        ctx.events.push(DinoEvent::BloodTrail { pos: c.pos });
                    }
                }
            }
            if c.afraid_time <= 0 {
                c.aware_hunter = false;
                c.heard_shot = false;
            }
            if predation::step::feeding(c, sp, ctx.dt) {
                continue;
            }
            ctx.target = target_of(c, hunter, steer);
            ctx.struck = None;
            // Pack alarms and attacks are about the player: a member after
            // (or running from) another animal raises neither.
            let mut scratch: Pack;
            let pack = match c.pack.and_then(|p| self.packs.get_mut(p)) {
                Some(p) if !ctx.target.alarms_pack() => {
                    scratch = *p;
                    Some(&mut scratch)
                }
                p => p,
            };
            animate(c, i, sp, ctx, pack, leader, group, carrier);
            if let Some(p) = self.predation.as_deref() {
                p.finish(i, &mut self.chars, &self.species, ctx.struck);
            }
        }
        ctx.target = Target::default();
        ctx.struck = None;
        ctx.dt = base_dt;
    }
}

/// What character `c`'s AI chases this step: predation's steered target,
/// else a survival waypoint on the way to the player, else the player.
fn target_of(c: &Character, hunter: HunterView, steer: Option<predation::run::Steer>) -> Target {
    if let Some(s) = steer {
        return s.target(c);
    }
    match c.lure {
        Some((x, z)) if c.health != 0 => Target {
            view: HunterView { x, z, ..hunter },
            kind: TargetKind::Waypoint,
            ..Target::hunter(hunter, c)
        },
        _ => Target::hunter(hunter, c),
    }
}

/// One character's AI step: the dialect's AI for its family, or the
/// original's by AI number.
#[allow(clippy::too_many_arguments)]
fn animate(
    c: &mut Character,
    i: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    pack: Option<&mut Pack>,
    leader: Option<[f32; 3]>,
    group: Option<&SpawnGroup>,
    carrier: Option<dead::Carrier>,
) {
    const LOOK: (f32, f32) = (100.0, 96.0);
    let alive = c.health != 0;
    if let Some(m) = &sp.mee {
        if alive {
            let mut w = huntable::World {
                pack,
                leader,
                group,
            };
            if m.ai == ai::TREX {
                mee_trex::animate(c, i, sp, m, ctx, &mut w);
            } else if mee::is_ambient(m.ai) {
                mee_ambient::animate(c, i, sp, m, ctx, &mut w);
            } else if mee::is_flyer(m.ai) {
                mee_flyer::animate(c, i, sp, m, ctx, &mut w);
            } else if mee::is_fish(m.ai) {
                mee_fish::animate(c, i, sp, m, ctx, &mut w);
            } else if m.ai == mee::mee_ai::ICTH {
                mee_icth::animate(c, i, sp, m, ctx, &mut w);
            } else if m.ai == mee::mee_ai::BRACH {
                mee_brachi::animate_old(c, i, sp, m, ctx, &mut w);
            } else if mee::is_brachi(m.ai) {
                mee_brachi::animate(c, i, sp, m, ctx, &mut w);
            } else {
                huntable::animate(c, i, sp, m, ctx, &mut w);
            }
        } else if mee::is_flyer(m.ai) || m.ai == mee::mee_ai::ICTH {
            mee_flyer::animate_dead(c, i, sp, m, ctx);
        } else if mee::is_fish(m.ai) {
            mee_fish::animate_dead(c, i, sp, m, ctx);
        } else {
            huntable::animate_dead(c, i, sp, m, ctx);
        }
        return;
    }
    match c.ai {
        ai::HUNTER => dead::animate_hunter(c, i, sp, ctx, carrier),
        ai::BRACH => brachi::animate(c, i, sp, ctx),
        ai::DIMOR | ai::PTERA if alive => flyer::animate(c, i, sp, ctx),
        ai::DIMOR | ai::PTERA => flyer::animate_dead(c, i, sp, ctx),
        ai::TREX if alive => trex::animate(c, i, sp, ctx),
        ai::TREX => dead::animate(c, i, sp, ctx, trex::DIE, None, (200.0, 196.0)),
        a => {
            let t = &sp.tuning;
            if let Some(p) = HerbivoreParams::for_ai(a).map(|p| t.herbivore(p)) {
                if alive {
                    herbivore::animate(c, i, sp, ctx, &p);
                } else {
                    dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                }
            } else if let Some(p) = CarnivoreParams::for_ai(a).map(|p| t.carnivore(p)) {
                if alive {
                    carnivore::animate(c, i, sp, ctx, &p);
                } else {
                    dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                }
            } else if let Some(p) = SmallParams::for_ai(a).map(|p| t.small(p)) {
                if alive {
                    small::animate(c, i, sp, ctx, &p);
                } else {
                    dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                }
            }
            // Unknown AI numbers (mods made for patched exes) stand still.
        }
    }
}
