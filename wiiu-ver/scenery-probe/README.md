# Wii U AREA1 static scenery probe

Run `build/c2-wiiu-scenery-probe.rpx` in Cemu with your original Carnivores 2
`AREA1.MAP` and `AREA1.RSC` in `wiiu/apps/carnivores-probe/` on the SD card,
as with the prior probes. Game data is not bundled. Loading may take longer
while the object models and textures are prepared.

Textured terrain and nearby scenery should appear on both screens. Move with
left stick, look with right stick, jump with A, toggle sprint with B, and toggle
crouch with D-pad up. Look around and walk toward the scenery. Please report
whether scenery appears, whether its placement and textures look plausible,
whether foliage has solid rectangular patches, and any flickering, slowdown,
missing geometry, or crashes. In this installation the raw map has 46 valid
object cells within 10 cells of the first landing, although not all are visible
from the initial viewing direction. The user reported a positive Cemu result; specific placement and texture details, HOME and hardware remain unverified.

This first pass uses the shared model preparation, object placement heights,
quarter-turn rotations and per-object light. It uses a 2048x2048 RGBA object
atlas, zero-texel cutout on applicable model textures, and a 21x21-cell
neighborhood. The GPU vertex buffers cap output at 65,535 vertices, processing
near cells first; excess geometry is omitted. The probe allows at most 64 object
textures, draws model faces double-sided, and treats transparent faces as solid.
It does not yet animate objects, calculate per-vertex object light, draw distant
billboards, water, sky, or UI. The terrain window, frame-step cap, and other
limitations from the [textured terrain probe](../terrain-texture-probe/README.md)
remain. No performance or visual accuracy claims are established yet.

The shader is compiled with CafeGLSL v1.0.0-rc1; see the textured terrain
probe notes for download and checksum. GX2 setup derives from the WUT example;
see [WUT-LICENSE.txt](WUT-LICENSE.txt).

Build from the repository root:

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler \
  -vs wiiu-ver/scenery-probe/terrain.vert \
  -ps wiiu-ver/scenery-probe/terrain.frag \
  -o wiiu-ver/scenery-probe/terrain.gsh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/scenery-probe/Cargo.toml \
  --target wiiu-ver/scenery-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/scenery-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```
