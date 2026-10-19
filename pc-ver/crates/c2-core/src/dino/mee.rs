//! What the Modder's Edition Engine adds to a species (`TDinoInfo`, read by
//! `Resources.cpp: ReadCharacterLine`) and its per-AI constants (`AIInfo`,
//! set in `LoadResourcesScript`). Reading `_RES.TXT` into these is
//! `crate::mee::info`.
//!
//! A dialect species' phases are *logical*: each `xxxAnim = n` appends model
//! animation n to `anim_index`, and the phase is its position there
//! (`AddAnim`). A key that is never given leaves its phase at 0, the first
//! animation listed, as in the original.

/// The dialect's AI numbers beyond the original's (`Hunt.h`).
pub mod mee_ai {
    pub const PIG: i32 = 6;
    pub const POACHER: i32 = 8;
    pub const HUNTDOG: i32 = 9;
    pub const PACH: i32 = 19;
    pub const BRONT: i32 = 20;
    pub const HOG: i32 = 21;
    pub const WOLF: i32 = 22;
    pub const RHINO: i32 = 23;
    pub const DEER: i32 = 24;
    pub const SMILO: i32 = 25;
    pub const MAMM: i32 = 26;
    pub const BEAR: i32 = 27;
    pub const TITAN: i32 = 28;
    pub const MICRO: i32 = 29;
    pub const BRACH: i32 = 30;
    pub const ICTH: i32 = 31;
    pub const FISH: i32 = 32;
    pub const MOSA: i32 = 33;
    pub const BRACHDANGER: i32 = 34;
    pub const LANDBRACH: i32 = 35;
    pub const WARYDIMOR: i32 = 36;
}

/// `idlegroup { ... }`: idles played as a set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IdleGroup {
    /// `startChance`, `endChance` (0..1).
    pub start: f32,
    pub end: f32,
    /// `randomStarting`, `randomEnding`, `instantRepeat`.
    pub start_on_any: bool,
    pub end_on_any: bool,
    pub instant_repeat: bool,
    /// Logical phases.
    pub anims: Vec<usize>,
}

/// `deathtype { dieAnim sleepAnim fallAnim noSleep }`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeathType {
    pub die: usize,
    pub sleep: usize,
    pub fall: usize,
    pub no_sleep: bool,
}

/// `killtype { ... }`: how this species kills the hunter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KillType {
    /// The hunter body's animation (`hunterAnim`, a `HUNT_*` model index).
    pub hunter_anim: i32,
    /// Body distance ahead of the killer, before scaling (`hunterOffset`).
    pub offset: f32,
    /// The killer's logical phase while eating (`eatAnim`).
    pub anim: usize,
    pub carry_corpse: bool,
    /// Play `anim` once, then walk off (`dontloop`).
    pub dont_loop: bool,
    pub scream: bool,
}

/// A dialect species' settings.
#[derive(Debug, Clone, PartialEq)]
pub struct MeeInfo {
    /// The dialect's AI number (`Clone`).
    pub ai: i32,
    /// Logical phase -> model animation (`animIndex`).
    pub anim_index: Vec<usize>,
    pub run: usize,
    pub walk: usize,
    pub swim: usize,
    pub jump: usize,
    pub slide: usize,
    pub fly: usize,
    pub glide: usize,
    pub takeoff: usize,
    pub land: usize,
    pub shake_land: usize,
    pub shake_water: usize,
    pub idle_groups: Vec<IdleGroup>,
    /// Idle groups on water (`waterIgroup`).
    pub idle2_groups: Vec<IdleGroup>,
    /// `lookAnim` (or `fishIdleAnim`), `smellAnim`, `roarAnim` lists.
    pub looks: Vec<usize>,
    pub smells: Vec<usize>,
    pub roars: Vec<usize>,
    pub death_types: Vec<DeathType>,
    pub kill_types: Vec<KillType>,
    /// `runspd`, `wlkspd`, `swmspd`, `jmpspd`.
    pub run_speed: f32,
    pub walk_speed: f32,
    pub swim_speed: f32,
    pub jump_speed: f32,
    /// `flyspd`, `gldspd`.
    pub fly_speed: f32,
    pub glide_speed: f32,
    /// `tkfspd`, `lndspd`.
    pub takeoff_speed: f32,
    pub land_speed: f32,
    /// Flyers: glide down from `maxalt`, flap up from `minalt` (above the
    /// ground); fish: the water depths they keep to (`mindepth`,
    /// `maxdepth`).
    pub min_depth: i32,
    pub max_depth: i32,
    /// Deaths in water (`waterDAnim`), one picked per animal.
    pub water_dies: Vec<usize>,
    /// Kills a hunter closer than this (`killdist`; 0 = never).
    pub kill_dist: i32,
    /// Attacks within roughly this many cells (`aggress`; 0 = never).
    pub aggress: i32,
    /// Only fights back once hurt (`defensive`).
    pub defensive: bool,
    /// Flees once hurt (`fearShotHit`) / on hearing a shot (`fearShotHear`).
    pub fear_shot: bool,
    pub fear_hear_shot: bool,
    /// Zig-zags while alarmed beyond this distance (`weaveRange`) unless
    /// `dontWeave`.
    pub weave_range: f32,
    pub dont_weave: bool,
    pub can_swim: bool,
    /// Water deeper than this (times scale) is swum (`waterLevel`).
    pub water_level: i32,
    /// Animation blend speed (`morphTime`, default 1: the manual's
    /// "vanilla morphing speed").
    pub morph_time: f32,
    /// Extra health for the largest specimens (`hpA`).
    pub health_a: i32,
    /// T-Rex: obstacles turn it while hunting too (`TRexObjCollide`).
    pub trex_obj_collide: bool,
    /// Ambients: speed up when the hunter is very close (`dash`).
    pub dash: bool,
    /// Brachiosaurs: path test step (`collisiondist`) and turn speed while
    /// alarmed (`runrotatespeed`).
    pub collision_dist: i32,
    pub rot_speed_multi: f32,
    /// Fish: hunts swimmers (`DangerFish`), never attacks (`dontswimaway`),
    /// leap length (`JumpRange`), distance kept from surface and bottom
    /// (`spcdepth`, grown with size).
    pub danger_fish: bool,
    pub dont_swim_away: bool,
    pub jump_range: i32,
    pub spacing_depth: i32,
    /// The menu slot whose call it answers (`callNo`, `menuDino`; 0 when
    /// not given, as in the engine).
    pub call_no: i32,
    /// Slots whose calls scare it off (`fearCall = n`, `dontFear = n`).
    pub fear_calls: Vec<i32>,
}

