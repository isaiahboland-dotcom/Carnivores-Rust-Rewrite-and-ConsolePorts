#![no_std]
extern crate alloc;
mod hunt_setup;
mod static_scene;
mod ui;
mod menu;
use static_scene::{TerrainCache, SceneryCandidates, visible_sphere};
use alloc::{boxed::Box, vec::Vec};
use c2_core::{
    area::Area,
    player::{Controls, Player},
    dino::{self, Herd},
    game::{GameEvent, Hunt, HuntInput, HuntOptions, Weapons},
    rng::MsRand,
    model::PreparedModel,
};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
const CACHE_CHUNK_CELLS: usize = 8;
const CACHE_CHUNKS_PER_SIDE: usize = 1024 / CACHE_CHUNK_CELLS;
const CACHE_LIMIT: usize = 32 * 1024 * 1024;
struct CachedGeometry { positions: Vec<[f32; 4]>, attrs: Vec<[f32; 4]> }
struct SceneryChunk {
    objects: Vec<Option<CachedGeometry>>,
    bytes: usize,
    last_used: u64,
}
impl SceneryChunk {
    fn new() -> Self {
        Self { objects: (0..64).map(|_| None).collect(), bytes: 0, last_used: 0 }
    }
}
extern "C" {
    fn c2_move_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_move_free(ptr: *mut u8);
    fn c2_move_panic() -> !;
    fn c2_hunt_stage(stage: u32);
    fn c2_hunt_values(toggle: u32, hidden: u32, underwater: u32, health: i32, active: u32);
    fn c2_play_sound(kind: u32, slot: u32, owner: i32, left: f32, right: f32);
    fn c2_set_ambient(zone: i32, gain: f32);
    fn c2_stop_animal(index: u32);
}
fn first_tick_state(toggle: u32, hidden: u32, underwater: u32, health: i32, active: u32) {
    unsafe { c2_hunt_values(toggle, hidden, underwater, health, active); }
}
// The hunt profiler invokes this at the start and after each tick section.
// Only enabled for the first tick so a failed update leaves a log breadcrumb.
static mut HUNT_STAGE: u32 = 0;
fn first_tick_clock() -> f64 {
    unsafe {
        HUNT_STAGE += 1;
        c2_hunt_stage(HUNT_STAGE);
    }
    0.0
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_move_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_move_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
struct Simulation {
    area: Area,
    terrain_cache: TerrainCache,
    scenery_candidates: SceneryCandidates,
    resident_scenery: static_scene::ResidentScenery,
    resident_terrain: static_scene::ResidentTerrain,
    terrain_written: Vec<static_scene::DrawRange>,
    scenery_written: Vec<static_scene::DrawRange>,
    choice: hunt_setup::HuntChoice,
    models: Vec<PreparedModel>,
    model_uvs: Vec<Vec<[[f32; 2]; 3]>>,
    object_lights: Vec<f32>,
    model_radii: Vec<f32>,
    transformed: Vec<[f32; 4]>,
    chunks: Vec<Option<SceneryChunk>>,
    cache_bytes: usize,
    cache_clock: u64,
    hunt: Hunt,
    events: Vec<GameEvent>,
    sprint: bool,
    crouch: bool,
    first_tick_done: bool,
    death_ms: u32,
    cars: Vec<c2_formats::car::Car>,
    actor_models: Vec<PreparedModel>,
    actor_radii: Vec<f32>,
    call_sounds: Vec<Vec<u8>>,
    fx_sounds: Vec<Vec<u8>>,
    panel_icons: Vec<c2_formats::tga::Picture>,
}
#[repr(C)]
pub struct Snapshot {
    x: i32,
    y: i32,
    z: i32,
    yaw_milli: i32,
    speed_milli: i32,
    sprint: u32,
    crouch: u32,
}
#[repr(C)]
pub struct HudState { ammo: i32, weapon_ready: u32, call: i32, weapon: i32 }
#[no_mangle]
pub extern "C" fn c2_hud_state(ptr: *mut core::ffi::c_void, out: *mut HudState) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let w = &sim.hunt.weapons;
    unsafe { *out = HudState {
        call: sim.hunt.target_call,
        weapon: w.current as i32,
        ammo: w.shots_left.get(w.current).copied().unwrap_or(0),
        weapon_ready: (w.state == c2_core::game::WeaponState::Ready) as u32,
    }; }
}
#[no_mangle]
pub extern "C" fn c2_hunt_dead(ptr: *mut core::ffi::c_void, shots: *mut u32, kills: *mut u32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    unsafe { *shots = sim.hunt.shots_made; *kills = sim.hunt.successes; }
    hunt_setup::death_menu_ready(&sim.hunt, sim.death_ms) as u32
}

#[no_mangle]
pub extern "C" fn c2_death_active(ptr: *mut core::ffi::c_void) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    (sim.hunt.health <= 0) as u32
}

#[no_mangle]
pub extern "C" fn c2_restart_hunt(ptr: *mut core::ffi::c_void, seed: u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let hunt = match hunt_setup::restart_with(&sim.area, &sim.hunt, &sim.choice, seed) { Ok(h) => h, Err(e) => return e };
    sim.hunt = hunt;
    sim.death_ms = 0;
    sim.events.clear(); sim.sprint = false; sim.crouch = false; sim.first_tick_done = false;
    0
}

