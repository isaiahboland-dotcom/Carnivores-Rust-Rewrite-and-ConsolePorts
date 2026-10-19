//! Characters (dinosaurs) and their AI: `Characters.cpp`.
//!
//! Each AI is ported from its `Animate*` function. Shared helpers live here;
//! the per-family logic is in the submodules. Fields mirror `TCharacter`.

mod brachi;
mod carnivore;
mod dead;
mod flyer;
mod herbivore;
mod place;
pub mod pose;
mod small;
mod trex;

use c2_formats::car::Car;
use c2_formats::res_txt::CharacterDef;

use crate::area::{cell_flags, idx, Area, NO_OBJECT};
use crate::model::car_anim_duration_ms;
use crate::rng::MsRand;
use crate::CELL;

pub use carnivore::CarnivoreParams;
pub use herbivore::HerbivoreParams;
pub use place::PlaceOptions;
pub use small::SmallParams;

/// `PMORPHTIME`: ms over which a previous animation blends out.
pub const PMORPH_TIME: i32 = 256;
/// `Hunt.h: pi` (3.1415926535f, i.e. f32 pi).
pub const PI: f32 = std::f32::consts::PI;

/// `AI_*` from `Hunt.h`.
pub mod ai {
    pub const HUNTER: i32 = 0;
    pub const MOSH: i32 = 1;
    pub const GALL: i32 = 2;
    pub const DIMOR: i32 = 3;
    pub const PTERA: i32 = 4;
    pub const DIMET: i32 = 5;
    pub const BRACH: i32 = 6;
    pub const PARA: i32 = 10;
    pub const ANKY: i32 = 11;
    pub const STEGO: i32 = 12;
    pub const ALLO: i32 = 13;
    pub const CHASM: i32 = 14;
    pub const VELO: i32 = 15;
    pub const SPINO: i32 = 16;
    pub const CERAT: i32 = 17;
    pub const TREX: i32 = 18;
}

/// `csONWATER` bit in `State`.
pub const CS_ON_WATER: i32 = 0x0001_0000;

/// `TDinoInfo`: one `_RES.TXT` character with `Game.cpp: InitGameInfo` defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct DinoInfo {
    pub name: String,
    pub file: String,
    pub ai: i32,
    pub health0: i32,
    pub mass: f32,
    pub length: f32,
    pub radius: f32,
    pub base_score: i32,
    pub smell_k: f32,
    pub hear_k: f32,
    pub look_k: f32,
    pub sh_delta: f32,
    pub scale0: i32,
    pub scale_a: i32,
    pub danger_call: bool,
}

impl DinoInfo {
    pub fn from_def(d: &CharacterDef) -> Self {
        Self {
            name: d.name.clone().unwrap_or_default(),
            file: d.file.clone().unwrap_or_default(),
            ai: d.ai.unwrap_or(0),
            health0: d.health.unwrap_or(0),
            mass: d.mass.unwrap_or(0.0),
            length: d.length.unwrap_or(0.0),
            radius: d.radius.unwrap_or(0.0),
            base_score: d.basescore.unwrap_or(0),
            smell_k: d.smell.unwrap_or(0.0),
            hear_k: d.hear.unwrap_or(0.0),
            look_k: d.look.unwrap_or(0.0),
            sh_delta: d.shipdelta.unwrap_or(0.0),
            scale0: d.scale0.unwrap_or(800),
            scale_a: d.scale_a.unwrap_or(600),
            danger_call: d.danger,
        }
    }
}

/// What the AI needs from a `.CAR`: per-animation length and sound map.
#[derive(Debug, Clone)]
pub struct Species {
    pub info: DinoInfo,
    /// `AniTime` per animation, ms (at least 1 so `%` is safe).
    pub ani_time: Vec<i32>,
    /// Frames per animation.
    pub frames: Vec<usize>,
    /// `Anifx`: sound index per animation, -1 = none.
    pub anifx: Vec<i32>,
}

