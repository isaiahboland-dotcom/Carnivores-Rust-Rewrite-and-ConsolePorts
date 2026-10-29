//! Hunting carnivores: `AnimateRaptor` (Allosaurus), `AnimateVelo`,
//! `AnimateSpin`, `AnimateCera`. Same body, different constants; Spino and
//! Cerato also idle, Cerato never jumps or slides.

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
    ai, angle_difference, corrected_alpha, delta_func, find_vector_alpha, hunt, sgn, wrap_angle,
    Character, Ctx, DinoEvent, Species, CS_ON_WATER, PI,
};

/// Marks a phase the species doesn't have.
const NONE: usize = usize::MAX;

#[derive(Debug, Clone, Copy)]
pub struct CarnivoreParams {
    pub run: usize,
    pub walk: usize,
    pub swim: usize,
    pub slide: usize,
    pub jump: usize,
    pub die: usize,
    pub eat: usize,
    pub slp: usize,
    pub idles: [usize; 3],
    pub n_idles: usize,
    /// Head offset used for the distance to the hunter; scaled by size for Allo.
    pub head: f32,
    pub head_scaled: bool,
    /// Wades (`csONWATER`) when water is deeper than this * scale.
    pub water_depth: f32,
    /// Gives up when farther than `ctViewR * k + OptAgres / div`.
    pub give_up_k: f32,
    pub give_up_div: i32,
    pub eat_dist: f32,
    pub eat_dy: f32,
    /// `AfraidTime = 0` while not hunting (all but Allo).
    pub calm_resets_afraid: bool,
    /// On being alerted (`State == 2`): 0 = Allo/Velo, 1 = Spino (run unless
    /// mid-jump), 2 = Cerato (always restart running).
    pub alert_mode: u8,
    /// Weave amplitude while chasing from afar: `sin(t/824) / div`.
    pub weave_div: f32,
    pub bend_div: f32,
    pub run_speed: f32,
    pub jump_speed: f32,
    pub walk_speed: f32,
    pub swim_speed: f32,
    /// Slow down for sharp turns (`drspd > pi/2`); Cerato has it commented out.
    pub turn_slowdown: bool,
    pub water_y: f32,
    pub think: (f32, f32, f32, f32),
    pub gamma_walk: f32,
    pub gamma_run: f32,
}

impl CarnivoreParams {
    pub fn for_ai(ai_id: i32) -> Option<Self> {
        // RAP_*: RUN 0, WALK 1, SWIM 2, SLIDE 3, JUMP 4, DIE 5, EAT 6, SLP 7
        let allo = Self {
            run: 0,
            walk: 1,
            swim: 2,
            slide: 3,
            jump: 4,
            die: 5,
            eat: 6,
            slp: 7,
            idles: [NONE; 3],
            n_idles: 0,
            head: 100.0,
            head_scaled: true,
            water_depth: 180.0,
            give_up_k: 128.0,
            give_up_div: 4,
            eat_dist: 256.0,
            eat_dy: 160.0,
            calm_resets_afraid: false,
            alert_mode: 0,
            weave_div: 2.0,
            bend_div: 2.0,
            run_speed: 1.2,
            jump_speed: 1.1,
            walk_speed: 0.428,
            swim_speed: 0.4,
            turn_slowdown: true,
            water_y: 200.0,
            think: (64.0, 32.0, 0.5, 0.4),
            gamma_walk: 10.0,
            gamma_run: 8.0,
        };
        let later = Self {
            head: 108.0,
            head_scaled: false,
            water_depth: 140.0,
            give_up_div: 8,
            eat_dy: 120.0,
            calm_resets_afraid: true,
            bend_div: 3.0,
            water_y: 160.0,
            ..allo
        };
        match ai_id {
            ai::ALLO => Some(allo),
            // VEL_*: RUN 0, WALK 1, SLIDE 2, SWIM 3, JUMP 4, DIE 5, EAT 6, SLP 7
            ai::VELO => Some(Self {
                slide: 2,
                swim: 3,
                give_up_k: 160.0,
                think: (48.0, 24.0, 0.5, 0.4),
                gamma_walk: 7.0,
                gamma_run: 5.0,
                ..later
            }),
            // SPN_*: RUN 0, WALK 1, SLIDE 2, SWIM 1, IDLE1 3, IDLE2 4, JUMP 5, DIE 6, EAT 7, SLP 8
            ai::SPINO => Some(Self {
                slide: 2,
                swim: 1,
                idles: [3, 4, NONE],
                n_idles: 2,
                jump: 5,
                die: 6,
                eat: 7,
                slp: 8,
                give_up_k: 140.0,
                eat_dist: 300.0,
                alert_mode: 1,
                weave_div: 4.0,
                run_speed: 1.6,
                jump_speed: 1.4,
                walk_speed: 0.70,
                swim_speed: 0.5,
                think: (98.0, 84.0, 0.4, 0.3),
                gamma_walk: 9.0,
                gamma_run: 6.0,
                ..later
            }),
            // CER_*: WALK 0, RUN 1, IDLE1 2, IDLE2 3, IDLE3 4, DIE 5, SLP 6, EAT 7, SWIM 0
            ai::CERAT => Some(Self {
                walk: 0,
                run: 1,
                swim: 0,
                slide: NONE,
                jump: NONE,
                idles: [2, 3, 4],
                n_idles: 3,
                die: 5,
                slp: 6,
                eat: 7,
                give_up_k: 200.0,
                eat_dist: 350.0,
                alert_mode: 2,
                weave_div: 4.0,
                run_speed: 2.2,
                jump_speed: 0.0,
                walk_speed: 0.75,
                swim_speed: 0.0,
                turn_slowdown: false,
                think: (348.0, 324.0, 0.5, 0.4),
                gamma_walk: 9.0,
                gamma_run: 6.0,
                ..later
            }),
            _ => None,
        }
    }

