//! The Modder's Edition Engine's T-Rex (`Characters.cpp: AnimateTRex`): the
//! original's hunter with the dialect's data. It does not flee; once it
//! notices the hunter it stops to look or sniff (`lookAnim`, `smellAnim`,
//! else its roar), then chases. Shares the kill, idle and pack rules with
//! [`super::huntable`].

use super::huntable::{calm_phase, end_phase, try_kill, wander, water_flag, World};
use super::mee::MeeInfo;
use super::{
    angle_difference, corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character,
    Ctx, Species, CS_ON_WATER, PI,
};

/// `goto TBEGIN` limit, as in [`super::huntable`].
const MAX_RETHINK: usize = 16;

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

    // TBEGIN.
    let mut rethink = 0;
    let (targetdx, targetdz, pdist) = loop {
        rethink += 1;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * 108.0;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * 108.0;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let palpha = find_vector_alpha(playerdx, playerdz);
        let done = (targetdx, targetdz, pdist);

        // Hurt (state 5) or its pack alarmed: start hunting.
        let alert = c.state == 5 || (c.state == 0 && w.pack.as_ref().is_some_and(|p| p.alert_prev));
        if alert && rethink < MAX_RETHINK {
            new_phase = true;
            c.state = 1;
            c.phase = m.walk;
            c.ftime = 0;
            c.tgx = ctx.hunter.x;
            c.tgz = ctx.hunter.z;
            continue;
        }
        if c.state != 0 {
            if let Some(p) = w.pack.as_deref_mut() {
                p.alert = true;
            }
        }
        let on_water = water_flag(c, m, ctx);
        if eating(c) {
            break done;
        }

        if c.state != 0 {
            c.idle_group = None;
            c.tgx = ctx.hunter.x;
            c.tgz = ctx.hunter.z;
            c.tgtime = 0;
            // Noticed it (2: saw or heard, 3: smelt): face it, then look
            // or sniff before the chase.
            if c.state > 1 && angle_difference(c.alpha, palpha) < 0.4 {
                let first = if c.state == 2 { &m.looks } else { &m.smells };
                if !first.is_empty() {
                    c.phase = first[ctx.rng.r_rand(first.len() as i32 - 1) as usize];
                    c.rspeed = 0.0;
                } else if !m.roars.is_empty() {
                    c.phase = c.roar_anim;
                    c.rspeed = 0.0;
                } else {
                    c.phase = m.run;
                }
                c.state = 1;
            }
            if ctx.hunter.alive {
                try_kill(c, index, m, ctx, pdist, on_water);
            }
        }

        if rethink < MAX_RETHINK
            && pdist > ((ctx.view_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }
        if c.state == 0 && rethink < MAX_RETHINK && wander(c, sp, w, ctx, tdist, 1224.0, 8048.0) {
            continue;
        }
        break done;
    };
    let on_water = c.state_f & CS_ON_WATER != 0;

    // NOTHINK.
    if pdist < 2048.0 {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.state != 0 && pdist > m.weave_range && !m.dont_weave {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / 6.0;
            wrap_angle(&mut c.tgalpha);
        }
    }
    let mc = c.state == 0 || m.trex_obj_collide;
    c.look_for_a_way_avoid(ctx.area, !m.can_swim, mc, avoid);
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + ctx.rng.r_rand(20);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);
    let look_mode = m.looks.contains(&c.phase) || m.smells.contains(&c.phase);

    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        if eating(c) && kill.is_some_and(|k| k.dont_loop) {
            c.phase = m.walk;
            c.state = 0;
        }
        new_phase = true;
    }
    let roaring = |c: &Character| !m.roars.is_empty() && c.phase == c.roar_anim;

    // Select a new phase.
    'select: {
        if eating(c) || (!new_phase && roaring(c)) {
            break 'select;
        }
        if c.state == 0 && new_phase && calm_phase(c, m, ctx, on_water) {
            break 'select;
        }
        if !new_phase && (look_mode || c.idle_group.is_some()) {
            break 'select;
        }
        // A look or sniff ends in a roar.
        if c.state != 0 && new_phase && look_mode && !m.roars.is_empty() {
            c.phase = c.roar_anim;
            break 'select;
        }
        let turning = (c.tgalpha - c.alpha).abs();
        c.phase = if c.state != 1 {
            m.walk
        } else if turning < 1.0 || turning > 2.0 * PI - 1.0 {
            m.run
        } else {
            m.walk
        };
        if m.can_swim && on_water {
            c.phase = m.swim;
        }
    }
    end_phase(c, index, sp, m, ctx, old_phase, old_ftime, new_phase);

    // Turn, unless standing to roar, eat, look or idle.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if !(roaring(c) || eating(c) || look_mode || c.idle_group.is_some()) {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.7 + drspd * 1.4
            } else {
                -0.7 - drspd * 1.4
            }
        } else {
            0.0
        };
        if c.afraid_time != 0 {
            currspeed *= 2.5;
        }
        if dalpha > PI {
            currspeed *= -1.0;
        }
        let rate = if c.state != 0 { 440.0 } else { 620.0 };
        delta_func(&mut c.rspeed, currspeed, dt as f32 / rate);
        let tgbend = (drspd / 2.0).min(PI / 6.0) * sgn(currspeed);
        delta_func(&mut c.bend, tgbend, dt as f32 / 1800.0);
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
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dt as f32 / 200.0);
    let k = c.vspeed * dt as f32 * c.scale;
    c.move_character_avoid(ctx.area, c.lookx * k, c.lookz * k, !m.can_swim, avoid);

    if on_water && m.can_swim {
        c.pos[1] = ctx.area.land_up_h(c.pos[0], c.pos[2]) - (m.water_level - 20) as f32 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        c.think_y_beta_gamma(ctx.area, dt, 348.0, 324.0, 0.5, 0.4);
    }
    c.tggamma += c.rspeed / if c.phase == m.walk { 16.0 } else { 12.0 };
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2024.0);
}