impl Species {
    pub fn new(info: DinoInfo, car: &Car) -> Self {
        let ani_time = car
            .animations
            .iter()
            .map(|a| car_anim_duration_ms(a).max(1) as i32)
            .collect();
        let frames = car.animations.iter().map(|a| a.frames).collect();
        let anifx = car.anim_sounds.map_or_else(|| vec![-1; 64], |t| t.to_vec());
        Self {
            info,
            ani_time,
            frames,
            anifx,
        }
    }

    /// `AniTime` of `phase`, tolerating mods with fewer animations.
    pub fn time(&self, phase: usize) -> i32 {
        self.ani_time.get(phase).copied().unwrap_or(1)
    }
}

/// `TCharacter`.
#[derive(Debug, Clone, Default)]
pub struct Character {
    pub ctype: usize,
    pub ai: i32,
    pub state_f: i32,
    pub state: i32,
    pub no_way_cnt: i32,
    pub no_find_cnt: i32,
    pub afraid_time: i32,
    pub tgtime: i32,
    pub pp_morph_time: i32,
    pub prev_phase: usize,
    pub prev_pf_time: i32,
    pub phase: usize,
    pub ftime: i32,
    pub vspeed: f32,
    pub rspeed: f32,
    pub bend: f32,
    pub scale: f32,
    pub slide: i32,
    pub slidex: f32,
    pub slidez: f32,
    pub tgx: f32,
    pub tgz: f32,
    pub pos: [f32; 3],
    pub tgalpha: f32,
    pub alpha: f32,
    pub beta: f32,
    pub tggamma: f32,
    pub gamma: f32,
    pub lookx: f32,
    pub lookz: f32,
    pub health: i32,
    pub blood_time: i32,
    pub blood_ttime: i32,
}

/// What the AI sees of the hunter.
#[derive(Debug, Clone, Copy, Default)]
pub struct HunterView {
    /// `CameraAlpha`: where the hunter is looking (yaw).
    pub alpha: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub head_y: f32,
    /// `MyHealth != 0`.
    pub alive: bool,
    pub underwater: bool,
}

/// Side effects the AI asks the game for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DinoEvent {
    /// `ActivateCharacterFx`: play sound `sound` of character `index` at `pos`.
    Sound {
        index: usize,
        sound: usize,
        pos: [f32; 3],
    },
    /// `AddDeadBody(cptr, kind)`: the hunter is killed by character `index`.
    KillHunter { index: usize, kind: i32 },
    /// `AddBloodTrail`.
    BloodTrail { pos: [f32; 3] },
    /// A falling flyer hit the water surface (`AddWCircle` ripples).
    Splash { pos: [f32; 3] },
    /// A falling flyer landed in shallow water (`AddElements` spray).
    Spray { pos: [f32; 3] },
}

/// `HUNT_*` kill kinds.
pub mod hunt {
    pub const EAT: i32 = 0;
    pub const BREATH: i32 = 1;
    pub const FALL: i32 = 2;
    pub const KILL: i32 = 3;
}

/// Shared state for one AI step (`TimeDt`, `RealTime`, options...).
pub struct Ctx<'a> {
    pub area: &'a Area,
    pub rng: &'a mut MsRand,
    /// `TimeDt`, ms.
    pub dt: i32,
    /// `RealTime`, ms.
    pub real_time: i32,
    pub hunter: HunterView,
    /// `OptAgres` 0..=255 (128 default).
    pub opt_agres: i32,
    /// Tranquilizer mode: dying animals fall asleep instead.
    pub tranq: bool,
    /// `ctViewR`: view distance in cells (carnivores use it to give up).
    pub view_r: i32,
    /// Multiplies every species' running speed (1 in the original; a 3DS
    /// setting lowers it).
    pub run_k: f32,
    pub events: &'a mut Vec<DinoEvent>,
}

// ------------------------------------------------------------ math helpers

/// `DeltaFunc`: move `a` toward `b` by at most `d`.
pub fn delta_func(a: &mut f32, b: f32, d: f32) {
    if b > *a {
        *a = (*a + d).min(b);
    } else {
        *a = (*a - d).max(b);
    }
}