// C keeps both buffers alive for the call; this function parses owned data.
// Return codes distinguish map and resource parsing errors.
#[no_mangle]
pub extern "C" fn c2_move_create(
    map_ptr: *const u8,
    map_len: usize,
    rsc_ptr: *const u8,
    rsc_len: usize,
    time: u32,
    result: *mut *mut core::ffi::c_void,
) -> i32 {
    let map_data = unsafe { core::slice::from_raw_parts(map_ptr, map_len) };
    let rsc_data = unsafe { core::slice::from_raw_parts(rsc_ptr, rsc_len) };
    let map = match c2_formats::map::parse(map_data) {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let rsc = match c2_formats::rsc::parse(rsc_data) {
        Ok(v) => v,
        Err(_) => return -2,
    };
    let time = hunt_setup::HuntChoice { time, ..Default::default() }.time_of_day();
    let area = Area::new(map, rsc, time);
    // The map and time of day are fixed for the session; object lighting is static.
    let mut object_lights = alloc::vec![0.0; area.map.objects.len()];
    for placement in area.placements() {
        let index = usize::from(placement.cell.1) * 1024 + usize::from(placement.cell.0);
        object_lights[index] = area.object_light(&placement);
    }
    let cell = area.landings.first().copied().unwrap_or((256, 256));
    let player = Player::new(&area, cell);
    // Start with an empty hunt until assets and menu selections are wired in.
    let save = c2_formats::sav::Save {
        player_name_raw: [0; 128], reg_number: 0, score: 0, rank: 0,
        last: Default::default(), total: Default::default(),
        bodies: [c2_formats::sav::TrophyItem::default(); 24], options: Default::default(),
    };
    let weapons = Weapons::new(Vec::new(), Vec::new(), c2_formats::bits::Bits::NONE, false);
    let hunt = Hunt::new(&area, player, Herd::default(), weapons, MsRand::new(1), HuntOptions::default(), save);
    let models: Vec<PreparedModel> = area.rsc.objects.iter().map(|o| PreparedModel::from_mesh(&o.model.mesh)).collect();
    // Atlas coordinates depend only on the model and face, never on the camera.
    let model_uvs = static_scene::scenery_uvs(&models);
    let model_radii = models.iter().map(PreparedModel::radius).collect();
    let max_vertices = models.iter().map(|model| model.positions.len()).max().unwrap_or(0);
    let transformed = alloc::vec![[0.0; 4]; max_vertices];
    let terrain_cache = TerrainCache::new(&area);
    let sim = Simulation {
        area,
        terrain_cache,
        scenery_candidates: SceneryCandidates::default(),
        resident_scenery: static_scene::ResidentScenery::default(),
        resident_terrain: static_scene::ResidentTerrain::default(),
        terrain_written: Vec::new(),
        scenery_written: Vec::new(),
        choice: hunt_setup::HuntChoice::default(),
        models,
        model_uvs,
        object_lights,
        model_radii,
        transformed,
        chunks: (0..CACHE_CHUNKS_PER_SIDE * CACHE_CHUNKS_PER_SIDE).map(|_| None).collect(),
        cache_bytes: 0,
        cache_clock: 0,
        hunt,
        events: Vec::new(),
        sprint: false,
        crouch: false,
        first_tick_done: false,
        death_ms: 0,
        cars: Vec::new(),
        actor_models: Vec::new(),
        actor_radii: Vec::new(),
        call_sounds: alloc::vec![Vec::new(); hunt_setup::TARGET_AI.len() * 3],
        fx_sounds: alloc::vec![Vec::new(); 11],
        panel_icons: Vec::new(),
    };
    unsafe {
        *result = Box::into_raw(Box::new(sim)).cast();
    }
    0
}
// Compare a scripted path against a host run of this installation's AREA1.
// Run on a clone so the interactive player remains at the landing point.
#[no_mangle]
pub extern "C" fn c2_move_checks(ptr: *mut core::ffi::c_void) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let mut player = sim.hunt.player.clone();
    let mut events = Vec::new();
    let near = |a: f32, b: f32| (a - b).abs() < 4.0;
    let mut result = 0;
    if near(player.x, 155008.0) && near(player.y, 5248.0) && near(player.z, 45696.0) {
        result |= 1;
    }
    let walk = Controls {
        forward: true,
        ..Controls::default()
    };
    for _ in 0..125 {
        player.tick(&sim.area, &walk, 16, &mut events);
    }
    if near(player.x, 155008.0)
        && near(player.y, 5150.5)
        && near(player.z, 45126.102)
        && (player.vspeed - 0.3).abs() < 1e-4
    {
        result |= 2;
    }
    let run = Controls {
        forward: true,
        run: true,
        ..Controls::default()
    };
    for _ in 0..125 {
        player.tick(&sim.area, &run, 16, &mut events);
    }
    if near(player.x, 155008.0)
        && near(player.y, 5264.768)
        && near(player.z, 43802.586)
        && (player.vspeed - 0.7).abs() < 1e-4
    {
        result |= 4;
    }
    result
}

// Flags: held A, newly pressed B, newly pressed D-pad up.
#[no_mangle]
pub extern "C" fn c2_move_tick(
    ptr: *mut core::ffi::c_void,
    lx: f32,
    ly: f32,
    rx: f32,
    ry: f32,
    flags: u32,
    out: *mut Snapshot,
) {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let dead = sim.hunt.health <= 0;
    let (lx, ly, rx, ry, flags) = if dead { (0.0, 0.0, 0.0, 0.0, 0) } else { (lx, ly, rx, ry, flags) };
    if flags & 2 != 0 {
        sim.sprint = !sim.sprint;
    }
    if flags & 4 != 0 {
        sim.crouch = !sim.crouch;
    }
    let controls = Controls {
        forward: ly > 0.25,
        backward: ly < -0.25,
        strafe_left: lx < -0.25,
        strafe_right: lx > 0.25,
        turn_left: rx < -0.25,
        turn_right: rx > 0.25,
        look_up: ry > 0.25,
        look_down: ry < -0.25,
        jump: flags & 1 != 0,
        run: sim.sprint,
        crouch: sim.crouch,
        ..Controls::default()
    };
    sim.events.clear();
    if !sim.first_tick_done {
        unsafe { HUNT_STAGE = 0; c2_hunt_stage(0); }
        sim.hunt.prof.clock = Some(first_tick_clock);
        sim.hunt.prof.diagnostic_state = Some(first_tick_state);
    }
    let cars: [Option<&c2_formats::car::Car>; hunt_setup::HUNTER_SPECIES + 1] = core::array::from_fn(|i| sim.cars.get(hunt_setup::model_for_species(i)));
    if flags & 128 != 0 {
        let current = hunt_setup::TARGET_AI.iter().position(|&ai| ai == sim.hunt.target_call).unwrap_or(0);
        for step in 1..=hunt_setup::TARGET_AI.len() {
            let ai = hunt_setup::TARGET_AI[(current + step) % hunt_setup::TARGET_AI.len()];
            if sim.hunt.opts.targets.has(ai as usize) { sim.hunt.target_call = ai; break; }
        }
    }
    let direct_call = ((flags >> 8) & 15) as usize;
    if (1..=9).contains(&direct_call) && sim.hunt.opts.targets.has(9 + direct_call) {
        sim.hunt.target_call = (9 + direct_call) as i32;
    }
    let tapped_weapon = ((flags >> 16) & 7) as usize;
    let select_weapon = if flags & 64 != 0 { Some(tapped_weapon) } else { None };
    let toggle_weapon = flags & 8 != 0;
    sim.hunt.tick(&sim.area, &cars, &HuntInput { controls, toggle_weapon, fire: flags & 16 != 0, call: flags & 32 != 0, select_weapon: if flags & 4096 != 0 && !sim.hunt.weapons.info.is_empty() { Some((sim.hunt.weapons.current+1) % sim.hunt.weapons.info.len()) } else { select_weapon }, ..Default::default() }, 16, &mut sim.events);
    if sim.hunt.health <= 0 { sim.death_ms += 16; }
    let (eye, alpha, _) = sim.hunt.camera();
    for event in &sim.events {
        match *event {
            GameEvent::WeaponSound { weapon, slot } => {
                let (l, r) = c2_core::sound::lr_gains(256.0, None, eye, alpha);
                unsafe { c2_play_sound(20 + weapon as u32, slot as u32, -1, l, r); }
            }
            GameEvent::Dino(dino::DinoEvent::Sound { index, sound, pos }) => {
                if let Some(ch) = sim.hunt.herd.chars.get(index) {
                    if ch.ctype == hunt_setup::HUNTER_SPECIES { continue; } // Hunter scream uses the dedicated WAV effect.
                    let (l, r) = c2_core::sound::lr_gains(256.0, Some(pos), eye, alpha);
                    unsafe { c2_play_sound(ch.ctype as u32, sound as u32, index as i32, l, r); }
                }
            }
            GameEvent::Player(c2_core::player::PlayerEvent::Step) |
            GameEvent::Player(c2_core::player::PlayerEvent::StepWater) |
            GameEvent::Player(c2_core::player::PlayerEvent::Land) => {
                let variant = (sim.hunt.real_time % 3) as u32;
                let (slot, volume) = match *event {
                    GameEvent::Player(c2_core::player::PlayerEvent::Step) => (variant, 24 + (sim.hunt.player.vspeed * 50.0) as i32),
                    GameEvent::Player(c2_core::player::PlayerEvent::StepWater) => (3 + variant, 64 + (sim.hunt.player.vspeed * 30.0) as i32),
                    _ => (variant, 64),
                };
                let (l, r) = c2_core::sound::lr_gains(volume as f32, None, eye, alpha);
                unsafe { c2_play_sound(19, slot, -1, l, r); }
            }
            GameEvent::HunterDied { kind, scream: true } if kind != dino::hunt::BREATH => {
                let (l, r) = c2_core::sound::lr_gains(256.0, None, eye, alpha);
                unsafe { c2_play_sound(19, 7 + (sim.hunt.real_time % 4) as u32, -1, l, r); }
            }
            GameEvent::RandomSound { sound, pos, volume } => {
                let (l, r) = c2_core::sound::lr_gains(volume as f32, Some(pos), eye, alpha);
                unsafe { c2_play_sound(17, sound as u32, -1, l, r); }
            }
            GameEvent::Ambient { zone, volume } => {
                unsafe { c2_set_ambient(zone.map_or(-1, |z| z as i32), c2_core::sound::ambient_gain(volume)); }
            }
            GameEvent::Killed { index, .. } => unsafe { c2_stop_animal(index as u32); },
            GameEvent::Call { call, variant, pos } => {
                let Some(index) = hunt_setup::TARGET_AI.iter().position(|&ai| ai == call) else { continue; };
                let (l, r) = c2_core::sound::lr_gains(256.0, pos, eye, alpha);
                unsafe { c2_play_sound(16, (index * 3 + variant) as u32, -1, l, r); }
            }
            _ => {}
        }
    }
    if !sim.first_tick_done {
        unsafe { c2_hunt_stage(99); }
        sim.hunt.prof.clock = None;
        sim.hunt.prof.diagnostic_state = None;
        sim.first_tick_done = true;
    }
    let p = &sim.hunt.player;
    unsafe {
        *out = Snapshot {
            x: p.x as i32,
            y: p.y as i32,
            z: p.z as i32,
            yaw_milli: (p.alpha * 1000.0) as i32,
            speed_milli: (p.vspeed * 1000.0) as i32,
            sprint: sim.sprint as u32,
            crouch: sim.crouch as u32,
        };
    }
}
#[no_mangle]
pub extern "C" fn c2_move_destroy(ptr: *mut core::ffi::c_void) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr.cast::<Simulation>()) });
    }
}

