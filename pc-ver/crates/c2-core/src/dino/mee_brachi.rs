//! The Modder's Edition Engine's brachiosaurs (`Characters.cpp`):
//! `AnimateBrahiOld` (dialect AI 30, the original's peaceful wader) and
//! `AnimateBrahi` (34, a wader that may attack a hunter in shallow water;
//! 35, the same on land). They wade or walk between target places, idle in
//! groups and follow packs; they never run into the shared obstacle
//! checks when moving (the original's `MoveCharacter` call is commented
//! out).

use super::huntable::{calm_phase, kill_now, wander, World};
use super::mee::{mee_ai, MeeInfo};
use super::{
    check_place_collision_avoid, check_place_collision_brahi, check_place_collision_land_brahi,
    corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, Species,
    CS_ON_WATER, PI,
};
use crate::area::Area;
use c2_formats::mee::Region;

/// `goto TBEGIN` limit, as in [`super::huntable`].
const MAX_RETHINK: usize = 16;

/// `ENDPSELECT` of these AIs: no gait time carried over.
fn end_phase(
    c: &mut Character,
    index: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    old: usize,
    old_ftime: i32,
    new_phase: bool,
) {
    if old != c.phase || new_phase {
        c.activate_fx(index, sp, ctx);
    }
    if old != c.phase && c.pp_morph_time > 128 {
        c.prev_phase = old;
        c.prev_pf_time = old_ftime;
        c.pp_morph_time = 0;
    }
    c.ftime %= sp.time(c.phase);
}

/// Slow turn toward `tgalpha`, `k` times faster while afraid.
fn turn(c: &mut Character, dt: i32, k: f32) -> f32 {
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    let mut currspeed = if drspd > 0.02 {
        if c.tgalpha > c.alpha {
            0.2 + drspd * 0.2
        } else {
            -0.2 - drspd * 0.2
        }
    } else {
        0.0
    };
    if c.afraid_time != 0 {
        currspeed *= k;
    }
    if dalpha > PI {
        currspeed *= -1.0;
    }
    delta_func(&mut c.rspeed, currspeed, dt as f32 / 600.0);
    let tgbend = (drspd / 4.0).min(PI / 4.0) * sgn(currspeed);
    delta_func(&mut c.bend, tgbend, dt as f32 / 3200.0);
    let rspd = c.rspeed * dt as f32 / 1024.0;
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
    drspd
}

/// Walk (no collision), settle on the ground and tilt slowly.
fn advance(c: &mut Character, ctx: &Ctx<'_>, mut curspeed: f32, drspd: f32) {
    let dt = ctx.dt;
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dt as f32 / 1024.0);
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;
    c.think_y_beta_gamma(ctx.area, dt, 256.0, 128.0, 0.1, 0.2);
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 4048.0);
}

/// `AnimateBrahiOld`: wanders between wading spots, never alarmed.
pub fn animate_old(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    w: &mut World<'_>,
) {
    let dt = ctx.dt;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let mut rethink = 0;
    let (targetdx, targetdz) = loop {
        rethink += 1;
        c.tgtime = 0;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * 108.0;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * 108.0;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        if rethink < MAX_RETHINK {
            if pdist > ((ctx.view_r + 20) * 256) as f32 && c.replace_forward_in(sp, ctx, w.group) {
                continue;
            }
            if wander(c, sp, w, ctx, tdist, 256.0, 2048.0) {
                continue;
            }
        }
        break (targetdx, targetdz);
    };
    c.tgalpha = find_vector_alpha(targetdx, targetdz);
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    if new_phase {
        calm_phase(c, m, ctx, false);
    }
    end_phase(c, index, sp, ctx, old_phase, old_ftime, new_phase);

    let drspd = if c.idle_group.is_none() {
        turn(c, dt, 1.0)
    } else {
        let d = (c.tgalpha - c.alpha).abs();
        if d > PI {
            2.0 * PI - d
        } else {
            d
        }
    };
    let curspeed = if c.phase == m.walk { m.walk_speed } else { 0.0 };
    advance(c, ctx, curspeed, drspd);
}

/// The dangerous brachiosaur's `CheckPossiblePath`: its own wading check
/// every `collisiondist`, and (a missing `else` in the original) the
/// common land check 64 further on as well.
fn path_danger(c: &Character, area: &Area, m: &MeeInfo, avoid: &[Region]) -> i32 {
    let (lx, lz) = (c.tgalpha.cos(), c.tgalpha.sin());
    let step = m.collision_dist as f32;
    let mut p = c.pos;
    let mut n = 0;
    for _ in 0..20 {
        p[0] += lx * step;
        p[2] += lz * step;
        n += i32::from(check_place_collision_brahi(area, &mut p, true, true, avoid));
        p[0] += lx * 64.0;
        p[2] += lz * 64.0;
        n += i32::from(check_place_collision_avoid(area, &mut p, true, true, avoid));
    }
    n
}

/// The land brachiosaur's `CheckPossiblePath`.
fn path_land(c: &Character, area: &Area, m: &MeeInfo, avoid: &[Region]) -> i32 {
    let (lx, lz) = (c.tgalpha.cos(), c.tgalpha.sin());
    let step = m.collision_dist as f32;
    let mut p = c.pos;
    let mut n = 0;
    for _ in 0..20 {
        p[0] += lx * step;
        p[2] += lz * step;
        let wl = m.water_level as f32;
        n += i32::from(check_place_collision_land_brahi(
            area,
            &mut p,
            !m.can_swim,
            true,
            avoid,
            wl,
        ));
    }
    n
}

