# Rust / WUT boot probe

Experimental integration spike. WUT handles startup, lifecycle and
console output; a `no_std` Rust static library supplies the displayed message.
This is not yet the game or a test of Rust `std`. The temporary built-in
PowerPC Linux target is used only to test simple C ABI linkage; its ABI and
generated code must be validated against WUT before extending this probe.

Requirements: devkitPro `wiiu-dev`, CMake, a Rust nightly with `rust-src`.
From this directory, with `DEVKITPRO` set to the devkitPro installation:

```sh
RUSTFLAGS='-C relocation-model=static' cargo +nightly build --release --target powerpc-unknown-linux-gnu -Z build-std=core,compiler_builtins
"$DEVKITPRO/devkitPPC/bin/powerpc-eabi-objcopy" --remove-section .note.GNU-stack target/powerpc-unknown-linux-gnu/release/libc2_wiiu_boot_probe.a target/wiiu-probe.a
"$DEVKITPRO/portlibs/wiiu/bin/powerpc-eabi-cmake" -S . -B build
cmake --build build
```

Find the resulting RPX in `build/` and run it using your homebrew environment
or an emulator. Confirm the Rust message appears and the HOME menu can exit.
Record toolchain versions and actual results before calling the boot gate
complete. On 2026-10-28 the archive built using nightly 1.101.0
(2026-10-16) and the RPX linked using `devkitpro/devkitppc` image digest
`sha256:d50f8a886f2ce1d3c22685a774afb0845379876b47d9b41a9b729f4f63a54813`
(GCC 16.2.0). The image can run the objcopy and CMake steps with this
directory mounted at `/src`. The user confirmed seeing the Rust message on a
red screen in Cemu; the test otherwise intentionally stays on that screen. HOME exit
and real hardware remain unverified. The current source has an extra success
message that was added after the RPX was built; rebuild to include it. Static relocation
avoids unsupported RPX relocations; removing GNU stack notes avoids an RPX
conversion error. Neither workaround establishes general target ABI support.