impl MeeInfo {
    pub fn new(ai: i32) -> Self {
        Self {
            ai,
            anim_index: Vec::new(),
            run: 0,
            walk: 0,
            swim: 0,
            jump: 0,
            slide: 0,
            fly: 0,
            glide: 0,
            takeoff: 0,
            land: 0,
            shake_land: 0,
            shake_water: 0,
            idle_groups: Vec::new(),
            idle2_groups: Vec::new(),
            looks: Vec::new(),
            smells: Vec::new(),
            roars: Vec::new(),
            death_types: Vec::new(),
            kill_types: Vec::new(),
            run_speed: 0.0,
            walk_speed: 0.0,
            swim_speed: 0.0,
            jump_speed: 0.0,
            fly_speed: 0.0,
            glide_speed: 0.0,
            takeoff_speed: 0.0,
            land_speed: 0.0,
            min_depth: 0,
            max_depth: 0,
            water_dies: Vec::new(),
            kill_dist: 0,
            aggress: 0,
            defensive: false,
            fear_shot: false,
            fear_hear_shot: false,
            weave_range: 0.0,
            dont_weave: false,
            can_swim: false,
            water_level: 0,
            morph_time: 1.0,
            health_a: 0,
            trex_obj_collide: false,
            dash: false,
            collision_dist: 0,
            rot_speed_multi: 0.0,
            danger_fish: false,
            dont_swim_away: false,
            jump_range: 0,
            spacing_depth: 0,
            call_no: 0,
            fear_calls: Vec::new(),
        }
    }

    /// `AddAnim`: a new logical phase showing model animation `model`.
    pub fn add_anim(&mut self, model: i32) -> usize {
        self.anim_index.push(model.max(0) as usize);
        self.anim_index.len() - 1
    }

    /// `deathType[i]` (a zeroed one when the species lists none).
    pub fn death(&self, i: usize) -> DeathType {
        self.death_types.get(i).copied().unwrap_or_default()
    }

    /// `killType[i]`, `None` when the species lists none.
    pub fn kill(&self, i: usize) -> Option<KillType> {
        self.kill_types.get(i).copied()
    }
}

/// `AIInfo[ai]`: steering and timing constants of the shared AIs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AiInfo {
    pub target_distance: f32,
    pub agress_multi: i32,
    pub no_way_cnt_min: i32,
    pub no_find_way_med: i32,
    pub no_find_way_range: i32,
    pub target_bend_rot_spd: f32,
    pub target_bend_min: f32,
    pub target_bend_delta1: f32,
    pub target_bend_delta2: f32,
    pub walk_target_gamma_rot: f32,
    pub target_gamma_rot: f32,
    pub y_beta_gamma: [f32; 4],
    /// Fish: start an idle when `rRand(128)` beats this.
    pub idle_start: i32,
    pub tga_increment: f32,
    pub rot1: f32,
    pub rot2: f32,
    pub pw_min: f32,
    pub jumper: bool,
    pub carnivore: bool,
    pub ice_age: bool,
    /// `CheckAfraid` considers it (sight and smell).
    pub sniffer: bool,
}

