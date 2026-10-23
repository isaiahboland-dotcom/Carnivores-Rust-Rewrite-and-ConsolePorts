//! The Modder's Edition Engine's shared AI (`Characters.cpp:
//! AnimateHuntable`, `AnimateDeadCommon`): one routine for the original's
//! herbivores and hunters (AI 10-17) and the dialect's Pachycephalosaurus
//! and Ice Age animals (19-27), steered by the AI's [`AiInfo`] constants
//! and the species' [`MeeInfo`]. Ported from the upstream release the mods
//! were made for; see docs/DECISIONS.md.

use c2_formats::mee::SpawnGroup;

use super::mee::{ai_info, mee_ai, IdleGroup, MeeInfo};
use super::{
    ai, angle_difference, corrected_alpha, delta_func, find_vector_alpha, hunt, sgn, wrap_angle,
    Character, Ctx, DinoEvent, Pack, Species, CS_ON_WATER, PI,
};
use crate::rng::MsRand;

/// What a character sees of the rest of the herd.
pub struct World<'a> {
    pub pack: Option<&'a mut Pack>,
    /// The pack leader's position.
    pub leader: Option<[f32; 3]>,
    pub group: Option<&'a SpawnGroup>,
}

/// `goto TBEGIN` repeats at most this often per step (the original has no
/// limit; a target place right next to the character could loop forever).
const MAX_RETHINK: usize = 16;

pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    w: &mut World<'_>,
) {
    let a = ai_info(c.ai);
    let raptor = a.raptor();
    let dt = ctx.dt;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let kill = m.kill(c.kill_type);
    let eating = |c: &Character| kill.is_some_and(|k| c.phase == k.anim);
    let avoid = w.group.map_or(&[][..], |g| &g.avoid);

    if (!a.carnivore || a.ice_age) && c.afraid_time != 0 {
        c.afraid_time = (c.afraid_time - dt).max(0);
    }
    let mut alert_init = c.state == 2;
    if c.state == 0 && w.pack.as_ref().is_some_and(|p| p.alert_prev) {
        alert_init = true;
    }
    if alert_init && (ctx.hunter.alive || a.carnivore) {
        if !a.carnivore {
            new_phase = true;
        }
        if a.jumper && c.phase != m.jump {
            new_phase = true;
        }
        c.state = 1;
        if c.ai == ai::SPINO || c.ai == ai::CERAT {
            c.phase = m.run;
        }
    }

    // TBEGIN.
    let mut rethink = 0;
    let (targetdx, targetdz, pdist) = loop {
        rethink += 1;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        // Distance from the mouth or horns.
        let head = match c.ai {
            ai::ALLO => 100.0 * c.scale,
            ai::CHASM
            | mee_ai::HOG
            | mee_ai::BRONT
            | mee_ai::BEAR
            | mee_ai::WOLF
            | mee_ai::RHINO
            | mee_ai::SMILO => 300.0 * c.scale,
            _ if a.carnivore => 108.0,
            _ => 0.0,
        };
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * head;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * head;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let done = (targetdx, targetdz, pdist);

        let on_water = water_flag(c, m, ctx);

        if eating(c) {
            break done;
        }
        if !ctx.hunter.alive {
            c.state = 0;
        }

        if c.state != 0 {
            c.idle_group = None;
            let a_dist = if a.carnivore && (!a.ice_age || c.ai == mee_ai::WOLF) {
                (ctx.view_r * m.aggress + ctx.opt_agres / a.agress_multi.max(1)) as f32
            } else {
                if pdist < 6000.0 && c.ai != mee_ai::DEER {
                    c.afraid_time = 8000;
                }
                (a.agress_multi * m.aggress + ctx.opt_agres / 8) as f32
            };
            // A survival wave's never backs off (ours).
            let mut flee = c.wave == 0
                && (pdist > a_dist
                    || m.aggress <= 0
                    || !c.aware_hunter
                    || (m.defensive && !c.damaged)
                    || (m.fear_shot && c.damaged)
                    || (m.fear_hear_shot && c.heard_shot));
            if !flee {
                if let Some(p) = w.pack.as_deref_mut() {
                    p.attack = true;
                }
            }
            if w.pack.as_ref().is_some_and(|p| p.attack_prev) {
                flee = false;
            }
            if flee {
                let l = pdist.max(1e-6);
                c.tgx = c.pos[0] - playerdx / l * 2048.0;
                c.tgz = c.pos[2] - playerdz / l * 2048.0;
                c.tgtime = 0;
                if a.carnivore {
                    c.afraid_time -= dt;
                }
                let calm = match w.pack.as_deref_mut() {
                    Some(p) if c.afraid_time > 0 => {
                        p.alert = true;
                        false
                    }
                    Some(p) => !p.alert_prev,
                    None => c.afraid_time <= 0,
                };
                if calm {
                    if a.carnivore {
                        c.afraid_time = 0;
                    } else {
                        c.set_new_target_place_in(ctx.area, ctx.rng, a.target_distance, w.group);
                    }
                    c.state = 0;
                }
            } else {
                c.tgx = ctx.hunter.x;
                c.tgz = ctx.hunter.z;
                c.tgtime = 0;
                if a.carnivore {
                    if let Some(p) = w.pack.as_deref_mut() {
                        p.alert = true;
                    }
                }
            }
            if a.jumper
                && !on_water
                && pdist < 1324.0 * c.scale
                && pdist > 900.0 * c.scale
                && angle_difference(c.alpha, find_vector_alpha(playerdx, playerdz)) < 0.2
            {
                c.phase = m.jump;
            }
            try_kill(c, index, m, ctx, pdist, on_water);
        }

        if rethink < MAX_RETHINK
            && pdist > ((ctx.render_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }

        if c.state == 0 {
            if c.ai == ai::VELO || c.ai == ai::CERAT || !a.carnivore {
                c.afraid_time = 0;
            }
            if rethink < MAX_RETHINK && pdist < 1024.0 && c.ai == mee_ai::DEER {
                c.state = 1;
                c.afraid_time = (6 + ctx.rng.r_rand(8)) * 1024;
                c.phase = m.run;
                continue;
            }
            if rethink < MAX_RETHINK && wander(c, sp, w, ctx, tdist, 456.0, a.target_distance) {
                continue;
            }
        }
        break done;
    };
    let on_water = c.state_f & CS_ON_WATER != 0;

    // NOTHINK.
    if pdist < a.pw_min && (a.carnivore || a.ice_age) {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.state != 0 && pdist > m.weave_range && !m.dont_weave {
            let rtd = if raptor { 824.0 } else { 1024.0 };
            c.tgalpha +=
                (c.tgalpha_offset + ctx.real_time as f32 / rtd).sin() / a.tga_increment.max(1e-3);
            wrap_angle(&mut c.tgalpha);
        }
    }

    c.look_for_a_way_avoid(ctx.area, !m.can_swim, true, avoid);
    if c.no_way_cnt > a.no_way_cnt_min {
        c.no_way_cnt = 0;
        c.no_find_cnt = a.no_find_way_med + ctx.rng.r_rand(a.no_find_way_range);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        if eating(c) && kill.is_some_and(|k| k.dont_loop) {
            c.phase = m.walk;
            c.state = 0;
        }
        new_phase = true;
    }

    // Select a new phase.
    'select: {
        if eating(c) {
            break 'select;
        }
        if a.jumper {
            if new_phase && old_phase == m.jump {
                c.phase = m.run;
                break 'select;
            }
            if c.phase == m.jump {
                break 'select;
            }
        }
        if new_phase {
            if c.state != 0 {
                c.phase = m.run;
            } else if calm_phase(c, m, ctx, on_water) {
                break 'select;
            }
        }
        if c.idle_group.is_none() {
            let turning = (c.tgalpha - c.alpha).abs();
            c.phase = if c.state == 0 {
                m.walk
            } else if !raptor || turning < 1.0 || turning > 2.0 * PI - 1.0 {
                m.run
            } else {
                m.walk
            };
        }
        if m.can_swim && on_water {
            c.phase = m.swim;
        }
        if c.ai != ai::CERAT && raptor && c.slide > 40 {
            c.phase = m.slide;
        }
    }

    // ENDPSELECT.
    end_phase(c, index, sp, m, ctx, old_phase, old_ftime, new_phase);

    // Turn.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    let skip_rot = (a.jumper && c.phase == m.jump) || eating(c) || c.idle_group.is_some();
    if !skip_rot {
        let turn = |base: f32, k: f32| {
            if drspd > 0.02 {
                if c.tgalpha > c.alpha {
                    base + drspd * k
                } else {
                    -base - drspd * k
                }
            } else {
                0.0
            }
        };
        let rspd = if raptor {
            let mut currspeed = turn(0.6, 1.2);
            if c.afraid_time != 0 {
                currspeed *= 2.5;
            }
            if dalpha > PI {
                currspeed *= -1.0;
            }
            if on_water || c.phase == m.walk {
                currspeed /= 1.4;
            }
            let rate = if c.afraid_time != 0 { 160.0 } else { 180.0 };
            delta_func(&mut c.rspeed, currspeed, dt as f32 / rate);
            let tgbend = (drspd / a.target_bend_rot_spd).min(PI / 5.0) * sgn(currspeed);
            let rate = if tgbend.abs() > c.bend.abs() {
                800.0
            } else {
                600.0
            };
            delta_func(&mut c.bend, tgbend, dt as f32 / rate);
            c.rspeed * dt as f32 / 1024.0
        } else {
            let mut currspeed = turn(a.rot1, a.rot2);
            if c.afraid_time != 0 {
                currspeed *= 1.5;
            }
            if dalpha > PI {
                currspeed *= -1.0;
            }
            // The original tests `State` here, not `StateF`: only walking
            // slows the turn.
            if c.state & CS_ON_WATER != 0 || c.phase == m.walk {
                currspeed /= 1.4;
            }
            delta_func(&mut c.rspeed, currspeed, dt as f32 / 400.0);
            let tgbend =
                (drspd / a.target_bend_rot_spd).min(PI / a.target_bend_min) * sgn(currspeed);
            let rate = if tgbend.abs() > c.bend.abs() {
                a.target_bend_delta1
            } else {
                a.target_bend_delta2
            };
            delta_func(&mut c.bend, tgbend, dt as f32 / rate);
            c.rspeed * dt as f32 / 612.0
        };
        if drspd < rspd.abs() {
            c.alpha = c.tgalpha;
        } else {
            c.alpha += rspd;
        }
        if c.alpha > PI * 2.0 {
            c.alpha -= PI * 2.0;
        }
        if c.alpha < 0.0 {
            c.alpha += PI * 2.0;
        }
    }

    // SKIPROT: a hunter overshooting a sharp turn skids.
    let skids = c.ai != ai::CERAT && raptor;
    if skids
        && c.slide == 0
        && c.vspeed > 0.6
        && (c.phase != m.jump || !a.jumper)
        && angle_difference(c.tgalpha, c.alpha) > PI * 2.0 / 3.0
    {
        c.slide = (c.vspeed * 700.0) as i32;
        c.slidex = c.lookx;
        c.slidez = c.lookz;
        c.vspeed = 0.0;
    }

    // Move.
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == m.run {
        curspeed = m.run_speed * ctx.run_k;
    }
    if c.phase == m.walk {
        curspeed = m.walk_speed;
    }
    if m.can_swim && c.phase == m.swim {
        curspeed = m.swim_speed;
    }
    if a.jumper && c.phase == m.jump {
        curspeed = m.jump_speed;
    }
    if eating(c) {
        curspeed = 0.0;
    }
    if c.phase == m.run && c.slide != 0 && skids {
        curspeed /= 8.0;
        if drspd > PI / 2.0 {
            curspeed = 0.0;
        } else if drspd > PI / 4.0 {
            curspeed *= 2.0 - 4.0 * drspd / PI;
        }
    } else if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    let wc = !m.can_swim;
    if raptor {
        delta_func(&mut c.vspeed, curspeed, dt as f32 / 500.0);
        if a.jumper && c.phase == m.jump {
            c.vspeed = m.jump_speed;
        }
        let k = c.vspeed * dt as f32 * c.scale;
        c.move_character_avoid(ctx.area, c.lookx * k, c.lookz * k, wc, avoid);
        if c.ai != ai::CERAT && c.slide != 0 {
            let k = c.slide as f32 / 600.0 * dt as f32 * c.scale;
            c.move_character_avoid(ctx.area, c.slidex * k, c.slidez * k, wc, avoid);
            c.slide = (c.slide - dt).max(0);
        }
    } else {
        curspeed *= c.scale;
        let rate = if curspeed > c.vspeed { 1024.0 } else { 256.0 };
        delta_func(&mut c.vspeed, curspeed, dt as f32 / rate);
        let k = c.vspeed * dt as f32;
        c.move_character_avoid(ctx.area, c.lookx * k, c.lookz * k, wc, avoid);
    }

    if on_water && m.can_swim {
        c.pos[1] = ctx.area.land_up_h(c.pos[0], c.pos[2]) - (m.water_level + 20) as f32 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        let [b, g, bl, gl] = a.y_beta_gamma;
        c.think_y_beta_gamma(ctx.area, dt, b, g, bl, gl);
    }
    c.tggamma += c.rspeed
        / if c.phase == m.walk {
            a.walk_target_gamma_rot
        } else {
            a.target_gamma_rot
        };
    if a.jumper && c.phase == m.jump {
        c.tggamma = 0.0;
    }
    let rate = if raptor { 1624.0 } else { 2048.0 };
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / rate);
}

