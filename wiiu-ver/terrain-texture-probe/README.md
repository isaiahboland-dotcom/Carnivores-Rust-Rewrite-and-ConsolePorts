# Wii U textured AREA1 terrain probe

Run `build/c2-wiiu-terrain-texture-probe.rpx` in Cemu with the original
Carnivores 2 `AREA1.MAP` and `AREA1.RSC` already used by the earlier probes,
under `wiiu/apps/carnivores-probe/` on the SD card. No game data is bundled.

Both displays should show the first map's textured ground. Move with the left
stick, look with the right stick, jump with A, toggle sprint with B, and toggle
crouch with D-pad up. Please check both screens and walk across several tiles:
look for recognizable ground, correct colors and orientation, seams, blank or
corrupted textures, missing ground, and crashes. The user reported that the
textured terrain works in Cemu. HOME and real hardware remain unverified.

The probe converts up to 64 original RGB555 terrain images into a 1024x1024
RGBA8 atlas at runtime, applies the map's texture indices and rotation flags,
and samples it with GX2. AREA1 in this installation has 58 textures, and all
its tile indices are in range. Atlas capacity is a probe limit, not a final
format limit. Terrain lighting, near/far clipping, the conservative GPU wait,
and fixed-step simulation retain the previous probe's approximations. Both
screens currently show the same view; objects, water, sky textures, fog and UI
are not implemented.

The shader sources `terrain.vert` and `terrain.frag` were compiled together
using [CafeGLSL v1.0.0-rc1](https://github.com/Exzap/CafeGLSL/releases/tag/v1.0.0-rc1)
for Linux x86-64. The downloaded compiler archive SHA-256 was
`fdbd561539080f1db74ec3f248edee4e1988d329a4d20f2273c72adf55015f97`.
The GX2 setup derives from the WUT example; see [WUT-LICENSE.txt](WUT-LICENSE.txt).

Build from the repository root (the compiled `terrain.gsh` is included; to
regenerate it, run the first command with the named compiler version):

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler \
  -vs wiiu-ver/terrain-texture-probe/terrain.vert \
  -ps wiiu-ver/terrain-texture-probe/terrain.frag \
  -o wiiu-ver/terrain-texture-probe/terrain.gsh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/terrain-texture-probe/Cargo.toml \
  --target wiiu-ver/terrain-texture-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/terrain-texture-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```
