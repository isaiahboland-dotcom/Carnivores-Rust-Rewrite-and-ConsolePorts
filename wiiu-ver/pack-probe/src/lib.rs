#![no_std]
extern crate alloc;
use alloc::{boxed::Box, ffi::CString};
use c2_vfs::{PackSource, RangePackBackend, Vfs, VfsError};
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_char;
use core::panic::PanicInfo;
extern "C" {
    fn c2_pack_alloc(size: usize, align: usize) -> *mut u8;
    fn c2_pack_free(ptr: *mut u8);
    fn c2_pack_panic() -> !;
    fn c2_pack_size(path: *const c_char, size: *mut u32) -> i32;
    fn c2_pack_read(path: *const c_char, offset: u32, out: *mut u8, len: u32) -> i32;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        c2_pack_alloc(layout.size(), layout.align())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        c2_pack_free(ptr)
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

// The C bridge serializes operations. Mount, client setup, and teardown must
// surround all uses of this source. Only the probe creates an instance.
struct SdFile {
    path: CString,
    size: u32,
}
impl SdFile {
    fn open(path: &str) -> Result<Self, VfsError> {
        if path.split('/').any(|s| s == ".." || s.is_empty()) || path.contains('\\') {
            return Err(VfsError::PlatformIo {
                path: path.into(),
                code: -1002,
            });
        }
        let cpath = CString::new(path).map_err(|_| VfsError::PlatformIo {
            path: path.into(),
            code: -1002,
        })?;
        let mut size = 0;
        let status = unsafe { c2_pack_size(cpath.as_ptr(), &mut size) };
        if status != 0 {
            return Err(VfsError::PlatformIo {
                path: path.into(),
                code: status,
            });
        }
        Ok(Self { path: cpath, size })
    }
}
impl PackSource for SdFile {
    fn len(&self) -> u64 {
        self.size.into()
    }
    fn read_exact_at(&self, offset: u64, out: &mut [u8]) -> Result<(), VfsError> {
        let fail = |code| VfsError::PlatformIo {
            path: self.path.to_string_lossy().into_owned(),
            code,
        };
        let start = u32::try_from(offset).map_err(|_| fail(-1002))?;
        let len = u32::try_from(out.len()).map_err(|_| fail(-1002))?;
        if offset + u64::from(len) > u64::from(self.size) {
            return Err(fail(-1001));
        }
        let status = unsafe { c2_pack_read(self.path.as_ptr(), start, out.as_mut_ptr(), len) };
        if status != 0 {
            return Err(fail(status));
        }
        Ok(())
    }
}
#[no_mangle]
pub extern "C" fn c2_pack_run() -> u32 {
    let mut flags = 0;
    let source = match SdFile::open("wiiu/apps/carnivores-probe/fixture.c2pk") {
        Ok(s) => s,
        Err(_) => return flags,
    };
    flags |= 1;
    let pack = match RangePackBackend::open(source, "fixture.c2pk") {
        Ok(p) => p,
        Err(_) => return flags,
    };
    if pack.entries().len() == 2 {
        flags |= 2;
    }
    let mut vfs = Vfs::new();
    vfs.push("sd pack", Box::new(pack));
    if let Ok(bytes) = vfs.read("huntdat/menu/fixture.tga") {
        if let Ok(pic) = c2_formats::tga::parse(&bytes) {
            if pic.width == 2 && pic.height == 2 && pic.pixels == [0x0102, 0x5678, 0x1234, 0xabcd] {
                flags |= 4;
            }
        }
    }
    if let Ok(bytes) = vfs.read("data/pattern.bin") {
        if bytes.len() == 5000 && bytes.iter().enumerate().all(|(i, b)| *b == (i % 251) as u8) {
            flags |= 8;
        }
    }
    if matches!(vfs.read("no-such-entry"), Err(VfsError::NotFound { .. })) {
        flags |= 16;
    }
    if matches!(
        SdFile::open("wiiu/apps/carnivores-probe/no-such-pack.c2pk"),
        Err(VfsError::PlatformIo { code: -6, .. })
    ) {
        flags |= 32;
    }
    flags
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { c2_pack_panic() }
}
