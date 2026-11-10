//! Characters (dinosaurs) and their AI: `Characters.cpp`.
//!
//! Each AI is ported from its `Animate*` function. The shared types live
//! here (fields mirror `TCharacter`); species, angle helpers, place
//! collision, shared AI steps and the herd step are in their own files, the
//! per-family logic in the rest.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

mod angles;
mod brachi;
mod carnivore;
mod character;
mod collide;
mod dead;
mod flyer;
mod herbivore;
mod herd;
mod huntable;
pub mod mee;
mod mee_ambient;
mod mee_brachi;
mod mee_fish;
mod mee_flyer;
mod mee_icth;
mod mee_trex;
mod place;
pub mod pose;
pub mod predation;
mod small;
mod species;
#[cfg(test)]
mod tests;
mod trex;
pub mod variants;

use crate::area::Area;
use crate::rng::MsRand;

pub use angles::{
    angle_difference, corrected_alpha, delta_func, find_vector_alpha, sgn, wrap_angle,
};
pub use carnivore::CarnivoreParams;
pub use collide::{
    check_place_collision, check_place_collision2, check_place_collision_aquatic,
    check_place_collision_p,
};
pub(crate) use collide::{
    check_place_collision2_avoid, check_place_collision_avoid, check_place_collision_brahi,
    check_place_collision_brahi_p, check_place_collision_fish, check_place_collision_fish_p,
    check_place_collision_land_brahi, in_region,
};
pub use herbivore::HerbivoreParams;
pub use herd::{Herd, Pack};
pub use place::PlaceOptions;
pub use small::SmallParams;
pub use species::{anim, stock_ai, DinoInfo, Species, Tuning};

/// `PMORPHTIME`: ms over which a previous animation blends out.
pub const PMORPH_TIME: i32 = 256;
/// `Hunt.h: pi` (3.1415926535f, i.e. f32 pi).
pub const PI: f32 = core::f32::consts::PI;

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

/// `TCharacter`.
#[derive(Debug, Clone, Default)]
pub struct Character {
    /// Unprocessed simulation time for optional distant-AI scheduling.
    pub pending_ai_ms: i32,
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
    // The dialect's additions (`TCharacter`).
    /// Has seen, smelt or been hit by the hunter (`awareHunter`).
    pub aware_hunter: bool,
    /// Heard a shot (`heardShot`).
    pub heard_shot: bool,
    /// Has been hit (`damaged`).
    pub damaged: bool,
    /// The last round that hit it was a tranquilizer dart (multi ammo; not
    /// in the original, where the hunt's tranquilizer option decides).
    pub darted: bool,
    /// Hit by a tracker (a dialect weapon's `radar`): shown on the map in
    /// this colour, for the ms left (`None`: for good) (`tracker`, `RTime`).
    pub tracked: Option<([u8; 3], Option<i32>)>,
    /// Index into `Herd::packs` (`packId`).
    pub pack: Option<usize>,
    pub follow_leader: bool,
    pub pack_density: f32,
    /// Index into `Herd::groups` (`SpawnGroupType`).
    pub spawn_group: Option<usize>,
    /// Chosen kill type and death type of the species.
    /// The survival wave it came in (0: not a wave animal).
    pub wave: u32,
    /// Survival: where its AI is told the hunter is while it is far off (a
    /// point on a walkable way to him; ours).
    pub lure: Option<(f32, f32)>,
    /// Survival with a fence: its last place on the fenced ground, where it
    /// is put back when it strays onto a wall.
    pub fenced_at: Option<(f32, f32)>,
    pub kill_type: usize,
    pub death_type: usize,
    /// The idle group being played (`currentIdleGroup`).
    pub idle_group: Option<usize>,
    /// Phase of the zig-zag while alarmed (`tgalphaOffset`).
    pub tgalpha_offset: f32,
    /// The wary flyer has called its alarm since it was alarmed (`_State`).
    pub alarm_called: bool,
    /// The roar this animal uses (`roarAnim`, picked at reset).
    pub roar_anim: usize,
    /// Its death in water (`waterDieAnim`, picked at reset).
    pub water_die_anim: usize,
    /// Falling flyers: the phase it was shot in, and whether it may fall
    /// asleep (tranquilizer) when down.
    pub death_phase: usize,
    pub can_sleep: bool,
    /// Fish: swimming height and the one aimed for, the pitch easing
    /// (`turny`, `lastTBeta`), idling, and the gap kept from surface and
    /// bottom (`spcDepth`).
    pub depth: f32,
    pub tdepth: f32,
    pub turny: f32,
    pub last_tbeta: f32,
    pub aquatic_idle: bool,
    pub spc_depth: f32,
    /// Ichthyornis: water idle group, gliding down, a take-off without a
    /// call (`notFlushed`), walk cycles since the last shake, and the water
    /// height where it was placed (`spawnAlt`).
    pub idle2_group: Option<usize>,
    pub gliding: bool,
    pub not_flushed: bool,
    pub shake_time: i32,
    pub spawn_alt: f32,
    /// Predation (a setting, [`predation`]): its goal, and as a body,
    /// whether an animal killed or ate from it.
    pub predation: predation::Track,
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

/// What an AI's chase-or-flee code is aimed at for one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetKind {
    /// The player ([`Ctx::hunter`]).
    #[default]
    Hunter,
    /// Survival: a point on a walkable way to the player. Counts as the
    /// player for pack alarms, but reaching it is no kill.
    Waypoint,
    /// Character `n`, which it attacks (predation: a hunt or a charge).
    Animal(usize),
    /// A place to reach or run from (predation: a body to scavenge, or a
    /// point beyond what it flees): nothing to strike, no pack alarm.
    Place,
}

