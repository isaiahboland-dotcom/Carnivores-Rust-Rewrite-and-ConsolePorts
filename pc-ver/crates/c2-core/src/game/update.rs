//! Fixed-tick hunt scheduling and player effects.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{GameEvent, Hunt, HuntInput, WeaponState, MAX_HEALTH};
use crate::area::Area;
use crate::dino::{hunt, Ctx, DinoEvent, PI};
use crate::player::PlayerEvent;
use crate::ship::ShipEvent;
use c2_formats::car::Car;

impl Hunt {
    /// Advances one tick of `dt` ms.
    pub fn tick(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        input: &HuntInput,
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        // The main loop skips processing while paused, unless dead.
        if self.paused && self.health > 0 {
            return;
        }
        if self.time_stopped && self.free_observer() && self.health > 0 {
            self.stopped_tick(area, input, dt, out);
            return;
        }
        self.prof.start();
        self.real_time += dt;
        self.takt = self.takt.wrapping_add(1);
        // ProcessSyncro: health recovers 4 per ms while alive.
        if self.health > 0 {
            self.health = (self.health + dt * 4).min(MAX_HEALTH);
        }

        if let Some(d) = self.death {
            self.death_cam(area, d, dt);
        } else {
            if self.opts.trophy {
                let sp = &self.herd.species;
                self.player.obstacles.clear();
                self.player
                    .obstacles
                    .extend(self.herd.chars.iter().map(|c| {
                        [
                            c.pos[0],
                            c.pos[2],
                            sp.get(c.ctype).map_or(0.0, |s| s.info.radius),
                        ]
                    }));
            }
            let mut pev = Vec::new();
            // Hunt2.cpp: running only with the weapon away (the dialect's
            // `canRun` weapons too).
            let mut controls = input.controls;
            controls.run &= self.weapons.state == WeaponState::Hidden
                || self.weapons.mee().is_some_and(|m| m.can_run);
            if !self.fly_step(area, input, dt) {
                self.player.tick(area, &controls, dt as u32, &mut pev);
            }
            self.apply_recoil(dt);
            self.prof.mark(0);
            self.player_effects(area, &pev);
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            out.extend(pev.into_iter().map(GameEvent::Player));
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            if self.player.underwater && self.health > 0 {
                // Drowning.
                self.health -= dt * 12;
                if self.health <= 0 {
                    self.kill_hunter(area, None, hunt::BREATH, true, out);
                }
            }
            self.weapon_input(area, cars, input, out);
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            self.ambience(area, dt, out);
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            self.binoculars(input, dt);
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            self.calls(input, dt, out);
            #[cfg(feature = "hunt-diagnostics")]
            self.prof.mark(0);
            self.map_input(input, out);
            if self.weapons.state != WeaponState::Hidden || self.bin_mode {
                self.map_mode = false;
            }
            if !self.opts.trophy {
                self.path += dt as f32 * self.player.vspeed / 128.0;
                self.time_s += dt as f32 / 1000.0;
            }
        }
        self.weapon_step(dt, out);
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        self.animate_bullets(area, cars, dt, out);
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        self.track_step(dt);
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        self.prof.mark(1);

        let mut dev = Vec::new();
        let hv = self.hunter_view();
        let mut ctx = Ctx {
            area,
            rng: &mut self.rng,
            dt,
            real_time: self.real_time,
            hunter: hv,
            opt_agres: self.opts.opt_agres,
            tranq: self.opts.tranq,
            view_r: self.opts.view_r,
            render_r: self.opts.render_r.max(self.opts.view_r),
            run_k: self.opts.dino_run_k,
            events: &mut dev,
        };
        if !self.opts.trophy {
            self.herd.step_scheduled(&mut ctx, self.opts.distant_ai);
        }
        self.prof.mark(2);
        for e in dev {
            match e {
                DinoEvent::KillHunter {
                    index,
                    kind,
                    scream,
                } => self.kill_hunter(area, Some(index), kind, scream, out),
                DinoEvent::BloodTrail { pos } if self.opts.blood => {
                    self.effects.blood_trail(area, &mut self.rng, pos);
                }
                DinoEvent::BloodTrail { .. } => {}
                DinoEvent::Splash { pos } => {
                    // AnimateDimorDead hitting the water: five rings.
                    for sc in [2.0, 2.5, 3.0, 3.5, 3.0] {
                        let (x, z) = (
                            pos[0] + self.rng.si_rand(128) as f32,
                            pos[2] + self.rng.si_rand(128) as f32,
                        );
                        self.effects.ring(area, x, z, sc);
                    }
                }
                DinoEvent::Spray { pos } => {
                    let a = self.camera().1;
                    for _ in 0..3 {
                        let p = [
                            pos[0] + self.rng.si_rand(128) as f32,
                            pos[1],
                            pos[2] + self.rng.si_rand(128) as f32,
                        ];
                        self.effects.add(
                            area,
                            &mut self.rng,
                            p,
                            crate::effects::Part::Bubble,
                            10,
                            a,
                        );
                    }
                }
                _ => {}
            }
            out.push(GameEvent::Dino(e));
        }
        self.sync_killed_body(area);
        self.prof.mark(3);
        // AnimateProcesses: now and then, bubbles rise in deep water ahead.
        if self.takt & 63 == 0 {
            let (cam, a, _) = self.camera();
            let al = a + self.rng.si_rand(60) as f32 * PI / 180.0;
            let l = (1024 + self.rng.r_rand(3120)) as f32;
            let (x, z) = (cam[0] + al.sin() * l, cam[2] - al.cos() * l);
            if area.land_up_h(x, z) > area.land_height(x, z) + 256.0 {
                let n = 6 + self.rng.r_rand(6);
                self.effects.add(
                    area,
                    &mut self.rng,
                    [x, area.land_height(x, z), z],
                    crate::effects::Part::Bubble,
                    n,
                    a,
                );
            }
        }
        self.effects.step(area, dt, self.takt);
        if let Some(s) = &mut self.effects.snow {
            let wind = (self.wind.nv(), self.wind.speed);
            let at = (self.player.x, self.player.z);
            s.step(
                area,
                &mut self.rng,
                at,
                wind,
                self.player.alpha,
                self.real_time,
                dt,
            );
        }
        self.prof.mark(4);

        // AnimateProcesses: wind drifts every other frame.
        if self.takt & 1 != 0 {
            self.wind.alpha += self.rng.si_rand(16) as f32 / 4096.0;
            self.wind.speed += self.rng.si_rand(400) as f32 / 6400.0;
        }
        self.wind.speed = self.wind.speed.clamp(4.0, 18.0);
        if self.takt.is_multiple_of(self.opts.afraid_every.max(1)) {
            self.check_afraid(area);
        }
        self.prof.mark(5);
        if self.opts.trophy {
            self.process_trophy();
        } else if self.trophy_time > 0 {
            self.trophy_time -= dt;
            if self.trophy_time <= 0 {
                self.trophy_time = 0;
                self.trophy_body = None;
            }
        }
        self.exit_step(dt);
        self.survival_step(area, dt, out);
        self.teleport_step(area, dt, out);
        if (self.opts.observer || self.opts.trophy) && self.health > 0 {
            self.health = MAX_HEALTH;
        }

        let mut sev = Vec::new();
        let player = [self.player.x, self.player.y, self.player.z];
        let (view_r, rt, anim) = (self.opts.view_r, self.real_time, self.ship_anim_ms);
        self.ship
            .step(area, &mut self.herd, player, view_r, rt, dt, anim, &mut sev);
        for e in sev {
            if let ShipEvent::Removed { index } = e {
                self.remove_character(index);
            }
            out.push(GameEvent::Ship(e));
        }
        self.prof.mark(6);
        self.prof.end();
    }

