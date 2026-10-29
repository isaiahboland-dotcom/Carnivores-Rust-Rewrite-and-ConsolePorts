# Wii U AREA1 water and map probe

Run `build/c2-wiiu-water-probe.rpx` in Cemu with the original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` on the SD card in
`wiiu/apps/carnivores-probe/`. No game data is bundled.

This builds on the map probe with three testable changes: textured water
surfaces in the 3D view, blue water cells on the map, and a red directional
player marker. The map remains north-up. The map coloring is a diagnostic
visualization; the water surfaces use the area's water level and texture, and
the same four-water-corner rule used by the PC renderer. Water is currently
opaque and static, without transparency, animation or effects.

The TV starts on the 3D view and the GamePad on the zoomed map. R toggles the
full TV map. Minus switches the GamePad between the zoomed map and the 3D view;
when the GamePad is in gameplay mode, R also opens its full map. Movement is
left stick, look is right stick, A jumps, B toggles sprint and D-pad up toggles
crouch. Movement continues while the map is open.

For the base AREA1 files in this workspace, the nearest raw water cell to the
landing is approximately 51 cells away, mostly north and slightly east. Look
for blue on the zoomed map, sprint toward it, and inspect the water from the
shore. Please check the water's position and texture, whether it meets the
shore, and whether the marker points in the direction of travel when you turn
and move forward. Also check the R and minus display switches and report any
flickering, missing scenery, lag or crashes. The user confirmed water and the turning arrow in Cemu; shoreline detail, screen switching and
hardware remain unverified; the inherited scenery and map probe limitations apply.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/water-probe/Cargo.toml \
  --target wiiu-ver/water-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/water-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

The precompiled shader and GX2 setup derive from the scenery probe; see its
[notes](../scenery-probe/README.md) and [WUT license](WUT-LICENSE.txt).
