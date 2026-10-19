//! The Modder's Edition Engine's fish and Mosasaurus (`Characters.cpp:
//! AnimateFish`, `AnimateDeadFish`, dialect AI 32 and 33). They swim at a
//! depth (`Character::depth`) within the water between `mindepth` and
//! `maxdepth` deep, pitching toward a target depth; `DangerFish` hunt a
//! hunter in the water (a jumper leaps at one on a low bridge), others
//! swim away. Sounds are heard only underwater, except idles and leaps.

use c2_formats::mee::{Region, SpawnGroup};

use super::huntable::World;
use super::mee::{ai_info, mee_ai, MeeInfo};
use super::{
    angle_difference, check_place_collision_fish, corrected_alpha, delta_func, find_vector_alpha,
    hunt, in_region, sgn, wrap_angle, Character, Ctx, DinoEvent, Species, CS_ON_WATER, PI,
};
use crate::area::Area;
use crate::rng::MsRand;

/// `goto TBEGIN` limit, as in [`super::huntable`].
const MAX_RETHINK: usize = 16;

impl Character {
    /// `SetNewTargetPlaceFish`: a spot at least `0.7 r` away (in the
    /// group's regions), at a depth reachable without pitching more than
    /// about a radian, kept off the surface and bottom.
    pub(crate) fn set_new_target_place_fish(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        r: f32,
        group: Option<&SpawnGroup>,
    ) {
        let mut tr = 0;
        let p = loop {
            let p = [
                (self.pos[0] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
                (self.pos[2] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
            ];
            tr += 1;
            if tr < 128 {
                if (p[0] - self.pos[0]).abs() + (p[1] - self.pos[2]).abs() < r * 0.7 {
                    continue;
                }
                if let Some(g) = group {
                    if g.stay_in_region && !g.regions.iter().any(|g| in_region(g, p[0], p[1])) {
                        continue;
                    }
                    if g.avoid.iter().any(|g| in_region(g, p[0], p[1])) {
                        continue;
                    }
                }
            }
            break p;
        };
        let (up, land) = (area.land_up_h(p[0], p[1]), area.land_height(p[0], p[1]));
        // `int spcdm = 0.75` in the original: 0 while idling.
        let spcdm = if self.aquatic_idle { 0.0 } else { 1.0 };
        let tdist = ((p[0] - self.pos[0]).powi(2) + (p[1] - self.pos[2]).powi(2)).sqrt();
        let mut tr = 0;
        let target = loop {
            let mut t = if self.aquatic_idle {
                rng.r_rand((up - self.spc_depth * 0.68 - self.depth) as i32) as f32
            } else {
                rng.si_rand(tdist as i32) as f32
            };
            tr += 1;
            if tr < 1024 {
                // An `int` in the original: only a whole radian counts.
                let dbeta = ((-(t / tdist.max(1e-3)).atan() - self.beta) as i32).abs();
                if dbeta as f32 > PI / 16.0 {
                    continue;
                }
            }
            t += self.depth;
            let (lo, hi) = (land + self.spc_depth * spcdm, up - self.spc_depth * spcdm);
            if t < lo || t > hi {
                if tr < 3024 {
                    continue;
                }
                t = t.clamp(lo.min(hi), hi.max(lo));
            }
            break t;
        };
        self.tgtime = 0;
        self.tgx = p[0];
        self.tgz = p[1];
        self.tdepth = target;
        self.last_tbeta = self.beta;
        self.turny = 0.0;
    }

    /// `MoveCharacterFish`.
    fn move_fish(&mut self, area: &Area, m: &MeeInfo, dx: f32, dz: f32, avoid: &[Region]) {
        let (lo, hi) = (m.min_depth as f32, m.max_depth as f32);
        let blocked = |p: &[f32; 3]| check_place_collision_fish(area, p, lo, hi, avoid);
        if blocked(&self.pos) {
            self.pos[0] += dx / 2.0;
            self.pos[2] += dz / 2.0;
            return;
        }
        let mut p = self.pos;
        p[0] += dx;
        p[2] += dz;
        if !blocked(&p) {
            self.pos = p;
            return;
        }
        let mut p = self.pos;
        p[0] += dx / 2.0;
        p[2] += dz / 2.0;
        if !blocked(&p) {
            self.pos = p;
        }
        self.pos[0] += dx / 4.0;
        self.pos[2] += dz / 4.0;
    }
}

/// The fish branch of `CheckPossiblePath`.
fn path(c: &Character, area: &Area, m: &MeeInfo, avoid: &[Region]) -> i32 {
    let (lx, lz) = (c.tgalpha.cos(), c.tgalpha.sin());
    let (lo, hi) = (m.min_depth as f32, m.max_depth as f32);
    let mut p = c.pos;
    let mut n = 0;
    for _ in 0..20 {
        p[0] += lx * 64.0;
        p[2] += lz * 64.0;
        n += i32::from(check_place_collision_fish(area, &p, lo, hi, avoid));
    }
    n
}

pub fn animate(
    c: &mut Character,
    index: usize,
    sp: &Species,
    m: &MeeInfo,
    ctx: &mut Ctx<'_>,
    w: &mut World<'_>,
) {
    let a = ai_info(c.ai);
    let dt = ctx.dt;
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let kill = m.kill(c.kill_type);
    let eating = |c: &Character| kill.is_some_and(|k| c.phase == k.anim);
    let looking = |c: &Character| m.looks.contains(&c.phase);
    let avoid = w.group.map_or(&[][..], |g| &g.avoid);
    let area = ctx.area;
    let up = |x: f32, z: f32| area.land_up_h(x, z);
    let land = |x: f32, z: f32| area.land_height(x, z);
    // `tv`: a `switch` without `break` in the original, so always 5024.
    let tv = 5024.0;
    let h = ctx.hunter;
    let in_water = up(h.x, h.z) - land(h.x, h.z) > 0.0;
    let ao = if m.danger_fish { ctx.opt_agres } else { 0 };
    let reach = (ctx.view_r * m.aggress + ao / a.agress_multi.max(1)) as f32;

    let mut rethink = 0;
    let (targetdx, targetdz, pdist) = loop {
        rethink += 1;
        let again = rethink < MAX_RETHINK;
        let targetdx = c.tgx - c.pos[0];
        let targetdz = c.tgz - c.pos[2];
        let tdist2 = (targetdx * targetdx + targetdz * targetdz).sqrt();
        let playerdx = h.x - c.pos[0] - c.lookx * 100.0 * c.scale;
        let playerdz = h.z - c.pos[2] - c.lookz * 100.0 * c.scale;
        let pdist = (playerdx * playerdx + playerdz * playerdz).sqrt();
        let done = (targetdx, targetdz, pdist);
        if again
            && pdist > ((ctx.view_r + 20) * 256) as f32
            && c.replace_forward_in(sp, ctx, w.group)
        {
            continue;
        }
        // (Leap spray particles, `partCnt`, are not drawn yet.)
        c.state_f = if up(c.pos[0], c.pos[2]) - land(c.pos[0], c.pos[2]) > 180.0 * c.scale {
            c.state_f | CS_ON_WATER
        } else {
            0
        };
        if eating(c) {
            break done;
        }
        if !h.alive {
            c.state = 0;
        }

        if c.state == 0 {
            let mut attack = pdist <= reach && in_water && !m.dont_swim_away && h.alive;
            if attack {
                c.afraid_time = 10 * 1024;
            }
            if let Some(p) = w.pack.as_deref_mut().filter(|_| h.alive) {
                if attack {
                    p.alert = true;
                }
                if p.alert_prev {
                    attack = true;
                }
            }
            if attack {
                c.state = 1;
                c.turny = 0.0;
                c.last_tbeta = c.beta;
            } else {
                if let Some(l) = w.leader.filter(|_| c.pack.is_some()) {
                    let leaderdist = ((l[0] - c.pos[0]).powi(2) + (l[2] - c.pos[2]).powi(2)).sqrt();
                    if c.follow_leader {
                        if again && leaderdist < c.pack_density * 128.0 * 0.6 {
                            c.follow_leader = false;
                            c.set_new_target_place_fish(area, ctx.rng, tv, w.group);
                            continue;
                        }
                    } else if leaderdist > c.pack_density * 128.0 * 1.3 {
                        c.follow_leader = true;
                        c.turny = 0.0;
                        c.last_tbeta = c.beta;
                    }
                }
                if let Some(l) = w.leader.filter(|_| c.follow_leader) {
                    // A fish's `pos.y` is its depth.
                    (c.tgx, c.tgz, c.tdepth) = (l[0], l[2], l[1]);
                } else if again && tdist2 < 456.0 {
                    c.set_new_target_place_fish(area, ctx.rng, tv, w.group);
                    continue;
                }
            }
        }

        if c.state != 0 {
            if pdist > reach || !in_water {
                c.afraid_time -= dt;
                let calm = match w.pack.as_deref_mut() {
                    Some(p) if c.afraid_time > 0 => {
                        p.alert = true;
                        false
                    }
                    Some(p) => !p.alert_prev,
                    None => c.afraid_time <= 0,
                };
                if calm && again {
                    c.afraid_time = 0;
                    c.state = 0;
                    c.set_new_target_place_fish(area, ctx.rng, tv, w.group);
                    continue;
                }
            }
            if m.danger_fish {
                c.tgx = h.x;
                c.tgz = h.z;
                c.tdepth = h.y;
                if c.tdepth > up(c.tgx, c.tgz) - c.spc_depth * 0.75 {
                    c.tdepth = up(c.pos[0], c.pos[2]) - c.spc_depth * 0.75;
                }
                if a.jumper && c.depth < c.tdepth {
                    c.tdepth += (c.tdepth - c.depth) * 3.0;
                }
                if let Some(p) = w.pack.as_deref_mut() {
                    p.alert = true;
                }
            } else {
                let l = pdist.max(1e-6);
                c.tgx = c.pos[0] - playerdx / l * 2048.0;
                c.tgz = c.pos[2] - playerdz / l * 2048.0;
                let (u, g) = (up(c.pos[0], c.pos[2]), land(c.pos[0], c.pos[2]));
                c.tdepth = g + (u - g) / 2.0;
            }
            c.tgtime = 0;
            // A leap at a hunter near the surface (less high the higher the
            // hunter stands above the water).
            if c.phase != m.jump
                && a.jumper
                && m.danger_fish
                && c.depth > up(c.pos[0], c.pos[2]) - c.spc_depth * 0.95
            {
                let p_up = (h.y - up(h.x, h.z)).max(0.0);
                let md = (m.jump_range as f32 * m.jump_speed - p_up * 1.3) * c.scale;
                let (lo, hi) = (m.min_depth as f32, m.max_depth as f32);
                if pdist < md
                    && pdist > md - 200.0
                    && angle_difference(c.alpha, find_vector_alpha(playerdx, playerdz)) < 0.2
                    && !check_place_collision_fish(area, &[h.x, 0.0, h.z], lo, hi, avoid)
                {
                    c.phase = m.jump;
                    new_phase = true;
                    c.ftime = 0;
                    c.bend = 0.0;
                }
            }
            if m.kill_dist > 0 && pdist < m.kill_dist as f32 * c.scale {
                let mut kill_alt = c.spc_depth.max(256.0);
                if a.jumper && c.phase == m.jump {
                    kill_alt += 80.0;
                }
                if (h.y - c.pos[1]).abs() < kill_alt + 20.0 * c.scale {
                    let (kind, scream) = match kill {
                        Some(k) => {
                            c.vspeed /= 8.0;
                            c.state = 1;
                            c.phase = k.anim;
                            if k.dont_loop {
                                c.ftime = 0;
                            }
                            (k.hunter_anim, k.scream)
                        }
                        None => {
                            c.state = 0;
                            (hunt::EAT, true)
                        }
                    };
                    ctx.events.push(DinoEvent::KillHunter {
                        index,
                        kind,
                        scream,
                    });
                    c.aquatic_idle = false;
                }
            }
        }
        break done;
    };

    // NOTHINK.
    if pdist < 2048.0 {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = corrected_alpha(find_vector_alpha(targetdx, targetdz), c.alpha);
        if c.state != 0 && pdist > m.weave_range && !m.dont_weave {
            c.tgalpha += (ctx.real_time as f32 / 824.0).sin() / 2.0;
            wrap_angle(&mut c.tgalpha);
        }
    }
    c.look_for_a_way_by(|c| path(c, area, m, avoid));
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + ctx.rng.r_rand(20);
    }
    wrap_angle(&mut c.tgalpha);

    c.process_prev_phase(sp, dt);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime %= sp.time(c.phase);
        if eating(c) && kill.is_some_and(|k| k.dont_loop) {
            c.phase = m.walk;
            c.state = 0;
        }
        new_phase = true;
    }