    /// Ripples from the hunter: wading, diving in or out, swimming.
    fn player_effects(&mut self, area: &Area, events: &[PlayerEvent]) {
        let [cx, _, cz] = self.player.eye();
        for e in events {
            match e {
                PlayerEvent::StepWater => self.effects.ring(area, cx, cz, 1.2),
                PlayerEvent::WaterIn | PlayerEvent::WaterOut => {
                    self.effects.ring(area, cx, cz, 2.0)
                }
                _ => {}
            }
        }
        if self.player.swim && self.takt & 31 == 0 {
            self.effects.ring(area, cx, cz, 1.5);
        }
    }
}

impl Hunt {
    /// The map key: the map on or off with the weapon put away (the
    /// original's rule); with `map_holsters`, pressed with a weapon out it
    /// puts the weapon away and opens once it is.
    fn map_input(&mut self, input: &HuntInput, out: &mut Vec<GameEvent>) {
        let can = !self.opts.trophy && self.health > 0 && !self.bin_mode;
        // Taking a weapon out (again) forgets the map asked for.
        if !can || input.toggle_weapon || input.select_weapon.is_some() {
            self.map_pending = false;
        }
        if input.toggle_map && can {
            if self.weapons.state == WeaponState::Hidden {
                self.map_mode = !self.map_mode;
                self.map_pending = false;
            } else if self.opts.map_holsters {
                self.map_pending = !self.map_pending;
            }
        }
        if self.map_pending {
            match self.weapons.state {
                WeaponState::Hidden => {
                    self.map_mode = true;
                    self.map_pending = false;
                }
                WeaponState::Ready if self.weapons.ftime == 0 => self.hide_weapon(out),
                _ => {}
            }
        }
    }
}
