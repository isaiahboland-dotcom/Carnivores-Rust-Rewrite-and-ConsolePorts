//! Flying reptiles: `AnimateDimor` (Dimorphodon and Pteranodon) and
//! `AnimateDimorDead` (falls out of the sky, then lies dead).

use super::{
    corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, Species, PI,
};

// DIM_*
const FLY: usize = 0;
const FLYP: usize = 1;
const FALL: usize = 2;
const DIE: usize = 3;

pub fn animate(c: &mut Character, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let area = ctx.area;

    let (targetdx, targetdz) = loop {
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let (_, _, pdist) = c.to_hunter(&ctx.hunter);
        if pdist > (ctx.view_r + 20) as f32 * 256.0 && c.replace_forward(ctx) {
            continue;
        }
        if tdist < 1024.0 {
            c.set_new_target_place(area, ctx.rng, 4048.0);
            continue;
        }
        break (targetdx, targetdz);
    };
    c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    // Climb (flap) until high, then glide down, and repeat.
    if new_phase {
        let ground = area.land_height(c.pos[0], c.pos[2]);
        if c.phase == FLY {
            if c.pos[1] > ground + 2800.0 {
                c.phase = FLYP;
            }
        } else if c.phase == FLYP && c.pos[1] < ground + 1800.0 {
            c.phase = FLY;
        }
    }

    // Calls are rare: only 43 times in 1024.
    if (old_phase != c.phase || new_phase) && (ctx.rng.rand() & 1023) > 980 {
        c.activate_fx(index, sp, ctx);
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
    delta_func(&mut c.rspeed, currspeed, dtf / 460.0);
    let tgbend = (drspd / 2.0).min(PI / 2.0) * sgn(currspeed);
    let rate = if tgbend.abs() > c.bend.abs() {
        800.0
    } else {
        400.0
    };
    delta_func(&mut c.bend, tgbend, dtf / rate);
    let rspd = c.rspeed * dtf / 1024.0;
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

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = match c.phase {
        FLY => 1.5,
        FLYP => 1.3,
        _ => 0.0,
    };
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    let ground = area.land_height(c.pos[0], c.pos[2]);
    if c.phase == FLY {
        delta_func(&mut c.pos[1], ground + 4048.0, dtf / 6.0);
    } else {
        delta_func(&mut c.pos[1], ground, dtf / 16.0);
    }
    if c.pos[1] < ground + 236.0 {
        c.pos[1] = ground + 256.0;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dtf / 2024.0);
    c.pos[0] += c.lookx * c.vspeed * dtf;
    c.pos[2] += c.lookz * c.vspeed * dtf;

    c.tggamma = (c.rspeed / 4.0).clamp(-PI / 6.0, PI / 6.0);
    delta_func(&mut c.gamma, c.tggamma, dtf / 2048.0);
}

pub fn animate_dead(c: &mut Character, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let area = ctx.area;
    if c.phase != FALL && c.phase != DIE {
        if c.pp_morph_time > 128 {
            c.prev_phase = c.phase;
            c.prev_pf_time = c.ftime;
            c.pp_morph_time = 0;
        }
        c.ftime = 0;
        c.phase = FALL;
        c.rspeed = 0.0;
        c.activate_fx(index, sp, ctx);
        return;
    }
    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        if c.phase == DIE {
            c.ftime = sp.time(c.phase) - 1;
        } else {
            c.ftime %= sp.time(c.phase);
        }
    }
    delta_func(
        &mut c.vspeed,
        0.0,
        dtf / if c.phase == DIE { 400.0 } else { 1200.0 },
    );
    c.pos[0] += c.lookx * c.vspeed * dtf;
    c.pos[2] += c.lookz * c.vspeed * dtf;

    if c.phase == FALL {
        // rspeed doubles as the fall speed here.
        let wh = area.land_up_h(c.pos[0], c.pos[2]);
        let lh = area.land_height(c.pos[0], c.pos[2]);
        let on_water = wh > lh;
        let dy = c.rspeed * dtf / 1024.0;
        if on_water && c.pos[1] >= wh && c.pos[1] + dy < wh {
            ctx.events.push(super::DinoEvent::Splash {
                pos: [c.pos[0], wh, c.pos[2]],
            });
        }
        c.pos[1] += dy;
        c.rspeed -= dtf * 2.56;
        if c.pos[1] < lh {
            c.pos[1] = lh;
            if on_water {
                ctx.events.push(super::DinoEvent::Spray {
                    pos: [c.pos[0], lh, c.pos[2]],
                });
            }
            if c.pp_morph_time > 128 {
                c.prev_phase = c.phase;
                c.prev_pf_time = c.ftime;
                c.pp_morph_time = 0;
            }
            c.phase = DIE;
            c.ftime = 0;
            c.activate_fx(index, sp, ctx);
        }
    } else {
        c.think_y_beta_gamma(area, dt, 140.0, 126.0, 0.6, 0.5);
        delta_func(&mut c.gamma, c.tggamma, dtf / 1600.0);
    }
}