/// The target of one AI step: where it is, what it is, and what the animal
/// knows of it. [`Herd::step`] sets it per character; other callers leave
/// the default, which it overwrites.
#[derive(Debug, Clone, Copy, Default)]
pub struct Target {
    pub view: HunterView,
    pub kind: TargetKind,
    /// The animal is aware of it (`awareHunter` for the player).
    pub aware: bool,
    /// The animal heard a shot from it (`heardShot`; only the player shoots).
    pub heard_shot: bool,
}

impl Target {
    /// The player, as `c` knows him.
    pub fn hunter(view: HunterView, c: &Character) -> Self {
        Self {
            view,
            kind: TargetKind::Hunter,
            aware: c.aware_hunter,
            heard_shot: c.heard_shot,
        }
    }

    /// Whether reaching it is a strike (the player or an attacked animal).
    pub fn strikes(&self) -> bool {
        matches!(self.kind, TargetKind::Hunter | TargetKind::Animal(_))
    }

    /// Whether it is the player.
    pub fn is_hunter(&self) -> bool {
        self.kind == TargetKind::Hunter
    }

    /// Whether the pack may hear of it: alarms and attacks are about the
    /// player (or the way to him), never another animal.
    pub fn alarms_pack(&self) -> bool {
        matches!(self.kind, TargetKind::Hunter | TargetKind::Waypoint)
    }
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
    /// `AddDeadBody(cptr, kind)`: the hunter is killed by character `index`
    /// (and screams, unless a dialect kill type says not to).
    KillHunter {
        index: usize,
        kind: i32,
        scream: bool,
    },
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
    /// The player. Sounds are heard and animals re-placed relative to him.
    pub hunter: HunterView,
    /// What the stepping AI chases or flees ([`Herd::step`] sets it).
    pub target: Target,
    /// Set by [`Ctx::strike`] when the AI struck an animal target; read
    /// and cleared by the herd after each character's step.
    pub struck: Option<usize>,
    /// `OptAgres` 0..=255 (128 default).
    pub opt_agres: i32,
    /// Tranquilizer mode: dying animals fall asleep instead.
    pub tranq: bool,
    /// `ctViewR`: view distance in cells (carnivores use it to give up).
    pub view_r: i32,
    /// Cells the player can see (`view_r`, or more with a longer PC render
    /// distance): animals out of sight are re-placed just beyond it.
    pub render_r: i32,
    /// Multiplies every species' running speed (1 in the original; a 3DS
    /// setting lowers it).
    pub run_k: f32,
    pub events: &'a mut Vec<DinoEvent>,
}

impl Ctx<'_> {
    /// Character `index` strikes its target: kills the player (an event for
    /// the game) or, for an animal target, records the strike for the herd.
    /// A survival waypoint or a place is never struck.
    pub fn strike(&mut self, index: usize, kind: i32, scream: bool) {
        match self.target.kind {
            TargetKind::Hunter if self.target.view.alive => {
                self.events.push(DinoEvent::KillHunter {
                    index,
                    kind,
                    scream,
                });
            }
            TargetKind::Animal(n) => self.struck = Some(n),
            _ => {}
        }
    }
}
