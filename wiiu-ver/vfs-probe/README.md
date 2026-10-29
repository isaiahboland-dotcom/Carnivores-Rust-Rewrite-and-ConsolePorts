# Wii U Rust VFS probe

Run `build/c2-wiiu-vfs-probe.rpx` in Cemu. It uses the same
`wiiu/apps/carnivores-probe/fixture.tga` on the emulated SD card as the
[SD probe](../sd-probe/README.md). On this machine the fixture was installed
in `~/.local/share/Cemu/sdcard` for the previous test. The adjacent
`missing.tga` file must not exist; the probe expects an open failure for it.

Expected PASS lines: SD mount, Rust VFS SD read, shared parser, SD missing-file
error, VFS missing-file error, and layer override. The last check also verifies
layer provenance. Report any failure, crash, or freeze and whether HOME exits.

The shared `c2-vfs` crate's optional `no_std` mode supplies the layered index,
case-insensitive lookup, memory layer, and platform error variant. The probe's
Rust backend requests a read through a C/WUT FS bridge and passes the result
to `c2-formats`. This backend deliberately indexes just two fixed test paths,
reads only 26 bytes, and assumes single-threaded use. It is not a production
SD backend, general directory scanner, pack reader, or concurrency test. The
standard PC directory, zip, and pack backends remain available with `std`.
Hardware, lifecycle, writes, and allocation failure behavior remain untested.

Build from the repository root with Rust nightly and the devkitPro image:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/vfs-probe/Cargo.toml \
  --target wiiu-ver/vfs-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/vfs-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_vfs_probe.a target/wiiu-vfs.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Cemu result: the user reported PASS for SD mount, Rust VFS SD read, shared
parser, SD missing-file error, VFS missing-file error, and layer override.
HOME exit could not be verified in their Cemu setup; hardware remains untested.
