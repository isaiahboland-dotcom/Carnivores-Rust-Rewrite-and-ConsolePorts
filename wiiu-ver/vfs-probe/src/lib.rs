#![no_std]
extern crate alloc;
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use c2_vfs::{Backend, MemBackend, Vfs, VfsError};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
extern "C" {
    fn c2_vfs_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_vfs_free(ptr: *mut u8);
    fn c2_vfs_panic() -> !;
    fn c2_vfs_read_fixture(which: u32, output: *mut u8) -> i32;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_vfs_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_vfs_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

// This fixed index is deliberately only a probe, not a general SD directory backend.
struct ProbeSd;
impl Backend for ProbeSd {
    fn files(&self) -> Vec<String> {
        vec!["Fixture.TGA".into(), "missing.tga".into()]
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let which = match path {
            "Fixture.TGA" => 0,
            "missing.tga" => 1,
            _ => return Err(VfsError::NotFound { path: path.into() }),
        };
        let mut bytes = vec![0; 26];
        // C copies at most 26 bytes to this live buffer and returns 0 or an error.
        let status = unsafe { c2_vfs_read_fixture(which, bytes.as_mut_ptr()) };
        if status != 0 {
            return Err(VfsError::PlatformIo {
                path: path.into(),
                code: status,
            });
        }
        Ok(bytes)
    }
}
#[no_mangle]
pub extern "C" fn c2_vfs_run() -> u32 {
    let mut flags = 0;
    let mut vfs = Vfs::new();
    vfs.push("sd", Box::new(ProbeSd));
    if let Ok(bytes) = vfs.read("./FIXTURE.tga") {
        flags |= 1;
        if let Ok(pic) = c2_formats::tga::parse(&bytes) {
            if pic.width == 2 && pic.height == 2 && pic.pixels == [0x0102, 0x5678, 0x1234, 0xabcd] {
                flags |= 2;
            }
        }
    }
    if matches!(
        vfs.read("missing.tga"),
        Err(VfsError::PlatformIo { code: -6, .. })
    ) {
        flags |= 4;
    }
    if matches!(vfs.read("absent.tga"), Err(VfsError::NotFound { .. })) {
        flags |= 8;
    }
    let mut overlay = MemBackend::new();
    overlay.insert("fixture.tga", b"override".to_vec());
    vfs.push("override", Box::new(overlay));
    if vfs.read("FIXTURE.TGA").ok().as_deref() == Some(b"override")
        && vfs
            .resolve("FIXTURE.TGA")
            .is_some_and(|r| r.layer == 1 && r.providers == [0, 1])
    {
        flags |= 16;
    }
    flags
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_vfs_panic() }
}
