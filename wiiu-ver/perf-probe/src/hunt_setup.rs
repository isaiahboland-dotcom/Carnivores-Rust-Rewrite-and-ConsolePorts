//! Base C2 roster used by the Wii U loader and its host integration test.
use alloc::vec::Vec;
use c2_core::{
    area::Area,
    dino::{self, Character, DinoInfo, Herd, Species},
    game::{Hunt, HuntOptions, SurvivalKind, SurvivalOptions, WeaponInfo, Weapons},
    player::Player,
    rng::MsRand,
};
use c2_formats::{
    bits::Bits,
    car::{self, Car},
    res_txt::ResTxt,
    sav::Save,
};

pub const SPECIES_FILES: [&str; 15] = [
    "MOSH.CAR",
    "GALL.CAR",
    "DIMOR2.CAR",
    "PTERA.CAR",
    "DIMET.CAR",
    "BRACHI1.CAR",
    "PAR2.CAR",
    "ANKYLO1.CAR",
    "STEGO.CAR",
    "ALLO.CAR",
    "CHASMOSA.CAR",
    "VELO2.CAR",
    "SPINO.CAR",
    "CERATO1.CAR",
    "TIREX.CAR",
];
pub const AMBIENT_COUNT: usize = 6;
pub const AMBIENT_POPULATION: usize = 18;
pub const TARGET_POPULATION: usize = 48;
pub const POPULATION: usize = AMBIENT_POPULATION + TARGET_POPULATION;
pub const TARGET_AI: [i32; 9] = [10, 11, 12, 13, 14, 15, 16, 17, 18];
pub const TARGET_MASK: u32 = (1 << TARGET_AI.len()) - 1;
pub const WEAPON_FILES: [&str; 6] = [
    "PISTOL.CAR",
    "SHOTGUN.CAR",
    "DBSGUN.CAR",
    "X_BOW.CAR",
    "RIFLE.CAR",
    "SNIPER.CAR",
];
pub const WEAPON_MODEL: usize = SPECIES_FILES.len();
/// The view model's own depth range (camera units): it is drawn into the
/// front of the depth buffer, so its parts hide each other but the world never
/// hides it (main.c, as the 3DS does).
pub const GUN_NEAR: f32 = 1.0;
pub const GUN_FAR: f32 = 4096.0;
/// Clip-space z for a view-model vertex `depth` units ahead (w = depth):
/// z / w runs 0..1 from GUN_NEAR to GUN_FAR.
pub fn gun_clip_z(depth: f32) -> f32 {
    GUN_FAR / (GUN_FAR - GUN_NEAR) * (depth - GUN_NEAR)
}
pub const WEAPON_COUNT: usize = WEAPON_FILES.len();
pub const HUNTER_SPECIES: usize = SPECIES_FILES.len();
pub const HUNTER_MODEL: usize = WEAPON_MODEL + WEAPON_COUNT;
pub const MODEL_COUNT: usize = HUNTER_MODEL + 1;
pub fn model_for_species(id: usize) -> usize {
    if id == HUNTER_SPECIES {
        HUNTER_MODEL
    } else {
        id
    }
}

/// Everything the hunt menu picks, shared with C (`HuntChoice` in menu.h).
/// Flags are 0 or 1.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HuntChoice {
    /// Area 0..5 (`AREAn`, n = area + 1).
    pub area: u32,
    /// 0 dawn, 1 day, 2 night.
    pub time: u32,
    /// Bit n: huntable slot n (AI 10 + n).
    pub animals: u32,
    /// Bit n: `WEAPON_FILES[n]`.
    pub weapons: u32,
    pub radar: u32,
    pub camo: u32,
    pub scent: u32,
    pub double_ammo: u32,
    pub tranq: u32,
    pub observer: u32,
    /// Animals' running speed, 50..=100 (%).
    pub run_pct: u32,
    pub blood: u32,
    pub death_anim: u32,
    /// The small animals that score nothing.
    pub ambient: u32,
    /// Descriptions in US units (`DINOn.TXU`) instead of metric (`.TXM`).
    pub units_us: u32,
    /// `MODE_HUNT`, `MODE_SURVIVAL` or `MODE_TROPHY` (the trophy room).
    pub mode: u32,
    /// A survival's kind: 0 normal, 1 hardcore, 2 custom (`SurvivalKind::ALL`).
    pub survival: u32,
}
pub const MODE_HUNT: u32 = 0;
pub const MODE_SURVIVAL: u32 = 1;
pub const MODE_TROPHY: u32 = 2;
/// The most animals in a survival wave.
pub const SURVIVAL_MAX: usize = 8;
/// The trophy room's door (`TROPHY.MAP`), as the PC and 3DS.
pub const TROPHY_DOOR: (u16, u16) = (76, 70);
impl Default for HuntChoice {
    fn default() -> Self {
        Self {
            area: 0,
            time: 1,
            animals: 1,
            weapons: 1,
            radar: 1,
            camo: 0,
            scent: 0,
            double_ammo: 0,
            tranq: 0,
            observer: 0,
            run_pct: 100,
            blood: 1,
            death_anim: 1,
            ambient: 1,
            units_us: 0,
            mode: MODE_HUNT,
            survival: 0,
        }
    }
}
impl HuntChoice {
    /// The menu's times in order: dawn, day, night.
    pub fn time_of_day(&self) -> c2_core::area::TimeOfDay {
        use c2_core::area::TimeOfDay;
        match self.time {
            0 => TimeOfDay::Dusk,
            2 => TimeOfDay::Night,
            _ => TimeOfDay::Day,
        }
    }
    pub fn trophy(&self) -> bool {
        self.mode == MODE_TROPHY
    }
    pub fn survival_kind(&self) -> Option<SurvivalKind> {
        (self.mode == MODE_SURVIVAL).then(|| SurvivalKind::ALL[(self.survival as usize).min(2)])
    }
}