    fn is_idle(&self, phase: usize) -> bool {
        self.idles[..self.n_idles].contains(&phase)
    }
}

pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    p: &CarnivoreParams,
) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;

    let (targetdx, targetdz, pdist) = 'tbegin: loop {
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let head = if p.head_scaled {
            p.head * c.scale
        } else {
            p.head
        };
        let playerdx = ctx.hunter.x - c.pos[0] - c.lookx * head;
        let playerdz = ctx.hunter.z - c.pos[2] - c.lookz * head;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        if c.state == 2 {
            match p.alert_mode {
                0 => {
                    if c.phase != p.jump {
                        new_phase = true;
                    }
                }
                1 => {
                    if c.phase != p.jump {
                        new_phase = true;
                    }
                    c.phase = p.run;
                }
                _ => {
                    new_phase = true;
                    c.phase = p.run;
                }
            }
            c.state = 1;
        }

        let a = ctx.area;
        if a.land_up_h(c.pos[0], c.pos[2]) - a.land_height(c.pos[0], c.pos[2])
            > p.water_depth * c.scale
        {
            c.state_f |= CS_ON_WATER;
        } else {
            // The original writes `StateF &= (!csONWATER)`, clearing every bit.
            c.state_f = 0;
        }

        if c.phase != p.eat {
            if !ctx.hunter.alive {
                c.state = 0;
            }
            if c.state != 0 {
                if pdist > ctx.view_r as f32 * p.give_up_k + (ctx.opt_agres / p.give_up_div) as f32
                {
                    // Too far: wander off and lose interest.
                    let l = pdist.max(1e-6);
                    c.tgx = c.pos[0] - playerdx / l * 2048.0;
                    c.tgz = c.pos[2] - playerdz / l * 2048.0;
                    c.tgtime = 0;
                    c.afraid_time -= dt;
                    if c.afraid_time <= 0 {
                        c.afraid_time = 0;
                        c.state = 0;
                    }
                } else {
                    c.tgx = ctx.hunter.x;
                    c.tgz = ctx.hunter.z;
                    c.tgtime = 0;
                }

                if p.jump != NONE
                    && c.state_f & CS_ON_WATER == 0
                    && pdist < 1324.0 * c.scale
                    && pdist > 900.0 * c.scale
                    && angle_difference(c.alpha, find_vector_alpha(playerdx, playerdz)) < 0.2
                {
                    c.phase = p.jump;
                }

                if pdist < p.eat_dist && (ctx.hunter.y - c.pos[1] - p.eat_dy).abs() < 256.0 {
                    if c.state_f & CS_ON_WATER == 0 {
                        c.vspeed /= 8.0;
                        c.state = 1;
                        c.phase = p.eat;
                    }
                    if ctx.hunter.alive {
                        ctx.events.push(DinoEvent::KillHunter {
                            index,
                            kind: hunt::EAT,
                            scream: true,
                        });
                    }
                }
            }
            if c.state == 0 {
                if p.calm_resets_afraid {
                    c.afraid_time = 0;
                }
                if tdist < 456.0 {
                    c.set_new_target_place(ctx.area, ctx.rng, 8048.0);
                    continue 'tbegin;
                }
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
        if c.state != 0 && pdist > 1648.0 {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / p.weave_div;
            wrap_angle(&mut c.tgalpha);
        }
    }
    c.look_for_a_way(ctx.area, false, true);
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + ctx.rng.r_rand(20);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);

    // Select a new phase.
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        new_phase = true;
    }
    'select: {
        if c.phase == p.eat {
            break 'select;
        }
        if new_phase && old_phase == p.jump {
            c.phase = p.run;
            break 'select;
        }
        if c.phase == p.jump {
            break 'select;
        }
        if p.n_idles > 0 && new_phase {
            if c.state == 0 {
                if ctx.rng.r_rand(128) > 110 {
                    c.phase = p.idles[0] + ctx.rng.r_rand(p.n_idles as i32 - 1) as usize;
                    break 'select;
                }
                c.phase = p.walk;
            } else {
                c.phase = p.run;
            }
        }
        if !p.is_idle(c.phase) {
            let d = (c.tgalpha - c.alpha).abs();
            c.phase = if c.state == 0 {
                p.walk
            } else if d < 1.0 || d > 2.0 * PI - 1.0 {
                p.run
            } else {
                p.walk
            };
        }
        if c.state_f & CS_ON_WATER != 0 {
            c.phase = p.swim;
        }
        if p.slide != NONE && c.slide > 40 {
            c.phase = p.slide;
        }
    }
    c.finish_phase_change(index, sp, ctx, old_phase, old_ftime, new_phase, 3);

    // Rotate toward tgalpha.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.phase != p.jump && c.phase != p.eat && !p.is_idle(c.phase) {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.6 + drspd * 1.2
            } else {
                -0.6 - drspd * 1.2
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
        if c.state_f & CS_ON_WATER != 0 || c.phase == p.walk {
            currspeed /= 1.4;
        }
        let rate = if c.afraid_time != 0 { 160.0 } else { 180.0 };
        delta_func(&mut c.rspeed, currspeed, dtf / rate);

        let tgbend = (drspd / p.bend_div).min(PI / 5.0) * sgn(currspeed);
        let rate = if tgbend.abs() > c.bend.abs() {
            800.0
        } else {
            600.0
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

    // Skid when turning hard at speed.
    if p.slide != NONE
        && c.slide == 0
        && c.vspeed > 0.6
        && c.phase != p.jump
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
    let mut curspeed = match c.phase {
        ph if ph == p.run => p.run_speed * ctx.run_k,
        ph if ph == p.jump => p.jump_speed,
        ph if ph == p.walk => p.walk_speed,
        ph if ph == p.swim => p.swim_speed,
        _ => 0.0,
    };
    if c.phase == p.eat {
        curspeed = 0.0;
    }
    if c.phase == p.run && c.slide != 0 {
        curspeed /= 8.0;
        if drspd > PI / 2.0 {
            curspeed = 0.0;
        } else if drspd > PI / 4.0 {
            curspeed *= 2.0 - 4.0 * drspd / PI;
        }
    } else if p.turn_slowdown && drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dtf / 500.0);
    if c.phase == p.jump {
        c.vspeed = p.jump_speed;
    }
    let s = c.vspeed * dtf * c.scale;
    let (mx, mz) = (c.lookx * s, c.lookz * s);
    c.move_character(ctx.area, mx, mz, false);
    if c.slide != 0 {
        let k = c.slide as f32 / 600.0 * dtf * c.scale;
        let (sx, sz) = (c.slidex * k, c.slidez * k);
        c.move_character(ctx.area, sx, sz, false);
        c.slide = (c.slide - dt).max(0);
    }

    // Height: swim at the surface or follow the ground.
    if c.state_f & CS_ON_WATER != 0 {
        c.pos[1] = ctx.area.land_up_h(c.pos[0], c.pos[2]) - p.water_y * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        let (b, g, bl, gl) = p.think;
        c.think_y_beta_gamma(ctx.area, dt, b, g, bl, gl);
    }
    c.tggamma += c.rspeed
        / if c.phase == p.walk {
            p.gamma_walk
        } else {
            p.gamma_run
        };
    if c.phase == p.jump {
        c.tggamma = 0.0;
    }
    delta_func(&mut c.gamma, c.tggamma, dtf / 1624.0);
}
