//! The hunter: `Hunt2.cpp: ProcessPlayerMovement`, `ProcessSlide` and the
//! Y-movement / swimming part of `ProcessControls`.
//!
//! Units follow the original: positions in world units, speeds in units per
//! millisecond (`VSpeed`, `SSpeed`), `YSpeed` in units per second, angles in
//! radians with `alpha` = yaw (forward is `(sin a, 0, -cos a)`) and `beta` =
//! pitch (positive looks down).

use crate::area::Area;
use crate::CELL;

/// Standing eye height (`HeadY`); crouching goes down to 110.
pub const HEAD_STAND: f32 = 220.0;
pub const HEAD_CROUCH: f32 = 110.0;

/// One tick's controls, already mapped from keys/mouse.
#[derive(Debug, Clone, Copy, Default)]
pub struct Controls {
    pub forward: bool,
    pub backward: bool,
    pub strafe_left: bool,
    pub strafe_right: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub crouch: bool,
    pub jump: bool,
    /// `RunMode` (toggled with the run key in the original).
    pub run: bool,
    /// Raw mouse motion in pixels since the last tick.
    pub mouse: (f32, f32),
    /// `OptMsSens`, 0..=255 (128 default).
    pub mouse_sens: f32,
}

/// Things that happened this tick, for sound and effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerEvent {
    Jump,
    Land,
    Step,
    StepWater,
    WaterIn,
    WaterOut,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub alpha: f32,
    pub beta: f32,
    pub vspeed: f32,
    pub sspeed: f32,
    pub yspeed: f32,
    pub head_y: f32,
    /// Smoothed mouse velocity (`rav`, `rbv`).
    rav: f32,
    rbv: f32,
    /// Head bob (`stepdy`) and its last delta (`stepdd`).
    stepdy: f32,
    stepdd: f32,
    pub swim: bool,
    pub underwater: bool,
    pub on_water: bool,
    /// Milliseconds since the hunt started (`RealTime`).
    pub time_ms: u32,
}

impl Player {
    /// `PlaceHunter`: on the ground at a landing cell.
    pub fn new(area: &Area, cell: (u16, u16)) -> Self {
        let (x, z) = (
            f32::from(cell.0) * CELL + 128.0,
            f32::from(cell.1) * CELL + 128.0,
        );
        let y = area.land_qh(x, z, f32::MAX);
        Self {
            x,
            y,
            z,
            alpha: 0.0,
            beta: 0.0,
            vspeed: 0.0,
            sspeed: 0.0,
            yspeed: 0.0,
            head_y: HEAD_STAND,
            rav: 0.0,
            rbv: 0.0,
            stepdy: 0.0,
            stepdd: 0.0,
            swim: false,
            underwater: false,
            on_water: false,
            time_ms: 0,
        }
    }

    /// Eye position (`CameraX/Y/Z`).
    pub fn eye(&self) -> [f32; 3] {
        [self.x, self.y + self.head_y + self.stepdy, self.z]
    }

    /// Advances `dt_ms` milliseconds.
    pub fn tick(&mut self, area: &Area, c: &Controls, dt_ms: u32, events: &mut Vec<PlayerEvent>) {
        let dt = dt_ms as f32;
        let delta_t = dt / 1000.0;
        self.time_ms = self.time_ms.wrapping_add(dt_ms);
        self.movement(area, c, dt, delta_t, events);
        self.vertical(area, c, delta_t, events);
    }

