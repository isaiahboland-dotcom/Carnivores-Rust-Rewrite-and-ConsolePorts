//! The pickup ship (`Game.cpp: AddShipTask, InitShip, AnimateShip`): after a
//! trophy kill it flies in, lowers its gear, winches the body up and leaves.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    collections::BinaryHeap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use alloc::collections::VecDeque;

use crate::area::Area;
use crate::dino::{delta_func, find_vector_alpha, Herd, PI};

/// Ship `.CAR` sound slots.
pub mod ship_sound {
    /// Engine loop (3D, while active).
    pub const ENGINE: usize = 0;
    pub const ARRIVE: usize = 1;
    pub const DEPART: usize = 2;
    /// Radio call when a trophy is registered.
    pub const CALL: usize = 3;
    pub const GEAR_DOWN: usize = 4;
    pub const LIFT: usize = 5;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShipEvent {
    /// Play `slot` (`ship_sound::*`); `at` = 3D position, `None` = non-positional.
    Sound { slot: usize, at: Option<[f32; 3]> },
    /// The body was carried off: remove character `index`.
    Removed { index: usize },
}

/// `TShip`. `state`: -1 idle, 0 flying to the body, 3 gear down,
/// 2 winching up, 1 flying away with it.
#[derive(Debug, Clone)]
pub struct Ship {
    pub pos: [f32; 3],
    pub tgpos: [f32; 3],
    pub retpos: [f32; 3],
    pub alpha: f32,
    pub tgalpha: f32,
    pub speed: f32,
    pub rspeed: f32,
    pub delta_y: f32,
    pub state: i32,
    pub cindex: Option<usize>,
    /// Landing animation time.
    pub ftime: i32,
    /// Bodies waiting for pickup (`ShipTask`).
    pub tasks: VecDeque<usize>,
}

impl Ship {
    /// `ReInitGame`'s ship setup: idle above the hunter.
    pub fn new(area: &Area, player: (f32, f32)) -> Self {
        let pos = [
            player.0,
            area.land_up_h(player.0, player.1) + 2048.0,
            player.1,
        ];
        let tg = [pos[0], 0.0, pos[2] + 60.0 * 256.0];
        Self {
            pos,
            tgpos: [tg[0], area.land_up_h(tg[0], tg[2]) + 2048.0, tg[2]],
            retpos: pos,
            alpha: 0.0,
            tgalpha: 0.0,
            speed: 0.0,
            rspeed: 0.0,
            delta_y: 0.0,
            state: -1,
            cindex: None,
            ftime: 0,
            tasks: VecDeque::new(),
        }
    }

    /// Quarter turns for the renderer (`-alpha - pi/2`).
    pub fn quarter_turns(&self) -> f32 {
        -1.0 - self.alpha * 2.0 / PI
    }

    /// `RemoveCharacter` shifts later indices down; keep ours in step.
    pub fn character_removed(&mut self, index: usize) {
        for t in &mut self.tasks {
            if *t > index {
                *t -= 1;
            }
        }
        if let Some(c) = &mut self.cindex {
            if *c > index {
                *c -= 1;
            }
        }
    }

    /// `InitShip`.
    fn init(&mut self, area: &Area, herd: &Herd, cindex: usize, player: (f32, f32)) {
        let Some(c) = herd.chars.get(cindex) else {
            return;
        };
        let sh = herd.species.get(c.ctype).map_or(0.0, |s| s.info.sh_delta);
        self.delta_y = 2048.0 + sh * c.scale;
        let mut x = player.0 - 90.0 * 256.0;
        if x < 256.0 {
            x = player.0 + 90.0 * 256.0;
        }
        let mut z = player.1 - 90.0 * 256.0;
        if z < 256.0 {
            z = player.1 + 90.0 * 256.0;
        }
        self.pos = [x, area.land_up_h(x, z) + self.delta_y + 1024.0, z];
        self.tgpos = [
            c.pos[0],
            area.land_up_h(c.pos[0], c.pos[2]) + self.delta_y,
            c.pos[2],
        ];
        self.state = 0;
        self.retpos = self.pos;
        self.cindex = Some(cindex);
        self.ftime = 0;
    }

