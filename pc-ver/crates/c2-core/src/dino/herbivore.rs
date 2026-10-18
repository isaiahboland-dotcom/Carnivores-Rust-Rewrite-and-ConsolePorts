//! Large herbivores: `AnimatePar`, `AnimateAnky`, `AnimateSteg` (timid) and
//! `AnimateTric` (Chasmosaurus: defends itself). The functions are identical
//! apart from the constants below and Tric's `defensive` branch.

use super::{
    ai, corrected_alpha, delta_func, find_vector_alpha, hunt, sgn, wrap_angle, Character, Ctx,
    DinoEvent, Species, CS_ON_WATER, PI,
};

#[derive(Debug, Clone, Copy)]
pub struct HerbivoreParams {
    pub walk: usize,
    pub run: usize,
    /// First idle phase; idles are consecutive (`IDLE1 + rRand(n - 1)`).
    pub idle1: usize,
    pub n_idles: usize,
    pub die: usize,
    pub slp: usize,
    /// `NoWayCnt` limit and the `NoFindCnt = base + rRand(rand)` that follows.
    pub no_way_limit: i32,
    pub no_find_base: i32,
    pub no_find_rand: i32,
    /// `tgbend = drspd / div`, capped at `max`.
    pub bend_div: f32,
    pub bend_max: f32,
    /// Par: bend grows at dt/1600 and relaxes at dt/1200; others: dt/2000.
    pub bend_split: bool,
    pub run_speed: f32,
    pub walk_speed: f32,
    /// `tggamma += rspeed / div` while walking / otherwise.
    pub gamma_walk: f32,
    pub gamma_run: f32,
    /// Leave walking for an idle when `rRand(128) > this`.
    pub idle_threshold: i32,
    /// Highest phase index treated as a gait for time remapping.
    pub gait_max: usize,
    /// `ThinkY_Beta_Gamma` gamma limit.
    pub gamma_lim: f32,
    /// Chasmosaurus: never spooked by proximity alone; when alarmed it charges
    /// a close hunter, and gores anyone within 300 units.
    pub defensive: bool,
}

impl HerbivoreParams {
    pub fn for_ai(ai_id: i32) -> Option<Self> {
        // PAR_*: WALK 0, RUN 1, IDLE1 2, IDLE2 3, DIE 4, SLP 5
        let par = Self {
            walk: 0,
            run: 1,
            idle1: 2,
            n_idles: 2,
            die: 4,
            slp: 5,
            no_way_limit: 8,
            no_find_base: 44,
            no_find_rand: 80,
            bend_div: 3.0,
            bend_max: PI / 2.0,
            bend_split: true,
            run_speed: 1.6,
            walk_speed: 0.40,
            gamma_walk: 12.0,
            gamma_run: 8.0,
            idle_threshold: 120,
            gait_max: 2,
            gamma_lim: 0.4,
            defensive: false,
        };
        let heavy = Self {
            no_way_limit: 12,
            no_find_base: 32,
            no_find_rand: 60,
            bend_div: 2.0,
            bend_max: PI / 3.0,
            bend_split: false,
            gamma_walk: 16.0,
            gamma_run: 10.0,
            ..par
        };
        match ai_id {
            ai::PARA => Some(par),
            // ANK_*: RUN 0, WALK 1, IDLE1 2, IDLE2 3, DIE 4, SLP 5
            ai::ANKY => Some(Self {
                run: 0,
                walk: 1,
                run_speed: 0.75,
                walk_speed: 0.28,
                ..heavy
            }),
            // STG_*: RUN 0, WALK 1, DIE 2, IDLE1 3, IDLE2 4, SLP 5
            ai::STEGO => Some(Self {
                run: 0,
                walk: 1,
                die: 2,
                idle1: 3,
                slp: 5,
                run_speed: 0.96,
                walk_speed: 0.36,
                ..heavy
            }),
            // TRI_*: RUN 0, WALK 1, IDLE1 2, IDLE2 3, IDLE3 4, DIE 5, SLP 6
            ai::CHASM => Some(Self {
                run: 0,
                walk: 1,
                idle1: 2,
                n_idles: 3,
                die: 5,
                slp: 6,
                no_find_base: 48,
                bend_div: 3.5,
                run_speed: 1.2,
                walk_speed: 0.30,
                idle_threshold: 124,
                gait_max: 1,
                gamma_lim: 0.3,
                defensive: true,
                ..par
            }),
            _ => None,
        }
    }
}