// Load original definitions and models from the caller-owned SD buffers.
#[no_mangle]
pub extern "C" fn c2_hunt_setup(ptr: *mut core::ffi::c_void, files: *const *const u8, lengths: *const u32, choice: *const hunt_setup::HuntChoice, seed: u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let read = |i: usize| unsafe { core::slice::from_raw_parts(*files.add(i), *lengths.add(i) as usize) };
    let Ok(text) = core::str::from_utf8(read(0)) else { return -1; };
    let assets: [&[u8]; hunt_setup::MODEL_COUNT] = core::array::from_fn(|i| read(i + 1));
    let choice = unsafe { *choice };
    let (hunt, cars) = match hunt_setup::setup_with(&sim.area, text, &assets, &choice, seed) { Ok(v) => v, Err(e) => return e };
    sim.hunt = hunt;
    sim.choice = choice;
    sim.death_ms = 0;
    sim.cars = cars;
    sim.actor_models = sim.cars.iter().map(|c| PreparedModel::from_mesh(&c.mesh)).collect();
    sim.actor_radii = sim.cars.iter().zip(&sim.actor_models).map(|(car, model)| hunt_setup::animation_radius(car).max(model.radius())).collect();
    sim.first_tick_done = false;
    0
}

#[no_mangle]
pub extern "C" fn c2_actor_atlas(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if sim.actor_models.len() != hunt_setup::MODEL_COUNT || pitch < 2048 { return -1; }
    unsafe { core::ptr::copy_nonoverlapping([160u8, 8, 4, 255].as_ptr(), pixels.add((800 * pitch as usize + 300) * 4), 4); }
    for (i, model) in sim.actor_models.iter().enumerate() {
        for y in 0..256usize {
            unsafe { core::ptr::copy_nonoverlapping(model.texture_rgba.as_ptr().add(y * 1024), pixels.add(((y + i / 8 * 256) * pitch as usize + i % 8 * 256) * 4), 1024); }
        }
    }
    unsafe { night_vision_rows(sim, pixels, pitch, 2048, sim.actor_models.len().div_ceil(8) * 256); }
    0
}

#[no_mangle]
pub extern "C" fn c2_call_load(ptr: *mut core::ffi::c_void, slot: u32, data: *const u8, len: u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let Some(dest) = sim.call_sounds.get_mut(slot as usize) else { return -1; };
    let Ok(wav) = c2_formats::wav::parse(unsafe { core::slice::from_raw_parts(data, len as usize) }) else { return -2; };
    if wav.format != Some(c2_formats::wav::WavFormat::VANILLA) || wav.pcm.is_empty() || wav.pcm.len() % 2 != 0 { return -3; }
    *dest = wav.pcm;
    0
}

// Night hunts are seen through night vision (`BrightenTexture`): every texel
// of the world becomes (G/8, G, G/8).
fn night_vision(pixels: &mut [u8]) {
    for p in pixels.chunks_exact_mut(4) { let g = p[1]; p[0] = g / 8; p[2] = g / 8; }
}
unsafe fn night_vision_rows(sim: &Simulation, pixels: *mut u8, pitch: u32, width: usize, rows: usize) {
    if !sim.area.time.night_vision() { return; }
    for y in 0..rows { night_vision(core::slice::from_raw_parts_mut(pixels.add(y * pitch as usize * 4), width * 4)); }
}

#[no_mangle]
pub extern "C" fn c2_fx_load(ptr: *mut core::ffi::c_void, slot: u32, data: *const u8, len: u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let Some(dest) = sim.fx_sounds.get_mut(slot as usize) else { return -1; };
    let Ok(wav) = c2_formats::wav::parse(unsafe { core::slice::from_raw_parts(data, len as usize) }) else { return -2; };
    if wav.format != Some(c2_formats::wav::WavFormat::VANILLA) || wav.pcm.is_empty() || wav.pcm.len() % 2 != 0 { return -3; }
    *dest = wav.pcm;
    0
}

