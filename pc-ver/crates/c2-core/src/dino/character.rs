//! Shared AI steps on one character (reset, senses, ways, re-placing).

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

use c2_formats::mee::{Region, SpawnGroup};

use super::{
    ai, anim, check_place_collision2_avoid, check_place_collision_avoid,
    check_place_collision_brahi_p, check_place_collision_fish_p, check_place_collision_p,
    delta_func, in_region, mee, Character, Ctx, DinoEvent, HunterView, Species, PI, PMORPH_TIME,
};
use crate::area::Area;
use crate::rng::MsRand;

impl Character {
    /// `ResetCharacter`.
    pub fn reset(&mut self, species: &Species, rng: &mut MsRand, opt_agres: i32) {
        self.pending_ai_ms = 0;
        self.ai = species.runs_as();
        self.state = 0;
        self.state_f = 0;
        self.phase = 0;
        self.ftime = 0;
        self.prev_phase = 0;
        self.prev_pf_time = 0;
        self.pp_morph_time = 0;
        self.beta = 0.0;
        self.gamma = 0.0;
        self.tggamma = 0.0;
        self.bend = 0.0;
        self.rspeed = 0.0;
        self.afraid_time = 0;
        self.blood_ttime = 0;
        self.blood_time = 0;
        self.lookx = self.alpha.cos();
        self.lookz = self.alpha.sin();
        if let Some(m) = &species.mee {
            // The original rolls these before the scale.
            self.tgalpha_offset = rng.r_rand((PI * 2.0) as i32) as f32;
            self.idle_group = None;
            self.idle2_group = None;
            self.aware_hunter = false;
            self.heard_shot = false;
            if m.kill_types.len() > 1 {
                self.kill_type = rng.r_rand(m.kill_types.len() as i32 - 1) as usize;
            }
            if !m.roars.is_empty() {
                self.roar_anim = m.roars[rng.r_rand(m.roars.len() as i32 - 1) as usize];
            }
            if m.death_types.len() > 1 {
                self.death_type = rng.r_rand(m.death_types.len() as i32 - 1) as usize;
            }
            if !m.water_dies.is_empty() {
                let n = m.water_dies.len() as i32 - 1;
                self.water_die_anim = m.water_dies[rng.r_rand(n) as usize];
            }
        }
        self.health = species.info.health0;
        if opt_agres > 128 {
            self.health = self.health * opt_agres / 128;
        }
        let scale_a = rng.r_rand(species.info.scale_a);
        self.scale = (species.info.scale0 + scale_a) as f32 / 1000.0;
        if let Some(m) = &species.mee {
            if scale_a > 0 && species.info.scale_a > 0 && m.health_a != 0 {
                self.health +=
                    (scale_a as f32 / species.info.scale_a as f32 * m.health_a as f32) as i32;
            }
            self.damaged = false;
            self.darted = false;
            self.follow_leader = false;
            self.aquatic_idle = false;
            self.turny = 0.0;
            self.last_tbeta = 0.0;
            self.spc_depth = m.spacing_depth as f32 + self.scale * 500.0 - 500.0;
        }
    }

    /// `ProcessPrevPhase`.
    pub fn process_prev_phase(&mut self, sp: &Species, dt: i32) {
        // The dialect scales the blend by `morphTime`.
        let dt = sp
            .mee
            .as_ref()
            .map_or(dt, |m| (dt as f32 * m.morph_time) as i32);
        self.pp_morph_time += dt;
        if self.pp_morph_time > PMORPH_TIME {
            self.prev_phase = self.phase;
        }
        self.prev_pf_time += dt;
        self.prev_pf_time %= sp.time(self.prev_phase);
    }

