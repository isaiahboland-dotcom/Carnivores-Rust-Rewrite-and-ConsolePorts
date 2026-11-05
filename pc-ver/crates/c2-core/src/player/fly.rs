//! Flying (advanced observer mode; not in the original): the camera goes
//! where it looks, jump rises and crouch sinks, with no gravity and no
//! collision but the ground and the sky's ceiling.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::{Controls, Player, HEAD_STAND};
use crate::area::Area;

/// Flying speed, world units per millisecond (walking tops out at 0.3,
/// running at 0.7).
const FLY: f32 = 1.0;
/// With run on.
const FLY_FAST: f32 = 3.0;
/// The highest the camera goes above the ground.
const CEILING: f32 = 8192.0;

impl Player {
    /// Advances `dt_ms` milliseconds of flying.
    pub fn fly(&mut self, area: &Area, c: &Controls, dt_ms: u32) {
        let dt = dt_ms as f32;
        let delta_t = dt / 1000.0;
        self.time_ms = self.time_ms.wrapping_add(dt_ms);
        // The same look as on foot.
        let k = (c.mouse_sens + 64.0) / 600.0 / 192.0;
        self.rav += c.mouse.0 * k;
        self.rbv += c.mouse.1 * k;
        self.alpha += self.rav;
        self.beta += self.rbv;
        let decay = 2.0 + dt / 20.0;
        self.rav /= decay;
        self.rbv /= decay;
        if c.turn_right {
            self.alpha += delta_t * 1.5;
        }
        if c.turn_left {
            self.alpha -= delta_t * 1.5;
        }
        if c.look_up {
            self.beta -= delta_t;
        }
        if c.look_down {
            self.beta += delta_t;
        }
        self.beta = self.beta.clamp(-1.26, 1.46);

        let speed = if c.run { FLY_FAST } else { FLY } * dt;
        let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        let (f, s, u) = (
            axis(c.forward, c.backward),
            axis(c.strafe_right, c.strafe_left),
            axis(c.jump, c.crouch),
        );
        let (sa, ca) = self.alpha.sin_cos();
        let (sb, cb) = self.beta.sin_cos();
        self.x += (sa * cb * f + ca * s) * speed;
        self.z += (-ca * cb * f + sa * s) * speed;
        self.y += (-sb * f + u) * speed;
        use crate::collide::{WALK_MAX, WALK_MIN};
        self.x = self.x.clamp(WALK_MIN, WALK_MAX);
        self.z = self.z.clamp(WALK_MIN, WALK_MAX);
        let ground = area.land_qh(self.x, self.z, self.y);
        self.y = self.y.clamp(ground, ground + CEILING);
        // No bob, kick, swimming or speed carried back to walking.
        self.head_y = HEAD_STAND;
        self.head_back_r = 0.0;
        self.stepdy = 0.0;
        (self.vspeed, self.sspeed, self.yspeed) = (0.0, 0.0, 0.0);
        self.swim = false;
        let [cx, cy, cz] = self.eye();
        self.on_water = area.land_up_h(cx, cz) > area.land_height(cx, cz);
        self.underwater = area.land_up_h(cx, cz) + 28.0 >= cy;
    }
}
