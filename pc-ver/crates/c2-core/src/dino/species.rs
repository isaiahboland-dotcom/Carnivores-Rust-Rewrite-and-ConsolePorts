//! Species: a `_RES.TXT` character, its tuning and clips.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

use c2_formats::car::Car;
use c2_formats::res_txt::CharacterDef;

use super::{ai, mee, trex, CarnivoreParams, HerbivoreParams, SmallParams};
use crate::model::car_anim_duration_ms;

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
    /// `BaseScore`: whole points in the original (`atoi`), any number in
    /// the Modder's Edition (`atof`, set by `crate::mee::tune`).
    pub base_score: f32,
    pub smell_k: f32,
    pub hear_k: f32,
    pub look_k: f32,
    pub sh_delta: f32,
    pub scale0: i32,
    pub scale_a: i32,
    pub danger_call: bool,
    /// The original AI an extra AI number runs as, when the data says
    /// (`runsAs`: our key, in games we build from an APK); else inferred
    /// ([`stock_ai`]).
    pub runs_as: Option<i32>,
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
            base_score: d.basescore.unwrap_or(0) as f32,
            smell_k: d.smell.unwrap_or(0.0),
            hear_k: d.hear.unwrap_or(0.0),
            look_k: d.look.unwrap_or(0.0),
            sh_delta: d.shipdelta.unwrap_or(0.0),
            scale0: d.scale0.unwrap_or(800),
            scale_a: d.scale_a.unwrap_or(600),
            danger_call: d.danger,
            runs_as: d
                .extra
                .iter()
                .find(|e| e.key.eq_ignore_ascii_case("runsAs"))
                .and_then(|e| e.value.trim().parse().ok()),
        }
    }
}

/// Per-species settings the Modder's Edition dialect allows over an AI
/// family's built-in ones (`runAnim`, `runspd`, `deathtype{ dieAnim }`, ...).
/// `None` keeps the family's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tuning {
    /// A combined pack's selection slot, independent of source AI/calls.
    /// Negative means a roamer that has not been offered a slot yet.
    pub menu_slot: Option<i32>,
    pub run: Option<usize>,
    pub walk: Option<usize>,
    pub swim: Option<usize>,
    pub jump: Option<usize>,
    pub slide: Option<usize>,
    pub die: Option<usize>,
    pub sleep: Option<usize>,
    /// Idle animations (the first `idlegroup`'s).
    pub idles: Vec<usize>,
    pub run_speed: Option<f32>,
    pub walk_speed: Option<f32>,
    pub swim_speed: Option<f32>,
    pub jump_speed: Option<f32>,
    /// Shown on the radar / counted as a target (`radar = TRUE`); `None`:
    /// the hunt's chosen targets decide, as in the original.
    pub radar: Option<bool>,
    /// The dialect's own AI number (`info.ai` holds the family it runs as).
    pub mee_ai: Option<i32>,
    /// Fish / mosasaur depth limits (`mindepth`, `maxdepth`).
    pub depth: Option<(i32, i32)>,
}

impl Tuning {
    pub fn herbivore(&self, mut p: HerbivoreParams) -> HerbivoreParams {
        p.run = self.run.unwrap_or(p.run);
        p.walk = self.walk.unwrap_or(p.walk);
        p.die = self.die.unwrap_or(p.die);
        p.slp = self.sleep.unwrap_or(p.slp);
        if let Some(&first) = self.idles.iter().min() {
            p.idle1 = first;
            p.n_idles = self.idles.len();
        }
        p.run_speed = self.run_speed.unwrap_or(p.run_speed);
        p.walk_speed = self.walk_speed.unwrap_or(p.walk_speed);
        p
    }

    pub fn carnivore(&self, mut p: CarnivoreParams) -> CarnivoreParams {
        p.run = self.run.unwrap_or(p.run);
        p.walk = self.walk.unwrap_or(p.walk);
        p.swim = self.swim.unwrap_or(p.swim);
        p.jump = self.jump.unwrap_or(p.jump);
        p.slide = self.slide.unwrap_or(p.slide);
        p.die = self.die.unwrap_or(p.die);
        p.slp = self.sleep.unwrap_or(p.slp);
        if !self.idles.is_empty() {
            p.n_idles = self.idles.len().min(p.idles.len());
            for (d, s) in p.idles.iter_mut().zip(&self.idles) {
                *d = *s;
            }
        }
        p.run_speed = self.run_speed.unwrap_or(p.run_speed);
        p.walk_speed = self.walk_speed.unwrap_or(p.walk_speed);
        p.swim_speed = self.swim_speed.unwrap_or(p.swim_speed);
        p.jump_speed = self.jump_speed.unwrap_or(p.jump_speed);
        p
    }

    pub fn small(&self, mut p: SmallParams) -> SmallParams {
        p.run = self.run.unwrap_or(p.run);
        p.walk = self.walk.unwrap_or(p.walk);
        p.die = self.die.unwrap_or(p.die);
        p.slp = self.sleep.unwrap_or(p.slp);
        if let [a, rest @ ..] = self.idles.as_slice() {
            p.idle1 = *a;
            p.idle2 = rest.first().copied().unwrap_or(*a);
        }
        p.run_speed = self.run_speed.unwrap_or(p.run_speed);
        p.walk_speed = self.walk_speed.unwrap_or(p.walk_speed);
        p
    }
}