    fn movement(
        &mut self,
        area: &Area,
        c: &Controls,
        dt: f32,
        delta_t: f32,
        events: &mut Vec<PlayerEvent>,
    ) {
        // Mouse: accumulate, then decay (smoothing).
        let k = (c.mouse_sens + 64.0) / 600.0 / 192.0;
        self.rav += c.mouse.0 * k;
        self.rbv += c.mouse.1 * k;
        self.alpha += self.rav;
        self.beta += self.rbv;
        let decay = 2.0 + dt / 20.0;
        self.rav /= decay;
        self.rbv /= decay;

        // Acceleration and friction.
        let toward = |v: f32| {
            if v > 0.0 {
                (v - delta_t * 2.0).max(0.0)
            } else {
                (v + delta_t * 2.0).min(0.0)
            }
        };
        if !(c.forward || c.backward) {
            self.vspeed = toward(self.vspeed);
        }
        if !(c.strafe_left || c.strafe_right) {
            self.sspeed = toward(self.sspeed);
        }
        let push = |v: &mut f32, pos: bool, neg: bool| {
            if pos {
                *v += if *v > 0.0 { delta_t } else { delta_t * 4.0 };
            }
            if neg {
                *v -= if *v < 0.0 { delta_t } else { delta_t * 4.0 };
            }
        };
        push(&mut self.vspeed, c.forward, c.backward);
        push(&mut self.sspeed, c.strafe_right, c.strafe_left);

        let max = if self.swim {
            0.25
        } else if c.run && self.head_y == HEAD_STAND {
            0.7
        } else {
            0.3
        };
        self.vspeed = self.vspeed.clamp(-max, max);
        self.sspeed = self.sspeed.clamp(-max, max);

        if c.jump && self.yspeed == 0.0 && !self.swim {
            self.yspeed = 600.0 + self.vspeed.abs() * 600.0;
            events.push(PlayerEvent::Jump);
        }
        if c.turn_right {
            self.alpha += delta_t * 1.5;
        }
        if c.turn_left {
            self.alpha -= delta_t * 1.5;
        }

        let (sa, ca) = self.alpha.sin_cos();
        // Underwater, forward follows the view pitch (`nv.y = -sin(beta)`).
        let (sb, cb) = self.beta.sin_cos();
        let (pitch_xz, fy) = if self.underwater {
            (cb, -sb * dt * self.vspeed)
        } else {
            (1.0, 0.0)
        };
        let (fx, fz) = (
            sa * pitch_xz * dt * self.vspeed,
            -ca * pitch_xz * dt * self.vspeed,
        );
        let (sx, sz) = (sa * dt * self.sspeed, -ca * dt * self.sspeed);
        // Sub-steps keep fast movement from tunnelling through objects.
        let mvi = 1 + dt_steps(dt);
        let n = mvi as f32;
        for _ in 0..mvi {
            self.x += fx / n;
            self.y += fy / n;
            self.z += fz / n;
            self.x -= sz / n;
            self.z += sx / n;
            area.check_collision(&mut self.x, &mut self.z, self.y);
            if self.y <= area.land_qh_no_obj(self.x, self.z) + 16.0 {
                self.slide(area);
                self.slide(area);
            }
        }
        if self.y <= area.land_qh_no_obj(self.x, self.z) + 16.0 {
            self.slide(area);
            self.slide(area);
        }
    }

    /// `ProcessSlide`: slip toward lower ground on steep slopes.
    fn slide(&mut self, area: &Area) {
        if self.underwater {
            return;
        }
        let ch = area.land_qh_no_obj(self.x, self.z);
        const DIRS: [(f32, f32); 8] = [
            (-16.0, 0.0),
            (16.0, 0.0),
            (0.0, -16.0),
            (0.0, 16.0),
            (-12.0, -12.0),
            (12.0, -12.0),
            (-12.0, 12.0),
            (12.0, 12.0),
        ];
        let mut mh = ch;
        let mut sd = None;
        for (i, (dx, dz)) in DIRS.iter().enumerate() {
            let h = area.land_qh_no_obj(self.x + dx, self.z + dz);
            if h < mh {
                mh = h;
                sd = Some(i);
            }
        }
        let Some(i) = sd else { return };
        if mh >= ch - 16.0 {
            return;
        }
        let mut delta = (ch - mh) / 4.0;
        if i >= 4 {
            delta *= 0.7;
        }
        let (dx, dz) = DIRS[i];
        self.x += dx.signum() * delta * f32::from(u8::from(dx != 0.0));
        self.z += dz.signum() * delta * f32::from(u8::from(dz != 0.0));
    }