/// The original numbers characters in `_RES.TXT` order, the hunter first;
/// this port has the hunter last (`HUNTER_SPECIES`). Saves keep the
/// original's numbers, so they work on the PC and the 3DS too.
pub fn room_from_save(save: &Save) -> Save {
    let mut room = save.clone();
    for b in room.bodies.iter_mut().filter(|b| b.ctype > 0) {
        b.ctype -= 1;
    }
    room
}
pub fn room_to_save(room: &Save) -> Save {
    let mut save = room.clone();
    for b in save.bodies.iter_mut().filter(|b| b.ctype > 0) {
        b.ctype += 1;
    }
    save
}

/// A new hunter's save: no trophies yet.
pub fn new_save() -> Save {
    c2_core::profile::new_player("Hunter", 0, 0)
}

#[allow(dead_code)] // the host tests' shorthand
pub fn setup(
    area: &Area,
    definitions: &str,
    assets: &[&[u8]],
    selected: u32,
    radar: bool,
    weapons: u32,
    seed: u32,
) -> Result<(Hunt, Vec<Car>), i32> {
    let choice = HuntChoice { animals: selected, radar: radar as u32, weapons, ..Default::default() };
    setup_with(area, definitions, assets, &choice, &new_save(), seed)
}

pub fn setup_with(
    area: &Area,
    definitions: &str,
    assets: &[&[u8]],
    choice: &HuntChoice,
    room: &Save,
    seed: u32,
) -> Result<(Hunt, Vec<Car>), i32> {
    // The trophy room: no animals to hunt, no weapons. A survival: no
    // observing.
    let trophy = choice.trophy();
    let observer = choice.observer != 0 && choice.mode == MODE_HUNT;
    let (selected, weapons) = (choice.animals, if observer || trophy { 0 } else { choice.weapons });
    if assets.len() != MODEL_COUNT
        || (!trophy && (selected == 0 || selected & !TARGET_MASK != 0))
        || (weapons == 0 && !observer && !trophy)
        || weapons & !((1 << WEAPON_COUNT) - 1) != 0
    {
        return Err(-10);
    }
    let res = ResTxt::parse(definitions).map_err(|_| -2)?.to_data();
    let cars: Vec<Car> = assets
        .iter()
        .map(|bytes| car::parse(bytes).map_err(|_| -6))
        .collect::<Result<_, _>>()?;
    let mut species = Vec::new();
    for (i, file) in SPECIES_FILES.iter().enumerate() {
        let def = res
            .characters
            .iter()
            .find(|d| {
                d.file
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(file))
            })
            .ok_or(-4)?;
        species.push(Species::new(DinoInfo::from_def(def), &cars[i]));
    }
    let hunter_def = res
        .characters
        .iter()
        .find(|d| {
            d.file
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("HUNTER1.CAR"))
        })
        .ok_or(-4)?;
    species.push(Species::new(
        DinoInfo::from_def(hunter_def),
        &cars[HUNTER_MODEL],
    ));
    let mut infos = Vec::new();
    let mut anims = Vec::new();
    let mut present = Bits::NONE;
    for (i, file) in WEAPON_FILES.iter().enumerate() {
        if weapons & (1 << i) == 0 {
            continue;
        }
        let def = res
            .weapons
            .iter()
            .find(|d| {
                d.file
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(file))
            })
            .ok_or(-3)?;
        let info = WeaponInfo::from_def(def);
        anims.push(core::array::from_fn(|slot| {
            cars[WEAPON_MODEL + i]
                .animations
                .get(info.anim_of(slot))
                .map_or(0, |a| c2_core::model::car_anim_duration_ms(a) as i32)
        }));
        infos.push(info);
        present = present | Bits::bit(infos.len() - 1);
    }
    let hunt = new_hunt(area, species, infos, anims, present, choice, room, seed)?;
    Ok((hunt, cars))
}

