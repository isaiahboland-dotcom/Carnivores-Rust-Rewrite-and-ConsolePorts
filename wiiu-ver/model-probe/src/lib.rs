#![no_std]
extern crate alloc;
use alloc::vec;
use c2_core::model::{car_anim_duration_ms, pose_car, vertex_lights, PreparedModel};
use c2_formats::{
    car::Animation,
    model::{Face, Mesh, Vertex},
};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
extern "C" {
    fn c2_model_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_model_free(ptr: *mut u8);
    fn c2_model_panic() -> !;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_model_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_model_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[no_mangle]
pub extern "C" fn c2_model_run() -> u32 {
    let mut flags = 0;
    let anim = Animation {
        name_raw: [0; 32],
        kps: 2,
        frames: 2,
        data: vec![0, 0, 0, 8, 16, 24],
    };
    if car_anim_duration_ms(&anim) == 1000
        && pose_car(&anim, 1, 0).as_deref() == Some(&[[0.0, 0.0, -0.0]][..])
    {
        flags |= 1;
    }
    if pose_car(&anim, 1, 250).as_deref() == Some(&[[0.25, 0.5, -0.75]][..])
        && pose_car(&anim, 1, 500).as_deref() == Some(&[[0.5, 1.0, -1.5]][..])
    {
        flags |= 2;
    }
    if pose_car(&anim, 1, 1000).as_deref() == Some(&[[0.0, 0.0, -0.0]][..])
        && pose_car(&anim, 2, 0).is_none()
    {
        flags |= 4;
    }
    let vertex = |x, y| Vertex {
        x,
        y,
        z: 0.0,
        owner: 0,
        hide: 0,
    };
    let mesh = Mesh {
        vertices: vec![
            vertex(0.0, 0.0),
            vertex(1.0, 0.0),
            vertex(0.0, 1.0),
            vertex(1.0, 1.0),
        ],
        faces: vec![Face {
            v: [0, 1, 2],
            tu: [0; 3],
            tv: [0; 3],
            flags: 0,
            dmask: 0,
            distant: 0,
            next: 0,
            group: 0,
            reserved: [0; 12],
        }],
        texture: vec![],
    };
    let prepared = PreparedModel::from_mesh(&mesh);
    if prepared.positions[3] == [2.0, 2.0, -0.0]
        && (prepared.radius() - core::f32::consts::SQRT_2 * 2.0).abs() < 1e-5
    {
        flags |= 8;
    }
    let lights = vertex_lights(&mesh, [0.0, 0.0, -1.0]);
    if lights[0]
        .iter()
        .zip([0.0, 64.0, 0.0, -64.0])
        .all(|(actual, expected)| (*actual - expected).abs() < 1e-4)
        && lights[3] == [0.0; 4]
    {
        flags |= 16;
    }
    flags
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_model_panic() }
}
