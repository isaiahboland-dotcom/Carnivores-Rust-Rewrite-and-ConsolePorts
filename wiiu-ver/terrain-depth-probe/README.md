# Wii U terrain depth and timing probe

Run `build/c2-wiiu-terrain-depth-probe.rpx` in Cemu with the same original
`AREA1.MAP` and `AREA1.RSC` SD files used by the earlier movement and terrain
probes, under `wiiu/apps/carnivores-probe/`. Game data is not included.

The same green terrain and blue background should appear on both screens.
Use the left stick to move, right stick to look, A to jump, B to toggle sprint,
and D-pad up to toggle crouch. Walk and look around slopes, and check both
screens for missing terrain, incorrect overlaps, flickering, or crashes. Compare
movement with the earlier terrain probe if useful. The user reported that the
build looked solid in Cemu. HOME and real hardware remain unverified, and timing
has not been quantitatively measured.

This version uses WHB's separately cleared TV and GamePad depth buffers, with
a perspective depth range of 16 to 8192 world units. Movement runs in fixed
16 ms steps using elapsed system time; button toggles are retained until a
simulation step. Elapsed time is capped at 128 ms to avoid runaway catch-up
after a long pause, so time is deliberately dropped during longer stalls.
Performance and timing behavior still need measurement, and no textures,
objects, water or UI are present. Both screens currently show the same image.

The prior [terrain probe](../terrain-probe/README.md) remains available. GX2
shader and drawing code derive from the devkitPro WUT sample; see
[WUT-LICENSE.txt](WUT-LICENSE.txt).

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/terrain-depth-probe/Cargo.toml \
  --target wiiu-ver/terrain-depth-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/terrain-depth-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```
