# Wii U SD and Rust parser probe

Copy the supplied `sd-card/wiiu/apps/carnivores-probe/fixture.tga` to
`wiiu/apps/carnivores-probe/fixture.tga` on the emulated SD card, then run
`build/c2-wiiu-sd-probe.rpx` in Cemu. Do not put the fixture under the MLC
storage directory. Cemu's SD location depends on installation; see the [Cemu
1.17.4 release notes](https://wiki.cemu.info/wiki/Release_1.17.4) for the
standard `sdcard` folder mapping. On this machine the Cemu SD directory is
`/home/trash/.local/share/Cemu/sdcard`. The fixture was copied there for testing.

Expected results: SD mount, open fixture, read 26 bytes, Rust TGA parse, and
seek and read all show PASS. On an early error, later checks are skipped. The
numeric error is a WUT FS status. If opening fails, check the fixture's path
and Cemu's SD folder. Note any FAIL, hang or crash, and whether HOME exits.

This read-only probe uses WHB to mount the SD card, the WUT FS API to open,
read, seek, close and unmount, and a newlib allocator plus shared Rust TGA
parser on the bytes read. It does not test Rust-initiated file I/O, writes,
large files, game assets, hardware, or lifecycle after backgrounding.

Build from the repository root with Rust nightly and the devkitPro image:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/sd-probe/Cargo.toml \
  --target wiiu-ver/sd-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/sd-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_sd_probe.a target/wiiu-sd.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

First Cemu run: SD mount passed; open returned -6 (not found). The local
emulated SD directory was empty, and the fixture was subsequently copied into
the expected path. On rerun, the user confirmed mount, open, 26-byte read, Rust TGA parse,
and seek/read all passed in Cemu. HOME exit and hardware remain unverified.