    fn vertical(&mut self, area: &Area, c: &Controls, delta_t: f32, events: &mut Vec<PlayerEvent>) {
        // Crouch / stand.
        if c.crouch || self.underwater {
            self.head_y = self.head_y.max(HEAD_CROUCH);
            self.head_y -= delta_t * (60.0 + (self.head_y - HEAD_CROUCH) * 5.0);
            self.head_y = self.head_y.max(HEAD_CROUCH);
        } else {
            self.head_y = self.head_y.min(HEAD_STAND);
            self.head_y += delta_t * (60.0 + (HEAD_STAND - self.head_y) * 5.0);
            self.head_y = self.head_y.min(HEAD_STAND);
        }

        let h = area.land_qh(self.x, self.z, self.y);
        let hu = area.land_ceil_h(self.x, self.z, self.y) - 64.0;
        let hwater = area.land_up_h(self.x, self.z);

        // Gravity (in water: rise back up).
        if !self.underwater {
            if self.y > h {
                self.yspeed -= delta_t * 3000.0;
            }
        } else if self.yspeed < 0.0 {
            self.yspeed = (self.yspeed + delta_t * 4000.0).min(0.0);
        }
        self.y += self.yspeed * delta_t;

        // Ceiling.
        if self.y + self.head_y > hu {
            if self.yspeed > 0.0 {
                self.yspeed = -1.0;
            }
            self.y = hu - self.head_y;
            if self.y < h {
                self.y = h;
                self.head_y = (hu - self.y).max(HEAD_CROUCH);
            }
        }

        // Ground: ease up onto it, absorbing hard landings in the knees.
        if self.y < h {
            if self.yspeed < -800.0 {
                self.head_y += self.yspeed / 100.0;
            }
            self.y = self.y.max(h - 80.0);
            self.y = (self.y + (h - self.y + 32.0) * delta_t * 4.0).min(h);
            if self.yspeed < -600.0 {
                events.push(PlayerEvent::Land);
            }
            self.yspeed = 0.0;
        }

        // Swimming: holding jump in deep water floats at the surface.
        self.swim = false;
        if !self.underwater && c.jump && self.y < hwater - 148.0 {
            self.swim = true;
            self.y = hwater - 148.0;
            self.yspeed = 0.0;
        }

        // Head bob and footsteps.
        let t = self.time_ms as f32;
        let prev = self.stepdy;
        self.stepdy = if self.swim {
            (t / 360.0).sin() * 20.0
        } else {
            (self.vspeed.abs() + self.sspeed.abs()).min(1.0) * (t / 80.0).sin() * 22.0
        };
        let d = self.stepdy - prev;
        if !self.underwater
            && self.y < h + 64.0
            && d < 0.0
            && self.stepdd >= 0.0
            && self.vspeed.abs() + self.sspeed.abs() > 0.0
        {
            events.push(if self.on_water {
                PlayerEvent::StepWater
            } else {
                PlayerEvent::Step
            });
        }
        self.stepdd = d;

        self.beta = self.beta.clamp(-1.26, 1.46);

        // Water state from the camera position.
        let [cx, cy, cz] = self.eye();
        self.on_water = area.land_up_h(cx, cz) > area.land_height(cx, cz);
        if self.underwater {
            self.underwater = area.land_up_h(cx, cz) - 4.0 >= cy;
            if !self.underwater {
                self.head_y += 20.0;
                events.push(PlayerEvent::WaterOut);
            }
        } else {
            self.underwater = area.land_up_h(cx, cz) + 28.0 >= cy;
            if self.underwater {
                self.head_y -= 20.0;
                events.push(PlayerEvent::WaterIn);
            }
        }
        if self.swim {
            // The original drifts swimmers slowly (+32 units/s on x and z).
            self.x += delta_t * 32.0;
            self.z += delta_t * 32.0;
        }
    }
}

/// `mvi = 1 + TimeDt / 16`.
fn dt_steps(dt: f32) -> u32 {
    (dt / 16.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut Player, area: &Area, c: &Controls, ms: u32) -> Vec<PlayerEvent> {
        let mut ev = Vec::new();
        for _ in 0..ms / 10 {
            p.tick(area, c, 10, &mut ev);
        }
        ev
    }

    #[test]
    fn walks_at_capped_speed_on_flat_ground() {
        let area = Area::flat(10);
        let mut p = Player::new(&area, (500, 500));
        let ground = 10.0 * crate::H_SCALE;
        assert_eq!(p.y, ground);
        let (x0, z0) = (p.x, p.z);
        run(
            &mut p,
            &area,
            &Controls {
                forward: true,
                ..Default::default()
            },
            2000,
        );
        assert!((p.vspeed - 0.3).abs() < 1e-6, "walk cap");
        // alpha = 0 walks toward -z.
        assert!((p.x - x0).abs() < 1e-3);
        assert!(z0 - p.z > 400.0 && z0 - p.z < 600.0, "moved {}", z0 - p.z);
        assert_eq!(p.y, ground);
        run(
            &mut p,
            &area,
            &Controls {
                forward: true,
                run: true,
                ..Default::default()
            },
            2000,
        );
        assert!((p.vspeed - 0.7).abs() < 1e-6, "run cap");
    }

    #[test]
    fn jump_goes_up_and_lands() {
        let area = Area::flat(10);
        let mut p = Player::new(&area, (500, 500));
        let ground = p.y;
        let ev = run(
            &mut p,
            &area,
            &Controls {
                jump: true,
                ..Default::default()
            },
            100,
        );
        assert!(ev.contains(&PlayerEvent::Jump));
        assert!(p.y > ground + 40.0, "y {}", p.y);
        run(&mut p, &area, &Controls::default(), 2000);
        assert_eq!(p.y, ground);
        assert_eq!(p.yspeed, 0.0);
    }

    #[test]
    fn crouch_lowers_head_and_clamps_pitch() {
        let area = Area::flat(10);
        let mut p = Player::new(&area, (500, 500));
        run(
            &mut p,
            &area,
            &Controls {
                crouch: true,
                mouse: (0.0, 1e6),
                mouse_sens: 128.0,
                ..Default::default()
            },
            2000,
        );
        assert_eq!(p.head_y, HEAD_CROUCH);
        assert_eq!(p.beta, 1.46);
    }
}