#[no_mangle]
pub extern "C" fn c2_icon_load(ptr: *mut core::ffi::c_void, data: *const u8, len: u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let Ok(pic) = c2_formats::tga::parse(unsafe { core::slice::from_raw_parts(data, len as usize) }) else { return -1; };
    if pic.width == 0 || pic.height == 0 { return -2; }
    sim.panel_icons.push(pic);
    0
}
#[no_mangle]
pub extern "C" fn c2_icon_atlas(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32, page: u32, weapon: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if sim.panel_icons.len() != hunt_setup::TARGET_AI.len() + hunt_setup::WEAPON_COUNT || pitch < 512 || page > 1 || (!sim.hunt.weapons.info.is_empty() && weapon as usize >= sim.hunt.weapons.info.len()) { return -1; }
    let out = unsafe { core::slice::from_raw_parts_mut(pixels, pitch as usize * 256 * 4) };
    for rgba in out.chunks_exact_mut(4) { rgba.copy_from_slice(&[16,22,16,255]); }
    let mut draw = |id: usize, x0: usize, y0: usize, width: usize, height: usize, highlight: bool| {
        let pic = &sim.panel_icons[id];
        for y in 0..height { for x in 0..width {
            let sx = x * pic.width / width; let sy = y * pic.height / height;
            let rgba = c2_core::color::rgb555_to_rgba8(pic.pixels[sy * pic.width + sx], 255);
            let offset = ((y0+y)*pitch as usize+x0+x)*4;
            out[offset..offset+4].copy_from_slice(&rgba);
        }}
        if highlight {
            for x in x0.saturating_sub(2)..(x0+width+2).min(256) {
                for y in [y0.saturating_sub(2), (y0+height+1).min(255)] {
                    let offset=(y*pitch as usize+x)*4;
                    out[offset..offset+4].copy_from_slice(&[255,210,60,255]);
                }
            }
            for y in y0.saturating_sub(2)..(y0+height+2).min(256) {
                for x in [x0.saturating_sub(2), (x0+width+1).min(255)] {
                    let offset=(y*pitch as usize+x)*4;
                    out[offset..offset+4].copy_from_slice(&[255,210,60,255]);
                }
            }
        }
    };
    for (slot, info) in sim.hunt.weapons.info.iter().enumerate() {
        let Some(icon) = hunt_setup::WEAPON_FILES.iter().position(|f| info.file.eq_ignore_ascii_case(f)) else { continue; };
        let x = (slot % 2) * 128 + 4;
        let y = (slot / 2) * 84 + 2;
        draw(icon, x, y, 120, 80, slot == weapon as usize);
    }
    for (slot, (i, _)) in hunt_setup::TARGET_AI.iter().enumerate()
        .filter(|(_, ai)| sim.hunt.opts.targets.has(**ai as usize))
        .skip(page as usize * 6).take(6).enumerate() {
        draw(hunt_setup::WEAPON_COUNT+i, 260+(slot%2)*128, 4+(slot/2)*72, 120, 46, false);
    }
    0
}

// Returned bytes remain owned by Simulation until it is destroyed.
#[no_mangle]
pub extern "C" fn c2_sound_clip(ptr: *mut core::ffi::c_void, kind: u32, slot: u32, data: *mut *const u8, len: *mut u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let bytes = if (20..26).contains(&kind) { sim.hunt.weapons.info.get((kind-20) as usize).and_then(|w| hunt_setup::WEAPON_FILES.iter().position(|f| w.file.eq_ignore_ascii_case(f))).and_then(|i| sim.cars.get(hunt_setup::WEAPON_MODEL+i)).and_then(|c| c.sounds.get(slot as usize)).map(|s| s.pcm.as_slice()) }
        else if kind == 16 { sim.call_sounds.get(slot as usize).map(|v| v.as_slice()) }
        else if kind == 17 { sim.area.rsc.random_sounds.get(slot as usize).map(|v| v.as_slice()) }
        else if kind == 18 { sim.area.rsc.ambients.get(slot as usize).map(|v| v.pcm.as_slice()) }
        else if kind == 19 { sim.fx_sounds.get(slot as usize).map(|v| v.as_slice()) }
        else { sim.cars.get(kind as usize).and_then(|c| c.sounds.get(slot as usize)).map(|s| s.pcm.as_slice()) };
    let Some(bytes) = bytes.filter(|b| !b.is_empty() && b.len() % 2 == 0) else { return -1; };
    unsafe { *data = bytes.as_ptr(); *len = bytes.len() as u32; }
    0
}

// mode 0 emits world-space animated animals; mode 1 emits the near pistol.
#[no_mangle]
pub extern "C" fn c2_actor_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, attrs: *mut f32, capacity: u32, mode: u32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let mut n = 0usize;
    let cap = capacity as usize;
    let mut emit = |model: &PreparedModel, posed: &[[f32; 3]], id: usize, transform: &dyn Fn([f32; 3]) -> [f32; 4]| {
        let limit = if mode == 0 { cap.saturating_sub(8192) } else { cap };
        if n + model.faces.len()*3 > limit { return; }
        for face in &model.faces {
            for k in 0..3 {
                let p = transform(posed[face.v[k] as usize]);
                let uv = [((id % 8 * 256) as f32 + (face.uv[k][0] * 256.0).clamp(0.0, 255.0) + 0.5) / 2048.0,
                          ((id / 8 * 256) as f32 + (face.uv[k][1] * 256.0).clamp(0.0, 255.0) + 0.5) / 1024.0, 0.75, 1.0];
                unsafe { core::ptr::copy_nonoverlapping(p.as_ptr(), positions.add(n * 4), 4); core::ptr::copy_nonoverlapping(uv.as_ptr(), attrs.add(n * 4), 4); }
                n += 1;
            }
        }
    };
    if mode != 0 && sim.hunt.health <= 0 { return 0; }
    if mode == 0 {
        let (eye, yaw, pitch) = sim.hunt.camera();
        let distance = |ch: &dino::Character| (ch.pos[0]-eye[0])*(ch.pos[0]-eye[0]) + (ch.pos[1]-eye[1])*(ch.pos[1]-eye[1]) + (ch.pos[2]-eye[2])*(ch.pos[2]-eye[2]);
        let mut visible: Vec<_> = sim.hunt.herd.chars.iter().filter(|ch| {
            let id = hunt_setup::model_for_species(ch.ctype);
            sim.actor_radii.get(id).is_some_and(|r| hunt_setup::actor_visible(ch.pos, r*ch.scale, eye, yaw, pitch))
        }).collect();
        visible.sort_unstable_by(|a,b| distance(a).total_cmp(&distance(b)));
        for ch in visible {
            let id = hunt_setup::model_for_species(ch.ctype);
            let (Some(car), Some(model), Some(species)) = (sim.cars.get(id), sim.actor_models.get(id), sim.hunt.herd.species.get(ch.ctype)) else { continue; };
            let Some(posed) = dino::pose::pose(ch, car, species.anims()) else { continue; };
            let angle = dino::pose::quarter_turns(ch) * core::f32::consts::FRAC_PI_2;
            let (s, c) = (libm::sinf(angle), libm::cosf(angle));
            emit(model, &posed, id, &|v| [ch.pos[0] + v[0]*c + v[2]*s, ch.pos[1] + v[1], ch.pos[2] - v[0]*s + v[2]*c, 1.0]);
        }
    } else if let Some((slot, ftime)) = sim.hunt.weapons.pose() {
        let model_slot = sim.hunt.weapons.info.get(sim.hunt.weapons.current).and_then(|w| hunt_setup::WEAPON_FILES.iter().position(|f| w.file.eq_ignore_ascii_case(f))).map(|i| hunt_setup::WEAPON_MODEL+i);
        if let Some(model_slot) = model_slot {
        let (Some(car),Some(model)) = (sim.cars.get(model_slot),sim.actor_models.get(model_slot)) else { return 0; };
        if let Some(anim) = car.animations.get(slot) {
            let duration = c2_core::model::car_anim_duration_ms(anim).max(1);
            if let Some(posed) = c2_core::model::pose_car(anim, car.mesh.vertices.len(), (ftime.max(0) as u32).min(duration - 1)) {
                let (da, db) = sim.hunt.weapons.sway(sim.hunt.real_time);
                let (sa, ca, sb, cb) = (libm::sinf(-da), libm::cosf(-da), libm::sinf(-db), libm::cosf(-db));
                emit(model, &posed, model_slot, &|v| {
                    let x = v[0]*ca + v[2]*sa; let vz = v[2]*ca - v[0]*sa;
                    let y = v[1]*cb - vz*sb; let depth = -(vz*cb + v[1]*sb);
                    [x*0.75, y*1.333, hunt_setup::gun_clip_z(depth), depth]
                });
            }
        }
        }
    }
    if mode == 0 && sim.hunt.opts.blood {
        let (_, a, b) = sim.hunt.camera();
        let right = [libm::cosf(a), 0.0, libm::sinf(a)];
        let up = [libm::sinf(a)*libm::sinf(b), libm::cosf(b), -libm::cosf(a)*libm::sinf(b)];
        let mut diamond = |pos: [f32;3], r: f32, ground: bool, light: f32| {
            if n + 6 > cap { return; }
            for (x,y) in [(0.0,1.0),(-1.0,0.0),(0.0,-1.0),(0.0,1.0),(0.0,-1.0),(1.0,0.0)] {
                let p = if ground { [pos[0]+x*r,pos[1]+2.0,pos[2]+y*r,1.0] }
                    else { [pos[0]+r*(right[0]*x+up[0]*y),pos[1]+r*up[1]*y,pos[2]+r*(right[2]*x+up[2]*y),1.0] };
                let uv = [300.5/2048.0,800.5/1024.0,light,0.0];
                unsafe { core::ptr::copy_nonoverlapping(p.as_ptr(),positions.add(n*4),4); core::ptr::copy_nonoverlapping(uv.as_ptr(),attrs.add(n*4),4); }
                n += 1;
            }
        };
        for burst in &sim.hunt.effects.bursts {
            if burst.kind != c2_core::effects::Part::Blood { continue; }
            for part in &burst.parts { diamond(part.pos, part.r*0.64, false, 1.0); }
        }
        for drop in &sim.hunt.effects.blood {
            diamond(drop.pos, 12.0 * (drop.life as f32 / 20000.0).clamp(0.0,1.0), true, 0.65);
        }
    }
    n as u32
}

