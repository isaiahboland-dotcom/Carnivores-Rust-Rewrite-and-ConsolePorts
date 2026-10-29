//! Small ambient animals: `AnimateMosh` (Moschops), `AnimateDimet`
//! (Dimetrodon, identical apart from phase numbering) and `AnimateGall`
//! (Gallimimus, slightly different alarm logic). They re-spawn ahead of the
//! hunter when left far behind.

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

use super::{
    ai, corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, Species,
    CS_ON_WATER, PI,
};

#[derive(Debug, Clone, Copy)]
pub struct SmallParams {
    pub run: usize,
    pub walk: usize,
    pub idle1: usize,
    pub idle2: usize,
    pub die: usize,
    pub slp: usize,
    /// Gallimimus re-alarms itself (`State = 1`) when the hunter is near.
    pub gall: bool,
    pub gait_max: usize,
    pub bend_div: f32,
    pub run_speed: f32,
    pub walk_speed: f32,
    pub no_find_rand: i32,
}

impl SmallParams {
    pub fn for_ai(ai_id: i32) -> Option<Self> {
        // MOS_*: RUN 0, WALK 1, DIE 2, IDLE1 3, IDLE2 4, SLP 5
        let mosh = Self {
            run: 0,
            walk: 1,
            idle1: 3,
            idle2: 4,
            die: 2,
            slp: 5,
            gall: false,
            gait_max: 1,
            bend_div: 2.0,
            run_speed: 0.6,
            walk_speed: 0.3,
            no_find_rand: 80,
        };
        match ai_id {
            ai::MOSH => Some(mosh),
            // DMT_*: WALK 0, RUN 1, IDLE1 2, IDLE2 3, DIE 4, SLP 5
            ai::DIMET => Some(Self {
                walk: 0,
                run: 1,
                idle1: 2,
                idle2: 3,
                die: 4,
                slp: 5,
                ..mosh
            }),
            // GAL_*: RUN 0, WALK 1, SLIDE 2, DIE 3, IDLE1 4, IDLE2 5, SLP 6
            ai::GALL => Some(Self {
                idle1: 4,
                idle2: 5,
                die: 3,
                slp: 6,
                gall: true,
                gait_max: 2,
                bend_div: 3.0,
                run_speed: 0.9,
                walk_speed: 0.32,
                no_find_rand: 40,
                ..mosh
            }),
            _ => None,
        }
    }
}

pub fn animate(c: &mut Character, index: usize, sp: &Species, ctx: &mut Ctx<'_>, p: &SmallParams) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    if c.afraid_time != 0 {
        c.afraid_time = (c.afraid_time - dt).max(0);
    }
    if c.state == 2 {
        new_phase = true;
        c.state = 1;
    }

    let (targetdx, targetdz) = loop {
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let (playerdx, playerdz, pdist) = c.to_hunter(&ctx.hunter);

        // Run away.
        if c.state != 0 {
            if c.afraid_time == 0 {
                if pdist < 2048.0 {
                    if p.gall {
                        c.state = 1;
                    }
                    c.afraid_time = (5 + ctx.rng.r_rand(5)) * 1024;
                }
                // Mosh only calms down if that didn't re-alarm it; Gall always checks.
                if (p.gall || c.afraid_time == 0) && pdist > 4096.0 {
                    c.state = 0;
                    c.set_new_target_place(ctx.area, ctx.rng, 2048.0);
                    continue;
                }
            }
            let l = pdist.max(1e-6);
            c.tgx = c.pos[0] - playerdx / l * 2048.0;
            c.tgz = c.pos[2] - playerdz / l * 2048.0;
            c.tgtime = 0;
        }

        if pdist > (ctx.render_r + 20) as f32 * 256.0 && c.replace_forward(ctx) {
            continue;
        }

        // Explore.
        if c.state == 0 {
            c.afraid_time = 0;
            if pdist < 812.0 {
                c.state = 1;
                c.afraid_time = (5 + ctx.rng.r_rand(5)) * 1024;
                c.phase = p.run;
                continue;
            }
            if tdist < 456.0 {
                c.set_new_target_place(ctx.area, ctx.rng, 2048.0);
                continue;
            }
        }
        break (targetdx, targetdz);
    };

    // Gall's counter test is `> 0`; Mosh's is `!= 0` (same for non-negative).
    if c.no_find_cnt > 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
    }
    c.look_for_a_way(ctx.area, true, true);
    if c.no_way_cnt > 8 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 8 + ctx.rng.r_rand(p.no_find_rand);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    if new_phase {
        if c.state == 0 {
            if c.phase == p.idle1 || c.phase == p.idle2 {
                c.phase = if ctx.rng.r_rand(128) > 76 && c.phase == p.idle2 {
                    p.walk
                } else {
                    // rRand(3) / 3: idle2 one time in four.
                    p.idle1 + (ctx.rng.r_rand(3) / 3) as usize
                };
            } else {
                c.phase = if ctx.rng.r_rand(128) > 120 {
                    p.idle1
                } else {
                    p.walk
                };
            }
        } else {
            c.phase = if c.afraid_time != 0 { p.run } else { p.walk };
        }
    }
    c.finish_phase_change(index, sp, ctx, old_phase, old_ftime, new_phase, p.gait_max);

    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.phase != p.idle1 && c.phase != p.idle2 {
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
        if c.state & CS_ON_WATER != 0 || c.phase == p.walk {
            currspeed /= 1.4;
        }
        delta_func(&mut c.rspeed, currspeed, dtf / 260.0);
        let tgbend = (drspd / p.bend_div).min(PI / 2.0) * sgn(currspeed);
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
    }

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = if c.phase == p.run {
        p.run_speed * ctx.run_k
    } else if c.phase == p.walk {
        p.walk_speed
    } else {
        0.0
    };
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dtf / 1024.0);
    let (mx, mz) = (c.lookx * c.vspeed * dtf, c.lookz * c.vspeed * dtf);
    c.move_character(ctx.area, mx, mz, true);

    c.think_y_beta_gamma(ctx.area, dt, 64.0, 32.0, 0.7, 0.4);
    c.tggamma += c.rspeed / if c.phase == p.walk { 12.0 } else { 8.0 };
    delta_func(&mut c.gamma, c.tggamma, dtf / 2048.0);
}
