//! Base C2 roster used by the Wii U loader and its host integration test.
use alloc::vec::Vec;
use c2_core::{
    area::Area,
    dino::{self, Character, DinoInfo, Herd, Species},
    game::{Hunt, HuntOptions, WeaponInfo, Weapons},
    player::Player,
    rng::MsRand,
};
use c2_formats::{
    bits::Bits,
    car::{self, Car},
    res_txt::ResTxt,
    sav::{Save, TrophyItem},
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

pub fn setup(
    area: &Area,
    definitions: &str,
    assets: &[&[u8]],
    selected: u32,
    radar: bool,
    weapons: u32,
    seed: u32,
) -> Result<(Hunt, Vec<Car>), i32> {
    if assets.len() != MODEL_COUNT
        || selected == 0
        || selected & !TARGET_MASK != 0
        || weapons == 0
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
    let hunt = new_hunt(area, species, infos, anims, present, selected, radar, seed)?;
    Ok((hunt, cars))
}

pub fn restart(area: &Area, previous: &Hunt, seed: u32) -> Result<Hunt, i32> {
    let selected = TARGET_AI.iter().enumerate().fold(0, |mask, (i, ai)| {
        mask | if previous.opts.targets.has(*ai as usize) {
            1 << i
        } else {
            0
        }
    });
    let weapons = previous.weapons.info.clone();
    let anims = previous.weapons.anim_ms.clone();
    let present = (0..weapons.len()).fold(Bits::NONE, |bits, i| bits | Bits::bit(i));
    let mut hunt = new_hunt(
        area,
        previous.herd.species.clone(),
        weapons,
        anims,
        present,
        selected,
        previous.opts.radar,
        seed,
    )?;
    hunt.opts.blood = previous.opts.blood;
    hunt.opts.death_anim = previous.opts.death_anim;
    Ok(hunt)
}

fn new_hunt(
    area: &Area,
    species: Vec<Species>,
    info: Vec<WeaponInfo>,
    anim_ms: Vec<[i32; 4]>,
    present: Bits,
    selected: u32,
    radar: bool,
    seed: u32,
) -> Result<Hunt, i32> {
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
    for slot in 0..POPULATION {
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
    let weapons = Weapons::new(info, anim_ms, present, false);
    let save = Save {
        player_name_raw: [0; 128],
        reg_number: 0,
        score: 9999,
        rank: 0,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: Default::default(),
    };
    let hunt = Hunt::new(
        area,
        player,
        herd,
        weapons,
        rng,
        HuntOptions {
            targets,
            radar,
            distant_ai: true,
            ..Default::default()
        },
        save,
    );
    Ok(hunt)
}

/// Bit 0 disables blood; bit 1 skips death playback. Zero keeps original behavior.
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