/// `AnimateBrahi`: like the old one, but an aware brachiosaur with
/// `aggress` charges a hunter standing in water no deeper than 550 (and
/// within `128 * aggress`), kills within `killdist`, else flees; one that
/// strays deeper than 550 heads back to its depth band.
pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    w: &mut World<'_>,
) {
    let dt = ctx.dt;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let kill = m.kill(c.kill_type);
    let eating = |c: &Character| kill.is_some_and(|k| c.phase == k.anim);
    let avoid = w.group.map_or(&[][..], |g| &g.avoid);
    let area = ctx.area;
    let depth = |x: f32, z: f32| area.land_up_h(x, z) - area.land_height(x, z);
    let attack_dist = (128 * m.aggress + ctx.opt_agres / 8) as f32;
    let attackable = depth(ctx.hunter.x, ctx.hunter.z) <= 550.0;
    let mut auto_correct = false;

    let mut rethink = 0;
    let (targetdx, targetdz, pdist) = loop {
        rethink += 1;
        let again = rethink < MAX_RETHINK;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * 108.0;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * 108.0;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let done = (targetdx, targetdz, pdist);
        if c.state == 2 || (c.state == 0 && w.pack.as_ref().is_some_and(|p| p.alert_prev)) {
            new_phase = true;
            c.state = 1;
        }
        // Clears all of `StateF` otherwise, as the original.
        c.state_f = if depth(c.pos[0], c.pos[2]) > 140.0 * c.scale {
            c.state_f | CS_ON_WATER
        } else {
            0
        };
        let on_water = c.state_f & CS_ON_WATER != 0;
        if eating(c) {
            break done;
        }
        if !ctx.hunter.alive {
            c.state = 0;
        }
        if c.state != 0 {
            c.idle_group = None;
            let mut flee = pdist > attack_dist
                || !attackable
                || m.aggress <= 0
                || !c.aware_hunter
                || (m.defensive && !c.damaged)
                || (m.fear_shot && c.damaged)
                || (m.fear_hear_shot && c.heard_shot);
            if !flee {
                if let Some(p) = w.pack.as_deref_mut() {
                    p.attack = true;
                }
            }
            if w.pack.as_ref().is_some_and(|p| p.attack_prev) {
                flee = false;
            }
            if !auto_correct {
                if depth(c.pos[0], c.pos[2]) > 550.0 && again {
                    auto_correct = true;
                    c.retarget(sp, ctx.area, ctx.rng, 2048.0, w.group);
                    continue;
                } else if !flee {
                    c.tgx = ctx.hunter.x;
                    c.tgz = ctx.hunter.z;
                    c.tgtime = 0;
                    if let Some(p) = w.pack.as_deref_mut() {
                        p.alert = true;
                    }
                } else {
                    let l = pdist.max(1e-6);
                    c.tgx = c.pos[0] - playerdx / l * 2048.0;
                    c.tgz = c.pos[2] - playerdz / l * 2048.0;
                    c.tgtime = 0;
                    c.afraid_time -= dt;
                    let calm = match w.pack.as_deref_mut() {
                        Some(p) if c.afraid_time > 0 => {
                            p.alert = true;
                            false
                        }
                        Some(p) => !p.alert_prev,
                        None => c.afraid_time <= 0,
                    };
                    if calm {
                        c.afraid_time = 0;
                        c.state = 0;
                    }
                }
            }
            if m.kill_dist > 0
                && pdist < m.kill_dist as f32
                && (ctx.hunter.y - c.pos[1] - 120.0).abs() < 256.0
            {
                kill_now(c, index, m, ctx, on_water);
            }
        }
        if again
            && pdist > ((ctx.view_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }
        if c.state == 0 {
            c.afraid_time = 0;
            if again && wander(c, sp, w, ctx, tdist, 256.0, 2048.0) {
                continue;
            }
        }
        break done;
    };

    // NOTHINK.
    let weave = |c: &mut Character, ctx: &Ctx<'_>| {
        if c.state != 0 && pdist > m.weave_range && !m.dont_weave {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / 4.0;
            wrap_angle(&mut c.tgalpha);
        }
    };
    if (c.ai == mee_ai::LANDBRACH || c.state != 0) && !auto_correct {
        if pdist < 2048.0 {
            c.no_find_cnt = 0;
        }
        if c.no_find_cnt != 0 {
            c.no_find_cnt -= 1;
        } else {
            c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
            weave(c, ctx);
        }
        if c.ai == mee_ai::LANDBRACH {
            c.look_for_a_way_by(|c| path_land(c, area, m, avoid));
        } else {
            c.look_for_a_way_by(|c| path_danger(c, area, m, avoid));
        }
        if c.no_way_cnt > 12 {
            c.no_way_cnt = 0;
            c.no_find_cnt = 16 + ctx.rng.r_rand(20);
        }
    } else {
        c.tgalpha = find_vector_alpha(targetdx, targetdz);
        weave(c, ctx);
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
    if !eating(c) && new_phase {
        if c.state != 0 {
            c.phase = m.run;
        } else {
            calm_phase(c, m, ctx, false);
        }
    }
    end_phase(c, index, sp, ctx, old_phase, old_ftime, new_phase);

    let drspd = if eating(c) || c.idle_group.is_some() {
        let d = (c.tgalpha - c.alpha).abs();
        if d > PI {
            2.0 * PI - d
        } else {
            d
        }
    } else {
        turn(c, dt, m.rot_speed_multi)
    };
    let mut curspeed = 0.0;
    if c.phase == m.walk {
        curspeed = m.walk_speed;
    }
    if c.phase == m.run {
        curspeed = m.run_speed * ctx.run_k;
    }
    if eating(c) {
        curspeed = 0.0;
    }
    advance(c, ctx, curspeed, drspd);
}