/// C `SGN`.
pub fn sgn(v: f32) -> f32 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `FindVectorAlpha`: angle of (vx, vy) in [0, 2pi), found by the same
/// 10-step bisection as the original (so results match to the last bit
/// that matters for AI decisions).
pub fn find_vector_alpha(vx: f32, vy: f32) -> f32 {
    let (adx, ady) = (vx.abs(), vy.abs());
    let mut alpha = PI / 4.0;
    let mut dalpha = PI / 8.0;
    for _ in 0..10 {
        // Mul2dVectors(adx, ady, cos, sin) = adx*sin - cos*ady
        alpha -= dalpha * sgn(adx * alpha.sin() - alpha.cos() * ady);
        dalpha /= 2.0;
    }
    if vx < 0.0 {
        if vy < 0.0 {
            alpha += PI;
        } else {
            alpha = PI - alpha;
        }
    } else if vy < 0.0 {
        alpha = 2.0 * PI - alpha;
    }
    alpha
}

/// `CorrectedAlpha`.
pub fn corrected_alpha(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    if d < PI {
        return (a + b) / 2.0;
    }
    let mut d = a + PI * 2.0 - b;
    if d < 0.0 {
        d += 2.0 * PI;
    }
    if d > 2.0 * PI {
        d -= 2.0 * PI;
    }
    d
}

/// `AngleDifference`.
pub fn angle_difference(a: f32, b: f32) -> f32 {
    let mut a = (a - b).abs();
    if a > PI {
        a = 2.0 * PI - a;
    }
    a
}

pub fn wrap_angle(a: &mut f32) {
    if *a < 0.0 {
        *a += 2.0 * PI;
    }
    if *a > 2.0 * PI {
        *a -= 2.0 * PI;
    }
}

// ------------------------------------------------------------ placement

fn cell_of(v: f32) -> i32 {
    (v as i32) / 256
}

fn flags_around(area: &Area, ccx: i32, ccz: i32) -> u16 {
    let f = |x: i32, z: i32| area.map.flags[idx(x as usize, z as usize)];
    f(ccx - 1, ccz)
        | f(ccx, ccz - 1)
        | f(ccx - 1, ccz - 1)
        | f(ccx, ccz)
        | f(ccx, ccz + 1)
        | f(ccx + 1, ccz)
        | f(ccx + 1, ccz + 1)
}

/// Any object with `Radius >= 10` within `Radius + 64` of (x, z)?
fn near_big_object(area: &Area, x: f32, z: f32, ccx: i32, ccz: i32) -> bool {
    for dz in -2..=2 {
        for dx in -2..=2 {
            let ob = area.map.objects[idx((ccx + dx) as usize, (ccz + dz) as usize)];
            if ob == NO_OBJECT {
                continue;
            }
            let Some(o) = area.rsc.objects.get(usize::from(ob)) else {
                continue;
            };
            // MObjects[].info.Radius, which LoadResources doubled.
            let r = o.info.radius as f32 * 2.0;
            if r < 10.0 {
                continue;
            }
            let (ox, oz) = (
                (ccx + dx) as f32 * CELL + 128.0,
                (ccz + dz) as f32 * CELL + 128.0,
            );
            if ((ox - x).powi(2) + (oz - z).powi(2)).sqrt() < r + 64.0 {
                return true;
            }
        }
    }
    false
}

/// `CheckPlaceCollisionP`: is this a bad spot to place/target a character?
pub fn check_place_collision_p(area: &Area, v: &mut [f32; 3]) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !(4..=1008).contains(&ccx) || !(4..=1008).contains(&ccz) {
        return true;
    }
    if flags_around(area, ccx, ccz) & (cell_flags::WATER | cell_flags::NO_WAY) != 0 {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    v[1] = h;
    for (dx, dz) in [
        (-164.0, -164.0),
        (164.0, -164.0),
        (-164.0, 164.0),
        (164.0, 164.0),
    ] {
        if (area.land_height(v[0] + dx, v[2] + dz) - h).abs() > 160.0 {
            return true;
        }
    }
    near_big_object(area, v[0], v[2], ccx, ccz)
}