/// What the AI needs from a `.CAR`: per-animation length and sound map.
#[derive(Debug, Clone)]
pub struct Species {
    pub variant: Option<super::variants::Variant>,
    pub info: DinoInfo,
    /// Resolved hunt-menu price; None when no menu entry or donor exists.
    pub hunt_price: Option<i32>,
    /// Dialect overrides (default: none).
    pub tuning: Tuning,
    /// A dialect species running the dialect's own AI; its phases are
    /// logical (see [`mee`]).
    pub mee: Option<Box<mee::MeeInfo>>,
    /// `AniTime` per animation, ms (at least 1 so `%` is safe).
    pub ani_time: Vec<i32>,
    /// Frames per animation.
    pub frames: Vec<usize>,
    /// `Anifx`: sound index per animation, -1 = none.
    pub anifx: Vec<i32>,
    /// The original AI an AI number the engine lacks runs as ([`stock_ai`]);
    /// `info.ai` stays its identity (menu slot, call, radar).
    pub stock_ai: Option<i32>,
    /// The model's reach from its origin, in world units (vertices `* 2`):
    /// how far the death camera stands back from a big killer.
    pub size: f32,
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
        let stock_ai = stock_ai(&info, car);
        let size = car
            .mesh
            .vertices
            .iter()
            .map(|v| 2.0 * (v.x * v.x + v.y * v.y + v.z * v.z).sqrt())
            .fold(0.0, f32::max);
        Self {
            info,
            variant: None,
            hunt_price: None,
            tuning: Tuning::default(),
            mee: None,
            ani_time,
            frames,
            anifx,
            stock_ai,
            size,
        }
    }

    /// The AI its characters run (`TCharacter::AI`).
    pub fn runs_as(&self) -> i32 {
        self.stock_ai.unwrap_or(self.info.ai)
    }

    /// Logical phase -> model animation (`animIndex`; the identity for
    /// the original's AIs).
    pub fn anims(&self) -> &[usize] {
        self.mee.as_ref().map_or(&[], |m| &m.anim_index)
    }

    /// `AniTime` of `phase`, tolerating mods with fewer animations.
    pub fn time(&self, phase: usize) -> i32 {
        self.ani_time
            .get(anim(self.anims(), phase))
            .copied()
            .unwrap_or(1)
    }
}

/// For an AI number the original engine lacks (a patched exe's extra slot:
/// Carnivores Triassic's Gojirasaurus is AI 19), the original huntable AI
/// whose clip layout its model has, as that exe's AI must have read it: as
/// many clips or more, the death clip (named so) at that AI's death index;
/// a hunter's first if `danger`, else a grazer's, an exact clip count
/// first. The Gojirasaurus has the T-Rex's ten clips. Without one it stands
/// still, as before. An original AI named by `runsAs` is taken as it is.
pub fn stock_ai(info: &DinoInfo, car: &Car) -> Option<i32> {
    if matches!(info.ai, 0..=6 | 10..=18) {
        return None;
    }
    if let Some(a) = info.runs_as.filter(|a| matches!(a, 1..=6 | 10..=18)) {
        return Some(a);
    }
    // (AI, clips it uses, death clip).
    let layout = |a: i32| -> Option<(usize, usize)> {
        if a == ai::TREX {
            return Some((10, trex::DIE));
        }
        if let Some(p) = CarnivoreParams::for_ai(a) {
            let used = [p.run, p.walk, p.swim, p.slide, p.jump, p.die, p.eat, p.slp];
            let idles = p.idles[..p.n_idles].iter();
            let n = used
                .iter()
                .chain(idles)
                .filter(|&&k| k != usize::MAX)
                .max()?;
            return Some((n + 1, p.die));
        }
        let p = HerbivoreParams::for_ai(a)?;
        let n = [
            p.walk,
            p.run,
            p.idle1 + p.n_idles.saturating_sub(1),
            p.die,
            p.slp,
        ];
        Some((n.iter().max()? + 1, p.die))
    };
    let hunters = [ai::TREX, ai::ALLO, ai::VELO, ai::SPINO, ai::CERAT];
    let grazers = [ai::PARA, ai::ANKY, ai::STEGO, ai::CHASM];
    let order: Vec<i32> = if info.danger_call {
        hunters.iter().chain(&grazers).copied().collect()
    } else {
        grazers.iter().chain(&hunters).copied().collect()
    };
    let clips = car.animations.len();
    let dies = |k: usize| {
        car.animations.get(k).is_some_and(|a| {
            let n = a.name().to_ascii_lowercase();
            ["die", "death", "dead"].iter().any(|w| n.contains(w))
        })
    };
    let fits = |exact: bool| {
        order.iter().copied().find(|&a| {
            layout(a)
                .is_some_and(|(n, die)| (if exact { clips == n } else { clips >= n }) && dies(die))
        })
    };
    fits(true).or_else(|| fits(false))
}

/// The model animation of `phase` under an `animIndex` table (empty: the
/// identity).
pub fn anim(anims: &[usize], phase: usize) -> usize {
    if anims.is_empty() {
        phase
    } else {
        anims.get(phase).copied().unwrap_or(0)
    }
}