/// Sets the on-water flag (`csONWATER` in `StateF`) and returns it. The
/// original clears it with `StateF &= !csONWATER`, i.e. all of `StateF`.
pub(super) fn water_flag(c: &mut Character, m: &MeeInfo, ctx: &Ctx<'_>) -> bool {
    let depth = ctx.area.land_up_h(c.pos[0], c.pos[2]) - ctx.area.land_height(c.pos[0], c.pos[2]);
    c.state_f = if depth > m.water_level as f32 * c.scale {
        c.state_f | CS_ON_WATER
    } else {
        0
    };
    c.state_f & CS_ON_WATER != 0
}

/// Kills a hunter within `killdist` and about level with it: with the
/// species' kill type (eat animation, body animation, scream) on land.
pub(super) fn try_kill(
    c: &mut Character,
    index: usize,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    pdist: f32,
    on_water: bool,
) {
    if m.kill_dist <= 0 || pdist >= m.kill_dist as f32 {
        return;
    }
    let kill_alt = m.water_level.max(256) as f32;
    if (ctx.hunter.y - c.pos[1]).abs() >= kill_alt + 20.0 {
        return;
    }
    kill_now(c, index, m, ctx, on_water);
}

/// The kill itself: the species' kill type on land (eat animation, body
/// animation, scream), else the plain one.
pub(super) fn kill_now(
    c: &mut Character,
    index: usize,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    on_water: bool,
) {
    let (kind, scream) = match m.kill(c.kill_type) {
        Some(k) if !on_water => {
            c.vspeed /= 8.0;
            c.state = 1;
            c.phase = k.anim;
            if k.dont_loop {
                c.ftime = 0;
            }
            (k.hunter_anim, k.scream)
        }
        Some(_) => (hunt::EAT, true),
        None => {
            c.state = 0;
            (hunt::EAT, true)
        }
    };
    ctx.events.push(DinoEvent::KillHunter {
        index,
        kind,
        scream,
    });
}