    'select: {
        if eating(c) {
            break 'select;
        }
        if a.jumper {
            if new_phase && old_phase == m.jump {
                c.phase = m.run;
                break 'select;
            }
            if c.phase == m.jump {
                break 'select;
            }
        }
        if c.state != 0 || (new_phase && m.looks.contains(&old_phase)) {
            c.aquatic_idle = false;
        }
        if new_phase {
            if c.state != 0 {
                c.phase = m.run;
            } else {
                c.phase = m.walk;
                if !m.looks.is_empty() {
                    let carries = kill.is_some_and(|k| k.carry_corpse);
                    if !c.aquatic_idle
                        && ctx.rng.r_rand(128) > a.idle_start
                        && (h.alive || !carries)
                    {
                        c.aquatic_idle = true;
                    }
                    let level = PI / 32.0;
                    if c.aquatic_idle
                        && h.alive
                        && c.depth > up(c.pos[0], c.pos[2]) - c.spc_depth * 0.8
                        && c.beta.abs() < level
                        && c.gamma.abs() < level
                        && c.bend.abs() < level
                    {
                        let i = ctx.rng.r_rand(m.looks.len() as i32 - 1) as usize;
                        c.phase = m.looks[i];
                        new_phase = true;
                        c.ftime = 0;
                    }
                }
            }
        }
    }

    // ENDPSELECT.
    if old_phase != c.phase || new_phase {
        c.activate_fx_aquatic(index, sp, ctx);
        if c.phase != m.walk && c.phase != m.run {
            c.activate_fx(index, sp, ctx);
        }
    }
    if old_phase != c.phase {
        let gait = |p: usize| p == m.run || p == m.walk;
        if gait(old_phase) && gait(c.phase) {
            c.ftime = old_ftime * sp.time(c.phase) / sp.time(old_phase) + 64;
        }
        if c.pp_morph_time > 128 {
            c.prev_phase = old_phase;
            c.prev_pf_time = old_ftime;
            c.pp_morph_time = 0;
        }
    }
    c.ftime %= sp.time(c.phase);

    // Turn.
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    if !((a.jumper && c.phase == m.jump) || eating(c) || looking(c)) {
        let mut currspeed = if drspd > 0.02 {
            if c.tgalpha > c.alpha {
                0.6 + drspd * 1.2
            } else {
                -0.6 - drspd * 1.2
            }
        } else {
            0.0
        };
        if dalpha > PI {
            currspeed *= -1.0;
        }
        currspeed /= 1.4;
        let rate = if c.afraid_time != 0 { 250.0 } else { 460.0 };
        delta_func(&mut c.rspeed, currspeed, dt as f32 / rate);
        let tgbend = (drspd / 2.0).min(PI / 5.0) * sgn(currspeed);
        let rate = if tgbend.abs() > c.bend.abs() {
            800.0
        } else {
            600.0
        };
        delta_func(&mut c.bend, tgbend, dt as f32 / rate);
        let rspd = c.rspeed * dt as f32 / 1024.0;
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

    // Swim.
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == m.run {
        curspeed = m.run_speed * ctx.run_k;
    }
    if c.phase == m.walk {
        curspeed = m.walk_speed;
    }
    if a.jumper && c.phase == m.jump {
        curspeed = m.jump_speed;
    }
    if looking(c) {
        curspeed = m.walk_speed;
    }
    if eating(c) {
        curspeed = 0.0;
    }
    if drspd > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * drspd / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dt as f32 / 500.0);
    if a.jumper && c.phase == m.jump {
        c.vspeed = m.jump_speed;
    }
    let k = c.vspeed * dt as f32 * c.scale;
    c.move_fish(area, m, c.lookx * k, c.lookz * k, avoid);

    // Pitch toward the target depth, easing in after a new target.
    let tdist = ((c.tgx - c.pos[0]).powi(2) + (c.tgz - c.pos[2]).powi(2)).sqrt();
    let mut tbeta = -((c.tdepth - c.depth) / tdist.max(1e-3)).atan();
    if c.turny < PI {
        tbeta = (c.turny.cos() + 1.0) / 2.0 * (c.last_tbeta - tbeta) + tbeta;
        c.turny += PI / 100.0;
    }
    delta_func(
        &mut c.beta,
        tbeta,
        c.vspeed * dt as f32 * c.scale * (PI / 5000.0),
    );
    let climb = if c.ai == mee_ai::MOSA && c.phase == m.walk {
        25.0
    } else {
        35.0
    };
    c.depth -= c.beta * climb * curspeed;
    // (The original's vertical bend, `bdepth`, is computed but never drawn.)
    c.tggamma = if c.phase == m.walk {
        c.bend
    } else {
        c.bend * 2.0
    };
    c.tggamma += c.rspeed / if c.phase == m.walk { 10.0 } else { 8.0 };
    if a.jumper && c.phase == m.jump {
        c.tggamma = 0.0;
    }
    delta_func(&mut c.gamma, c.tggamma, dt as f32 / 1624.0);

    // Keep off the surface and the bottom.
    let (u, g) = (up(c.pos[0], c.pos[2]), land(c.pos[0], c.pos[2]));
    if c.depth > u - c.spc_depth / 2.0 || c.depth < g + c.spc_depth / 2.0 {
        c.depth = if c.depth > u - c.spc_depth / 2.0 {
            u - c.spc_depth / 2.0
        } else {
            g + c.spc_depth / 2.0
        };
        c.tdepth = g + (u - g) / 2.0;
        c.last_tbeta = c.beta;
    }
    c.pos[1] = c.depth;
}

