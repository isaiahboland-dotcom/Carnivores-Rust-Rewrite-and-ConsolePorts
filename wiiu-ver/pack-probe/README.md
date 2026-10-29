# Wii U SD pack probe

Run `build/c2-wiiu-pack-probe.rpx` in Cemu. It reads the synthetic
`fixtures/fixture.c2pk`, installed at
`/home/trash/.local/share/Cemu/sdcard/wiiu/apps/carnivores-probe/fixture.c2pk`
on this machine. On other setups copy it to
`wiiu/apps/carnivores-probe/fixture.c2pk` on the emulated SD. Do not create a
`no-such-pack.c2pk` file in that directory: the probe expects it to be absent.

Expected PASS lines: SD mount, open SD pack, pack index, packed TGA,
multi-chunk read, missing entry, and missing pack. If opening or indexing
fails, the dependent checks will also fail. This probe reads but never writes
the SD card. HOME exit and real hardware still need separate verification.

The portable Rust range pack backend reads a C2PK header and index once, and
fetches entry payloads by offset on demand. The probe's SD file source uses a
WUT bridge to open, stat, seek and read arbitrary relative paths in chunks,
with a mutex guarding the shared FS client and aligned transfer buffer. The
fixture includes a synthetic TGA and a 5,000-byte pattern spanning two chunks.
The pack container is shared, but the 3DS PICA200 cooked texture format is
not a Wii U texture format. This probe does not yet establish a production SD
backend, resilience to card changes, memory budgets, lifecycle, multi-threaded
behavior, or real game-data compatibility.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/pack-probe/Cargo.toml \
  --target wiiu-ver/pack-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/pack-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_pack_probe.a target/wiiu-pack.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Cemu result: the user reported all seven checks passed. HOME exit and real
hardware remain unverified.
