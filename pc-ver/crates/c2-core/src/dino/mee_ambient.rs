//! The Modder's Edition Engine's small ambient animals
//! (`Characters.cpp: AnimateClassicAmbient`, dialect AI 1, 2, 5 and 6):
//! wander, idle in groups, bolt from a close hunter and zig-zag away,
//! calming down beyond 16 cells. `dash` animals speed up when the hunter is
//! very close.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use super::huntable::{calm_phase, end_phase, water_flag, World};
use super::mee::MeeInfo;
use super::{
    ai, corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, Species,
    CS_ON_WATER, PI,
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
    let avoid = w.group.map_or(&[][..], |g| &g.avoid);

    if c.afraid_time != 0 {
        c.afraid_time = (c.afraid_time - dt).max(0);
    }
    if c.state == 2 || (c.state == 0 && w.pack.as_ref().is_some_and(|p| p.alert_prev)) {
        new_phase = true;
        c.state = 1;
    }

    // TBEGIN.
    let mut rethink = 0;
    let (targetdx, targetdz, pdist) = loop {
        rethink += 1;
        let again = rethink < MAX_RETHINK;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = ctx.hunter.x - c.pos[0];
        let playerdz = ctx.hunter.z - c.pos[2];
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let done = (targetdx, targetdz, pdist);
        water_flag(c, m, ctx);

        if c.state != 0 {
            c.idle_group = None;
            if c.afraid_time == 0 {
                if pdist < 2048.0 {
                    c.afraid_time = (5 + ctx.rng.r_rand(5)) * 1024;
                    if let Some(p) = w.pack.as_deref_mut() {
                        p.alert = true;
                    }
                }
                // Far enough (and the pack calm): settle down.
                if pdist > 4096.0 && !w.pack.as_ref().is_some_and(|p| p.alert_prev) && again {
                    c.state = 0;
                    c.set_new_target_place_in(ctx.area, ctx.rng, 2048.0, w.group);
                    continue;
                }
            } else if let Some(p) = w.pack.as_deref_mut() {
                p.alert = true;
            }
            let l = pdist.max(1e-6);
            c.tgx = c.pos[0] - playerdx / l * 2048.0;
            c.tgz = c.pos[2] - playerdz / l * 2048.0;
            c.tgtime = 0;
        }

        if again
            && pdist > ((ctx.render_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }

        if c.state == 0 {
            c.afraid_time = 0;
            if pdist < 812.0 && again {
                c.state = 1;
                c.afraid_time = (5 + ctx.rng.r_rand(5)) * 1024;
                c.phase = m.run;
                continue;
            }
            if again && super::huntable::wander(c, sp, w, ctx, tdist, 456.0, 2048.0) {
                continue;
            }
        }
        break done;
    };
    let on_water = c.state_f & CS_ON_WATER != 0;

    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.state != 0 && pdist > m.weave_range && !m.dont_weave {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / 2.0;
            wrap_angle(&mut c.tgalpha);
        }
    }
    c.look_for_a_way_avoid(ctx.area, !m.can_swim, true, avoid);
    if c.no_way_cnt > 8 {
        c.no_way_cnt = 0;
        let range = if c.ai == ai::GALL { 40 } else { 80 };
        c.no_find_cnt = 8 + ctx.rng.r_rand(range);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    // `dash`: up to half again as fast within 600 units of the hunter.
    let dash = if m.dash && pdist < 600.0 {
        ((600.0 - pdist) / 600.0).min(0.5)
    } else {
        0.0
    };
    c.ftime += (dt as f32 * dash) as i32;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }

    // Select a new phase.
    'select: {
        if new_phase {
            if c.state != 0 {
                c.phase = m.run;
            } else if calm_phase(c, m, ctx, on_water) {
                break 'select;
            }
        }
        if m.can_swim && on_water {
            c.phase = m.swim;
        }
    }
    end_phase(c, index, sp, m, ctx, old_phase, old_ftime, new_phase);

    // Turn.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.idle_group.is_none() {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.8 + drspd * 1.4
            } else {
                -0.8 - drspd * 1.4
            }
        } else {
            0.0
        };
        if c.afraid_time != 0 {
            currspeed *= 1.5;
        }
        if dalpha > PI {
            currspeed *= -1.0;
        }
        // `State & csONWATER` in the original: only walking slows the turn.
        if c.state & CS_ON_WATER != 0 || c.phase == m.walk {
            currspeed /= 1.4;
        }
        delta_func(&mut c.rspeed, currspeed, dt as f32 / 260.0);
        let div = if c.ai == ai::GALL { 3.0 } else { 2.0 };
        let tgbend = (drspd / div).min(PI / 2.0) * sgn(currspeed);
        let rate = if tgbend.abs() > c.bend.abs() {
            800.0
        } else {
            400.0
        };
        delta_func(&mut c.bend, tgbend, dt as f32 / rate);
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
    curspeed += m.run_speed * dash;
    if m.can_swim && c.phase == m.swim {
        curspeed = m.swim_speed;
    }
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dt as f32 / 1024.0);
    let k = c.vspeed * dt as f32;
    c.move_character_avoid(ctx.area, c.lookx * k, c.lookz * k, !m.can_swim, avoid);

    if on_water && m.can_swim {
        c.pos[1] = ctx.area.land_up_h(c.pos[0], c.pos[2]) - (m.water_level + 20) as f32 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        c.think_y_beta_gamma(ctx.area, dt, 64.0, 32.0, 0.7, 0.4);
    }
    c.tggamma += c.rspeed / if c.phase == m.walk { 12.0 } else { 8.0 };
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2048.0);
}
