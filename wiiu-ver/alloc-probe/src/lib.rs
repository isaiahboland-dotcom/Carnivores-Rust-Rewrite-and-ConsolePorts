#![no_std]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

extern "C" {
    fn c2_probe_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_probe_free(ptr: *mut u8);
    fn c2_probe_alloc_count() -> u32;
    fn c2_probe_free_count() -> u32;
    fn c2_probe_panic() -> !;
}

struct WutAllocator;

// The C shim delegates to newlib memalign/free. It must return null on failure
// and honor the requested alignment; Rust's default realloc copies and frees.
unsafe impl GlobalAlloc for WutAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_probe_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        c2_probe_free(ptr);
    }
}

#[global_allocator]
static ALLOCATOR: WutAllocator = WutAllocator;

#[repr(align(64))]
struct Aligned([u8; 64]);

// Bit 0: vector allocation/growth/content and matching frees.
// Bit 1: over-aligned allocation. Bits 2-4: shared TGA parser, writer,
// and truncated input handling. Bits are set only after each check passes.
#[no_mangle]
pub extern "C" fn c2_probe_run() -> u32 {
    let mut result = 0;
    let before_alloc = unsafe { c2_probe_alloc_count() };
    let before_free = unsafe { c2_probe_free_count() };
    {
        let mut bytes = Vec::new();
        for n in 0..257u32 {
            bytes.push((n % 251) as u8);
        }
        if bytes.len() == 257 && bytes[0] == 0 && bytes[250] == 250 && bytes[256] == 5 {
            result |= 1;
        }
    }
    if unsafe { c2_probe_alloc_count() } <= before_alloc
        || unsafe { c2_probe_free_count() } <= before_free
    {
        result &= !1;
    }
    {
        let before = unsafe { c2_probe_alloc_count() };
        let aligned = Box::new(Aligned([0x5a; 64]));
        let address = core::hint::black_box(&*aligned as *const Aligned as usize);
        if unsafe { c2_probe_alloc_count() } > before && address % 64 == 0 && aligned.0[63] == 0x5a
        {
            result |= 2;
        }
    }
    // A 2x2 bottom-up, 16-bit TGA. Distinct values make both byte order and
    // row order observable: on disk bottom row then top row, little endian.
    let fixture: [u8; 26] = [
        0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 16, 0, 0x34, 0x12, 0xcd, 0xab, 0x02, 0x01,
        0x78, 0x56,
    ];
    if let Ok(pic) = c2_formats::tga::parse(&fixture) {
        if pic.width == 2 && pic.height == 2 && pic.pixels == [0x0102, 0x5678, 0x1234, 0xabcd] {
            result |= 4;
        }
        if c2_formats::tga::write(&pic).as_slice() == fixture {
            result |= 8;
        }
    }
    if c2_formats::tga::parse(&fixture[..25]).is_err() {
        result |= 16;
    }
    result
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    unsafe { c2_probe_panic() }
}