    /// `AnimateShip`. `anim_ms`: length of the ship's landing animation.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        area: &Area,
        herd: &mut Herd,
        player: [f32; 3],
        view_r: i32,
        real_time: i32,
        dt: i32,
        anim_ms: i32,
        out: &mut Vec<ShipEvent>,
    ) {
        let dtf = dt as f32;
        if self.state == -1 {
            if let Some(c) = self.tasks.pop_front() {
                self.init(area, herd, c, (player[0], player[2]));
            }
            return;
        }
        let Some(ci) = self.cindex else {
            self.state = -1;
            return;
        };
        // Ease in/out of the landing animation.
        let mut tdt = dt;
        if self.ftime != 0 {
            if self.ftime < 500 {
                tdt = dt * (self.ftime + 48) / 548;
            }
            if anim_ms - self.ftime < 500 {
                tdt = dt * (anim_ms - self.ftime + 48) / 548;
            }
            tdt = tdt.max(2);
        }
        let tdtf = tdt as f32;
        let d = [
            self.tgpos[0] - self.pos[0],
            self.tgpos[1] - self.pos[1],
            self.tgpos[2] - self.pos[2],
        ];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        // The original computes this from x twice (a typo); kept.
        let l2 = (d[0] * d[0] + d[0] * d[0]).sqrt();

        self.pos[1] += 0.3 * (real_time as f32 / 256.0).cos();
        self.tgalpha = find_vector_alpha(d[0], d[2]);
        let dalpha = (self.tgalpha - self.alpha).abs();
        let drspd = if dalpha > PI {
            2.0 * PI - dalpha
        } else {
            dalpha
        };

        // Leaving with the body: keep moving away while the hunter is close.
        if self.state != 0 && self.speed > 1.0 && l < 4000.0 {
            let pd = ((player[0] - self.pos[0]).powi(2)
                + (player[1] - self.pos[1]).powi(2)
                + (player[2] - self.pos[2]).powi(2))
            .sqrt();
            if pd < (view_r + 2) as f32 * 256.0 {
                self.tgpos[0] += self.alpha.cos() * 256.0 * 6.0;
                self.tgpos[2] += self.alpha.sin() * 256.0 * 6.0;
                self.tgpos[1] = (area.land_up_h(self.tgpos[0], self.tgpos[2]) + self.delta_y)
                    .max(area.land_up_h(self.pos[0], self.pos[2]) + self.delta_y);
            }
        }

        if self.state == 3 {
            self.ftime += tdt;
            if self.ftime >= anim_ms {
                self.ftime = anim_ms - 1;
                self.state = 2;
                out.push(ShipEvent::Sound {
                    slot: ship_sound::GEAR_DOWN,
                    at: None,
                });
                out.push(ShipEvent::Sound {
                    slot: ship_sound::ARRIVE,
                    at: Some(self.pos),
                });
            }
            return;
        }

        if self.state != 0 {
            let target_y = self.pos[1] - 650.0 - (self.delta_y - 2048.0);
            if let Some(c) = herd.chars.get_mut(ci) {
                delta_func(&mut c.pos[1], target_y, tdtf / 3.0);
                delta_func(&mut c.beta, 0.0, dtf / 4048.0);
                delta_func(&mut c.gamma, 0.0, dtf / 4048.0);
            }
            if self.state == 2 {
                self.ftime = (self.ftime - tdt).max(0);
                let up = herd
                    .chars
                    .get(ci)
                    .is_none_or(|c| (c.pos[1] - target_y).abs() < 1.0);
                if self.ftime == 0 && up {
                    self.state = 1;
                    out.push(ShipEvent::Sound {
                        slot: ship_sound::LIFT,
                        at: None,
                    });
                    out.push(ShipEvent::Sound {
                        slot: ship_sound::DEPART,
                        at: Some(self.pos),
                    });
                }
                return;
            }
        }

        // Speed.
        let mut vspeed = (1.0 + l / 128.0).min(24.0);
        if self.state != 0 {
            vspeed = 24.0;
        }
        if dalpha > 0.4 {
            vspeed = 0.0;
        }
        let was = self.speed;
        if vspeed > self.speed {
            delta_func(&mut self.speed, vspeed, dtf / 200.0);
        } else {
            self.speed = vspeed;
        }
        if self.speed > 0.0 && was == 0.0 {
            out.push(ShipEvent::Sound {
                slot: ship_sound::DEPART,
                at: Some(self.pos),
            });
        }

        // Fly.
        let mut step = dtf * self.speed / 16.0;
        if dalpha < 0.4 {
            if step < l {
                if step > l2 {
                    step = l2 * 0.5;
                }
                if l2 < 0.1 {
                    step = 0.0;
                }
                self.pos[0] += self.alpha.cos() * step;
                self.pos[2] += self.alpha.sin() * step;
            } else if self.state != 0 {
                // Gone: the body leaves with it.
                self.state = -1;
                out.push(ShipEvent::Removed { index: ci });
                self.cindex = None;
                return;
            } else {
                // Arrived over the body: lower the gear.
                self.pos = self.tgpos;
                self.state = 3;
                self.ftime = 1;
                self.tgpos = self.retpos;
                self.tgpos[1] = (area.land_up_h(self.tgpos[0], self.tgpos[2]) + self.delta_y)
                    .max(area.land_up_h(self.pos[0], self.pos[2]) + self.delta_y);
                if let Some(c) = herd.chars.get_mut(ci) {
                    c.state_f = 0xff;
                }
                out.push(ShipEvent::Sound {
                    slot: ship_sound::ARRIVE,
                    at: Some(self.pos),
                });
            }
        }

        // Height: hold above the ground.
        let h = area.land_up_h(self.pos[0], self.pos[2]);
        delta_func(&mut self.pos[1], self.tgpos[1], dtf / 4.0);
        if self.pos[1] < h + 1024.0 {
            if self.state != 0 {
                if let Some(c) = herd.chars.get_mut(ci) {
                    c.pos[1] += h + 1024.0 - self.pos[1];
                }
            }
            self.pos[1] = h + 1024.0;
        }

        // Turn.
        let mut currspeed = if self.tgalpha > self.alpha {
            0.1 + drspd / 2.0
        } else {
            -0.1 - drspd / 2.0
        };
        if dalpha > PI {
            currspeed = -currspeed;
        }
        delta_func(&mut self.rspeed, currspeed, dtf / 420.0);
        let rspd = self.rspeed * dtf / 1024.0;
        if drspd < rspd.abs() {
            self.alpha = self.tgalpha;
            self.rspeed /= 2.0;
        } else {
            self.alpha += rspd;
            if self.state != 0 {
                if let Some(c) = herd.chars.get_mut(ci) {
                    c.alpha += rspd;
                }
            }
        }
        if self.alpha < 0.0 {
            self.alpha += PI * 2.0;
        }
        if self.alpha > PI * 2.0 {
            self.alpha -= PI * 2.0;
        }

        // Carry the body / follow it while approaching.
        if self.state != 0 {
            if let Some(c) = herd.chars.get_mut(ci) {
                c.pos[0] = self.pos[0];
                c.pos[2] = self.pos[2];
            }
            if l > 1000.0 {
                self.tgpos[1] += dtf / 12.0;
            }
        } else if let Some(c) = herd.chars.get(ci) {
            self.tgpos = [c.pos[0], 0.0, c.pos[2]];
            self.tgpos[1] = (area.land_up_h(c.pos[0], c.pos[2]) + self.delta_y)
                .max(area.land_up_h(self.pos[0], self.pos[2]) + self.delta_y);
        }
    }
}
