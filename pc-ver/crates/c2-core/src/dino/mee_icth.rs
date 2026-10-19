//! The Modder's Edition Engine's Ichthyornis (`Characters.cpp:
//! AnimateIcth`, dialect AI 31): a sea bird that walks the shore and swims,
//! idles (on land `idlegroup`, on water `waterIgroup`), shakes itself dry,
//! and when startled (within 1050 of the hunter, or now and then on its
//! own) takes off, flies high, glides back down and lands. Shot, it falls
//! like the other flyers (`super::mee_flyer::animate_dead`).

use c2_formats::mee::SpawnGroup;

use super::huntable::{idle_groups, World};
use super::mee::MeeInfo;
use super::{
    check_place_collision_aquatic, corrected_alpha, delta_func, find_vector_alpha, in_region, sgn,
    wrap_angle, Character, Ctx, Species, CS_ON_WATER, PI,
};
use crate::area::{cell_flags, idx, Area};
use crate::rng::MsRand;

/// `goto TBEGIN` limit, as in [`super::huntable`].
const MAX_RETHINK: usize = 16;

/// `waterNear(x, z, 50)`. The original ignores its position and only tests
/// the map's corner cell (`FMap[-50 / 256][-50 / 256]` and so on are all
/// cell 0, 0); kept.
fn water_near(area: &Area) -> bool {
    area.map
        .flags
        .get(idx(0, 0))
        .is_some_and(|f| f & cell_flags::WATER_ANY != 0)
}