/// `CheckPlaceCollision(v, wc, mc)`.
pub fn check_place_collision(area: &Area, v: &mut [f32; 3], wc: bool, mc: bool) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !(4..=1018).contains(&ccx) || !(4..=1018).contains(&ccz) {
        return true;
    }
    if wc && flags_around(area, ccx, ccz) & cell_flags::WATER != 0 {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    if area.map.flags[idx(ccx as usize, ccz as usize)] & cell_flags::WATER == 0
        && (h - v[1]).abs() > 64.0
    {
        return true;
    }
    v[1] = h;
    if slope_too_steep(area, v[0], v[2], h) {
        return true;
    }
    mc && near_big_object(area, v[0], v[2], ccx, ccz)
}

fn slope_too_steep(area: &Area, x: f32, z: f32, h: f32) -> bool {
    [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)]
        .iter()
        .any(|(dx, dz)| (area.land_height(x + dx, z + dz) - h).abs() > 100.0)
}

/// `CheckPlaceCollision2(v, wc)`.
pub fn check_place_collision2(area: &Area, v: &mut [f32; 3], wc: bool) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !(4..=1018).contains(&ccx) || !(4..=1018).contains(&ccz) {
        return true;
    }
    if wc && flags_around(area, ccx, ccz) & cell_flags::WATER != 0 {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    v[1] = h;
    slope_too_steep(area, v[0], v[2], h)
}

// ------------------------------------------------------------ shared AI steps

impl Character {
    /// `ResetCharacter`.
    pub fn reset(&mut self, species: &Species, rng: &mut MsRand, opt_agres: i32) {
        self.ai = species.info.ai;
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
        self.health = species.info.health0;
        if opt_agres > 128 {
            self.health = self.health * opt_agres / 128;
        }
        self.scale = (species.info.scale0 + rng.r_rand(species.info.scale_a)) as f32 / 1000.0;
    }