/// A new hunt like `previous` (same animals, weapons and options).
#[allow(dead_code)] // the host tests' shorthand
pub fn restart(area: &Area, previous: &Hunt, seed: u32) -> Result<Hunt, i32> {
    let animals = TARGET_AI.iter().enumerate().fold(0, |mask, (i, ai)| {
        mask | if previous.opts.targets.has(*ai as usize) {
            1 << i
        } else {
            0
        }
    });
    let ambient = previous.herd.chars.iter().any(|c| c.ctype < AMBIENT_COUNT) as u32;
    let choice = HuntChoice { animals, ambient, ..Default::default() };
    restart_with(area, previous, &choice, &previous.room, seed)
}

/// A new hunt with `previous`'s loaded animals and weapons, its options
/// (camouflage, blood...), `choice`'s animals, ambient population, double
/// ammo and mode, and the trophies of `room` (the saved ones: a hunt that
/// ended in death keeps none of its own).
pub fn restart_with(area: &Area, previous: &Hunt, choice: &HuntChoice, room: &Save, seed: u32) -> Result<Hunt, i32> {
    let weapons = previous.weapons.info.clone();
    let anims = previous.weapons.anim_ms.clone();
    let present = (0..weapons.len()).fold(Bits::NONE, |bits, i| bits | Bits::bit(i));
    let mut hunt = new_hunt(area, previous.herd.species.clone(), weapons, anims, present, choice, room, seed)?;
    let targets = hunt.opts.targets;
    hunt.opts = previous.opts;
    hunt.opts.targets = targets;
    Ok(hunt)
}

fn new_hunt(
    area: &Area,
    species: Vec<Species>,
    info: Vec<WeaponInfo>,
    anim_ms: Vec<[i32; 4]>,
    present: Bits,
    choice: &HuntChoice,
    room: &Save,
    seed: u32,
) -> Result<Hunt, i32> {
    if choice.trophy() {
        return Ok(trophy_room(area, species, info, anim_ms, choice, room, seed));
    }
    let selected = choice.animals;
    let player = Player::new(area, *area.landings.first().ok_or(-11)?);
    let mut herd = Herd {
        species,
        ..Default::default()
    };
    let mut rng = MsRand::new(seed);
    let mut targets = Bits::NONE;
    let selected_types: Vec<usize> = TARGET_AI
        .iter()
        .enumerate()
        .filter_map(|(i, ai)| {
            if selected & (1 << i) == 0 {
                return None;
            }
            targets = targets | Bits::bit(*ai as usize);
            Some(AMBIENT_COUNT + i)
        })
        .collect();
    if selected_types.is_empty() {
        return Err(-10);
    }
    let first_target = rng.r_rand(selected_types.len() as i32 - 1) as usize;
    // Bounded rejection sampling across the walkable map, using the shared
    // slope/water/object collision rules. No target ring around the hunter.
    let first_slot = if choice.ambient != 0 { 0 } else { AMBIENT_POPULATION };
    for slot in first_slot..POPULATION {
        let id = if slot < AMBIENT_POPULATION {
            slot % AMBIENT_COUNT
        } else {
            selected_types[(slot - AMBIENT_POPULATION + first_target) % selected_types.len()]
        };
        let clearance = if id < AMBIENT_COUNT { 2048.0 } else { 10240.0 };
        let mut found = None;
        for _ in 0..16384 {
            let x = (508 * 256 + rng.si_rand(472 * 256)) as f32;
            let z = (508 * 256 + rng.si_rand(472 * 256)) as f32;
            let dx = x - player.x;
            let dz = z - player.z;
            if dx * dx + dz * dz < clearance * clearance {
                continue;
            }
            let mut pos = [x, 0.0, z];
            if dino::check_place_collision_p(area, &mut pos) {
                continue;
            }
            if herd.chars.iter().any(|c| {
                let dx = c.pos[0] - x;
                let dz = c.pos[2] - z;
                dx * dx + dz * dz < 2048.0 * 2048.0
            }) {
                continue;
            }
            found = Some(pos);
            break;
        }
        let pos = found.ok_or(-12)?;
        let alpha = rng.rand() as f32 / 32768.0 * core::f32::consts::TAU;
        let mut ch = Character {
            ctype: id,
            pos,
            alpha,
            tgx: pos[0],
            tgz: pos[2],
            ..Default::default()
        };
        ch.reset(&herd.species[id], &mut rng, 128);
        if matches!(ch.ai, 3 | 4) {
            ch.pos[1] += 2048.0;
        }
        herd.chars.push(ch);
    }
    let weapons = Weapons::new(info, anim_ms, present, choice.double_ammo != 0);
    let observer = choice.observer != 0 && choice.mode == MODE_HUNT;
    let mut hunt = Hunt::new(
        area,
        player,
        herd,
        weapons,
        rng,
        HuntOptions {
            targets,
            radar: choice.radar != 0,
            camo: choice.camo != 0,
            scent: choice.scent != 0,
            tranq: choice.tranq != 0 && choice.mode == MODE_HUNT,
            observer,
            dino_run_k: choice.run_pct.clamp(50, 100) as f32 / 100.0,
            blood: choice.blood != 0,
            death_anim: choice.death_anim != 0,
            distant_ai: true,
            ..Default::default()
        },
        room.clone(),
    );
    hunt.area_no = choice.area as i32 + 1;
    if let Some(kind) = choice.survival_kind() {
        let opts = SurvivalOptions::pick(&hunt, kind, SURVIVAL_MAX);
        hunt.start_survival(area, opts);
    }
    Ok(hunt)
}