/// A calm animal's heading: keep near the pack leader, else roam to a new
/// place within `range` once closer than `reached` to the old one. True
/// when a new target was picked (`goto TBEGIN`).
pub(super) fn wander(
    c: &mut Character,
    sp: &Species,
    w: &World<'_>,
    ctx: &mut Ctx<'_>,
    tdist: f32,
    reached: f32,
    range: f32,
) -> bool {
    if let Some(l) = w.leader.filter(|_| c.pack.is_some()) {
        let leaderdist = ((l[0] - c.pos[0]).powi(2) + (l[2] - c.pos[2]).powi(2)).sqrt();
        if c.follow_leader {
            if leaderdist < c.pack_density * 128.0 * 0.6 {
                c.follow_leader = false;
                c.retarget(sp, ctx.area, ctx.rng, range, w.group);
                return true;
            }
        } else if leaderdist > c.pack_density * 128.0 * 1.3 {
            c.follow_leader = true;
        }
    }
    if c.follow_leader {
        if let Some(l) = w.leader {
            c.tgx = l[0];
            c.tgz = l[2];
        }
    } else if tdist < reached {
        c.retarget(sp, ctx.area, ctx.rng, range, w.group);
        return true;
    }
    false
}

/// A calm animal's next phase when one ends: an idle group's animation, or
/// walking. True when an idle was chosen (`goto ENDPSELECT`).
pub(super) fn calm_phase(
    c: &mut Character,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    on_water: bool,
) -> bool {
    let carries = m.kill(c.kill_type).is_some_and(|k| k.carry_corpse);
    if m.idle_groups.is_empty() || !(ctx.hunter.alive || !carries) || on_water {
        c.phase = m.walk;
        return false;
    }
    idle_groups(
        &mut c.phase,
        &mut c.idle_group,
        &m.idle_groups,
        m.walk,
        ctx.rng,
    )
}

/// The idle-group rules (`currentIdleGroup`): stay in the group or leave it
/// for `fallback`, else maybe start a group. True when an idle was chosen
/// (`goto ENDPSELECT`).
pub(super) fn idle_groups(
    phase: &mut usize,
    current: &mut Option<usize>,
    groups: &[IdleGroup],
    fallback: usize,
    rng: &mut MsRand,
) -> bool {
    let pick = |g: &IdleGroup, rng: &mut MsRand| {
        let i = rng.r_rand(g.anims.len() as i32 - 1) as usize;
        g.anims.get(i).copied().unwrap_or(fallback)
    };
    if let Some(g) = current.and_then(|g| groups.get(g)) {
        let last = g.anims.last().copied();
        if (rng.r_rand(127) + 1) as f32 > (1.0 - g.end) * 128.0
            && (g.end_on_any || Some(*phase) == last)
        {
            *phase = fallback;
            *current = None;
            if !g.instant_repeat {
                return true;
            }
        } else {
            *phase = pick(g, rng);
            return true;
        }
    }
    for (gi, g) in groups.iter().enumerate() {
        if (rng.r_rand(127) + 1) as f32 > (1.0 - g.start) * 128.0 {
            *current = Some(gi);
        }
    }
    match current.and_then(|g| groups.get(g)) {
        Some(g) => {
            *phase = if g.start_on_any {
                pick(g, rng)
            } else {
                g.anims.first().copied().unwrap_or(fallback)
            };
            true
        }
        None => {
            *phase = fallback;
            false
        }
    }
}

