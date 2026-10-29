# Wii U dual-screen map probe

Run `build/c2-wiiu-map-probe.rpx` in Cemu with your original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` in `wiiu/apps/carnivores-probe/` on the SD card,
just as for the scenery probe. Game data is not bundled.

The TV starts with the 3D view; the GamePad starts with a zoomed-in,
north-up map centered near the player. The red square marks the player.
Move with the left stick, look with the right stick, jump with A, toggle
sprint with B and crouch with D-pad up. Press R to toggle the full map on
the TV. Press minus to switch the GamePad between its zoomed map and the 3D
view; while in that gameplay mode, R also opens the full map on the GamePad.
This provides a temporary way to test GamePad-only play. The user reported a
positive Cemu visual result; HOME, actual hardware, GamePad input, and specific
screen-switching behavior remain unverified. The map borders are dark green.

Please check that both screens show the expected views, the red marker moves
with the player, R opens and closes the full map, and minus switches the
GamePad view. Note whether north/south and east/west movement line up with the
map, and report display corruption, lag, or crashes.

The map is a diagnostic visualization, not the finished game map: each cell
uses a single terrain-texture sample multiplied by its light value. It has no
labels, heading indicator, water-specific rendering, touch controls or
teleportation. Movement continues while the full map is open. It uses a 4 MiB
texture and currently updates its marker and invalidates that texture each
frame; this is not optimized or profiled. The scenery probe's draw-distance,
transparency and other limitations still apply. Screen modes and controls are
provisional until tested.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/map-probe/Cargo.toml \
  --target wiiu-ver/map-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/map-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

The precompiled shader and GX2 setup derive from the scenery probe; see its
[notes](../scenery-probe/README.md) and [WUT license](WUT-LICENSE.txt).
