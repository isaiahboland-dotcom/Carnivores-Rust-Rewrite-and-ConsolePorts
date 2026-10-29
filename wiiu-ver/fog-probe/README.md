# Wii U AREA1 fog and sky probe

Run `build/c2-wiiu-fog-probe.rpx` in Cemu with the original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` in `wiiu/apps/carnivores-probe/` on the SD card.
Game data is not bundled.

This revision addresses the sharp blue band visible in the sky probe. It uses
AREA1's day sky fade color as the background, blends the cloud plane into that
color near the horizon, and fades distant terrain, scenery, and water into the
same color. The cloud plane is a single large quad to avoid grid seams. Sky fade is calculated per pixel using the PC renderer’s distance curve, to avoid the angular patches seen with the earlier per-vertex fade.
The terrain and scenery windows extend 74 cells, matching the PC hunt default;
fog and the mobile-map view circle use that range. Scenery has a 524,288-vertex
cap, so distant objects may be omitted in dense areas. Geometry is rebuilt on
the CPU every frame. Conservative horizontal-frustum culling reduces off-screen
work, but performance at this distance still needs measurement. Scenery now reaches full visibility closer to its draw limit. World fog remains a vertex-based approximation and may require refinement;
there is no claim of pixel-identical rendering or hardware performance.

Check the horizon while looking level and up, then turn and move toward distant
terrain and scenery. Please report whether the sky blends smoothly, whether
objects and terrain fade without popping, and whether there are seams,
flickering, unexpected colors, crashes or slowdown. The TV starts with the 3D
view; the GamePad shows the mobile-style map. R toggles the TV full map and
minus switches the GamePad between map and gameplay. Movement is left stick,
look is right stick, A jumps, B toggles sprint, and D-pad up toggles crouch.

The clouds remain static; sun, fog zones, water transparency and animation are
not yet implemented. The prior probe limitations still apply. This build needs
Cemu testing and has not been tested on real hardware.

Build from the repository root:

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler -vs wiiu-ver/fog-probe/terrain.vert \
  -ps wiiu-ver/fog-probe/terrain.frag -o wiiu-ver/fog-probe/terrain.gsh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/fog-probe/Cargo.toml \
  --target wiiu-ver/fog-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/fog-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

See the [sky probe](../sky-probe/README.md) and
[WUT license](WUT-LICENSE.txt) for inherited behavior and GX2 attribution.
