# Wii U shared core probe

Run `build/c2-wiiu-core-probe.rpx` in Cemu. Expected PASS lines: MSVC
random sequence, unsigned random range, signed random range, RGB555
conversion, and image alpha conversion. There is no SD fixture. Report any
FAIL, crash, or freeze; HOME exit and hardware remain unverified.

The probe links the shared `c2-core` crate with default features disabled.
The portable subset has since grown to include player movement and area
queries; see the [movement probe](../movement-probe/README.md). The dinosaur
and full hunt simulation are not yet available in that configuration. The checks
compare deterministic checkpoints and synthetic color values with host
expectations. This does not test gameplay, floating-point simulation parity,
file loading, performance, or real hardware. The full PC core remains enabled
by default.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/core-probe/Cargo.toml \
  --target wiiu-ver/core-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/core-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_core_probe.a target/wiiu-core.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Cemu result: the user reported all five checks passed. HOME exit and real
hardware remain unverified.