#[repr(C)]
pub struct RadarBlip { x: f32, z: f32, rgba: u32 }
#[no_mangle]
pub extern "C" fn c2_set_radar(ptr: *mut core::ffi::c_void, enabled: u32) {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    sim.hunt.opts.radar = enabled != 0;
}

#[no_mangle]
pub extern "C" fn c2_radar_blips(ptr: *mut core::ffi::c_void, out: *mut RadarBlip, capacity: u32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if !sim.hunt.opts.radar { return 0; }
    let p = &sim.hunt.player;
    let range = sim.hunt.opts.view_r as f32 * 256.0;
    let mut n = 0;
    for ch in &sim.hunt.herd.chars {
        let dx = ch.pos[0] - p.x; let dz = ch.pos[2] - p.z;
        let target = sim.hunt.is_target(ch);
        let huntable = sim.hunt.is_huntable(ch);
        if ch.health <= 0 || (dx*dx + dz*dz > range*range && !(huntable && sim.hunt.opts.radar)) { continue; }
        if n >= capacity { break; }
        let color = if target { 0x00f800ff } else if huntable { 0x3c8cffff } else { 0xbebe78ffu32 };
        unsafe { *out.add(n as usize) = RadarBlip { x: ch.pos[0] / 256.0, z: ch.pos[2] / 256.0, rgba: color }; }
        n += 1;
    }
    n
}

// Fill a 16x16 atlas using the original RSC's RGB555 textures. The C caller
// supplies a GX2 linear RGBA8 surface and its pitch, in pixels.
#[no_mangle]
pub extern "C" fn c2_terrain_atlas(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if sim.area.rsc.textures.is_empty() || sim.area.rsc.textures.len() > 256 || pitch < 2048 { return -1; }
    for (index, texture) in sim.area.rsc.textures.iter().enumerate() {
        if texture.len() != 128 * 128 { return -2; }
        for y in 0..128usize {
            for x in 0..128usize {
                let pixel = c2_core::color::rgb555_to_rgba8(texture[y * 128 + x], 255);
                let dst = ((index / 16 * 128 + y) * pitch as usize + index % 16 * 128 + x) * 4;
                unsafe { core::ptr::copy_nonoverlapping(pixel.as_ptr(), pixels.add(dst), 4); }
            }
        }
    }
    unsafe { night_vision_rows(sim, pixels, pitch, 2048, sim.area.rsc.textures.len().div_ceil(16) * 128); }
    0
}

// The C caller provides a 2048x2048 linear RGBA8 atlas: 64 256px models.
#[no_mangle]
pub extern "C" fn c2_scenery_atlas(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if sim.models.len() > 64 || pitch < 2048 { return -1; }
    for (index, model) in sim.models.iter().enumerate() {
        for y in 0..256usize {
            let dst = ((index / 8 * 256 + y) * pitch as usize + index % 8 * 256) * 4;
            unsafe { core::ptr::copy_nonoverlapping(model.texture_rgba.as_ptr().add(y * 1024), pixels.add(dst), 1024); }
        }
    }
    unsafe { night_vision_rows(sim, pixels, pitch, 2048, sim.models.len().div_ceil(8) * 256); }
    0
}

// Diagnostic top-down map: one texel per terrain cell, colored from its
// terrain texture and daylight. The game data remains on the SD card.
#[no_mangle]
pub extern "C" fn c2_map_texture(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if pitch < 1024 || sim.area.rsc.textures.is_empty() { return -1; }
    for z in 0..1024usize { for x in 0..1024usize {
        let i = z * 1024 + x;
        let tex = sim.area.map.texture1[i] as usize;
        if sim.area.map.flags[i] & c2_core::area::cell_flags::WATER_ANY != 0 {
            let blue = [35u8, 105, 155, 255];
            unsafe { core::ptr::copy_nonoverlapping(blue.as_ptr(), pixels.add((z * pitch as usize + x) * 4), 4); }
            continue;
        }
        let color = sim.area.rsc.textures.get(tex).and_then(|t| t.get(64 * 128 + 64)).copied().unwrap_or(0);
        let mut rgba = c2_core::color::rgb555_to_rgba8(color, 255);
        let light = u32::from(sim.area.map.light[0][i]);
        for c in &mut rgba[..3] { *c = (u32::from(*c) * light / 255) as u8; }
        unsafe { core::ptr::copy_nonoverlapping(rgba.as_ptr(), pixels.add((z * pitch as usize + x) * 4), 4); }
    }}
    0
}
#[no_mangle]
pub extern "C" fn c2_map_player(ptr: *mut core::ffi::c_void, x: *mut f32, z: *mut f32, dx: *mut f32, dz: *mut f32) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let (yaw, _) = sim.hunt.player.camera_angles();
    unsafe { *x = sim.hunt.player.x / 256.0; *z = sim.hunt.player.z / 256.0; *dx = libm::sinf(yaw); *dz = -libm::cosf(yaw); }
}

