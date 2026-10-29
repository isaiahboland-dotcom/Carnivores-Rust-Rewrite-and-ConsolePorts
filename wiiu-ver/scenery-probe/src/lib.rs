#![no_std]
extern crate alloc;
use alloc::{boxed::Box, vec::Vec};
use c2_core::{
    area::{Area, TimeOfDay},
    player::{Controls, Player, PlayerEvent},
    model::PreparedModel,
};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
extern "C" {
    fn c2_move_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_move_free(ptr: *mut u8);
    fn c2_move_panic() -> !;
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
    models: Vec<PreparedModel>,
    player: Player,
    events: Vec<PlayerEvent>,
    sprint: bool,
    crouch: bool,
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
// C keeps both buffers alive for the call; this function parses owned data.
// Return codes distinguish map and resource parsing errors.
#[no_mangle]
pub extern "C" fn c2_move_create(
    map_ptr: *const u8,
    map_len: usize,
    rsc_ptr: *const u8,
    rsc_len: usize,
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
    let area = Area::new(map, rsc, TimeOfDay::Day);
    let cell = area.landings.first().copied().unwrap_or((256, 256));
    let player = Player::new(&area, cell);
    let models = area.rsc.objects.iter().map(|o| PreparedModel::from_mesh(&o.model.mesh)).collect();
    let sim = Simulation {
        area,
        models,
        player,
        events: Vec::new(),
        sprint: false,
        crouch: false,
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
    let mut player = sim.player.clone();
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
    sim.player.tick(&sim.area, &controls, 16, &mut sim.events);
    let p = &sim.player;
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

// Fill an 8x8 atlas using the original RSC's RGB555 textures. The C caller
// supplies a GX2 linear RGBA8 surface and its pitch, in pixels.
#[no_mangle]
pub extern "C" fn c2_terrain_atlas(ptr: *mut core::ffi::c_void, pixels: *mut u8, pitch: u32) -> i32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    if sim.area.rsc.textures.is_empty() || sim.area.rsc.textures.len() > 64 || pitch < 1024 { return -1; }
    for (index, texture) in sim.area.rsc.textures.iter().enumerate() {
        if texture.len() != 128 * 128 { return -2; }
        for y in 0..128usize {
            for x in 0..128usize {
                let pixel = c2_core::color::rgb555_to_rgba8(texture[y * 128 + x], 255);
                let dst = ((index / 8 * 128 + y) * pitch as usize + index % 8 * 128 + x) * 4;
                unsafe { core::ptr::copy_nonoverlapping(pixel.as_ptr(), pixels.add(dst), 4); }
            }
        }
    }
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
    0
}

// Populate a bounded list of nearby static object triangles. Never emit a
// partial face if the supplied GPU buffers fill up.
#[no_mangle]
pub extern "C" fn c2_scenery_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, attribs: *mut f32, capacity: u32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let p = &sim.player; let eye = p.eye();
    let (a, b) = p.camera_angles();
    let (sa, ca) = (libm::sinf(a), libm::cosf(a));
    let (sb, cb) = (libm::sinf(b), libm::cosf(b));
    let cx = (p.x / 256.0) as i32; let cz = (p.z / 256.0) as i32;
    let mut n = 0usize;
    // Near cells first so a capacity limit drops the most distant scenery.
    for ring in 0i32..=10 {
        for dz in -10i32..=10 { for dx in -10i32..=10 {
            if dx.abs().max(dz.abs()) != ring { continue; }
            let x = cx + dx; let z = cz + dz;
            if !(0..1024).contains(&x) || !(0..1024).contains(&z) { continue; }
            let i = z as usize * 1024 + x as usize;
            let id = sim.area.map.objects[i] as usize;
            let Some(model) = sim.models.get(id) else { continue; };
            let turns = ((sim.area.map.flags[i] >> 2) & 3) as u8;
            let ox = x as f32 * 256.0 + 128.0;
            let oy = sim.area.map.object_height[i] as f32 * 64.0;
            let oz = z as f32 * 256.0 + 128.0;
            let place = c2_core::area::Placement { cell: (x as u16, z as u16), model: id as u8, pos: [ox, oy, oz], quarter_turns: turns };
            let light = sim.area.object_light(&place);
            for face in &model.faces {
                if n + 3 > capacity as usize { return n as u32; }
                for k in 0..3 {
                    let q = model.positions[face.v[k] as usize];
                    let (vx, vz) = match turns { 1 => (q[2], -q[0]), 2 => (-q[0], -q[2]), 3 => (-q[2], q[0]), _ => (q[0], q[2]) };
                    let wx = ox + vx - eye[0]; let wy = oy + q[1] - eye[1]; let wz = oz + vz - eye[2];
                    let side = wx * ca + wz * sa; let forward = wx * sa - wz * ca;
                    let up = wy * cb + forward * sb; let depth = forward * cb - wy * sb;
                    let clip_z = (8192.0 / (8192.0 - 16.0)) * (depth - 16.0);
                    let pos = [side * 0.75, up * 1.333, clip_z, depth];
                    let u = ((id % 8) as f32 * 256.0 + (face.uv[k][0] * 256.0).clamp(0.0, 255.0) + 0.5) / 2048.0;
                    let v = ((id / 8) as f32 * 256.0 + (face.uv[k][1] * 256.0).clamp(0.0, 255.0) + 0.5) / 2048.0;
                    let attr = [u, v, light, 1.0];
                    unsafe { core::ptr::copy_nonoverlapping(pos.as_ptr(), positions.add(n * 4), 4); core::ptr::copy_nonoverlapping(attr.as_ptr(), attribs.add(n * 4), 4); }
                    n += 1;
                }
            }
        }}
    }
    n as u32
}

// A bounded diagnostic mesh, with GX2-compatible perspective depth. The C
// caller supplies capacity for 24*24*6 float4 vertices in each buffer.
#[no_mangle]
pub extern "C" fn c2_terrain_vertices(ptr: *mut core::ffi::c_void, positions: *mut f32, colors: *mut f32) -> u32 {
    let sim = unsafe { &*ptr.cast::<Simulation>() };
    let p = &sim.player;
    let eye = p.eye();
    let (a, b) = p.camera_angles();
    let (sa, ca) = (libm::sinf(a), libm::cosf(a));
    let (sb, cb) = (libm::sinf(b), libm::cosf(b));
    let cx = (p.x / 256.0) as i32;
    let cz = (p.z / 256.0) as i32;
    let mut n = 0usize;
    for ring in (0i32..=12).rev() {
        for dz in -12i32..12 {
            for dx in -12i32..12 {
                // Exactly one ring per cell based on its top-left corner.
                if dx.abs().max(dz.abs()) != ring { continue; }
                let x = cx + dx; let z = cz + dz;
                if x < 0 || z < 0 || x >= 1023 || z >= 1023 { continue; }
                let i = z as usize * 1024 + x as usize;
                let normal = [(0,0),(1,0),(1,1),(0,0),(1,1),(0,1)];
                let reverse = [(0,0),(1,0),(0,1),(0,1),(1,0),(1,1)];
                let flags = sim.area.map.flags[i];
                let corners = if flags & 0x10 != 0 { reverse } else { normal };
                let tex = (sim.area.map.texture1[i] as usize).min(sim.area.rsc.textures.len().saturating_sub(1));
                for (ox, oz) in corners {
                    let vi = (z + oz) as usize * 1024 + (x + ox) as usize;
                    let wx = (x + ox) as f32 * 256.0 - eye[0];
                    let wy = f32::from(sim.area.map.height[vi]) * 64.0 - eye[1];
                    let wz = (z + oz) as f32 * 256.0 - eye[2];
                    let side = wx * ca + wz * sa;
                    let forward = wx * sa - wz * ca;
                    let up = wy * cb + forward * sb;
                    let depth = forward * cb - wy * sb;
                    // GX2 clip depth is 0..w: near 16, far 8192 world units.
                    let clip_z = (8192.0 / (8192.0 - 16.0)) * (depth - 16.0);
                    let pos = [side * 0.75, up * 1.333, clip_z, depth];
                    let light = f32::from(sim.area.map.light[0][vi]) / 255.0;
                    let (u, v) = match flags & 3 {
                        1 => (oz, 1 - ox), 2 => (1 - ox, 1 - oz),
                        3 => (1 - oz, ox), _ => (ox, oz),
                    };
                    // Half-texel inset prevents neighboring atlas tiles bleeding.
                    let u = ((tex % 8) as f32 * 128.0 + 0.9465 + u as f32 * 126.107) / 1024.0;
                    let v = ((tex / 8) as f32 * 128.0 + 0.9465 + v as f32 * 126.107) / 1024.0;
                    let color = [u, v, light, 1.0];
                    unsafe { core::ptr::copy_nonoverlapping(pos.as_ptr(), positions.add(n*4), 4); core::ptr::copy_nonoverlapping(color.as_ptr(), colors.add(n*4), 4); }
                    n += 1;
                }
            }
        }
    }
    n as u32
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_move_panic() }
}
