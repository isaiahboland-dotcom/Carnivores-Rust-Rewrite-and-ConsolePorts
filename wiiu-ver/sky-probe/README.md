# Wii U AREA1 sky probe

Run `build/c2-wiiu-sky-probe.rpx` in Cemu with the original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` in `wiiu/apps/carnivores-probe/` on the SD card.
Game data is not bundled.

This extends the mobile-map probe with a textured overhead sky plane using the
AREA1 day sky from the RSC. Look up with the right stick and turn around to
inspect it. The plane follows the camera and is drawn behind the world.

Check whether clouds appear above the landscape and remain behind terrain and
scenery as you move and look around. Please report missing sky, visual seams,
flashing, strange texture patterns, crashes, or noticeable slowdown. Confirm
that the GamePad map still shows the yellow circle and sector without the red
arrow. Use R for the full TV map and minus to switch the GamePad between the
map and gameplay. Movement is left stick; A jumps, B toggles sprint, and
D-pad up toggles crouch.

This is a first rendering pass, awaiting Cemu and hardware testing. It uses the
original sky texture and the PC renderer's plane height and texture scale, but
has no cloud scrolling, horizon fade, sun, time-of-day selection, or fog. The
plane's finite edges and clipping may be visible. Water remains opaque and
static, and inherited terrain, scenery, and map limitations still apply.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/sky-probe/Cargo.toml \
  --target wiiu-ver/sky-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/sky-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

See the [mobile map probe](../mobile-map-probe/README.md) and
[WUT license](WUT-LICENSE.txt) for inherited behavior and GX2 attribution.
