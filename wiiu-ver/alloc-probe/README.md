# Wii U allocation and shared parser probe

Run `build/c2-wiiu-alloc-probe.rpx` in Cemu. The screen should show five
`PASS` lines: allocation and growth, 64-byte alignment, TGA parse and endian,
TGA write round trip, and truncated TGA error. Report any `FAIL`, freeze,
crash, or panic, and whether HOME exits. The fixture is built in; no SD card
or game data is needed.

The Rust global allocator calls a small C shim around newlib `memalign` and
`free`. The probe checks real allocation and freeing, vector growth, and an
over-aligned allocation. The parser reads a synthetic 2x2, 16-bit TGA using
the shared `c2-formats` crate; the expected pixels check little-endian words
and bottom-up row order, and the writer must reproduce the input. The last
check verifies a truncated image returns an error. These tests do not establish
allocation failure handling, thread safety, SD access, full engine behavior,
math parity, or hardware compatibility. The panic handler calls `OSFatal` but
its behavior is not deliberately tested.

Built using Rust nightly 1.101.0 (2026-10-16), `rust-src`, and the devkitPro
`devkitppc` Docker image `sha256:d50f8a886f2ce1d3c22685a774afb0845379876b47d9b41a9b729f4f63a54813`.
From the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/alloc-probe/Cargo.toml \
  --target wiiu-ver/alloc-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/alloc-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_alloc_probe.a target/wiiu-alloc.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

The user confirmed all five checks passed in Cemu. HOME exit and hardware
execution remain unverified.