/// `ENDPSELECT`: sound on a new phase, gait time carried between walking
/// and running, and the blend from the old phase.
#[allow(clippy::too_many_arguments)]
pub(super) fn end_phase(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    old_phase: usize,
    old_ftime: i32,
    new_phase: bool,
) {
    if old_phase != c.phase || new_phase {
        c.activate_fx(index, sp, ctx);
    }
    if old_phase != c.phase {
        let gait = |p: usize| p == m.run || p == m.walk;
        if gait(old_phase) && gait(c.phase) {
            c.ftime = old_ftime * sp.time(c.phase) / sp.time(old_phase) + 64;
        } else if !new_phase {
            c.ftime = 0;
        }
        if c.pp_morph_time > 128 {
            c.prev_phase = old_phase;
            c.prev_pf_time = old_ftime;
            c.pp_morph_time = 0;
        }
    }
    c.ftime %= sp.time(c.phase);
}

/// `AnimateDeadCommon`: fall with the species' death type, then lie still
/// (or fall asleep, tranquilized).
pub fn animate_dead(c: &mut Character, index: usize, sp: &Species, m: &MeeInfo, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let d = m.death(c.death_type);
    let mut start = c.phase != d.die;
    if !d.no_sleep {
        start = start && c.phase != d.sleep;
    }
    if start {
        if c.pp_morph_time > 128 {
            c.prev_phase = c.phase;
            c.prev_pf_time = c.ftime;
            c.pp_morph_time = 0;
        }
        c.ftime = 0;
        c.phase = d.die;
        c.activate_fx(index, sp, ctx);
    } else {
        c.process_prev_phase(sp, dt);
        c.ftime += dt;
        if c.ftime >= sp.time(c.phase) {
            if ctx.tranq && !d.no_sleep {
                c.ftime = 0;
                c.phase = d.sleep;
                c.activate_fx(index, sp, ctx);
            } else {
                c.ftime = sp.time(c.phase) - 1;
            }
        }
    }
    delta_func(&mut c.vspeed, 0.0, dt as f32 / 800.0);
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;
    let look = if c.ai == ai::TREX { 200.0 } else { 100.0 };
    c.think_y_beta_gamma(ctx.area, dt, look, look - 4.0, 0.6, 0.5);
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 1600.0);
}

#[cfg(test)]
mod tests {
    use super::super::mee::{DeathType, IdleGroup, KillType};
    use super::super::{DinoInfo, Herd, HunterView};
    use super::*;
    use crate::area::Area;
    use crate::rng::MsRand;
    use c2_formats::car::{Animation, Car};

    // Logical phases of the test species: one model animation each.
    const RUN: usize = 0;
    const WALK: usize = 1;
    const JUMP: usize = 2;
    const EAT: usize = 3;
    const DIE: usize = 4;
    const IDLE: usize = 6;

    fn info(mee_ai: i32) -> MeeInfo {
        let mut m = MeeInfo::new(mee_ai);
        for a in 0..8 {
            m.add_anim(a);
        }
        (m.run, m.walk, m.jump) = (RUN, WALK, JUMP);
        m.run_speed = 1.2;
        m.walk_speed = 0.3;
        m.jump_speed = 1.5;
        m.death_types = vec![DeathType {
            die: DIE,
            sleep: 5,
            ..Default::default()
        }];
        m
    }

    fn species(m: MeeInfo) -> Species {
        let car = Car {
            name_raw: [0; 32],
            mesh: Default::default(),
            animations: (0..8)
                .map(|_| Animation {
                    name_raw: [0; 32],
                    kps: 2,
                    frames: 2,
                    data: vec![0; 6],
                })
                .collect(),
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        };
        let info = DinoInfo {
            name: "test".into(),
            file: String::new(),
            ai: m.ai,
            health0: 5,
            mass: 1.0,
            length: 1.0,
            radius: 256.0,
            base_score: 1.0,
            smell_k: 1.0,
            hear_k: 1.0,
            look_k: 1.0,
            sh_delta: 0.0,
            scale0: 1000,
            scale_a: 0,
            danger_call: false,
        };
        let mut s = Species::new(info, &car);
        s.mee = Some(Box::new(m));
        s
    }

