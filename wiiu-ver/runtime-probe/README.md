# Wii U Rust runtime and ABI probe

This is a `no_std` runtime foundation using a Wii U-oriented PowerPC target,
Rust `core`, and WUT startup and lifecycle. It checks calls from WUT C into
Rust with integer and floating-point arguments and results, plus a simple
little-endian decode. Rust panics call WUT `OSFatal`. It does not test the
panic path, allocation, SD access, or Rust `std`. Passing these checks does not
validate every ABI feature, thread behavior, or real hardware.

Built with Rust nightly 1.101.0 (2026-10-16), `rust-src`, and the devkitPro
`devkitppc` Docker image `sha256:d50f8a886f2ce1d3c22685a774afb0845379876b47d9b41a9b729f4f63a54813`.
From the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/runtime-probe/Cargo.toml \
  --target wiiu-ver/runtime-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/runtime-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_runtime_probe.a target/wiiu-runtime.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Run `build/c2-wiiu-runtime-probe.rpx` in Cemu. All three checks should say
`PASS`. Note any `FAIL`, freeze or crash, and whether HOME exits; repeat on
hardware before treating the runtime/ABI gate as complete. This target is
experimental and is not yet a replacement for the shared engine's `std`.

The user confirmed all three checks displayed `PASS` in Cemu. HOME exit,
panic handling, and hardware execution have not been confirmed.
