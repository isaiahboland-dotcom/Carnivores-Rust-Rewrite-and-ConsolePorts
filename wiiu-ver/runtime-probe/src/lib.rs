#![no_std]

use core::panic::PanicInfo;

extern "C" {
    fn c2_runtime_panic() -> !;
}

// Keep the C interface to primitive values and pointers until wider ABI
// compatibility has been verified. Inputs are supplied by the C caller.
#[no_mangle]
pub extern "C" fn c2_runtime_integer(a: u32, b: i32) -> i32 {
    (a as i32).wrapping_mul(3).wrapping_add(b)
}

#[no_mangle]
pub extern "C" fn c2_runtime_float(a: f32, b: f32) -> f32 {
    a * 2.0 + b
}

#[no_mangle]
pub extern "C" fn c2_runtime_decode(bytes: *const u8) -> u32 {
    // SAFETY: the C caller supplies a valid pointer to at least four bytes.
    let bytes = unsafe { core::slice::from_raw_parts(bytes, 4) };
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    unsafe { c2_runtime_panic() }
}
