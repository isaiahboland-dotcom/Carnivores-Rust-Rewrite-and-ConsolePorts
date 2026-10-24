//! `AnimateBrahi`: Brachiosaurus wading in shallow water. Never alarmed,
//! never killed (the dispatcher calls it regardless of health).

use super::{delta_func, find_vector_alpha, sgn, wrap_angle, Character, Ctx, Species, PI};

// BRA_*: WALK 0, IDLE1 1, IDLE2 2, IDLE3 3
const WALK: usize = 0;
const IDLE1: usize = 1;

impl Character {
    /// `SetNewTargetPlace_Brahi`: a point where the water is 200..400 deep.
    fn set_new_target_place_brahi(&mut self, ctx: &mut Ctx<'_>, r: f32) {
        let mut tr = 0;
        loop {
            let x = (self.pos[0] + ctx.rng.si_rand(r as i32) as f32)
                .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX);
            let z = (self.pos[2] + ctx.rng.si_rand(r as i32) as f32)
                .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX);
            tr += 1;
            if tr < 16 && (x - self.pos[0]).abs() + (z - self.pos[2]).abs() < r / 2.0 {
                continue;
            }
            let y = ctx.area.land_height(x, z);
            let wy = ctx.area.land_up_h(x, z) - y;
            if tr < 128 && !(200.0..=400.0).contains(&wy) {
                continue;
            }
            self.tgtime = 0;
            self.tgx = x;
            self.tgz = z;
            return;
        }
    }
}

pub fn animate(c: &mut Character, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;

    let (targetdx, targetdz) = loop {
        c.tgtime = 0;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        if (targetdx * targetdx + targetdz * targetdz).sqrt() < 256.0 {
            c.set_new_target_place_brahi(ctx, 2048.0);
            continue;
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
        if c.phase > WALK {
            c.phase = if ctx.rng.r_rand(128) > 90 {
                WALK
            } else {
                IDLE1 + ctx.rng.r_rand(2) as usize
            };
        } else {
            c.phase = if ctx.rng.r_rand(128) > 64 {
                IDLE1
            } else {
                WALK
            };
        }
    }
    if old_phase != c.phase || new_phase {
        c.activate_fx(index, sp, ctx);
    }
    if old_phase != c.phase && c.pp_morph_time > 128 {
        c.prev_phase = old_phase;
        c.prev_pf_time = old_ftime;
        c.pp_morph_time = 0;
    }
    c.ftime %= sp.time(c.phase);

    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.phase == WALK {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.2 + drspd * 0.2
            } else {
                -0.2 - drspd * 0.2
            }
        } else {
            0.0
        };
        if dalpha > PI {
            currspeed *= -1.0;
        }
        delta_func(&mut c.rspeed, currspeed, dtf / 600.0);
        let tgbend = (drspd / 4.0).min(PI / 4.0) * sgn(currspeed);
        delta_func(&mut c.bend, tgbend, dtf / 3200.0);
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
    let mut curspeed = if c.phase == WALK { 0.2 } else { 0.0 };
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dtf / 1024.0);
    c.pos[0] += c.lookx * c.vspeed * dtf;
    c.pos[2] += c.lookz * c.vspeed * dtf;
    c.think_y_beta_gamma(ctx.area, dt, 256.0, 128.0, 0.1, 0.2);
    delta_func(&mut c.gamma, c.tggamma, dtf / 4048.0);
}
