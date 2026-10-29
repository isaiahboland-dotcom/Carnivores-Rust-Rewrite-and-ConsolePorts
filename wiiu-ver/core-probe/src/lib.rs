#![no_std]
extern crate alloc;
use c2_core::{color, rng::MsRand};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
extern "C" {
    fn c2_core_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_core_free(ptr: *mut u8);
    fn c2_core_panic() -> !;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_core_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_core_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[no_mangle]
pub extern "C" fn c2_core_run() -> u32 {
    let mut flags = 0;
    let mut random = MsRand::new(1);
    if [random.rand(), random.rand(), random.rand(), random.rand()] == [41, 18467, 6334, 26500] {
        flags |= 1;
    }
    let mut random = MsRand::new(7);
    if [random.r_rand(5), random.r_rand(5), random.r_rand(5)] == [1, 4, 5] {
        flags |= 2;
    }
    let mut random = MsRand::new(7);
    if [random.si_rand(3), random.si_rand(3), random.si_rand(3)] == [-3, 0, 0] {
        flags |= 4;
    }
    if color::rgb555_to_rgba8(0x7c1f, 0x80) == [255, 0, 255, 128] {
        flags |= 8;
    }
    if color::image_to_rgba8(&[0, 0x03e0, 0x801f], true).as_slice()
        == [0, 0, 0, 0, 0, 255, 0, 255, 0, 0, 255, 255]
    {
        flags |= 16;
    }
    flags
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_core_panic() }
}