#[no_mangle]
pub extern "C" fn c2_scenery_camera(ptr: *mut core::ffi::c_void, out: *mut f32) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let (eye, a, b) = sim.hunt.camera();
    let values = [eye[0], eye[1], eye[2], 0.0, libm::sinf(a), libm::cosf(a), libm::sinf(b), libm::cosf(b)];
    unsafe { core::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len()); }
}

// Static render dependencies include the player's terrain-window anchor even
// when the death camera has moved away from the player. No animated state is
// read by terrain, scenery, water or sky generation in this renderer.
#[no_mangle]
pub extern "C" fn c2_static_view_key(ptr: *mut core::ffi::c_void, out: *mut f32) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let (eye, yaw, pitch) = sim.hunt.camera();
    let key = [eye[0], eye[1], eye[2], yaw, pitch,
        (sim.hunt.player.x / 256.0) as i32 as f32,
        (sim.hunt.player.z / 256.0) as i32 as f32];
    unsafe { core::ptr::copy_nonoverlapping(key.as_ptr(), out, key.len()); }
}

// The original sky image for the time of day, converted to a linear RGBA GX2 texture.
#[no_mangle]
pub extern "C" fn c2_sky_texture(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let skies = &sim.area.rsc.skies;
    let sky = if skies[sim.area.time as usize].len() == 256 * 256 { &skies[sim.area.time as usize] } else { &skies[0] };
    if pitch < 256 || sky.len() != 256 * 256 { return -1; }
    for z in 0..256usize { for x in 0..256usize {
        let rgba = c2_core::color::rgb555_to_rgba8(sky[z * 256 + x], 255);
        unsafe { core::ptr::copy_nonoverlapping(rgba.as_ptr(), pixels.add((z * pitch as usize + x) * 4), 4); }
    }}
    unsafe { night_vision_rows(sim, pixels, pitch, 256, 256); }
    0
}

// Horizon/fog color for the time of day, from the original area resources.
#[no_mangle]
pub extern "C" fn c2_fog_color(ptr: *mut core::ffi::c_void, out: *mut f32) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    for (i, c) in sim.area.sky_rgb().iter().enumerate() {
        unsafe { *out.add(i) = (*c).clamp(0, 255) as f32 / 255.0; }
    }
}

// A single camera-following cloud plane avoids grid seams. The shader computes
// the distance fade per pixel, so extra subdivisions are unnecessary.
#[no_mangle]
pub extern "C" fn c2_sky_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, attribs: *mut f32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let (eye, a, b) = sim.hunt.camera();
    let (sa, ca) = (libm::sinf(a), libm::cosf(a));
    let (sb, cb) = (libm::sinf(b), libm::cosf(b));
    for (i, (x, z)) in [(-1.0,-1.0),(1.0,-1.0),(1.0,1.0),(-1.0,-1.0),(1.0,1.0),(-1.0,1.0)].iter().enumerate() {
        let wx = x * 1_000_000.0; let wz = z * 1_000_000.0; let wy = 32768.0;
        let side = wx * ca + wz * sa; let forward = wx * sa - wz * ca;
        let up = wy * cb + forward * sb; let depth = forward * cb - wy * sb;
        let pos = [side * 0.75, up * 1.333, 0.0, depth];
        let attr = [(eye[0] + wx) * 0.002 / 256.0, (eye[2] + wz) * 0.002 / 256.0, 1.0, 0.0];
        unsafe { core::ptr::copy_nonoverlapping(pos.as_ptr(), positions.add(i * 4), 4); core::ptr::copy_nonoverlapping(attr.as_ptr(), attribs.add(i * 4), 4); }
    }
    6
}

// Build only objects that become visible; keep their world-space faces grouped
// by map chunk. Evict least-recently-used chunks to bound cached mesh memory.
fn cache_scenery_object(sim: &mut Simulation, x: usize, z: usize, id: usize) -> CachedGeometry {
    let i = z * 1024 + x;
    let turns = ((sim.area.map.flags[i] >> 2) & 3) as u8;
    let ox = x as f32 * 256.0 + 128.0;
    let oy = sim.area.map.object_height[i] as f32 * 64.0;
    let oz = z as f32 * 256.0 + 128.0;
    let model = &sim.models[id];
    for (q, pos) in model.positions.iter().zip(sim.transformed.iter_mut()) {
        let (vx, vz) = match turns { 1 => (q[2], -q[0]), 2 => (-q[0], -q[2]), 3 => (-q[2], q[0]), _ => (q[0], q[2]) };
        *pos = [ox + vx, oy + q[1], oz + vz, 1.0];
    }
    let light = sim.object_lights[i];
    let mut positions = Vec::with_capacity(model.faces.len() * 3);
    let mut attrs = Vec::with_capacity(model.faces.len() * 3);
    for (face, face_uvs) in model.faces.iter().zip(&sim.model_uvs[id]) {
        for k in 0..3 {
            positions.push(sim.transformed[face.v[k] as usize]);
            attrs.push([face_uvs[k][0], face_uvs[k][1], light, 1.0]);
        }
    }
    CachedGeometry { positions, attrs }
}

fn emit_cached(vertices: &CachedGeometry, positions: *mut f32, attribs: *mut f32, n: &mut usize, capacity: usize) -> bool {
    let count = vertices.positions.len().min(capacity.saturating_sub(*n) / 3 * 3);
    unsafe {
        core::ptr::copy_nonoverlapping(vertices.positions.as_ptr().cast::<f32>(), positions.add(*n * 4), count * 4);
        core::ptr::copy_nonoverlapping(vertices.attrs.as_ptr().cast::<f32>(), attribs.add(*n * 4), count * 4);
    }
    *n += count;
    count == vertices.positions.len()
}

