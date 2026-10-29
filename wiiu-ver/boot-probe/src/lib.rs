#![no_std]

use core::panic::PanicInfo;

// WUT calls this function to verify that the displayed message comes from Rust.
#[no_mangle]
pub extern "C" fn c2_probe_message() -> *const u8 {
    b"Carnivores Rust Wii U boot probe\0".as_ptr()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