impl AiInfo {
    /// Carnivore steering (`carnivore && !iceAge` in the original).
    pub fn raptor(&self) -> bool {
        self.carnivore && !self.ice_age
    }
}

/// `AIInfo[ai]` as `LoadResourcesScript` fills it; zero for numbers it
/// leaves alone. (The original also sets unused `idleStart` values.)
pub fn ai_info(ai_id: i32) -> AiInfo {
    use super::ai;
    use mee_ai::*;
    // Herbivores: PARA, then the variants below.
    let para = AiInfo {
        target_distance: 8048.0,
        agress_multi: 128,
        no_way_cnt_min: 8,
        no_find_way_med: 44,
        no_find_way_range: 80,
        target_bend_rot_spd: 3.0,
        target_bend_min: 2.0,
        target_bend_delta1: 1600.0,
        target_bend_delta2: 1200.0,
        walk_target_gamma_rot: 12.0,
        target_gamma_rot: 8.0,
        y_beta_gamma: [128.0, 64.0, 0.6, 0.4],
        tga_increment: 3.0,
        rot1: 0.2,
        rot2: 1.0,
        pw_min: 2048.0,
        sniffer: true,
        ..Default::default()
    };
    // BRONT, HOG, BEAR, RHINO (and CHASM, minus `iceAge`).
    let ice = AiInfo {
        ice_age: true,
        agress_multi: 256,
        no_find_way_med: 48,
        target_bend_rot_spd: 3.5,
        y_beta_gamma: [128.0, 64.0, 0.6, 0.3],
        ..para
    };
    // ANKY, STEGO, DEER, MAMM.
    let heavy = AiInfo {
        no_way_cnt_min: 12,
        no_find_way_med: 32,
        no_find_way_range: 60,
        target_bend_rot_spd: 2.0,
        target_bend_min: 3.0,
        target_bend_delta1: 2000.0,
        target_bend_delta2: 2000.0,
        walk_target_gamma_rot: 16.0,
        target_gamma_rot: 10.0,
        ..para
    };
    // ALLO, VELO, SPINO, CERAT, TITAN, MICRO.
    let hunter = AiInfo {
        target_distance: 8048.0,
        no_way_cnt_min: 12,
        no_find_way_med: 16,
        no_find_way_range: 20,
        pw_min: 2048.0,
        carnivore: true,
        jumper: true,
        sniffer: true,
        ..Default::default()
    };
    let big_hunter = AiInfo {
        agress_multi: 8,
        tga_increment: 4.0,
        target_bend_rot_spd: 3.0,
        y_beta_gamma: [348.0, 324.0, 0.5, 0.4],
        walk_target_gamma_rot: 9.0,
        target_gamma_rot: 6.0,
        jumper: false,
        ..hunter
    };
    let small_hunter = AiInfo {
        agress_multi: 8,
        target_bend_rot_spd: 3.0,
        y_beta_gamma: [48.0, 24.0, 0.5, 0.4],
        walk_target_gamma_rot: 7.0,
        target_gamma_rot: 5.0,
        tga_increment: 2.0,
        ..hunter
    };
    // SMILO, WOLF.
    let cat = AiInfo {
        ice_age: true,
        carnivore: true,
        jumper: true,
        target_bend_min: 3.0,
        ..ice
    };
    match ai_id {
        ai::PARA => para,
        PACH => AiInfo {
            target_distance: 6048.0,
            no_way_cnt_min: 12,
            no_find_way_med: 32,
            no_find_way_range: 60,
            ..para
        },
        BRONT | BEAR => ice,
        HOG => AiInfo {
            rot1: 0.3,
            rot2: 1.4,
            ..ice
        },
        RHINO => AiInfo {
            rot1: 0.3,
            rot2: 1.2,
            ..ice
        },
        ai::CHASM => AiInfo {
            ice_age: false,
            agress_multi: 128,
            ..ice
        },
        DEER | MAMM => AiInfo {
            ice_age: true,
            agress_multi: 256,
            target_bend_delta2: 2000.0,
            pw_min: if ai_id == MAMM { 3048.0 } else { 2048.0 },
            ..heavy
        },
        ai::ANKY | ai::STEGO => heavy,
        SMILO => AiInfo {
            rot1: 0.3,
            rot2: 1.5,
            ..cat
        },
        WOLF => AiInfo {
            rot1: 0.4,
            rot2: 1.5,
            ..cat
        },
        ai::ALLO => AiInfo {
            agress_multi: 4,
            target_bend_rot_spd: 2.0,
            y_beta_gamma: [64.0, 32.0, 0.5, 0.4],
            walk_target_gamma_rot: 10.0,
            target_gamma_rot: 8.0,
            tga_increment: 2.0,
            ..hunter
        },
        ai::VELO => small_hunter,
        MICRO => AiInfo {
            jumper: false,
            ..small_hunter
        },
        ai::SPINO => AiInfo {
            y_beta_gamma: [98.0, 84.0, 0.4, 0.3],
            jumper: true,
            ..big_hunter
        },
        ai::CERAT | TITAN => big_hunter,
        ai::TREX | WARYDIMOR => AiInfo {
            sniffer: true,
            target_distance: if ai_id == WARYDIMOR { 4048.0 } else { 0.0 },
            tga_increment: if ai_id == WARYDIMOR { 2.0 } else { 0.0 },
            ..Default::default()
        },
        POACHER => AiInfo {
            agress_multi: 4,
            target_bend_rot_spd: 2.0,
            y_beta_gamma: [64.0, 32.0, 0.5, 0.4],
            walk_target_gamma_rot: 10.0,
            target_gamma_rot: 8.0,
            tga_increment: 2.0,
            jumper: false,
            ..hunter
        },
        HUNTDOG => AiInfo {
            target_distance: 8048.0,
            no_way_cnt_min: 8,
            no_find_way_med: 44,
            no_find_way_range: 80,
            target_bend_rot_spd: 3.0,
            target_bend_min: 2.0,
            target_bend_delta1: 1600.0,
            target_bend_delta2: 1200.0,
            walk_target_gamma_rot: 12.0,
            target_gamma_rot: 8.0,
            y_beta_gamma: [0.0, 0.0, 0.0, 0.4],
            ..Default::default()
        },
        FISH | MOSA => AiInfo {
            agress_multi: 4,
            idle_start: 126,
            jumper: ai_id == MOSA,
            ..Default::default()
        },
        ai::DIMOR | ai::PTERA => AiInfo {
            target_distance: 4048.0,
            ..Default::default()
        },
        ai::MOSH | ai::DIMET | ai::GALL | PIG => AiInfo {
            target_distance: 2048.0,
            ..Default::default()
        },
        _ => AiInfo::default(),
    }
}

