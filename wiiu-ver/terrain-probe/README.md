# Wii U AREA1 terrain probe

Run `build/c2-wiiu-terrain-probe.rpx` in Cemu with the same original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` SD files used by the movement probe, under
`wiiu/apps/carnivores-probe/`. The game files are not included. Loading starts
with the movement probe's text and checkpoint checks, then switches to graphics.

Both displays should show a blue background and a green, untextured section of
AREA1 around the first landing point. Use either GamePad or Pro Controller:
left stick moves, right stick looks, A jumps, B toggles sprint, and D-pad up
toggles crouch. Test movement and looking in both directions, including vertical
look, and report what appears on each screen and any flickering or failures.
HOME exit and physical hardware remain unverified.

Cemu result: the user reported that the terrain probe worked. Individual
display and control checks were not separately reported.

This is a diagnostic, not a finished renderer: the same image appears on both
screens, the terrain has simple light-based colors, and a 24-by-24-cell window
surrounds the player. It has no textures, objects, water, UI, or depth buffer;
far-to-near cell ordering is approximate, so overlapping slopes can look wrong.
The movement simulation advances by a fixed 16 ms per rendered frame, so its
speed is frame-rate dependent. GPU synchronization uses a conservative wait;
performance is not characterized.

The GX2 shader and drawing setup derive from the devkitPro WUT sample; see
[WUT-LICENSE.txt](WUT-LICENSE.txt). The AREA1 simulation is based on the
[movement probe](../movement-probe/README.md).

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/terrain-probe/Cargo.toml \
  --target wiiu-ver/terrain-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/terrain-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```