    fn hunter() -> HunterView {
        HunterView {
            x: 512.0 * 256.0,
            z: 512.0 * 256.0,
            y: Area::flat(10).land_height(512.0 * 256.0, 512.0 * 256.0),
            head_y: 220.0,
            alive: true,
            ..Default::default()
        }
    }

    fn add(herd: &mut Herd, ctype: usize, area: &Area, x: f32, z: f32) -> usize {
        let mut c = Character {
            ctype,
            pos: [x, area.land_height(x, z), z],
            tgx: x,
            tgz: z,
            ..Default::default()
        };
        c.reset(&herd.species[ctype], &mut MsRand::new(1), 128);
        herd.chars.push(c);
        herd.chars.len() - 1
    }

    fn run(herd: &mut Herd, area: &Area, h: HunterView, ms: i32) -> Vec<DinoEvent> {
        let mut rng = MsRand::new(5);
        let mut events = Vec::new();
        for t in 0..ms / 10 {
            let mut ctx = Ctx {
                area,
                rng: &mut rng,
                dt: 10,
                real_time: t * 10,
                hunter: h,
                opt_agres: 128,
                tranq: false,
                view_r: 74,
                render_r: 74,
                run_k: 1.0,
                events: &mut events,
            };
            herd.step(&mut ctx);
        }
        events
    }

    fn dist(c: &Character, h: &HunterView) -> f32 {
        ((c.pos[0] - h.x).powi(2) + (c.pos[2] - h.z).powi(2)).sqrt()
    }

    #[test]
    fn alarmed_herbivore_runs_away() {
        let area = Area::flat(10);
        let h = hunter();
        let mut herd = Herd {
            species: vec![species(info(ai::PARA))],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 800.0, h.z);
        herd.chars[i].state = 2;
        herd.chars[i].afraid_time = 10_000;
        run(&mut herd, &area, h, 5000);
        let c = &herd.chars[i];
        assert_eq!((c.state, c.phase), (1, RUN));
        assert!(dist(c, &h) > 3000.0, "{}", dist(c, &h));
    }

