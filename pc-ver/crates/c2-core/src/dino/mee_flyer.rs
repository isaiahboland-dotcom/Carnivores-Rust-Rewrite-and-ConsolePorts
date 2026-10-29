//! The Modder's Edition Engine's flyers (`Characters.cpp: AnimateDimor`,
//! dialect AI 3 and 4; `AnimateWaryDimor`, 36) and the fall of a shot flyer
//! (`AnimateIcthDead`). A flyer flaps up to `maxalt * 1.5` above the
//! ground, glides down once above `maxalt`, flaps again below `minalt`. The
//! plain ones never notice the hunter; the wary one is spooked by him
//! within 812, or by what it sees, hears and smells (`CheckAfraid`), flies
//! off weaving with an alarm call, and calms beyond 4096.

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

use super::huntable::{wander, World};
use super::mee::{mee_ai, MeeInfo};
use super::{
    corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, DinoEvent,
    Species, PI,
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
    let wary = m.ai == mee_ai::WARYDIMOR;
    let a = super::mee::ai_info(m.ai);
    let mut new_phase = false;
    if wary {
        c.afraid_time = (c.afraid_time - dt).max(0);
        let pack_alarm = w.pack.as_ref().is_some_and(|p| p.alert_prev);
        if c.state == 2 || (c.state == 0 && pack_alarm) {
            new_phase = true;
            c.state = 1;
        }
    }
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let ground = |c: &Character, ctx: &Ctx<'_>| ctx.area.land_height(c.pos[0], c.pos[2]);

    // TBEGIN.
    let mut rethink = 0;
    let (targetdx, targetdz) = loop {
        rethink += 1;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let (playerdx, playerdz) = (ctx.hunter.x - c.pos[0], ctx.hunter.z - c.pos[2]);
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        if rethink < MAX_RETHINK {
            if pdist > ((ctx.render_r + 20) * 256) as f32 && c.replace_forward_in(sp, ctx, w.group)
            {
                continue;
            }
            if wary && c.state != 0 {
                // Away from the hunter; scared a while when he is near;
                // calm again beyond 4096 (unless the pack is alarmed).
                let l = pdist.max(1e-6);
                c.tgx = c.pos[0] - playerdx / l * 2048.0;
                c.tgz = c.pos[2] - playerdz / l * 2048.0;
                c.tgtime = 0;
                if c.afraid_time == 0 {
                    if pdist < 2048.0 {
                        c.afraid_time = (15 + ctx.rng.r_rand(5)) * 1024;
                        if let Some(p) = w.pack.as_deref_mut() {
                            p.alert = true;
                        }
                    }
                    let calm = match w.pack.as_deref_mut() {
                        Some(p) if pdist > 4096.0 => !p.alert_prev,
                        Some(p) => {
                            p.alert = true;
                            false
                        }
                        None => pdist > 4096.0,
                    };
                    if calm {
                        c.retarget(sp, ctx.area, ctx.rng, a.target_distance, w.group);
                        c.state = 0;
                    }
                } else if let Some(p) = w.pack.as_deref_mut() {
                    p.alert = true;
                }
            }
            if !wary || c.state == 0 {
                if wary {
                    c.alarm_called = false;
                    c.afraid_time = 0;
                    if pdist < 812.0 {
                        c.state = 1;
                        c.afraid_time = (5 + ctx.rng.r_rand(5)) * 1024;
                        continue;
                    }
                }
                if wander(c, sp, w, ctx, tdist, 1024.0, 4048.0) {
                    continue;
                }
            }
        }
        break (targetdx, targetdz);
    };
    c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
    let pdist = ((ctx.hunter.x - c.pos[0]).powi(2) + (ctx.hunter.z - c.pos[2]).powi(2)).sqrt();
    if wary && c.state != 0 && pdist > m.weave_range && !m.dont_weave {
        c.tgalpha +=
            (c.tgalpha_offset + ctx.real_time as f32 / 1024.0).sin() / a.tga_increment.max(1e-3);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    if new_phase {
        let g = ground(c, ctx);
        if c.phase == m.fly {
            if c.pos[1] > g + m.max_depth as f32 {
                c.phase = m.glide;
            }
        } else if c.phase == m.glide {
            if c.pos[1] < g + m.min_depth as f32 {
                c.phase = m.fly;
            }
        } else {
            c.phase = m.fly;
        }
    }
    // Calls now and then; the wary one its alarm, once alarmed and then
    // now and then while it is.
    let alarm = |c: &Character, ctx: &mut Ctx<'_>| {
        if let Some(sound) = m.alarm_call {
            ctx.events.push(DinoEvent::Sound {
                index,
                sound,
                pos: c.pos,
            });
        }
    };
    if old_phase != c.phase || new_phase {
        if wary && c.state != 0 && !c.alarm_called {
            c.alarm_called = true;
            alarm(c, ctx);
        } else if (ctx.rng.rand() & 1023) > 980 {
            if wary && c.state != 0 && m.alarm_call.is_some() {
                alarm(c, ctx);
            } else {
                c.activate_fx(index, sp, ctx);
            }
        }
    }
    if old_phase != c.phase {
        if !new_phase {
            c.ftime = 0;
        }
        if c.pp_morph_time > 128 {
            c.prev_phase = old_phase;
            c.prev_pf_time = old_ftime;
            c.pp_morph_time = 0;
        }
    }
    c.ftime %= sp.time(c.phase);

    // Turn.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    let mut currspeed = if drspd > 0.02 {
        if c.tgalpha > c.alpha {
            0.6 + drspd * 1.2
        } else {
            -0.6 - drspd * 1.2
        }
    } else {
        0.0
    };
    if dalpha > PI {
        currspeed *= -1.0;
    }
    delta_func(&mut c.rspeed, currspeed, dt as f32 / 460.0);
    let tgbend = (drspd / 2.0).min(PI / 2.0) * sgn(currspeed);
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

    // Fly.
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == m.fly {
        curspeed = m.fly_speed;
    }
    if c.phase == m.glide {
        curspeed = m.glide_speed;
    }
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    let g = ground(c, ctx);
    if c.phase == m.fly {
        delta_func(&mut c.pos[1], g + m.max_depth as f32 * 1.5, dt as f32 / 6.0);
    } else {
        delta_func(&mut c.pos[1], g, dt as f32 / 16.0);
    }
    if c.pos[1] < g + 236.0 {
        c.pos[1] = g + 256.0;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dt as f32 / 2024.0);
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;
    c.tggamma = (c.rspeed / 4.0).clamp(-PI / 6.0, PI / 6.0);
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2048.0);
}

