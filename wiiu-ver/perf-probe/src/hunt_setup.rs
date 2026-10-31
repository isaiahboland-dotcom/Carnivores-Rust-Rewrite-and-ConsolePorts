//! A hunt from the install's game (`install.rs`): its area, the animals
//! and weapons picked, placed as this port places them (the original
//! game's data) or by the Modder's Edition's spawn tables. Characters keep
//! their `_RES.TXT` numbers (the hunter is 0 in the original's), as the
//! saves do, so saves work on the PC and the 3DS too.
use alloc::{format, string::String, vec::Vec};
use c2_core::{
    area::Area,
    dino::{self, Character, Herd, Species},
    game::{Hunt, HuntOptions, SurvivalKind, SurvivalOptions, WeaponInfo, Weapons},
    load,
    player::Player,
    rng::MsRand,
};
use c2_formats::{bits::Bits, car::Car, mee::Tables, sav::Save, tga::Picture};

use crate::install::{Install, Role, Roam};

pub const AMBIENT_POPULATION: usize = 18;
pub const TARGET_POPULATION: usize = 48;
/// Model slots in the actor atlas (2048x2048, 256x256 each); the last
/// slot's corner holds blood's texel.
pub const MODEL_SLOTS: usize = 63;
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
/// Snowflakes at most (the 3DS's).
const SNOW_MAX: usize = 300;

/// Everything the hunt menu picks, shared with C (`HuntChoice` in menu.h).
/// Flags are 0 or 1.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HuntChoice {
    /// Index in `Install::areas`.
    pub area: u32,
    /// 0 dawn, 1 day, 2 night.
    pub time: u32,
    /// Bit n: `Install::animals[n]`.
    pub animals: u64,
    /// Bit n: `_RES.TXT` weapon n.
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
    /// The maps in the mobile game's look (a yellow frame, ring and view
    /// cone) instead of the original's.
    pub map_mobile: u32,
    /// Tapping the GamePad map teleports there (the mobile game's).
    pub teleport: u32,
    /// The controls along the bottom of the TV in a hunt.
    pub hints: u32,
    /// Roaming add-ons listed among the animals; off, they only roam
    /// their own maps (as their makers meant).
    pub all_huntable: u32,
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
            map_mobile: 1,
            teleport: 0,
            hints: 0,
            all_huntable: 0,
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
    fn observer(&self) -> bool {
        self.observer != 0 && self.mode == MODE_HUNT
    }
}

/// A new hunter's save: no trophies yet.
pub fn new_save() -> Save {
    c2_core::profile::new_player("Hunter", 0, 0)
}

/// A call the hunter can make: its id (10 + menu slot), the three answers
/// (`CALLn_A/B/C.WAV`, 16-bit PCM), its icon and the animal's name.
pub struct Call {
    pub id: i32,
    pub sounds: [Vec<u8>; 3],
    pub icon: Option<Picture>,
    pub name: String,
}

/// A hunt and what it loaded.
pub struct Loaded {
    pub area: Area,
    pub hunt: Hunt,
    /// Per character (`_RES.TXT` order): its model, if loaded.
    pub cars: Vec<Option<Car>>,
    /// Per carried weapon (`hunt.weapons.info` order): its view model,
    /// `_RES.TXT` number and menu picture.
    pub weapon_cars: Vec<Car>,
    pub weapon_ids: Vec<usize>,
    pub weapon_icons: Vec<Option<Picture>>,
    pub weapon_names: Vec<String>,
    pub calls: Vec<Call>,
    /// Steps (HWALK1-3, FOOTW1-3), underwater, the hunter's screams
    /// (HUM_DIE1-4), as `c2_sound_clip`'s effects.
    pub fx: Vec<Vec<u8>>,
    pub tables: Option<Tables>,
    pub targets: Bits,
    pub roamers: Vec<Roam>,
}

/// A WAV's 16-bit PCM, if it is the original's format.
fn pcm(install: &Install, path: &str) -> Vec<u8> {
    install
        .vfs
        .read(path)
        .ok()
        .and_then(|d| c2_formats::wav::parse(&d).ok())
        .filter(|w| w.format == Some(c2_formats::wav::WavFormat::VANILLA) && w.pcm.len() % 2 == 0)
        .map_or_else(Vec::new, |w| w.pcm)
}

