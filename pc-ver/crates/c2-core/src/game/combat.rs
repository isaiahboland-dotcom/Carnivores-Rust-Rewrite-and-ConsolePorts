//! Shooting, hit resolution and flying bullets.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{weapon_anim, Bullet, GameEvent, Hunt, WeaponInfo, WeaponState};
use crate::area::Area;
use crate::dino::{ai, PI};
use crate::trace::{trace_shot, Hit};
use c2_formats::car::Car;

impl Hunt {
    /// `ProcessShoot`.
    pub(super) fn process_shoot(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        out: &mut Vec<GameEvent>,
    ) {
        let cur = self.weapons.current;
        let chambers = self.weapons.info.get(cur).is_some_and(WeaponInfo::chambers);
        let mee = self.weapons.mee();
        if !chambers {
            // The original fires whenever ready while the trigger is held,
            // a dialect weapon without the ammo model once per pull.
            if self.weapons.shots_left.get(cur).copied().unwrap_or(0) == 0 {
                return;
            }
        }
        let harpoon = mee.is_some_and(|m| m.harpoon);
        if self.player.underwater && !harpoon {
            self.hide_weapon(out);
            return;
        }
        if self.weapons.state != WeaponState::Ready || self.weapons.ftime != 0 {
            return;
        }
        if chambers {
            if self.weapons.chambered[cur] <= 0 {
                if !self.fire_held && !self.player.underwater {
                    out.push(GameEvent::Click(self.rng.r_rand(2) as usize));
                }
                return;
            }
            if self.fire_held && self.weapons.fire_mode[cur] == 0 {
                return;
            }
        } else if self.fire_held && mee.is_some() && !mee.is_some_and(|m| m.full_auto) {
            return;
        }
        self.weapons.ftime = 1;
        self.player.head_back_r = 64.0;
        self.weapon_sound(weapon_anim::FIRE, out);
        self.shots_made += 1;

        let info = self.weapons.info[cur].clone();
        let under = self.player.underwater;
        let (da, db) = self.weapon_sway();
        let p = &self.player;
        let origin = [p.x, p.y + p.head_y, p.z];
        let (alpha, beta) = (p.alpha, p.beta);
        let reach = 256.0 * self.opts.render_r.max(self.opts.view_r) as f32;
        if let Some(m) = &info.mee {
            // ProcessShoot: Recoil.y = -recoil / 100; x by up to half that.
            let r = m.recoil / 100.0;
            self.recoil[1] = -r;
            self.recoil[0] += (self.rng.r_rand(8) - 4) as f32 / 8.0 * r;
            if m.muzz_flash || m.cham_flash {
                self.weapons.flash = 1;
            }
            if m.muzz_flash {
                self.weapons.muzzle = Some(self.rng.r_rand(100) as f32 / 50.0 * PI);
            }
        }
        // Under water a harpoon has its own spread and speed; a round
        // crossing the surface takes the slower of the two (`ldif`).
        let prec = match &info.mee {
            Some(m) if under => m.aqua.prec,
            _ => info.prec,
        };
        let flying = info.mee.as_ref().filter(|m| m.veloc > 0.0).map(|m| {
            let v = if under { m.aqua.veloc } else { m.veloc };
            let l = if m.veloc > m.aqua.veloc {
                m.aqua.veloc
            } else {
                m.veloc
            };
            (v, l)
        });
        for _ in 0..=info.trace_c.max(0) {
            let ra = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - prec);
            let rb = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - prec);
            let (sa, ca) = (alpha + da + ra).sin_cos();
            let (sb, cb) = (beta + db + rb).sin_cos();
            let nv = [sa * cb, -sb, -ca * cb];
            if let Some((v, l)) = flying {
                // AddBullet: 64 * veloc units per 17 ms.
                self.bullets.push(Bullet {
                    pos: origin,
                    vel: nv.map(|n| n * 64.0 * v),
                    land_vel: nv.map(|n| n * 64.0 * l),
                    fall_total: 0.0,
                    aq_state: u8::from(under),
                    stuck: false,
                    ftime: 0,
                    weapon: cur,
                });
                continue;
            }
            let to = [
                origin[0] + nv[0] * reach,
                origin[1] + nv[1] * reach,
                origin[2] + nv[2] * reach,
            ];
            self.make_shot(area, cars, &info, origin, to, out);
        }
        if !under {
            // Stealth crouch: crouched, a shot carries half as far.
            let hush = if self.opts.stealth && self.player.head_y <= 180.0 {
                0.5
            } else {
                1.0
            };
            self.make_noise(
                [self.player.x, self.player.y, self.player.z],
                self.opts.view_r as f32 * 200.0 * info.loud * hush,
            );
        }
        if chambers {
            self.weapons.chambered[cur] -= 1;
        } else {
            self.weapons.shots_left[cur] -= 1;
        }
    }

    /// `MakeShot`: trace one round (in three falling segments for the
    /// crossbow) and apply damage.
    fn make_shot(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        info: &WeaponInfo,
        a: [f32; 3],
        b: [f32; 3],
        out: &mut Vec<GameEvent>,
    ) {
        let (chars, species) = (&self.herd.chars, &self.herd.species);
        let res = if !info.fall {
            trace_shot(area, chars, species, cars, a, b)
        } else {
            let dy = 40.0 * self.opts.view_r as f32 / 36.0;
            let dl = [
                (b[0] - a[0]) / 3.0,
                (b[1] - a[1]) / 3.0,
                (b[2] - a[2]) / 3.0,
            ];
            let mut from = a;
            let mut r = None;
            for drop in [dy / 2.0, dy * 3.0, dy * 5.0] {
                let to = [from[0] + dl[0], from[1] + dl[1] - drop, from[2] + dl[2]];
                r = trace_shot(area, chars, species, cars, from, to);
                if r.is_some() {
                    break;
                }
                from = to;
            }
            r
        };
        let Some((hit, pos)) = res else { return };
        self.apply_hit(area, info, hit, pos, false, out);
    }

    /// A round's hit (`MakeShot` / `AnimateBullet`): impact effects, then
    /// damage to an animal.
    /// `poon`: a harpoon's round under water (`AnimateBullet`): its
    /// `aqua_power`, and no dust where it stops.
    fn apply_hit(
        &mut self,
        area: &Area,
        info: &WeaponInfo,
        hit: Hit,
        pos: [f32; 3],
        poon: bool,
        out: &mut Vec<GameEvent>,
    ) {
        out.push(GameEvent::Impact { hit, pos });
        // AddElements per surface (MakeShot).
        let a = self.camera().1;
        let power = match &info.mee {
            Some(m) if poon => m.aqua.power,
            _ => info.power,
        };
        let pw = power.min(100.0) as i32;
        match hit {
            Hit::Ground | Hit::Model if poon => {}
            Hit::Ground | Hit::Model => self.effects.add(
                area,
                &mut self.rng,
                pos,
                crate::effects::Part::Ground,
                6 + pw * 4,
                a,
            ),
            Hit::Water => {
                self.effects.add(
                    area,
                    &mut self.rng,
                    pos,
                    crate::effects::Part::Water,
                    4 + pw * 3,
                    a,
                );
                self.effects.ring(area, pos[0], pos[2], 1.2);
                self.effects.ring(area, pos[0], pos[2], 1.2);
            }
            Hit::Character { .. } if self.opts.blood => self.effects.add(
                area,
                &mut self.rng,
                pos,
                crate::effects::Part::Blood,
                4 + pw * 4,
                a,
            ),
            Hit::Character { .. } => {}
        }
        let Hit::Character { index, mortal } = hit else {
            return;
        };
        let c = &mut self.herd.chars[index];
        if c.health == 0 {
            return;
        }
        // The dialect's `registerDamage` also marks the animal hurt and
        // aware of the hunter.
        c.damaged = true;
        if let Some((rgb, ms)) = info.mee.as_ref().and_then(|m| m.radar) {
            c.tracked = Some((rgb, (ms > 0).then_some(ms)));
        }
        let mortal = mortal && !info.mee.as_ref().is_some_and(|m| m.cannot_mortal);
        if mortal {
            c.health = 0;
        } else {
            c.health -= power as i32;
        }
        c.health = c.health.max(0);
        let mee = self
            .herd
            .species
            .get(c.ctype)
            .is_some_and(|s| s.mee.is_some());
        if c.health == 0 {
            let trophy = c.ai >= 10;
            let ct = c.ctype;
            out.push(GameEvent::Killed { index, trophy });
            if trophy {
                self.successes += 1;
                self.add_ship_task(area, index, out);
            }
            if self.herd.chars[index].wave > 0 {
                // A wave animal isn't replaced.
            } else if mee {
                let p = (self.player.x, self.player.z);
                let agres = self.opts.opt_agres;
                self.herd
                    .add_secondary_in(area, index, p, &mut self.rng, agres);
            } else if !trophy {
                self.add_secondary_one(area, ct);
            }
        } else {
            c.aware_hunter = true;
            c.afraid_time = 60 * 1000;
            if c.ai != ai::TREX || c.state == 0 {
                c.state = 2;
            }
            if mee || c.ai != ai::BRACH {
                c.blood_ttime += 90000;
            }
        }
        let c = &mut self.herd.chars[index];
        if c.ai == ai::TREX {
            c.state = if c.state != 0 { 5 } else { 1 };
        }
    }

    /// `AnimateBullets`: each flying round moves its way for this tick
    /// (scaled from the engine's 17 ms step), tracing what it passes; it
    /// falls by its weapon's `land_fall` a step, and is gone on a hit or
    /// past the view range.
    pub(super) fn animate_bullets(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        if self.bullets.is_empty() {
            return;
        }
        let k = dt as f32 / 17.0;
        let reach = 256.0 * self.opts.render_r.max(self.opts.view_r) as f32;
        let bullets = core::mem::take(&mut self.bullets);
        let mut kept = Vec::with_capacity(bullets.len());
        for mut b in bullets {
            if b.stuck {
                if !self.pick_up(&b, out) {
                    kept.push(b);
                }
                continue;
            }
            let Some(info) = self.weapons.info.get(b.weapon).cloned() else {
                continue;
            };
            let m = info.mee.clone().unwrap_or_default();
            // A harpoon's round under the surface (`poon`); crossing it, the
            // round takes its other speed, less the fall so far.
            let wet = |x: f32, y: f32, z: f32| {
                let up = area.land_up_h(x, z);
                m.harpoon && up > area.land_height(x, z) && up > y
            };
            let poon = wet(b.pos[0], b.pos[1], b.pos[2]);
            if b.aq_state < 2 && poon != (b.aq_state == 1) {
                b.aq_state = 2;
                b.vel = b.land_vel;
                b.vel[1] -= b.fall_total;
            }
            let to = [
                b.pos[0] + b.vel[0] * k,
                b.pos[1] + b.vel[1] * k,
                b.pos[2] + b.vel[2] * k,
            ];
            if wet(to[0], to[1], to[2]) {
                self.effects.add(
                    area,
                    &mut self.rng,
                    to,
                    crate::effects::Part::Bubble,
                    1,
                    0.0,
                );
            }
            let res = trace_shot(area, &self.herd.chars, &self.herd.species, cars, b.pos, to)
                // A harpoon goes through the surface.
                .filter(|(h, _)| !(m.harpoon && matches!(h, Hit::Water)));
            if let Some((hit, pos)) = res {
                let under = wet(pos[0], pos[1], pos[2]);
                self.apply_hit(area, &info, hit, pos, under, out);
                if m.retrieve && matches!(hit, Hit::Ground | Hit::Model) {
                    b.stuck = true;
                    b.pos = pos;
                    kept.push(b);
                }
                continue;
            }
            let (dx, dz) = (self.player.x - to[0], self.player.z - to[2]);
            if dx * dx + dz * dz > reach * reach {
                continue;
            }
            b.pos = to;
            let fall = if poon { m.aqua.fall } else { m.fall } * k;
            b.vel[1] -= fall;
            if b.aq_state < 2 {
                b.fall_total += fall;
            }
            b.ftime += dt;
            kept.push(b);
        }
        kept.append(&mut self.bullets);
        self.bullets = kept;
    }
}