/// `TROPHY.MAP`: the hunter at the door, the saved trophies on their stands
/// (every species is loaded), no weapons, nothing alive.
fn trophy_room(
    area: &Area,
    species: Vec<Species>,
    info: Vec<WeaponInfo>,
    anim_ms: Vec<[i32; 4]>,
    choice: &HuntChoice,
    room: &Save,
    seed: u32,
) -> Hunt {
    let mut rng = MsRand::new(seed);
    let player = Player::new(area, TROPHY_DOOR);
    let mut herd = Herd {
        species,
        ..Default::default()
    };
    herd.place_trophies(area, room, &mut rng);
    let weapons = Weapons::new(info, anim_ms, Bits::NONE, false);
    Hunt::new(
        area,
        player,
        herd,
        weapons,
        rng,
        HuntOptions {
            trophy: true,
            view_r: 60,
            blood: choice.blood != 0,
            ..Default::default()
        },
        room.clone(),
    )
}

/// Bit 0 disables blood; bit 1 skips death playback. Zero keeps original behavior.
#[allow(dead_code)] // the host tests' shorthand
pub fn configure(hunt: &mut Hunt, disabled: u32) {
    hunt.opts.blood = disabled & 1 == 0;
    hunt.opts.death_anim = disabled & 2 == 0;
}

pub fn death_menu_ready(hunt: &Hunt, elapsed_ms: u32) -> bool {
    hunt.health <= 0 && (!hunt.opts.death_anim || elapsed_ms >= 12000)
}

/// Conservative bound over every animation frame, before character scaling.
pub fn animation_radius(car: &Car) -> f32 {
    let squared = car
        .animations
        .iter()
        .flat_map(|a| a.data.chunks_exact(3))
        .map(|v| {
            let x = v[0] as f32 / 8.0;
            let y = v[1] as f32 / 8.0;
            let z = v[2] as f32 / 8.0;
            x * x + y * y + z * z
        })
        .fold(0.0, f32::max);
    libm::sqrtf(squared)
}

/// Matches the world's horizontal frustum and fog distance; accepts a sphere
/// spanning any edge so a dinosaur's limbs cannot disappear at the boundary.
pub fn actor_visible(pos: [f32; 3], radius: f32, eye: [f32; 3], yaw: f32, pitch: f32) -> bool {
    let x = pos[0] - eye[0];
    let y = pos[1] - eye[1];
    let z = pos[2] - eye[2];
    if x * x + y * y + z * z > (18944.0 + radius) * (18944.0 + radius) {
        return false;
    }
    let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
    let side = x * ca + z * sa;
    let depth = (x * sa - z * ca) * libm::cosf(pitch) - y * libm::sinf(pitch);
    depth + radius >= 0.0 && (side * 0.75).abs() <= depth + radius * 1.25
}