fn picture(install: &Install, path: &str) -> Option<Picture> {
    let p = c2_formats::tga::parse(&install.vfs.read(path).ok()?).ok()?;
    (p.width > 0 && p.height > 0).then_some(p)
}

/// The sound effects, in `c2_sound_clip`'s order.
pub const FX_FILES: [&str; 11] = [
    "STEPS/HWALK1.WAV", "STEPS/HWALK2.WAV", "STEPS/HWALK3.WAV",
    "STEPS/FOOTW1.WAV", "STEPS/FOOTW2.WAV", "STEPS/FOOTW3.WAV",
    "A_UNDERW.WAV", "HUM_DIE1.WAV", "HUM_DIE2.WAV", "HUM_DIE3.WAV", "HUM_DIE4.WAV",
];

/// Loads the hunt `choice` picks, with the hunter's trophies `room`.
/// Errors: -1 no install, -2 area, -10 nothing to hunt, -11 no landing,
/// -12 no room for an animal, -13 no weapon.
pub fn load(install: Option<&Install>, choice: &HuntChoice, room: &Save, seed: u32) -> Result<Loaded, i32> {
    let install = install.ok_or(-1)?;
    let trophy = choice.trophy();
    let area_file = if trophy {
        String::from("TROPHY")
    } else {
        install.areas.get(choice.area as usize).ok_or(-2)?.file.clone()
    };
    let time = if trophy { c2_core::area::TimeOfDay::Day } else { choice.time_of_day() };
    let area = load::area(&install.vfs, &area_file, time).map_err(|_| -2)?;
    let (targets, roamers) = if trophy { (Bits::NONE, Vec::new()) } else { install.targets(choice.animals) };
    let rules = install.rules.hunt_with(&area_file, time, targets, &roamers);
    // What to load: the hunter, the picked animals, the ambient ones (if
    // on), or the trophies on display; at most the atlas's slots with the
    // weapons, ambient animals giving way first.
    let carried: Vec<usize> = if trophy || choice.observer() {
        Vec::new()
    } else {
        (0..rules.res.weapons.len().min(32)).filter(|&i| choice.weapons & (1 << i) != 0).collect()
    };
    if carried.is_empty() && !trophy && !choice.observer() {
        return Err(-13);
    }
    let shown: Vec<usize> = room.bodies.iter().filter_map(|b| usize::try_from(b.ctype).ok().filter(|&c| c != 0)).collect();
    let mut keep: Vec<bool> = rules
        .roles
        .iter()
        .enumerate()
        .map(|(i, r)| match r {
            Role::Hunter => true,
            _ if trophy => shown.contains(&i),
            Role::Ambient => choice.ambient != 0,
            Role::Slot(k) => targets.has(10 + k),
            Role::Roamer | Role::Off => false,
        })
        .collect();
    // Roaming add-ons not picked whose tables put them on this map come
    // too (after the picks, before ambient animals, if there's room).
    let natives: Vec<usize> = match &rules.tables {
        Some(t) if !trophy && choice.mode == MODE_HUNT => (0..rules.roles.len())
            .filter(|&i| rules.roles[i] == Role::Roamer && t.characters.get(i).is_some_and(|c| c.spawns()))
            .collect(),
        _ => Vec::new(),
    };
    let mut budget = MODEL_SLOTS - carried.len();
    for (i, r) in rules.roles.iter().enumerate() {
        if keep[i] && *r != Role::Ambient {
            budget = budget.saturating_sub(1);
        }
    }
    for &i in &natives {
        if budget > 0 {
            keep[i] = true;
            budget -= 1;
        }
    }
    for (i, r) in rules.roles.iter().enumerate() {
        if keep[i] && *r == Role::Ambient {
            if budget == 0 {
                keep[i] = false;
            } else {
                budget -= 1;
            }
        }
    }
    if !trophy && !rules.roles.iter().enumerate().any(|(i, r)| keep[i] && matches!(r, Role::Slot(_))) {
        return Err(-10);
    }
    let (mut species, cars) = load::species_with(&install.vfs, &rules.res, &|i| keep.get(i).copied().unwrap_or(false));
    if let Some(t) = &rules.tables {
        c2_core::mee::tune(&mut species, t);
    }
    let weapons = load::weapons_with(&install.vfs, &rules.res, &|i| carried.contains(&i));
    let mut infos = Vec::new();
    let mut anims = Vec::new();
    let mut weapon_cars = Vec::new();
    let mut weapon_ids = Vec::new();
    let mut weapon_icons = Vec::new();
    let mut weapon_names = Vec::new();
    for (i, w) in weapons.into_iter().enumerate() {
        let Some(car) = w.car else { continue };
        infos.push(w.info);
        anims.push(w.anims);
        weapon_cars.push(car);
        weapon_ids.push(i);
        weapon_icons.push(picture(install, &format!("HUNTDAT/MENU/PICS/WEAPON{}.TGA", i + 1)));
        weapon_names.push(install.weapons.get(i).cloned().unwrap_or_default());
    }
    if infos.is_empty() && !carried.is_empty() {
        return Err(-13);
    }
    let mut hunt = new_hunt(&area, species, infos, anims, choice, rules.tables.as_ref(), targets, &roamers, room, seed)?;
    hunt.area_no = c2_formats::mee::area_of(&area_file).map_or(0, |n| n as i32);
    // Snow by the area's `.c2map` (Ice Age's areas beside C2's).
    if hunt.effects.snow.is_none() && !trophy && install.info.snow.iter().any(|s| s.eq_ignore_ascii_case(&area_file)) {
        hunt.effects.snow = Some(c2_core::effects::Snowfall::new(c2_formats::mee::Snow::ICE_AGE, SNOW_MAX));
    }
    // Calls: the picked menu slots (roamers have none), each with its
    // answers and icon (the animal's menu picture if the call has none).
    let mut calls = Vec::new();
    for (i, r) in rules.roles.iter().enumerate() {
        let Role::Slot(k) = *r else { continue };
        if !keep[i] || k >= crate::install::ROAM_SLOT0 || calls.iter().any(|c: &Call| c.id == 10 + k as i32) {
            continue;
        }
        let n = k + 1;
        let sounds = ['A', 'B', 'C'].map(|v| pcm(install, &format!("HUNTDAT/SOUNDFX/CALLS/CALL{n}_{v}.WAV")));
        if sounds.iter().all(Vec::is_empty) {
            continue;
        }
        let icon = picture(install, &format!("HUNTDAT/MENU/PICS/CALL{n}.TGA")).or_else(|| picture(install, &format!("HUNTDAT/MENU/PICS/DINO{n}.TGA")));
        let name = hunt.herd.species.get(i).map_or_else(String::new, |s| s.info.name.clone());
        calls.push(Call { id: 10 + k as i32, sounds, icon, name });
    }
    calls.sort_by_key(|c| c.id);
    // Start on a call that sounds.
    if let Some(c) = calls.first() {
        if !calls.iter().any(|c| c.id == hunt.target_call) {
            hunt.target_call = c.id;
        }
    }
    let fx = FX_FILES.iter().map(|f| pcm(install, &format!("HUNTDAT/SOUNDFX/{f}"))).collect();
    Ok(Loaded { area, hunt, cars, weapon_cars, weapon_ids, weapon_icons, weapon_names, calls, fx, tables: rules.tables, targets, roamers })
}