impl Character {
    /// `SetNewTargetPlace_Icth`.
    pub(crate) fn set_new_target_place_icth(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        r: f32,
        group: Option<&SpawnGroup>,
    ) {
        let mut tr = 0;
        loop {
            let mut p = [
                (self.pos[0] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
                0.0,
                (self.pos[2] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
            ];
            tr += 1;
            if tr < 16 && (p[0] - self.pos[0]).abs() + (p[2] - self.pos[2]).abs() < r / 2.0 {
                continue;
            }
            if tr < 1024 {
                if let Some(g) = group {
                    if g.stay_in_region && !g.regions.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                    if g.avoid.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                }
            }
            if tr < 100
                && (!water_near(area) || self.spawn_alt + 400.0 < area.land_up_h(p[0], p[2]))
            {
                continue;
            }
            if tr < 256 && check_place_collision_aquatic(area, &mut p) {
                continue;
            }
            self.tgtime = 0;
            self.tgx = p[0];
            self.tgz = p[2];
            return;
        }
    }
}

pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    w: &mut World<'_>,
) {
    let dt = ctx.dt;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let area = ctx.area;
    let up = |c: &Character| area.land_up_h(c.pos[0], c.pos[2]);
    let air = |p: usize| p == m.fly || p == m.glide || p == m.takeoff || p == m.land;
    let avoid = w.group.map_or(&[][..], |g| &g.avoid);
    let mut new_phase = false;

    if c.afraid_time != 0 {
        c.afraid_time = (c.afraid_time - dt).max(0);
    }
    if c.state == 2 || (c.state == 0 && w.pack.as_ref().is_some_and(|p| p.alert_prev)) {
        new_phase = true;
        c.state = 1;
    }
    c.ftime += dt;

    // TBEGIN.
    let mut rethink = 0;
    let wy = 'tbegin: loop {
        rethink += 1;
        let again = rethink < MAX_RETHINK;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let pdist = ((ctx.hunter.x - c.pos[0]).powi(2) + (ctx.hunter.z - c.pos[2]).powi(2)).sqrt();
        let pdist_up = (pdist * pdist + (ctx.hunter.y - c.pos[1]).powi(2)).sqrt();
        let ground = area.land_height(c.pos[0], c.pos[2]);
        c.state_f = if up(c) - ground > 20.0 {
            c.state_f | CS_ON_WATER
        } else {
            0
        };

        if c.state != 0 {
            c.idle_group = None;
            c.idle2_group = None;
            let relax = match w.pack.as_deref_mut() {
                Some(p) if c.afraid_time != 0 => {
                    p.alert = true;
                    false
                }
                Some(p) => !p.alert_prev,
                None => c.afraid_time == 0,
            };
            if relax && again {
                if c.pos[1] >= up(c) + 236.0 {
                    c.gliding = true;
                } else {
                    c.phase = m.land;
                    new_phase = true;
                }
                c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
                c.state = 0;
                continue;
            }
        }
        if again
            && pdist > ((ctx.view_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }
        if c.state == 0 {
            c.afraid_time = 0;
            if pdist_up < 1050.0 && again {
                c.state = 1;
                c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
                c.afraid_time = (50 + ctx.rng.r_rand(8)) * 1024;
                new_phase = true;
                if let Some(p) = w.pack.as_deref_mut() {
                    p.alert = true;
                }
                continue;
            }
        }
        let near = if c.phase == m.fly || c.phase == m.glide {
            2024.0
        } else {
            456.0
        };
        if let Some(l) = w.leader.filter(|_| c.pack.is_some()) {
            let leaderdist = ((l[0] - c.pos[0]).powi(2) + (l[2] - c.pos[2]).powi(2)).sqrt();
            if c.follow_leader {
                if again && leaderdist < c.pack_density * 128.0 * 0.6 {
                    c.follow_leader = false;
                    c.set_new_target_place_icth(area, ctx.rng, 4048.0, w.group);
                    continue;
                }
            } else if leaderdist > c.pack_density * 128.0 * 1.3 {
                c.follow_leader = true;
            }
        }
        if let Some(l) = w.leader.filter(|_| c.follow_leader) {
            c.tgx = l[0];
            c.tgz = l[2];
        } else if again && tdist < near {
            c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
            continue;
        }

        // The original repeats this on every `goto TBEGIN`; once here.
        if rethink == 1 {
            c.process_prev_phase(sp, dt);
        }
        if c.ftime >= sp.time(c.phase) {
            c.ftime %= sp.time(c.phase);
            new_phase = true;
        }
        let wy = up(c) - ground;
        let swimming = wy >= m.water_level as f32 * c.scale;

        // Select a new phase; `break 'think` is `goto ENDPSELECT`.
        'think: {
            if new_phase {
                if c.state == 0 {
                    if c.gliding {
                        c.phase = m.glide;
                    } else if c.phase != m.land {
                        let chose = if swimming {
                            idle_groups(
                                &mut c.phase,
                                &mut c.idle2_group,
                                &m.idle2_groups,
                                m.swim,
                                ctx.rng,
                            )
                        } else {
                            idle_groups(
                                &mut c.phase,
                                &mut c.idle_group,
                                &m.idle_groups,
                                m.walk,
                                ctx.rng,
                            )
                        };
                        if chose {
                            break 'think;
                        }
                    }
                }
                if c.state != 0 {
                    let afraid =
                        c.afraid_time != 0 || w.pack.as_ref().is_some_and(|p| p.alert_prev);
                    if afraid {
                        if c.phase == m.fly {
                            if c.pos[1] > up(c) + 2100.0 {
                                c.phase = m.glide;
                                c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
                            }
                        } else if c.phase == m.glide {
                            if c.pos[1] < up(c) + 1600.0 {
                                c.phase = m.fly;
                                c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
                            }
                        } else if c.phase == m.takeoff {
                            if c.pos[1] > up(c) + 236.0 {
                                c.phase = m.fly;
                            }
                        } else {
                            c.phase = m.takeoff;
                            if !c.not_flushed {
                                c.activate_fx(index, sp, ctx);
                            } else {
                                c.not_flushed = false;
                            }
                            c.gamma = 0.0;
                            c.beta = 0.0;
                            c.bend = 0.0;
                        }
                    } else if c.gliding {
                        c.phase = m.glide;
                    } else if c.phase != m.land {
                        c.phase = if swimming { m.swim } else { m.walk };
                    }
                    if (c.idle_group.is_some() || c.idle2_group.is_some())
                        && ctx.rng.r_rand(24) > 23
                        && again
                    {
                        c.state = 1;
                        c.set_new_target_place_icth(area, ctx.rng, 2048.0, w.group);
                        c.afraid_time = (50 + ctx.rng.r_rand(8)) * 1024;
                        c.not_flushed = true;
                        new_phase = true;
                        continue 'tbegin;
                    }
                }
            }
            if c.gliding && c.pos[1] <= up(c) + 236.0 && again {
                c.gliding = false;
                c.phase = m.land;
                new_phase = true;
                continue 'tbegin;
            }
            if c.phase == m.land {
                if c.pos[1] <= up(c) + 15.0 {
                    c.phase = if c.state_f & CS_ON_WATER != 0 {
                        m.shake_water
                    } else {
                        m.shake_land
                    };
                } else if c.pos[1] > up(c) + 256.0 && again {
                    c.gliding = true;
                    new_phase = true;
                    continue 'tbegin;
                }
            }
            // On the wrong footing for its walk / swim or idles: rethink.
            if again {
                let (gait, idle) = if swimming {
                    (m.walk, &mut c.idle_group)
                } else {
                    (m.swim, &mut c.idle2_group)
                };
                if c.phase == gait {
                    new_phase = true;
                    continue 'tbegin;
                }
                if idle.is_some() {
                    *idle = None;
                    new_phase = true;
                    continue 'tbegin;
                }
            }
            if new_phase {
                if c.phase == m.walk || c.idle_group.is_some() {
                    if c.shake_time < 9 {
                        c.shake_time += 1;
                    }
                    if c.shake_time == 8 {
                        c.phase = m.shake_land;
                    }
                } else {
                    c.shake_time = 0;
                }
            }
            if air(c.phase) {
                c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
            } else {
                if c.no_find_cnt != 0 {
                    c.no_find_cnt -= 1;
                } else {
                    c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
                    if c.afraid_time != 0 {
                        c.tgalpha += (ctx.real_time as f32 / 1024.0).sin() / 3.0;
                        wrap_angle(&mut c.tgalpha);
                    }
                }
                c.look_for_a_way_avoid(area, false, true, avoid);
                if c.no_way_cnt > 12 {
                    c.no_way_cnt = 0;
                    c.no_find_cnt = 32 + ctx.rng.r_rand(60);
                }
            }
            wrap_angle(&mut c.tgalpha);
        }
        break wy;
    };
    let swimming = wy >= m.water_level as f32 * c.scale;