/// `AnimateDeadFish`: sinks to the bottom and lies there.
pub fn animate_dead(c: &mut Character, index: usize, sp: &Species, m: &MeeInfo, ctx: &mut Ctx<'_>) {
    let dt = ctx.dt;
    let d = m.death(c.death_type);
    if c.phase != d.fall && c.phase != d.die {
        c.death_phase = c.phase;
        if c.pp_morph_time > 128 {
            c.prev_phase = c.phase;
            c.prev_pf_time = c.ftime;
            c.pp_morph_time = 0;
        }
        c.ftime = 0;
        c.phase = d.fall;
        c.rspeed = 0.0;
        c.activate_fx_aquatic(index, sp, ctx);
        return;
    }
    c.process_prev_phase(sp, dt);
    let lh = ctx.area.land_height(c.pos[0], c.pos[2]);
    c.ftime += dt;
    if c.ftime >= sp.time(c.phase) {
        c.ftime = sp.time(c.phase) - 1;
    }
    if c.phase == d.die {
        delta_func(&mut c.vspeed, 0.0, dt as f32 / 400.0);
    } else {
        for v in [&mut c.vspeed, &mut c.beta, &mut c.gamma, &mut c.bend] {
            delta_func(v, 0.0, dt as f32 / 1200.0);
        }
    }
    c.pos[0] += c.lookx * c.vspeed * dt as f32;
    c.pos[2] += c.lookz * c.vspeed * dt as f32;
    if c.phase == d.fall {
        c.pos[1] += c.rspeed * dt as f32 / 30000.0;
        c.rspeed -= dt as f32 * 2.56;
        if c.pos[1] <= lh {
            c.pos[1] = lh;
            c.phase = d.die;
            c.ftime = 0;
            c.activate_fx_aquatic(index, sp, ctx);
        }
    } else {
        c.think_y_beta_gamma(ctx.area, dt, 140.0, 126.0, 0.6, 0.5);
        delta_func(&mut c.gamma, c.tggamma, dt as f32 / 1600.0);
    }
}