/// A new hunt like `previous` (same animals, weapons and options) with the
/// trophies of `room` (the saved ones: a hunt that ended in death keeps
/// none of its own).
#[allow(clippy::too_many_arguments)]
pub fn restart(area: &Area, previous: &Hunt, choice: &HuntChoice, tables: Option<&Tables>, targets: Bits, roamers: &[Roam], room: &Save, seed: u32) -> Result<Hunt, i32> {
    let mut hunt = new_hunt(
        area,
        previous.herd.species.clone(),
        previous.weapons.info.clone(),
        previous.weapons.anim_ms.clone(),
        choice,
        tables,
        targets,
        roamers,
        room,
        seed,
    )?;
    hunt.area_no = previous.area_no;
    hunt.target_call = previous.target_call;
    Ok(hunt)
}

#[allow(clippy::too_many_arguments)]
fn new_hunt(
    area: &Area,
    species: Vec<Species>,
    info: Vec<WeaponInfo>,
    anim_ms: Vec<[i32; 4]>,
    choice: &HuntChoice,
    tables: Option<&Tables>,
    targets: Bits,
    roamers: &[Roam],
    room: &Save,
    seed: u32,
) -> Result<Hunt, i32> {
    let trophy = choice.trophy();
    let mut rng = MsRand::new(seed);
    let player = if trophy { Player::new(area, tables.and_then(|t| t.trophy_start).unwrap_or(TROPHY_DOOR)) } else { Player::new(area, *area.landings.first().ok_or(-11)?) };
    let mut herd = Herd { species, ..Default::default() };
    if let Some(t) = tables {
        herd.trophy_spots = t.trophies.clone();
    }
    if trophy {
        herd.place_trophies(area, room, &mut rng);
    } else if let Some(t) = tables {
        herd.place_mee(area, (player.x, player.z), &mut rng, t, 128);
    } else {
        place(area, &mut herd, &player, targets, &mut rng)?;
    }
    let present = (0..info.len()).fold(Bits::NONE, |bits, i| bits | Bits::bit(i));
    let weapons = Weapons::new(info, anim_ms, if trophy { Bits::NONE } else { present }, choice.double_ammo != 0);
    let observer = choice.observer();
    let opts = if trophy {
        HuntOptions { trophy: true, view_r: 60, blood: choice.blood != 0, ..Default::default() }
    } else {
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
            teleport: choice.teleport != 0,
            distant_ai: true,
            no_call_bonus: roamers.iter().map(|r| r.slot + 10).collect(),
            ..Default::default()
        }
    };
    let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room.clone());
    hunt.effects.snow = tables.and_then(|t| t.snow).filter(|_| !trophy).map(|s| c2_core::effects::Snowfall::new(s, SNOW_MAX));
    if let Some(kind) = choice.survival_kind() {
        let opts = SurvivalOptions::pick(&hunt, kind, SURVIVAL_MAX);
        hunt.start_survival(area, opts);
    }
    Ok(hunt)
}