impl HerbivoreParams {
    fn is_idle(&self, phase: usize) -> bool {
        (self.idle1..self.idle1 + self.n_idles).contains(&phase)
    }
}

pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    p: &HerbivoreParams,
) {
    let dt = ctx.dt;
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

    // TBEGIN: re-evaluated after every state change, as the original's goto.
    let (targetdx, targetdz) = loop {
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        // Tric measures from its horns, 300 units ahead.
        let head = if p.defensive { 300.0 * c.scale } else { 0.0 };
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * head;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * head;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();

        // Alarmed: run away (or, for Tric, charge when close).
        if c.state != 0 {
            if p.defensive && pdist < 6000.0 {
                c.afraid_time = 8000;
            }
            if c.afraid_time == 0 {
                c.state = 0;
                c.set_new_target_place(ctx.area, ctx.rng, 8048.0);
                continue;
            }
            if p.defensive && pdist <= 256.0 * 16.0 + (ctx.opt_agres / 8) as f32 {
                c.tgx = ctx.hunter.x;
                c.tgz = ctx.hunter.z;
            } else {
                let l = pdist.max(1e-6);
                c.tgx = c.pos[0] - playerdx / l * 2048.0;
                c.tgz = c.pos[2] - playerdz / l * 2048.0;
            }
            c.tgtime = 0;
        }

        if p.defensive
            && ctx.hunter.alive
            && pdist < 300.0
            && (ctx.hunter.y - c.pos[1] - 160.0).abs() < 256.0
        {
            c.state = 0;
            ctx.events.push(DinoEvent::KillHunter {
                index,
                kind: hunt::EAT,
            });
        }

        // Explore.
        if c.state == 0 {
            c.afraid_time = 0;
            if !p.defensive && pdist < 1024.0 {
                c.state = 1;
                c.afraid_time = (6 + ctx.rng.r_rand(8)) * 1024;
                c.phase = p.run;
                continue;
            }
            if tdist < 456.0 {
                c.set_new_target_place(ctx.area, ctx.rng, 8048.0);
                continue;
            }
        }
        break (targetdx, targetdz);
    };

    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.afraid_time != 0 {
            c.tgalpha += (ctx.real_time as f32 / 1024.0).sin() / 3.0;
            wrap_angle(&mut c.tgalpha);
        }
    }

    c.look_for_a_way(ctx.area, true, true);
    if c.no_way_cnt > p.no_way_limit {
        c.no_way_cnt = 0;
        c.no_find_cnt = p.no_find_base + ctx.rng.r_rand(p.no_find_rand);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    // Select a new phase.
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    if new_phase {
        if c.state == 0 {
            if p.is_idle(c.phase) {
                let last = p.idle1 + p.n_idles - 1;
                c.phase = if ctx.rng.r_rand(128) > 64 && c.phase == last {
                    p.walk
                } else {
                    p.idle1 + ctx.rng.r_rand(p.n_idles as i32 - 1) as usize
                };
            } else {
                c.phase = if ctx.rng.r_rand(128) > p.idle_threshold {
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

    // Rotate toward tgalpha.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if !p.is_idle(c.phase) {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.2 + drspd
            } else {
                -0.2 - drspd
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
        delta_func(&mut c.rspeed, currspeed, dt as f32 / 400.0);

        let tgbend = (drspd / p.bend_div).min(p.bend_max) * sgn(currspeed);
        if p.bend_split {
            let rate = if tgbend.abs() > c.bend.abs() {
                1600.0
            } else {
                1200.0
            };
            delta_func(&mut c.bend, tgbend, dt as f32 / rate);
        } else {
            delta_func(&mut c.bend, tgbend, dt as f32 / 2000.0);
        }

        let rspd = c.rspeed * dt as f32 / 612.0;
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
    let mut curspeed = if c.phase == p.run {
        p.run_speed
    } else if c.phase == p.walk {
        p.walk_speed
    } else {
        0.0
    };
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    curspeed *= c.scale;
    let rate = if curspeed > c.vspeed { 1024.0 } else { 256.0 };
    delta_func(&mut c.vspeed, curspeed, dt as f32 / rate);

    let (mx, mz) = (
        c.lookx * c.vspeed * dt as f32,
        c.lookz * c.vspeed * dt as f32,
    );
    c.move_character(ctx.area, mx, mz, true);

    c.think_y_beta_gamma(ctx.area, dt, 128.0, 64.0, 0.6, p.gamma_lim);
    c.tggamma += c.rspeed
        / if c.phase == p.walk {
            p.gamma_walk
        } else {
            p.gamma_run
        };
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2048.0);
}