    /// `ProcessPrevPhase`.
    pub fn process_prev_phase(&mut self, sp: &Species, dt: i32) {
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
        let Some(&fx) = sp.anifx.get(self.phase) else {
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
    fn check_possible_path(&self, area: &Area, wc: bool, mc: bool) -> i32 {
        let mut p = self.pos;
        let (lx, lz) = (self.tgalpha.cos(), self.tgalpha.sin());
        let mut c = 0;
        for _ in 0..20 {
            p[0] += lx * 64.0;
            p[2] += lz * 64.0;
            if check_place_collision(area, &mut p, wc, mc) {
                c += 1;
            }
        }
        c
    }

    /// `LookForAWay`: fan out +-15..180 degrees for a clearer heading.
    pub fn look_for_a_way(&mut self, area: &Area, wc: bool, mc: bool) {
        let alpha = self.tgalpha;
        let mut dalpha = 15.0f32;
        let mut afound = alpha;
        let mut maxp = 16;
        if self.check_possible_path(area, wc, mc) == 0 {
            self.no_way_cnt = 0;
            return;
        }
        self.no_way_cnt += 1;
        for i in 0..12 {
            for sign in [1.0, -1.0] {
                self.tgalpha = alpha + sign * dalpha * PI / 180.0;
                let curp = self.check_possible_path(area, wc, mc) + (i >> 1);
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
                (self.pos[0] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
                0.0,
                (self.pos[2] + rng.si_rand(r as i32) as f32).clamp(512.0, 1018.0 * 256.0),
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
        let mut p = self.pos;
        if check_place_collision2(area, &mut p, wc) {
            self.pos[0] += dx / 2.0;
            self.pos[2] += dz / 2.0;
            return;
        }
        p[0] += dx;
        p[2] += dz;
        if !check_place_collision2(area, &mut p, wc) {
            self.pos = p;
            return;
        }
        p = self.pos;
        p[0] += dx / 2.0;
        p[2] += dz / 2.0;
        if !check_place_collision2(area, &mut p, wc) {
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
        let d1 = ((ctx.view_r + ctx.rng.r_rand(10)) * 256) as f32;
        let d2 = ((ctx.view_r + ctx.rng.r_rand(10)) * 256) as f32;
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

/// All characters plus the species table, stepped together
/// (`AnimateCharacters`).
pub struct Herd {
    pub species: Vec<Species>,
    pub chars: Vec<Character>,
    /// `DemoPoint.CIndex`: who the death camera watches (the hunter's body,
    /// or the T-Rex carrying it).
    pub demo_index: Option<usize>,
}

impl Herd {
    /// `AnimateCharacters`.
    pub fn step(&mut self, ctx: &mut Ctx<'_>) {
        for i in 0..self.chars.len() {
            let carrier = self
                .demo_index
                .and_then(|d| self.chars.get(d))
                .filter(|t| t.ai == ai::TREX)
                .map(|t| dead::Carrier {
                    pos: t.pos,
                    ftime: t.ftime,
                    beta: t.beta,
                    gamma: t.gamma,
                });
            let c = &mut self.chars[i];
            if c.state_f == 0xff {
                continue;
            }
            let Some(sp) = self.species.get(c.ctype) else {
                continue;
            };
            c.tgtime += ctx.dt;
            if c.tgtime > 30 * 1000 {
                c.set_new_target_place(ctx.area, ctx.rng, 2048.0);
            }
            if c.health != 0 && c.blood_ttime != 0 {
                c.blood_ttime = (c.blood_ttime - ctx.dt).max(0);
                let k = ((20000.0 + c.blood_ttime as f32) / 90000.0).min(1.5);
                c.blood_time += (ctx.dt as f32 * k) as i32;
                if c.blood_time > 600 {
                    c.blood_time = ctx.rng.r_rand(228);
                    ctx.events.push(DinoEvent::BloodTrail { pos: c.pos });
                    if ctx.rng.r_rand(128) > 96 {
                        ctx.events.push(DinoEvent::BloodTrail { pos: c.pos });
                    }
                }
            }
            let alive = c.health != 0;
            const LOOK: (f32, f32) = (100.0, 96.0);
            match c.ai {
                ai::HUNTER => dead::animate_hunter(c, i, sp, ctx, carrier),
                ai::BRACH => brachi::animate(c, i, sp, ctx),
                ai::DIMOR | ai::PTERA if alive => flyer::animate(c, i, sp, ctx),
                ai::DIMOR | ai::PTERA => flyer::animate_dead(c, i, sp, ctx),
                ai::TREX if alive => trex::animate(c, i, sp, ctx),
                ai::TREX => dead::animate(c, i, sp, ctx, trex::DIE, None, (200.0, 196.0)),
                a => {
                    if let Some(p) = HerbivoreParams::for_ai(a) {
                        if alive {
                            herbivore::animate(c, i, sp, ctx, &p);
                        } else {
                            dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                        }
                    } else if let Some(p) = CarnivoreParams::for_ai(a) {
                        if alive {
                            carnivore::animate(c, i, sp, ctx, &p);
                        } else {
                            dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                        }
                    } else if let Some(p) = SmallParams::for_ai(a) {
                        if alive {
                            small::animate(c, i, sp, ctx, &p);
                        } else {
                            dead::animate(c, i, sp, ctx, p.die, Some(p.slp), LOOK);
                        }
                    }
                    // Unknown AI numbers (mods made for patched exes) stand still.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::car::Animation;

    /// A one-vertex species with `n` 1-second, 2-frame animations.
    fn species(ai_id: i32, n: usize) -> Species {
        let car = Car {
            name_raw: [0; 32],
            mesh: Default::default(),
            animations: (0..n)
                .map(|_| Animation {
                    name_raw: [0; 32],
                    kps: 2,
                    frames: 2,
                    data: vec![0; 6],
                })
                .collect(),
            sounds: vec![],
            anim_sounds: None,
        };
        let info = DinoInfo {
            name: "test".into(),
            file: String::new(),
            ai: ai_id,
            health0: 5,
            mass: 1.0,
            length: 1.0,
            radius: 256.0,
            base_score: 1,
            smell_k: 1.0,
            hear_k: 1.0,
            look_k: 1.0,
            sh_delta: 0.0,
            scale0: 1000,
            scale_a: 0,
            danger_call: false,
        };
        Species::new(info, &car)
    }

    fn run(herd: &mut Herd, area: &Area, hunter: HunterView, ms: i32) -> Vec<DinoEvent> {
        let mut rng = MsRand::new(3);
        let mut events = Vec::new();
        for t in 0..ms / 10 {
            let mut ctx = Ctx {
                area,
                rng: &mut rng,
                dt: 10,
                real_time: t * 10,
                hunter,
                opt_agres: 128,
                tranq: false,
                view_r: 74,
                run_k: 1.0,
                events: &mut events,
            };
            herd.step(&mut ctx);
        }
        events
    }

    fn herd_with(area: &Area, ai_id: i32, n_anims: usize, pos: [f32; 3]) -> Herd {
        let mut rng = MsRand::new(1);
        let mut herd = Herd {
            species: vec![species(ai_id, n_anims)],
            chars: vec![],
            demo_index: None,
        };
        let mut c = Character {
            pos,
            tgx: pos[0],
            tgz: pos[2],
            ..Default::default()
        };
        c.reset(&herd.species[0], &mut rng, 128);
        c.pos[1] = area.land_height(pos[0], pos[2]);
        herd.chars.push(c);
        herd
    }

    fn dist(c: &Character, h: &HunterView) -> f32 {
        ((c.pos[0] - h.x).powi(2) + (c.pos[2] - h.z).powi(2)).sqrt()
    }

    #[test]
    fn parasaurolophus_flees_a_close_hunter() {
        let area = Area::flat(10);
        let hunter = HunterView {
            x: 512.0 * 256.0,
            z: 512.0 * 256.0,
            y: 640.0,
            head_y: 220.0,
            alive: true,
            ..Default::default()
        };
        let mut herd = herd_with(&area, ai::PARA, 6, [hunter.x + 800.0, 0.0, hunter.z]);
        let d0 = dist(&herd.chars[0], &hunter);
        run(&mut herd, &area, hunter, 5000);
        let c = &herd.chars[0];
        assert_eq!(c.state, 1, "alarmed");
        assert!(
            dist(c, &hunter) > d0 + 2000.0,
            "ran from {d0} to {}",
            dist(c, &hunter)
        );
        assert!(c.pos.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn hunting_trex_kills_at_close_range() {
        let area = Area::flat(10);
        let hunter = HunterView {
            x: 512.0 * 256.0,
            z: 512.0 * 256.0,
            y: 640.0,
            head_y: 220.0,
            alive: true,
            ..Default::default()
        };
        let mut herd = herd_with(&area, ai::TREX, 11, [hunter.x + 3000.0, 0.0, hunter.z]);
        herd.chars[0].state = 1; // already hunting
        let events = run(&mut herd, &area, hunter, 10_000);
        assert!(
            events.iter().any(|e| matches!(
                e,
                DinoEvent::KillHunter {
                    kind: hunt::KILL,
                    ..
                }
            )),
            "no kill in {events:?}"
        );
    }

    #[test]
    fn find_vector_alpha_matches_atan2() {
        for (x, y) in [
            (1.0, 0.0),
            (0.0, 1.0),
            (-1.0, 0.5),
            (-0.3, -2.0),
            (4.0, -1.0),
        ] {
            let want = f32::atan2(y, x).rem_euclid(2.0 * PI);
            assert!((find_vector_alpha(x, y) - want).abs() < 0.002, "{x},{y}");
        }
    }
}
