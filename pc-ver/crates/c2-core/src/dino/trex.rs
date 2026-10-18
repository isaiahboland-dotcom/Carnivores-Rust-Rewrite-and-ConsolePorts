//! `AnimateTRex` / `AnimateTRexDead`.
//!
//! The T-Rex doesn't flee. When it notices the hunter (`State` 2 = saw,
//! 3 = smelled, via `CheckAfraid`/`MakeNoise`) it turns, stops to look or
//! sniff, roars, then charges. Its kill is a scripted grab (`HUNT_KILL`).

use super::{
    angle_difference, corrected_alpha, delta_func, find_vector_alpha, hunt, sgn, wrap_angle,
    Character, Ctx, DinoEvent, Species, CS_ON_WATER, PI,
};

// REX_*
const RUN: usize = 0;
const WALK: usize = 1;
const SCREAM: usize = 2;
const SWIM: usize = 3;
const SEE: usize = 4;
const SEE1: usize = 5;
const SMEL: usize = 6;
const SMEL1: usize = 7;
pub const DIE: usize = 8;
const EAT: usize = 9;

pub fn animate(c: &mut Character, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let area = ctx.area;

    let (targetdx, targetdz, pdist) = loop {
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * 108.0;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * 108.0;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let palpha = find_vector_alpha(playerdx, playerdz);
        if c.state == 5 {
            new_phase = true;
            c.state = 1;
            c.phase = WALK;
            c.ftime = 0;
            c.tgx = ctx.hunter.x;
            c.tgz = ctx.hunter.z;
            continue;
        }
        if area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2])
            > 560.0 * c.scale
        {
            c.state_f |= CS_ON_WATER;
        } else {
            c.state_f = 0; // `StateF &= (!csONWATER)`, as in the original
        }

        if c.phase != EAT {
            if !ctx.hunter.alive {
                c.state = 0;
            }
            if c.state != 0 {
                c.tgx = ctx.hunter.x;
                c.tgz = ctx.hunter.z;
                c.tgtime = 0;
                // Facing the hunter after being alerted: look or sniff.
                if c.state > 1 && angle_difference(c.alpha, palpha) < 0.4 {
                    c.phase = if c.state == 2 {
                        SEE1 + ctx.rng.r_rand(1) as usize
                    } else {
                        SMEL + ctx.rng.r_rand(1) as usize
                    };
                    c.state = 1;
                    c.rspeed = 0.0;
                }
                if pdist < 380.0 && (ctx.hunter.y - c.pos[1]).abs() < 256.0 {
                    c.vspeed /= 8.0;
                    c.state = 1;
                    c.phase = EAT;
                    if ctx.hunter.alive {
                        ctx.events.push(DinoEvent::KillHunter {
                            index,
                            kind: hunt::KILL,
                        });
                    }
                    c.bend = 0.0;
                }
            }
            if c.state == 0 && tdist < 1224.0 {
                c.set_new_target_place(area, ctx.rng, 8048.0);
                continue;
            }
        }
        break (targetdx, targetdz, pdist);
    };

    // NOTHINK:
    if pdist < 2048.0 {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.state != 0 && pdist > 5648.0 {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / 6.0;
            wrap_angle(&mut c.tgalpha);
        }
    }
    // Ignores trees while hunting (`mc = !State`).
    let mc = c.state == 0;
    c.look_for_a_way(area, false, mc);
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + ctx.rng.r_rand(20);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    let look_mode = matches!(c.phase, SEE | SEE1 | SMEL | SMEL1);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    'select: {
        if c.phase == EAT || (!new_phase && c.phase == SCREAM) {
            break 'select;
        }
        if c.state == 0 && new_phase && ctx.rng.r_rand(128) > 110 {
            c.phase = if ctx.rng.r_rand(128) > 64 {
                SEE1 + ctx.rng.r_rand(1) as usize
            } else {
                SMEL + ctx.rng.r_rand(1) as usize
            };
            break 'select;
        }
        if !new_phase && look_mode {
            break 'select;
        }
        if c.state != 0 && new_phase && look_mode {
            c.phase = SCREAM;
            break 'select;
        }
        let d = (c.tgalpha - c.alpha).abs();
        c.phase = if c.state == 0 || c.state > 1 {
            WALK
        } else if d < 1.0 || d > 2.0 * PI - 1.0 {
            RUN
        } else {
            WALK
        };
        if c.state_f & CS_ON_WATER != 0 {
            c.phase = SWIM;
        }
    }
    c.finish_phase_change(index, sp, ctx, old_phase, old_ftime, new_phase, 1);

    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.phase != SCREAM && c.phase != EAT && !look_mode {
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
        delta_func(&mut c.rspeed, currspeed, dtf / rate);
        let tgbend = (drspd / 2.0).min(PI / 6.0) * sgn(currspeed);
        delta_func(&mut c.bend, tgbend, dtf / 1800.0);
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
    }

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = match c.phase {
        RUN => 2.49,
        WALK => 0.76,
        SWIM => 0.70,
        _ => 0.0,
    };
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dtf / 200.0);
    let s = c.vspeed * dtf * c.scale;
    let (mx, mz) = (c.lookx * s, c.lookz * s);
    c.move_character(area, mx, mz, false);

    if c.state_f & CS_ON_WATER != 0 {
        c.pos[1] = area.land_up_h(c.pos[0], c.pos[2]) - 540.0 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        c.think_y_beta_gamma(area, dt, 348.0, 324.0, 0.5, 0.4);
    }
    c.tggamma += c.rspeed / if c.phase == WALK { 16.0 } else { 12.0 };
    delta_func(&mut c.gamma, c.tggamma, dtf / 2024.0);
}
