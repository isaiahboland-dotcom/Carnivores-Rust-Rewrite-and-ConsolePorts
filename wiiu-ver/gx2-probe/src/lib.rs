#![no_std]
use core::panic::PanicInfo;
extern "C" {
    fn c2_gx2_panic() -> !;
}
// C provides two writable buffers of twelve floats each. Values are consumed
// as GX2 float4 vertex attributes after the buffer is unlocked.
#[no_mangle]
pub extern "C" fn c2_gx2_vertices(positions: *mut f32, colors: *mut f32, frame: u32) {
    let positions = unsafe { core::slice::from_raw_parts_mut(positions, 12) };
    let colors = unsafe { core::slice::from_raw_parts_mut(colors, 12) };
    positions.copy_from_slice(&[
        1.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, -1.0, -1.0, 1.0, 1.0,
    ]);
    let fade = (frame % 120) as f32 / 120.0;
    colors.copy_from_slice(&[
        1.0, fade, 0.0, 1.0, 0.0, 1.0, fade, 1.0, fade, 0.0, 1.0, 1.0,
    ]);
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_gx2_panic() }
}