// Populate a bounded list of nearby static object triangles. Never emit a
// partial face if the supplied GPU buffers fill up.
#[no_mangle]
pub extern "C" fn c2_scenery_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, attribs: *mut f32, capacity: u32) -> u32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    let p = &sim.hunt.player; let (eye, a, b) = sim.hunt.camera();
    let (sa, ca) = (libm::sinf(a), libm::cosf(a));
    let (sb, cb) = (libm::sinf(b), libm::cosf(b));
    let cx = (p.x / 256.0) as i32; let cz = (p.z / 256.0) as i32;
    let mut n = 0usize;
    sim.scenery_candidates.update(&sim.area.map.objects, sim.models.len(), (cx, cz));
    for candidate in 0..sim.scenery_candidates.cells.len() {
            let i = sim.scenery_candidates.cells[candidate];
            let (x, z) = ((i % 1024) as i32, (i / 1024) as i32);
            let id = sim.area.map.objects[i] as usize;
            let ox = x as f32 * 256.0 + 128.0;
            let oy = sim.area.map.object_height[i] as f32 * 64.0;
            let oz = z as f32 * 256.0 + 128.0;
            if !visible_sphere(ox - eye[0], oy - eye[1], oz - eye[2], sim.model_radii[id], sa, ca, sb, cb) { continue; }
            let x = x as usize; let z = z as usize;
            let chunk_id = (z / CACHE_CHUNK_CELLS) * CACHE_CHUNKS_PER_SIDE + x / CACHE_CHUNK_CELLS;
            let slot = (z % CACHE_CHUNK_CELLS) * CACHE_CHUNK_CELLS + x % CACHE_CHUNK_CELLS;
            let missing = sim.chunks[chunk_id].as_ref().and_then(|c| c.objects[slot].as_ref()).is_none();
            if missing {
                let vertices = cache_scenery_object(sim, x, z, id);
                let bytes = (vertices.positions.capacity() + vertices.attrs.capacity()) * core::mem::size_of::<[f32; 4]>();
                if bytes > CACHE_LIMIT {
                    if !emit_cached(&vertices, positions, attribs, &mut n, capacity as usize) { return n as u32; }
                    continue;
                }
                while sim.cache_bytes + bytes > CACHE_LIMIT {
                    let victim = sim.chunks.iter().enumerate().filter_map(|(index, c)| c.as_ref().map(|v| (index, v.last_used))).min_by_key(|&(_, used)| used).map(|(index, _)| index);
                    let Some(victim) = victim else { break; };
                    if let Some(old) = sim.chunks[victim].take() { sim.cache_bytes -= old.bytes; }
                }
                let chunk = sim.chunks[chunk_id].get_or_insert_with(SceneryChunk::new);
                chunk.bytes += bytes;
                sim.cache_bytes += bytes;
                chunk.objects[slot] = Some(vertices);
            }
            sim.cache_clock = sim.cache_clock.wrapping_add(1);
            let chunk = sim.chunks[chunk_id].as_mut().unwrap();
            chunk.last_used = sim.cache_clock;
            if !emit_cached(chunk.objects[slot].as_ref().unwrap(), positions, attribs, &mut n, capacity as usize) { return n as u32; }
    }
    n as u32
}

// Keep nearby scenery in all directions resident, updating it when the player
// changes cell (see ResidentScenery). `reset` forces a full rebuild (the legacy
// path shares these buffers). The vertex ranges written go to `written` for the
// cache flush. Returns the vertices written, or -1 if the window cannot fit (or
// `capacity` ranges are too few); C then keeps the legacy visible-stream path.
// The existing 1,048,576-vertex GPU streams are reused, not extra resident copies.
#[no_mangle]
pub extern "C" fn c2_scenery_resident_prepare(ptr: *mut core::ffi::c_void, reset: i32,
    positions: *mut f32, attrs: *mut f32, capacity: u32,
    written: *mut static_scene::DrawRange, written_capacity: u32, written_count: *mut u32) -> i32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    if reset != 0 { sim.resident_scenery.invalidate(); }
    let cell = ((sim.hunt.player.x / 256.0) as i32, (sim.hunt.player.z / 256.0) as i32);
    unsafe { *written_count = 0; }
    let Simulation { area, models, model_uvs, model_radii, object_lights, transformed, scenery_candidates, resident_scenery, scenery_written, .. } = sim;
    let updated = resident_scenery.update(scenery_candidates, &area.map.objects, models.len(), cell, capacity,
        |i| models[area.map.objects[i] as usize].faces.len(),
        |i, n| {
            let id = area.map.objects[i] as usize;
            let center = [(i%1024) as f32*256.0+128.0,
                area.map.object_height[i] as f32*64.0, (i/1024) as f32*256.0+128.0];
            let count = unsafe { static_scene::emit_scenery_model(&models[id], &model_uvs[id],
                object_lights[i], center, ((area.map.flags[i] >> 2) & 3) as u8,
                transformed, positions.add(n as usize*4), attrs.add(n as usize*4)) };
            static_scene::ResidentObject { center, radius: model_radii[id],
                range: static_scene::DrawRange { first: n, count } }
        }, scenery_written);
    if updated.is_none() || scenery_written.len() > written_capacity as usize { resident_scenery.invalidate(); return -1; }
    unsafe {
        core::ptr::copy_nonoverlapping(scenery_written.as_ptr(), written, scenery_written.len());
        *written_count = scenery_written.len() as u32;
    }
    scenery_written.iter().map(|r| r.count).sum::<u32>() as i32
}
#[no_mangle]
pub extern "C" fn c2_scenery_resident_ranges(ptr: *mut core::ffi::c_void,
    ranges: *mut static_scene::DrawRange, capacity: u32, vertices: *mut u32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if (capacity as usize) < sim.resident_scenery.objects.len() { unsafe { *vertices = 0; } return u32::MAX; }
    let out = unsafe { core::slice::from_raw_parts_mut(ranges, capacity as usize) };
    let (eye,yaw,pitch) = sim.hunt.camera();
    let (count,total) = static_scene::resident_ranges(&sim.resident_scenery.objects,eye,yaw,pitch,out);
    unsafe { *vertices = total; }
    count as u32
}

// Keep the terrain/water window in world space on the GPU, rewriting only the
// cells that entered it. `reset` forces a full rewrite (the legacy path shares
// these buffers). Written vertex ranges go to `written` for the cache flush;
// returns the cells rewritten, or u32::MAX if `capacity` ranges are too few.
#[no_mangle]
pub extern "C" fn c2_terrain_resident_update(ptr: *mut core::ffi::c_void, reset: i32,
    terrain_positions: *mut f32, terrain_attrs: *mut f32, water_positions: *mut f32, water_attrs: *mut f32,
    written: *mut static_scene::DrawRange, capacity: u32, written_count: *mut u32) -> u32 {
    let sim = unsafe { &mut *ptr.cast::<Simulation>() };
    if reset != 0 { sim.resident_terrain.invalidate(); }
    let cell = ((sim.hunt.player.x / 256.0) as i32, (sim.hunt.player.z / 256.0) as i32);
    let cells = unsafe { sim.resident_terrain.update(&sim.area, &sim.terrain_cache, cell,
        (terrain_positions, terrain_attrs), (water_positions, water_attrs), &mut sim.terrain_written) };
    if sim.terrain_written.len() > capacity as usize { unsafe { *written_count = 0; } return u32::MAX; }
    unsafe {
        core::ptr::copy_nonoverlapping(sim.terrain_written.as_ptr(), written, sim.terrain_written.len());
        *written_count = sim.terrain_written.len() as u32;
    }
    cells
}
#[no_mangle]
pub extern "C" fn c2_terrain_resident_ranges(ptr: *mut core::ffi::c_void,
    terrain: *mut static_scene::DrawRange, water: *mut static_scene::DrawRange, capacity: u32,
    terrain_count: *mut u32, terrain_vertices: *mut u32, water_count: *mut u32, water_vertices: *mut u32) {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if (capacity as usize) < static_scene::BLOCK_COUNT { unsafe { c2_move_panic() } }
    let terrain = unsafe { core::slice::from_raw_parts_mut(terrain, capacity as usize) };
    let water = unsafe { core::slice::from_raw_parts_mut(water, capacity as usize) };
    let (eye, yaw, pitch) = sim.hunt.camera();
    let (tn, tv, wn, wv) = sim.resident_terrain.ranges(eye, yaw, pitch, terrain, water);
    unsafe { *terrain_count = tn as u32; *terrain_vertices = tv; *water_count = wn as u32; *water_vertices = wv; }
}

