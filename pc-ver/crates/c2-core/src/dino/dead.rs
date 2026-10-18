//! Dying and dead animals: `Animate*Dead`. All share this body except the
//! flyers (`flyer::animate_dead`); the T-Rex has no sleep phase (so no
//! tranquilizer) and probes the ground farther out.

use super::{delta_func, Character, Ctx, Species};

/// `slp = None`: the species can't be tranquilized.
pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    die: usize,
    slp: Option<usize>,
    look: (f32, f32),
) {
    let slp_or_die = slp.unwrap_or(die);
    let dt = ctx.dt;
    if c.phase != die && c.phase != slp_or_die {
        if c.pp_morph_time > 128 {
            c.prev_phase = c.phase;
            c.prev_pf_time = c.ftime;
            c.pp_morph_time = 0;
        }
        c.ftime = 0;
        c.phase = die;
        c.activate_fx(index, sp, ctx);
    } else {
        c.process_prev_phase(sp, dt);
        c.ftime += dt;
        if c.ftime >= sp.time(c.phase) {
            if let (true, Some(slp)) = (ctx.tranq, slp) {
                c.ftime = 0;
                c.phase = slp;
                c.activate_fx(index, sp, ctx);
            } else {
                c.ftime = sp.time(c.phase) - 1;
            }
        }
    }
    // Coast to a stop.
    delta_func(&mut c.vspeed, 0.0, dt as f32 / 800.0);
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;
    c.think_y_beta_gamma(ctx.area, dt, look.0, look.1, 0.6, 0.5);
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 1600.0);
}

/// What the hunter's body copies when a T-Rex is carrying it.
#[derive(Debug, Clone, Copy)]
pub struct Carrier {
    pub pos: [f32; 3],
    pub ftime: i32,
    pub beta: f32,
    pub gamma: f32,
}

/// `AnimateHuntDead`: the hunter's body (phase = `HUNT_*` kind).
pub fn animate_hunter(
    c: &mut Character,
    index: usize,
    sp: &Species,
    ctx: &mut Ctx<'_>,
    carrier: Option<Carrier>,
) {
    let dt = ctx.dt;
    let dtf = dt as f32;
    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime = if c.phase == 2 {
            sp.time(c.phase) - 1
        } else {
            0
        };
        if c.phase == 1 {
            c.ftime = 0;
            c.phase = 2;
        }
        c.activate_fx(index, sp, ctx);
    }
    let h = ctx.area.land_height(c.pos[0], c.pos[2]);
    delta_func(&mut c.pos[1], h, dtf / 5.0);
    if c.phase == 2 && c.pos[1] > h + 3.0 {
        c.ftime = 0;
    }
    if c.pos[1] < h + 256.0 {
        let look = 256.0;
        let g = |x: f32, z: f32| ctx.area.land_height(x, z);
        let hl = g(c.pos[0] + c.lookx * look, c.pos[2] + c.lookz * look);
        let hl2 = g(c.pos[0] - c.lookx * look, c.pos[2] - c.lookz * look);
        delta_func(&mut c.beta, (hl2 - hl) / (look * 3.2), dtf / 1800.0);
        c.beta = c.beta.clamp(-0.4, 0.4);
        let hl = g(c.pos[0] + c.lookz * look, c.pos[2] - c.lookx * look);
        let hl2 = g(c.pos[0] - c.lookz * look, c.pos[2] + c.lookx * look);
        c.tggamma = ((hl - hl2) / (look * 3.2)).clamp(-0.4, 0.4);
        delta_func(&mut c.gamma, c.tggamma, dtf / 1800.0);
    }
    if let Some(t) = carrier {
        c.pos = t.pos;
        c.ftime = t.ftime;
        c.beta = t.beta;
        c.gamma = t.gamma;
    }
}