    /// `ActivateCharacterFx`.
    pub fn activate_fx(&self, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
        if self.ai != ai::HUNTER && ctx.hunter.underwater {
            return;
        }
        let Some(&fx) = sp.anifx.get(anim(sp.anims(), self.phase)) else {
            return;
        };
        let Ok(sound) = usize::try_from(fx) else {
            return;
        };
        let d = ((ctx.hunter.x - self.pos[0]).powi(2)
            + (ctx.hunter.y - self.pos[1]).powi(2)
            + (ctx.hunter.z - self.pos[2]).powi(2))
        .sqrt();
        if d > 68.0 * 256.0 {
            return;
        }
        ctx.events.push(DinoEvent::Sound {
            index,
            sound,
            pos: self.pos,
        });
    }

    /// `ActivateCharacterFxAquatic`: a sound heard only underwater.
    pub fn activate_fx_aquatic(&self, index: usize, sp: &Species, ctx: &mut Ctx<'_>) {
        if !ctx.hunter.underwater {
            return;
        }
        let Some(&fx) = sp.anifx.get(anim(sp.anims(), self.phase)) else {
            return;
        };
        let Ok(sound) = usize::try_from(fx) else {
            return;
        };
        let d = ((ctx.hunter.x - self.pos[0]).powi(2)
            + (ctx.hunter.y - self.pos[1]).powi(2)
            + (ctx.hunter.z - self.pos[2]).powi(2))
        .sqrt();
        if d <= 68.0 * 256.0 {
            ctx.events.push(DinoEvent::Sound {
                index,
                sound,
                pos: self.pos,
            });
        }
    }

    /// `ThinkY_Beta_Gamma`: sit on the ground and tilt with the slope.
    pub fn think_y_beta_gamma(
        &mut self,
        area: &Area,
        dt: i32,
        blook: f32,
        glook: f32,
        blim: f32,
        glim: f32,
    ) {
        let h = |x: f32, z: f32| area.land_height(x, z);
        self.pos[1] = h(self.pos[0], self.pos[2]);
        let hlook = h(
            self.pos[0] + self.lookx * blook,
            self.pos[2] + self.lookz * blook,
        );
        let hlook2 = h(
            self.pos[0] - self.lookx * blook,
            self.pos[2] - self.lookz * blook,
        );
        delta_func(
            &mut self.beta,
            (hlook2 - hlook) / (blook * 3.2),
            dt as f32 / 800.0,
        );
        self.beta = self.beta.clamp(-blim, blim);
        let hlook = h(
            self.pos[0] + self.lookz * glook,
            self.pos[2] - self.lookx * glook,
        );
        let hlook2 = h(
            self.pos[0] - self.lookz * glook,
            self.pos[2] + self.lookx * glook,
        );
        self.tggamma = ((hlook - hlook2) / (glook * 3.2)).clamp(-glim, glim);
    }

    /// `CheckPossiblePath`: blocked samples along `tgalpha` (20 x 64 units).
    pub(crate) fn check_possible_path(
        &self,
        area: &Area,
        wc: bool,
        mc: bool,
        avoid: &[Region],
    ) -> i32 {
        let mut p = self.pos;
        let (lx, lz) = (self.tgalpha.cos(), self.tgalpha.sin());
        let mut c = 0;
        for _ in 0..20 {
            p[0] += lx * 64.0;
            p[2] += lz * 64.0;
            if check_place_collision_avoid(area, &mut p, wc, mc, avoid) {
                c += 1;
            }
        }
        c
    }

    /// `LookForAWay`: fan out +-15..180 degrees for a clearer heading.
    pub fn look_for_a_way(&mut self, area: &Area, wc: bool, mc: bool) {
        self.look_for_a_way_avoid(area, wc, mc, &[]);
    }

    /// [`Self::look_for_a_way`] also steering clear of `avoid` regions.
    pub fn look_for_a_way_avoid(&mut self, area: &Area, wc: bool, mc: bool, avoid: &[Region]) {
        self.look_for_a_way_by(|c| c.check_possible_path(area, wc, mc, avoid));
    }

