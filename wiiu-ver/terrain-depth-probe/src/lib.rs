#![no_std]
extern crate alloc;
use alloc::{boxed::Box, vec::Vec};
use c2_core::{
    area::{Area, TimeOfDay},
    player::{Controls, Player, PlayerEvent},
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
    let sim = Simulation {
        area,
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
                let corners = if sim.area.map.flags[i] & 0x10 != 0 { reverse } else { normal };
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
                    let shade = 0.3 + light * 0.5;
                    let color = [0.22 * shade, 0.72 * shade, 0.20 * shade, 1.0];
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