// Emit water only when all four corners belong to water, matching the
// existing PC renderer. This first pass is opaque and has no animation.
#[no_mangle]
pub extern "C" fn c2_water_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, attribs: *mut f32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let p = &sim.hunt.player; let (eye, a, b) = sim.hunt.camera();
    let (sa, ca) = (libm::sinf(a), libm::cosf(a));
    let (sb, cb) = (libm::sinf(b), libm::cosf(b));
    let cx = (p.x / 256.0) as i32; let cz = (p.z / 256.0) as i32;
    let mut n = 0usize;
    for dz in -74i32..74 { for dx in -74i32..74 {
        let x = cx + dx; let z = cz + dz;
        if x < 0 || z < 0 || x >= 1023 || z >= 1023 { continue; }
        let i = z as usize * 1024 + x as usize;
        let w = sim.area.map.water[i] as usize;
        let Some(water) = sim.area.rsc.waters.get(w) else { continue; };
        if [i + 1, i + 1024, i + 1025].iter().any(|&j| sim.area.map.water[j] == c2_core::area::NO_WATER) { continue; }
        let Some(level) = sim.area.water_level(x as usize, z as usize) else { continue; };
        if !visible_sphere(x as f32 * 256.0 + 128.0 - eye[0], level - eye[1], z as f32 * 256.0 + 128.0 - eye[2], 182.0, sa, ca, sb, cb) { continue; }
        let tex = (water.texture_index.max(0) as usize).min(sim.area.rsc.textures.len().saturating_sub(1));
        for (ox, oz) in [(0,0),(1,0),(1,1),(0,0),(1,1),(0,1)] {
            let wx = (x + ox) as f32 * 256.0 - eye[0]; let wy = level - eye[1];
            let wz = (z + oz) as f32 * 256.0 - eye[2];
            let side = wx * ca + wz * sa; let forward = wx * sa - wz * ca;
            let up = wy * cb + forward * sb; let depth = forward * cb - wy * sb;
            let pos = [side * 0.75, up * 1.333, (32768.0 / (32768.0 - 16.0)) * (depth - 16.0), depth];
            let u = ((tex % 16) as f32 * 128.0 + 0.9465 + ox as f32 * 126.107) / 2048.0;
            let v = ((tex / 16) as f32 * 128.0 + 0.9465 + oz as f32 * 126.107) / 2048.0;
            let attr = [u, v, 1.0, 0.0];
            unsafe { core::ptr::copy_nonoverlapping(pos.as_ptr(), positions.add(n * 4), 4); core::ptr::copy_nonoverlapping(attr.as_ptr(), attribs.add(n * 4), 4); }
            n += 1;
        }
    }}
    n as u32
}

// A bounded diagnostic mesh, with GX2-compatible perspective depth. The C
// caller supplies capacity for 148*148*6 float4 vertices in each buffer.
#[no_mangle]
pub extern "C" fn c2_terrain_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, colors: *mut f32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let (eye, yaw, pitch) = sim.hunt.camera();
    let cell = ((sim.hunt.player.x / 256.0) as i32, (sim.hunt.player.z / 256.0) as i32);
    unsafe { sim.terrain_cache.emit(&sim.area, eye, yaw, pitch, cell, positions, colors) }
}

// ---- The menus (menu.rs), for C. The handle lives for the whole app, so the
// picks survive between hunts. `font` is the console's system font (shared,
// never freed) or null for the built-in bitmap font.
#[no_mangle]
pub extern "C" fn c2_menu_new(font: *const u8, len: u32) -> *mut core::ffi::c_void {
    let bytes: Option<&'static [u8]> = if font.is_null() || len == 0 { None } else { Some(unsafe { core::slice::from_raw_parts(font, len as usize) }) };
    Box::into_raw(Box::new(menu::Menu::new(ui::Font::new(bytes)))).cast()
}
fn menu_ref<'a>(ptr: *mut core::ffi::c_void) -> &'a mut menu::Menu { unsafe { &mut *ptr.cast::<menu::Menu>() } }
/// The name of menu file `slot` as a C string in `out`; -1 past the last.
#[no_mangle]
pub extern "C" fn c2_menu_file_name(slot: u32, out: *mut u8, capacity: u32) -> i32 {
    let Some(name) = menu::FILES.get(slot as usize) else { return -1; };
    if name.len() + 1 > capacity as usize { return -1; }
    unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), out, name.len()); *out.add(name.len()) = 0; }
    name.len() as i32
}
#[no_mangle]
pub extern "C" fn c2_menu_set_file(ptr: *mut core::ffi::c_void, slot: u32, data: *const u8, len: u32) -> i32 {
    if data.is_null() { return -1; }
    if menu_ref(ptr).set_file(slot as usize, unsafe { core::slice::from_raw_parts(data, len as usize) }) { 0 } else { -2 }
}
#[no_mangle]
pub extern "C" fn c2_menu_load_settings(ptr: *mut core::ffi::c_void, data: *const u8, len: u32) {
    if data.is_null() { return; }
    menu_ref(ptr).load_settings(&alloc::string::String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(data, len as usize) }));
}
/// The settings file's text in `out`; returns its length (0 if it does not fit).
#[no_mangle]
pub extern "C" fn c2_menu_settings(ptr: *mut core::ffi::c_void, out: *mut u8, capacity: u32) -> u32 {
    let text = menu_ref(ptr).settings_text();
    if text.len() > capacity as usize { return 0; }
    unsafe { core::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len()); }
    text.len() as u32
}
/// Open the title (death = 0) or the death screen with its shots and kills.
#[no_mangle]
pub extern "C" fn c2_menu_open(ptr: *mut core::ffi::c_void, death: u32, shots: u32, kills: u32) {
    let m = menu_ref(ptr);
    if death != 0 { m.open_death(shots, kills) } else { m.open_title() }
}
/// Button edges (menu.rs `button`); 1 if the menu changed.
#[no_mangle]
pub extern "C" fn c2_menu_input(ptr: *mut core::ffi::c_void, buttons: u32) -> u32 { menu_ref(ptr).input(buttons) as u32 }
/// A GamePad touch in its 854x480 pixels; 1 if it hit something.
#[no_mangle]
pub extern "C" fn c2_menu_touch(ptr: *mut core::ffi::c_void, x: f32, y: f32) -> u32 {
    menu_ref(ptr).touch(x * ui::WIDTH / 854.0, y * ui::HEIGHT / 480.0) as u32
}
#[no_mangle]
pub extern "C" fn c2_menu_action(ptr: *mut core::ffi::c_void) -> u32 { menu_ref(ptr).action as u32 }
#[no_mangle]
pub extern "C" fn c2_menu_choice(ptr: *mut core::ffi::c_void, out: *mut hunt_setup::HuntChoice) {
    unsafe { *out = menu_ref(ptr).choice; }
}
/// Draw the menu into an RGBA8 surface (`pitch` in pixels).
#[no_mangle]
pub extern "C" fn c2_menu_draw(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32, width: u32, height: u32) {
    let px = unsafe { core::slice::from_raw_parts_mut(pixels, pitch as usize * height as usize * 4) };
    menu_ref(ptr).draw(&mut ui::Canvas::new(px, pitch as usize, width as usize, height as usize));
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_move_panic() }
}