/// This port's placement for the original game's data: 18 ambient animals
/// (the loaded ambient kinds in turn) and 48 of the picked animals, by
/// bounded rejection sampling over the walkable map with the shared slope,
/// water and object rules; none close to the hunter.
fn place(area: &Area, herd: &mut Herd, player: &Player, targets: Bits, rng: &mut MsRand) -> Result<(), i32> {
    let loaded = |s: &Species| !s.ani_time.is_empty() && s.info.ai > 0;
    let ambient: Vec<usize> = (0..herd.species.len()).filter(|&i| loaded(&herd.species[i]) && herd.species[i].info.ai < 10).collect();
    let hunted: Vec<usize> = (0..herd.species.len())
        .filter(|&i| loaded(&herd.species[i]) && herd.species[i].info.ai >= 10 && targets.has(herd.species[i].info.ai as usize))
        .collect();
    if hunted.is_empty() {
        return Err(-10);
    }
    let first_target = rng.r_rand(hunted.len() as i32 - 1) as usize;
    let first_slot = if ambient.is_empty() { AMBIENT_POPULATION } else { 0 };
    for slot in first_slot..AMBIENT_POPULATION + TARGET_POPULATION {
        let (id, clearance) = if slot < AMBIENT_POPULATION {
            (ambient[slot % ambient.len()], 2048.0)
        } else {
            (hunted[(slot - AMBIENT_POPULATION + first_target) % hunted.len()], 10240.0)
        };
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
        let mut ch = Character { ctype: id, pos, alpha, tgx: pos[0], tgz: pos[2], ..Default::default() };
        ch.reset(&herd.species[id], rng, 128);
        if matches!(ch.ai, 3 | 4) {
            ch.pos[1] += 2048.0;
        }
        herd.chars.push(ch);
    }
    Ok(())
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

/// Bit 0 disables blood; bit 1 skips death playback. Zero keeps original behavior.
#[allow(dead_code)] // the host tests' shorthand
pub fn configure(hunt: &mut Hunt, disabled: u32) {
    hunt.opts.blood = disabled & 1 == 0;
    hunt.opts.death_anim = disabled & 2 == 0;
}