/// AIs that run the shared `AnimateHuntable`.
pub fn is_huntable(ai_id: i32) -> bool {
    matches!(ai_id, 10..=17 | 19..=27)
}

/// AIs whose dialect behaviour is ported (the others run as an original
/// family for now, `crate::mee::family`).
pub fn is_ported(ai_id: i32) -> bool {
    is_huntable(ai_id)
        || is_ambient(ai_id)
        || is_flyer(ai_id)
        || is_brachi(ai_id)
        || is_fish(ai_id)
        || ai_id == mee_ai::ICTH
        || ai_id == super::ai::TREX
}

/// Fish and Mosasaurus: `AnimateFish`.
pub fn is_fish(ai_id: i32) -> bool {
    matches!(ai_id, mee_ai::FISH | mee_ai::MOSA)
}

/// Brachiosaurs: `AnimateBrahiOld` (30) and `AnimateBrahi` (34, 35).
pub fn is_brachi(ai_id: i32) -> bool {
    matches!(
        ai_id,
        mee_ai::BRACH | mee_ai::BRACHDANGER | mee_ai::LANDBRACH
    )
}

/// AIs that run `AnimateDimor` (and fall with `AnimateIcthDead`).
pub fn is_flyer(ai_id: i32) -> bool {
    matches!(ai_id, super::ai::DIMOR | super::ai::PTERA)
}

/// AIs that run `AnimateClassicAmbient`.
pub fn is_ambient(ai_id: i32) -> bool {
    use super::ai;
    matches!(ai_id, ai::MOSH | ai::GALL | ai::DIMET | mee_ai::PIG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dino::ai;

    #[test]
    fn table_spot_checks() {
        // Resources.cpp, LoadResourcesScript.
        let d = ai_info(mee_ai::DEER);
        assert_eq!((d.no_way_cnt_min, d.target_bend_min), (12, 3.0));
        assert!(d.ice_age && !d.carnivore && d.sniffer);
        let a = ai_info(ai::ALLO);
        assert!(a.raptor() && a.jumper);
        assert_eq!((a.target_bend_min, a.rot1), (0.0, 0.0));
        let w = ai_info(mee_ai::WOLF);
        assert!(w.carnivore && w.ice_age && !w.raptor());
        assert_eq!((w.rot1, w.target_bend_min), (0.4, 3.0));
        let c = ai_info(ai::CHASM);
        assert!(!c.ice_age && !c.carnivore);
        assert_eq!(c.agress_multi, 128);
        assert_eq!(ai_info(mee_ai::MAMM).pw_min, 3048.0);
        assert!(!ai_info(ai::CERAT).jumper && ai_info(ai::SPINO).jumper);
        assert!(!ai_info(mee_ai::MICRO).jumper);
    }
}