    #[test]
    fn aware_wolf_charges_and_kills_with_its_kill_type() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(mee_ai::WOLF);
        m.aggress = 50;
        m.kill_dist = 300;
        m.kill_types = vec![KillType {
            hunter_anim: 3,
            offset: 150.0,
            anim: EAT,
            scream: false,
            ..Default::default()
        }];
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 2500.0, h.z);
        let c = &mut herd.chars[i];
        (c.state, c.afraid_time, c.aware_hunter) = (2, 60_000, true);
        let ev = run(&mut herd, &area, h, 10_000);
        assert!(
            ev.iter().any(|e| matches!(
                e,
                DinoEvent::KillHunter {
                    kind: 3,
                    scream: false,
                    ..
                }
            )),
            "{ev:?}"
        );
        assert_eq!(herd.chars[i].phase, EAT);
    }

    #[test]
    fn a_hurt_defensive_animal_fights_an_unhurt_one_flees() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(ai::CHASM);
        m.aggress = 50;
        m.defensive = true;
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        for damaged in [false, true] {
            herd.chars.clear();
            let i = add(&mut herd, 0, &area, h.x + 3000.0, h.z);
            let c = &mut herd.chars[i];
            (c.state, c.afraid_time, c.aware_hunter, c.damaged) = (2, 60_000, true, damaged);
            run(&mut herd, &area, h, 3000);
            let d = dist(&herd.chars[i], &h);
            assert_eq!(d < 3000.0, damaged, "damaged {damaged}: {d}");
        }
    }

    #[test]
    fn calm_animal_plays_its_idle_group_standing() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(ai::STEGO);
        m.idle_groups = vec![IdleGroup {
            start: 1.0,
            anims: vec![IDLE, IDLE + 1],
            ..Default::default()
        }];
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 9000.0, h.z);
        herd.chars[i].tgx += 4000.0;
        run(&mut herd, &area, h, 1500);
        let c = &herd.chars[i];
        assert_eq!(c.idle_group, Some(0));
        assert!(c.phase == IDLE || c.phase == IDLE + 1, "{}", c.phase);
        // The model plays animation 6 or 7 for it.
        assert!(matches!(herd.species[0].anims()[c.phase], 6 | 7));
    }

    #[test]
    fn pack_member_follows_its_leader() {
        let area = Area::flat(10);
        let h = hunter();
        let mut still = info(ai::PARA);
        still.walk_speed = 0.0;
        let mut herd = Herd {
            species: vec![species(still), species(info(ai::PARA))],
            ..Default::default()
        };
        let l = add(&mut herd, 0, &area, h.x + 9000.0, h.z);
        let f = add(&mut herd, 1, &area, h.x + 9000.0, h.z + 4000.0);
        herd.packs.push(Pack {
            leader: l,
            ..Default::default()
        });
        for c in &mut herd.chars {
            c.pack = Some(0);
            c.pack_density = 4.0;
        }
        run(&mut herd, &area, h, 20_000);
        let (a, b) = (herd.chars[l].pos, herd.chars[f].pos);
        let d = ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!(d < 1500.0, "still {d} from the leader");
    }

    #[test]
    fn dead_animal_plays_its_death_type() {
        let area = Area::flat(10);
        let h = hunter();
        let mut herd = Herd {
            species: vec![species(info(ai::PARA))],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 3000.0, h.z);
        herd.chars[i].health = 0;
        run(&mut herd, &area, h, 3000);
        let c = &herd.chars[i];
        assert_eq!(c.phase, DIE);
        assert_eq!(c.ftime, herd.species[0].time(DIE) - 1);
    }

    #[test]
    fn trex_looks_roars_then_chases_and_kills() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(ai::TREX);
        (m.looks, m.roars) = (vec![IDLE], vec![IDLE + 1]);
        m.kill_dist = 400;
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 3000.0, h.z);
        herd.chars[i].alpha = PI; // facing the hunter
        herd.chars[i].state = 2; // heard something
        let mut seen = Vec::new();
        let mut killed = false;
        for _ in 0..1500 {
            let ev = run(&mut herd, &area, h, 10);
            killed |= ev.iter().any(|e| matches!(e, DinoEvent::KillHunter { .. }));
            if seen.last() != Some(&herd.chars[i].phase) {
                seen.push(herd.chars[i].phase);
            }
            if killed {
                break;
            }
        }
        assert_eq!(&seen[..3], &[IDLE, IDLE + 1, RUN], "{seen:?}");
        assert!(killed, "{seen:?}");
    }

    #[test]
    fn ambient_bolts_then_settles_far_away() {
        let area = Area::flat(10);
        let h = hunter();
        let mut herd = Herd {
            species: vec![species(info(ai::MOSH))],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 700.0, h.z);
        run(&mut herd, &area, h, 500);
        assert_eq!((herd.chars[i].state, herd.chars[i].phase), (1, RUN));
        run(&mut herd, &area, h, 30_000);
        let c = &herd.chars[i];
        assert_eq!(c.state, 0);
        assert!(dist(c, &h) > 4096.0, "{}", dist(c, &h));
    }

    #[test]
    fn flyer_climbs_glides_and_falls_when_shot() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(ai::PTERA);
        (m.fly, m.glide) = (IDLE, IDLE + 1);
        (m.fly_speed, m.glide_speed) = (0.6, 0.6);
        (m.min_depth, m.max_depth) = (300, 1200);
        m.death_types[0].fall = JUMP;
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 9000.0, h.z);
        let mut phases = std::collections::BTreeSet::new();
        for _ in 0..300 {
            run(&mut herd, &area, h, 100);
            phases.insert(herd.chars[i].phase);
        }
        assert!(
            phases.contains(&IDLE) && phases.contains(&(IDLE + 1)),
            "{phases:?}"
        );
        let ground = area.land_height(herd.chars[i].pos[0], herd.chars[i].pos[2]);
        assert!(herd.chars[i].pos[1] > ground + 200.0);
        herd.chars[i].health = 0;
        run(&mut herd, &area, h, 10);
        assert_eq!(herd.chars[i].phase, JUMP, "falling");
        run(&mut herd, &area, h, 5000);
        let c = &herd.chars[i];
        assert_eq!(c.phase, DIE);
        assert!((c.pos[1] - area.land_height(c.pos[0], c.pos[2])).abs() < 1.0);
    }

    #[test]
    fn land_brachiosaur_charges_or_wanders() {
        let area = Area::flat(10);
        let h = hunter();
        let mut m = info(mee_ai::LANDBRACH);
        (m.aggress, m.kill_dist, m.collision_dist) = (40, 600, 128);
        m.rot_speed_multi = 1.5; // `runrotatespeed`: required for this AI
        m.run_speed = 0.4; // a slow giant, or it circles the hunter
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let i = add(&mut herd, 0, &area, h.x + 2000.0, h.z);
        let calm = add(&mut herd, 0, &area, h.x + 12000.0, h.z);
        let start = herd.chars[calm].pos;
        let c = &mut herd.chars[i];
        (c.state, c.afraid_time, c.aware_hunter) = (2, 60_000, true);
        let ev = run(&mut herd, &area, h, 20_000);
        assert!(ev
            .iter()
            .any(|e| matches!(e, DinoEvent::KillHunter { index: 0, .. })));
        let moved = ((herd.chars[calm].pos[0] - start[0]).powi(2)
            + (herd.chars[calm].pos[2] - start[2]).powi(2))
        .sqrt();
        assert!(moved > 500.0, "calm one moved {moved}");
    }

    /// A flat sea bed under deep water.
    fn sea() -> Area {
        let mut a = Area::flat(10);
        a.map.flags.fill(crate::area::cell_flags::WATER);
        a.map.water.fill(0);
        a.rsc.waters = vec![c2_formats::rsc::Water {
            texture_index: 0,
            level: 100,
            transp: 0.5,
            fog_rgb: 0,
        }];
        a
    }

    #[test]
    fn fish_keep_their_depth_and_a_danger_fish_kills_a_swimmer() {
        let area = sea();
        let (x, z) = (512.0 * 256.0, 512.0 * 256.0);
        let (up, bed) = (area.land_up_h(x, z), area.land_height(x, z));
        assert!(up - bed > 2000.0, "{up} {bed}");
        let mut h = hunter();
        h.y = up - 300.0;
        let fish = |danger: bool| {
            let mut m = info(mee_ai::FISH);
            (m.min_depth, m.max_depth, m.spacing_depth) = (200, 8000, 400);
            // Hunts within `view_r * aggress` = 74 * 80 units.
            (m.aggress, m.kill_dist, m.danger_fish) = (80, 400, danger);
            m
        };
        let mut herd = Herd {
            species: vec![species(fish(false)), species(fish(true))],
            ..Default::default()
        };
        let calm = add(&mut herd, 0, &area, h.x + 15_000.0, h.z);
        let shark = add(&mut herd, 1, &area, h.x + 3000.0, h.z);
        for c in &mut herd.chars {
            c.depth = (up + bed) / 2.0;
            c.tdepth = c.depth;
        }
        let start = herd.chars[calm].pos;
        let ev = run(&mut herd, &area, h, 20_000);
        assert!(
            ev.iter()
                .any(|e| matches!(e, DinoEvent::KillHunter { index, .. } if *index == shark)),
            "{ev:?}"
        );
        let c = &herd.chars[calm];
        assert!(
            c.pos[1] > bed + 150.0 && c.pos[1] < up - 150.0,
            "depth {}",
            c.pos[1]
        );
        assert!((c.pos[0] - start[0]).abs() + (c.pos[2] - start[2]).abs() > 500.0);
    }

    #[test]
    fn ichthyornis_takes_off_when_startled_and_comes_back_down() {
        let area = sea();
        let mut m = info(mee_ai::ICTH);
        // More logical phases over the test model's eight animations.
        m.fly = m.add_anim(2);
        m.glide = m.add_anim(3);
        m.takeoff = m.add_anim(4);
        m.land = m.add_anim(5);
        m.swim = m.add_anim(6);
        m.shake_land = m.add_anim(7);
        m.shake_water = m.add_anim(0);
        (m.fly_speed, m.glide_speed, m.takeoff_speed, m.land_speed) = (2.0, 1.8, 1.5, 0.3);
        (m.swim_speed, m.water_level) = (0.25, 40);
        let (fly, glide, takeoff) = (m.fly, m.glide, m.takeoff);
        let mut herd = Herd {
            species: vec![species(m)],
            ..Default::default()
        };
        let mut h = hunter();
        let i = add(&mut herd, 0, &area, h.x + 600.0, h.z);
        let up = area.land_up_h(h.x, h.z);
        run(&mut herd, &area, h, 8000);
        let c = &herd.chars[i];
        assert!(
            [fly, glide, takeoff].contains(&c.phase),
            "phase {}",
            c.phase
        );
        assert!(c.pos[1] > up + 1000.0, "y {} water {up}", c.pos[1]);
        // Hunter gone: after its fright it glides down and lands.
        h.x += 200_000.0;
        run(&mut herd, &area, h, 120_000);
        let c = &herd.chars[i];
        assert_eq!(c.state, 0);
        let up = area.land_up_h(c.pos[0], c.pos[2]);
        assert!(c.pos[1] < up + 300.0, "y {} water {up}", c.pos[1]);
    }
}