    // ENDPSELECT.
    if old_phase != c.phase || new_phase {
        if c.phase == m.fly || c.phase == m.glide {
            if (ctx.rng.rand() & 1023) > 880 {
                c.activate_fx(index, sp, ctx);
            }
        } else if c.phase != m.takeoff {
            c.activate_fx(index, sp, ctx);
        }
    }
    if old_phase != c.phase {
        let moving = |p: usize| p == m.walk || p == m.swim || air(p);
        if moving(old_phase) && moving(c.phase) {
            c.ftime = old_ftime * sp.time(c.phase) / sp.time(old_phase) + 64;
        } else if !new_phase {
            c.ftime = 0;
        }
        if c.pp_morph_time > 128 {
            c.prev_phase = old_phase;
            c.prev_pf_time = old_ftime;
            c.pp_morph_time = 0;
        }
    }
    c.ftime %= sp.time(c.phase);

    // Turn.
    let flying = air(c.phase);
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if c.idle_group.is_none() && c.phase != m.shake_land {
        let (base, k) = if flying { (0.6, 1.2) } else { (0.2, 1.0) };
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                base + drspd * k
            } else {
                -base - drspd * k
            }
        } else {
            0.0
        };
        if dalpha > PI {
            currspeed *= -1.0;
        }
        if c.idle2_group.is_some() {
            currspeed /= 1.4;
        }
        if c.phase == m.swim || c.phase == m.shake_water {
            currspeed /= 1.4;
        }
        let rate = if flying { 460.0 } else { 400.0 };
        delta_func(&mut c.rspeed, currspeed, dt as f32 / rate);
        let max = if flying { PI / 2.0 } else { PI / 3.0 };
        let tgbend = (drspd / 2.0).min(max) * sgn(currspeed);
        let rspd = if flying {
            let rate = if tgbend.abs() > c.bend.abs() {
                800.0
            } else {
                400.0
            };
            delta_func(&mut c.bend, tgbend, dt as f32 / rate);
            c.rspeed * dt as f32 / 1024.0
        } else {
            delta_func(&mut c.bend, tgbend, dt as f32 / 2000.0);
            c.rspeed * dt as f32 / 612.0
        };
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
    let mut curspeed = 0.0;
    for (p, v) in [
        (m.fly, m.fly_speed),
        (m.glide, m.glide_speed),
        (m.takeoff, m.takeoff_speed),
        (m.land, m.land_speed),
        (m.walk, m.walk_speed),
        (m.swim, m.swim_speed),
        (m.shake_water, m.swim_speed),
    ] {
        if c.phase == p {
            curspeed = v;
        }
    }
    if c.idle2_group.is_some() {
        curspeed = m.swim_speed;
    }
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    if flying {
        let u = up(c);
        if c.phase == m.fly {
            delta_func(&mut c.pos[1], u + 4048.0, dt as f32 / 6.0);
        }
        if c.phase == m.glide {
            let rate = if c.gliding { 8.0 } else { 16.0 };
            delta_func(&mut c.pos[1], u, dt as f32 / rate);
        }
        if c.phase == m.takeoff {
            delta_func(&mut c.pos[1], u + 4048.0, dt as f32 / 5.0);
        }
        if c.phase == m.land {
            delta_func(&mut c.pos[1], u, dt as f32 / 4.0);
        }
        if !c.gliding {
            if c.phase != m.land && c.phase != m.takeoff {
                if c.pos[1] < u + 236.0 {
                    c.pos[1] = u + 256.0;
                }
            } else if c.pos[1] < u {
                c.pos[1] = u;
            }
        }
    }
    // `|| shakeWaterAnim` (an index, not a test) in the original: true for
    // any species that lists one after another animation; kept.
    let swimming_anim = c.phase == m.swim || m.shake_water != 0 || c.idle2_group.is_some();
    curspeed *= c.scale;
    if flying {
        delta_func(&mut c.vspeed, curspeed, dt as f32 / 2024.0);
        c.pos[0] += c.lookx * c.vspeed * dt as f32;
        c.pos[2] += c.lookz * c.vspeed * dt as f32;
        c.tggamma = (c.rspeed / 4.0).clamp(-PI / 6.0, PI / 6.0);
        delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2048.0);
    } else {
        let rate = if curspeed > c.vspeed { 1024.0 } else { 256.0 };
        delta_func(&mut c.vspeed, curspeed, dt as f32 / rate);
        let k = c.vspeed * dt as f32;
        c.move_character_avoid(area, c.lookx * k, c.lookz * k, false, avoid);
        if !swimming_anim {
            c.think_y_beta_gamma(area, dt, 128.0, 64.0, 0.6, 0.4);
            c.tggamma += c.rspeed / if c.phase == m.walk { 16.0 } else { 10.0 };
            delta_func(&mut c.gamma, c.tggamma, dt as f32 / 2048.0);
        } else {
            c.gamma = 0.0;
        }
    }
    if swimming && !flying {
        c.pos[1] = up(c) - m.water_level as f32 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    }
}
