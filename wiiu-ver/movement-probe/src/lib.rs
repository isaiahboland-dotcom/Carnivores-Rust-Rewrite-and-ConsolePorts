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
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_move_panic() }
}
