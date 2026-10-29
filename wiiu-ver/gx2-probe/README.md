# Wii U GX2 graphics probe

Run `build/c2-wiiu-gx2-probe.rpx` in Cemu. It needs no SD files. The TV
should show a large multicolored triangle over a dark blue background. The
GamePad display should show the triangle over a purple background. The triangle
colors should cycle. In Cemu, use its GamePad-view control to inspect the other
screen if it is not already visible. Report whether each display appears,
whether the colors animate, and any crash or error. HOME exit and hardware
remain unverified.

Cemu result: both displays showed the triangle on their expected backgrounds;
the colors shifted and repeated. This confirms the basic presentation and
animation path in that emulator configuration.

Rust supplies the triangle's vertex positions and changing colors. WUT's GX2
APIs load the embedded GFD shader, upload the vertex buffers, and draw to TV
and GamePad. This checks only a minimal shader and presentation path; it does
not render the map, accept movement, test texture upload or depth, establish
GPU synchronization/performance, or validate real hardware. The graphics code
is adapted from devkitPro WUT's `gx2_triangle` sample and the bundled
`pos_col_shader.gsh` is unmodified from that sample. See [WUT-LICENSE.txt](WUT-LICENSE.txt) for the WUT license.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/gx2-probe/Cargo.toml \
  --target wiiu-ver/gx2-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/gx2-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_gx2_probe.a target/wiiu-gx2.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```