    /// `LookForAWay` with the AI's own `CheckPossiblePath` (`path` counts
    /// blocked samples along `tgalpha`).
    pub fn look_for_a_way_by(&mut self, path: impl Fn(&Character) -> i32) {
        let alpha = self.tgalpha;
        let mut dalpha = 15.0f32;
        let mut afound = alpha;
        let mut maxp = 16;
        if path(self) == 0 {
            self.no_way_cnt = 0;
            return;
        }
        self.no_way_cnt += 1;
        for i in 0..12 {
            for sign in [1.0, -1.0] {
                self.tgalpha = alpha + sign * dalpha * PI / 180.0;
                let curp = path(self) + (i >> 1);
                if curp == 0 {
                    return;
                }
                if curp < maxp {
                    maxp = curp;
                    afound = self.tgalpha;
                }
            }
            dalpha += 15.0;
        }
        self.tgalpha = afound;
    }

    /// `SetNewTargetPlace`: random reachable point within ~R.
    pub fn set_new_target_place(&mut self, area: &Area, rng: &mut MsRand, mut r: f32) {
        let mut tr = 0;
        loop {
            let mut p = [
                (self.pos[0] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
                0.0,
                (self.pos[2] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
            ];
            p[1] = area.land_height(p[0], p[2]);
            tr += 1;
            if tr < 128 && (p[0] - self.pos[0]).abs() + (p[2] - self.pos[2]).abs() < r / 2.0 {
                continue;
            }
            r += 512.0;
            if tr < 256 && check_place_collision_p(area, &mut p) {
                continue;
            }
            self.tgtime = 0;
            self.tgx = p[0];
            self.tgz = p[2];
            return;
        }
    }

    /// `MoveCharacter`. The released source returns at the first line (no
    /// dinosaur would ever move); the retail game clearly runs the body, which
    /// is what this ports. See docs/DECISIONS.md.
    pub fn move_character(&mut self, area: &Area, dx: f32, dz: f32, wc: bool) {
        self.move_character_avoid(area, dx, dz, wc, &[]);
    }

    /// [`Self::move_character`] also kept out of `avoid` regions.
    pub fn move_character_avoid(
        &mut self,
        area: &Area,
        dx: f32,
        dz: f32,
        wc: bool,
        avoid: &[Region],
    ) {
        let collides = |p: &mut [f32; 3]| check_place_collision2_avoid(area, p, wc, avoid);
        let mut p = self.pos;
        if collides(&mut p) {
            self.pos[0] += dx / 2.0;
            self.pos[2] += dz / 2.0;
            return;
        }
        p[0] += dx;
        p[2] += dz;
        if !collides(&mut p) {
            self.pos = p;
            return;
        }
        p = self.pos;
        p[0] += dx / 2.0;
        p[2] += dz / 2.0;
        if !collides(&mut p) {
            self.pos = p;
        }
        let mut p = self.pos;
        p[0] += dx / 4.0;
        p[2] += dz / 4.0;
        self.pos = p;
    }

    /// `ReplaceCharacterForward`: re-spawn ahead of the hunter, just beyond
    /// view range. Returns false if the chosen spot was unsuitable.
    pub fn replace_forward(&mut self, ctx: &mut Ctx<'_>) -> bool {
        let al = ctx.hunter.alpha + ctx.rng.si_rand(2048) as f32 / 2048.0;
        let (sa, ca) = al.sin_cos();
        let d1 = ((ctx.render_r + ctx.rng.r_rand(10)) * 256) as f32;
        let d2 = ((ctx.render_r + ctx.rng.r_rand(10)) * 256) as f32;
        let mut p = [ctx.hunter.x + sa * d1, 0.0, ctx.hunter.z - ca * d2];
        p[1] = ctx.area.land_height(p[0], p[2]);
        if p[0] < 16.0 * 256.0
            || p[2] < 16.0 * 256.0
            || p[0] > 1000.0 * 256.0
            || p[2] > 1000.0 * 256.0
        {
            return false;
        }
        if check_place_collision_p(ctx.area, &mut p) {
            return false;
        }
        self.state = 0;
        self.pos = p;
        self.set_new_target_place(ctx.area, ctx.rng, 2048.0);
        if self.ai == ai::DIMOR {
            self.pos[1] += 1048.0;
        }
        true
    }

    /// `SetNewTargetPlaceRegion`, the dialect's `SetNewTargetPlace`: as the
    /// original, but staying inside the spawn group's regions when it says
    /// so (`stayInRegion`) and out of its avoided ones.
    pub fn set_new_target_place_in(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        mut r: f32,
        group: Option<&SpawnGroup>,
    ) {
        let mut tr = 0;
        loop {
            let mut p = [
                (self.pos[0] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
                0.0,
                (self.pos[2] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
            ];
            p[1] = area.land_height(p[0], p[2]);
            tr += 1;
            if tr < 128 {
                if (p[0] - self.pos[0]).abs() + (p[2] - self.pos[2]).abs() < r / 2.0 {
                    continue;
                }
                if let Some(g) = group {
                    if g.stay_in_region && !g.regions.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                    if g.avoid.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                }
            }
            r += 512.0;
            if tr < 256 && check_place_collision_p(area, &mut p) {
                continue;
            }
            self.tgtime = 0;
            self.tgx = p[0];
            self.tgz = p[2];
            return;
        }
    }

    /// The dialect's `SetNewTargetPlace_Brahi`: brachiosaurs pick spots 200-400 deep
    /// (the land brachiosaur: dry land, or up to 400 deep if it swims),
    /// inside their spawn regions; any spot after 128 tries.
    pub(crate) fn set_new_target_place_wading(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        r: f32,
        group: Option<&SpawnGroup>,
        land: Option<bool>,
    ) {
        let mut tr = 0;
        loop {
            let p = [
                (self.pos[0] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
                0.0,
                (self.pos[2] + rng.si_rand(r as i32) as f32)
                    .clamp(crate::collide::WALK_MIN, crate::collide::WALK_MAX),
            ];
            tr += 1;
            if tr < 16 && (p[0] - self.pos[0]).abs() + (p[2] - self.pos[2]).abs() < r / 2.0 {
                continue;
            }
            let wy = area.land_up_h(p[0], p[2]) - area.land_height(p[0], p[2]);
            if tr < 128 {
                let bad_depth = match land {
                    Some(true) => wy > 400.0,
                    Some(false) => wy > 0.0,
                    None => !(200.0..=400.0).contains(&wy),
                };
                if bad_depth {
                    continue;
                }
                if let Some(g) = group {
                    if g.stay_in_region && !g.regions.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                    if g.avoid.iter().any(|g| in_region(g, p[0], p[2])) {
                        continue;
                    }
                }
            }
            self.tgtime = 0;
            self.tgx = p[0];
            self.tgz = p[2];
            return;
        }
    }

    /// A dialect animal's new target place after 30 s, or after a forward
    /// respawn: fish 1024 away, Mosasaurus 5048, others 2048.
    pub(crate) fn retarget_default(
        &mut self,
        sp: &Species,
        area: &Area,
        rng: &mut MsRand,
        group: Option<&SpawnGroup>,
    ) {
        let r = match sp.mee.as_ref().map(|m| m.ai) {
            Some(mee::mee_ai::FISH) => 1024.0,
            Some(mee::mee_ai::MOSA) => 5048.0,
            _ => 2048.0,
        };
        self.retarget(sp, area, rng, r, group);
    }

    /// A dialect animal's new target place, by its AI.
    pub(crate) fn retarget(
        &mut self,
        sp: &Species,
        area: &Area,
        rng: &mut MsRand,
        r: f32,
        group: Option<&SpawnGroup>,
    ) {
        match sp.mee.as_ref() {
            Some(m) if mee::is_fish(m.ai) => self.set_new_target_place_fish(area, rng, r, group),
            Some(m) if mee::is_brachi(m.ai) => {
                // `Some(can swim)` for the land brachiosaur.
                let land = (m.ai == mee::mee_ai::LANDBRACH).then_some(m.can_swim);
                self.set_new_target_place_wading(area, rng, r, group, land);
            }
            _ => self.set_new_target_place_in(area, rng, r, group),
        }
    }

    /// The dialect's `ReplaceCharacterForward`: only for `moveForward`
    /// spawn groups, only into the group's regions, and the character is
    /// reset as if newly placed.
    pub fn replace_forward_in(
        &mut self,
        sp: &Species,
        ctx: &mut Ctx<'_>,
        group: Option<&SpawnGroup>,
    ) -> bool {
        let Some(g) = group.filter(|g| g.move_forward) else {
            return false;
        };
        let al = ctx.hunter.alpha + ctx.rng.si_rand(2048) as f32 / 2048.0;
        let (sa, ca) = al.sin_cos();
        let d1 = ((ctx.render_r + ctx.rng.r_rand(10)) * 256) as f32;
        let d2 = ((ctx.render_r + ctx.rng.r_rand(10)) * 256) as f32;
        let mut p = [ctx.hunter.x + sa * d1, 0.0, ctx.hunter.z - ca * d2];
        p[1] = ctx.area.land_height(p[0], p[2]);
        if p[0] < 16.0 * 256.0
            || p[2] < 16.0 * 256.0
            || p[0] > 1000.0 * 256.0
            || p[2] > 1000.0 * 256.0
        {
            return false;
        }
        if !g.regions.iter().any(|r| in_region(r, p[0], p[2]))
            || g.avoid.iter().any(|r| in_region(r, p[0], p[2]))
        {
            return false;
        }
        let wader = sp
            .mee
            .as_ref()
            .is_some_and(|m| matches!(m.ai, mee::mee_ai::BRACH | mee::mee_ai::BRACHDANGER));
        let fish = sp.mee.as_ref().filter(|m| mee::is_fish(m.ai));
        let blocked = if let Some(m) = fish {
            check_place_collision_fish_p(ctx.area, &p, m.min_depth as f32, m.max_depth as f32)
        } else if wader {
            check_place_collision_brahi_p(ctx.area, &mut p)
        } else {
            check_place_collision_p(ctx.area, &mut p)
        };
        if blocked {
            return false;
        }
        self.pos = p;
        self.reset(sp, ctx.rng, ctx.opt_agres);
        self.retarget_default(sp, ctx.area, ctx.rng, Some(g));
        if fish.is_some() {
            let (up, land) = (
                ctx.area.land_up_h(self.pos[0], self.pos[2]),
                ctx.area.land_height(self.pos[0], self.pos[2]),
            );
            self.pos[1] = up - (up - land) / 2.0;
        }
        if let Some(m) = sp.mee.as_ref().filter(|m| mee::is_flyer(m.ai)) {
            self.pos[1] += m.min_depth as f32;
        }
        true
    }

    /// Distance and offset to the hunter on the ground plane.
    pub fn to_hunter(&self, h: &HunterView) -> (f32, f32, f32) {
        let (dx, dz) = (h.x - self.pos[0], h.z - self.pos[2]);
        (dx, dz, (dx * dx + dz * dz).sqrt())
    }

    /// Phase bookkeeping shared by every live AI after choosing `phase`:
    /// sound on change, time remapping between gaits, prev-phase blend.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_phase_change(
        &mut self,
        index: usize,
        sp: &Species,
        ctx: &mut Ctx<'_>,
        old: usize,
        old_ftime: i32,
        new_phase: bool,
        gait_max: usize,
    ) {
        if old != self.phase || new_phase {
            self.activate_fx(index, sp, ctx);
        }
        if old != self.phase {
            if old <= gait_max && self.phase <= gait_max {
                self.ftime = old_ftime * sp.time(self.phase) / sp.time(old) + 64;
            } else if !new_phase {
                self.ftime = 0;
            }
            if self.pp_morph_time > 128 {
                self.prev_phase = old;
                self.prev_pf_time = old_ftime;
                self.pp_morph_time = 0;
            }
        }
        self.ftime %= sp.time(self.phase);
    }
}
