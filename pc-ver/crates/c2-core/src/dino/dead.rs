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
