#![no_std]
extern crate alloc;
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
extern "C" {
    fn c2_sd_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_sd_free(ptr: *mut u8);
    fn c2_sd_panic() -> !;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_sd_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_sd_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[no_mangle]
pub extern "C" fn c2_sd_parse(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the C caller supplies a valid buffer of len bytes.
    let input = unsafe { core::slice::from_raw_parts(ptr, len) };
    match c2_formats::tga::parse(input) {
        Ok(picture)
            if picture.width == 2
                && picture.height == 2
                && picture.pixels == [0x0102, 0x5678, 0x1234, 0xabcd] =>
        {
            1
        }
        _ => 0,
    }
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_sd_panic() }
}