/// `AnimateIcthDead`: a shot flyer falls (its death type's `fallAnim`),
/// splashes or lands, then plays `dieAnim` (or a water death on water)
/// and lies still.
pub fn animate_dead(c: &mut Character, index: usize, sp: &Species, m: &MeeInfo, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let d = m.death(c.death_type);
    let icth = c.ai == mee_ai::ICTH;
    let water_dies = !m.water_dies.is_empty();
    // Down: dying, asleep (Ichthyornis) or dying in water.
    let down = |c: &Character| {
        c.phase == d.die
            || (icth && c.phase == d.sleep)
            || (water_dies && c.phase == c.water_die_anim)
    };
    c.bend = 0.0;
    if c.phase != d.fall && !down(c) {
        c.death_phase = c.phase;
        if c.pp_morph_time > 128 {
            c.prev_phase = c.phase;
            c.prev_pf_time = c.ftime;
            c.pp_morph_time = 0;
        }
        c.ftime = 0;
        c.phase = d.fall;
        c.rspeed = 0.0;
        c.activate_fx(index, sp, ctx);
        return;
    }
    c.process_prev_phase(sp, dt);
    let wh = ctx.area.land_up_h(c.pos[0], c.pos[2]);
    let lh = ctx.area.land_height(c.pos[0], c.pos[2]);
    let on_water = water_dies && wh > lh;
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        if down(c) {
            if c.can_sleep {
                c.ftime = 0;
                c.phase = d.sleep;
                c.activate_fx(index, sp, ctx);
            } else {
                c.ftime = sp.time(c.phase) - 1;
            }
        } else {
            c.ftime %= sp.time(c.phase);
        }
    }
    let rate = if down(c) { 400.0 } else { 1200.0 };
    delta_func(&mut c.vspeed, 0.0, dt as f32 / rate);
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;

    if c.phase == d.fall {
        // `rspeed` is the fall speed here.
        let next = c.pos[1] + c.rspeed * dt as f32 / 1024.0;
        if on_water && c.pos[1] >= wh && next < wh {
            ctx.events.push(DinoEvent::Splash { pos: c.pos });
        }
        c.pos[1] = next;
        c.rspeed -= dt as f32 * 2.56;
        if c.pos[1] <= wh {
            c.pos[1] = wh;
            if c.pp_morph_time > 128 {
                c.prev_phase = c.phase;
                c.prev_pf_time = c.ftime;
                c.pp_morph_time = 0;
            }
            c.phase = if on_water { c.water_die_anim } else { d.die };
            c.ftime = 0;
            c.activate_fx(index, sp, ctx);
        }
        // Only Ichthyornis sleeps off a dart, if it was not flying.
        c.can_sleep = ctx.tranq
            && !on_water
            && icth
            && (c.death_phase == m.walk || c.death_phase == m.shake_land || c.idle_group.is_some());
    } else {
        c.think_y_beta_gamma(ctx.area, dt, 140.0, 126.0, 0.6, 0.5);
        delta_func(&mut c.gamma, c.tggamma, dt as f32 / 1600.0);
    }
    if water_dies && c.phase == c.water_die_anim {
        c.pos[1] = wh;
        c.gamma = 0.0;
        c.beta = 0.0;
        c.alpha = 0.0;
    }
}
