# Wii U mobile-style map probe

Run `build/c2-wiiu-mobile-map-probe.rpx` in Cemu with the original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` in `wiiu/apps/carnivores-probe/` on the SD card.
Game data is not bundled.

The zoomed GamePad map and full map now show a yellow circle and a translucent
yellow forward-facing sector, following the mobile-style map in the 3DS port.
The mobile view does not draw the red directional arrow. The temporary radius is 12 map
cells, matching the axis extent of the current 24-by-24-cell terrain window;
the window is square, so this circle is only an approximation of what is
rendered. The actual camera field of view also differs from the 90-degree
sector. This is a probe, not the final view-distance setting or a settings menu.
A mobile-map option is planned for the eventual menu.

The TV starts in 3D, and the GamePad starts on the zoomed map. R toggles the
full map on the TV. Minus switches the GamePad between the 3D view and zoomed
map; R also opens the full map there when it is in gameplay mode. Left stick
moves, right stick looks, A jumps, B toggles sprint, and D-pad up toggles
crouch. Water remains opaque and static.

In Cemu, check that the yellow circle follows the player and the yellow sector
turns as you look around; try both the zoomed and full maps. Check for trails,
flickering or corruption while moving and turning, and report anything that
looks wrong. This build and hardware behavior remain untested.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/mobile-map-probe/Cargo.toml \
  --target wiiu-ver/mobile-map-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/mobile-map-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

See the [water probe](../water-probe/README.md) for inherited limitations and
[WUT license](WUT-LICENSE.txt) for the GX2 setup.
