# Wii U model and animation probe

Run `build/c2-wiiu-model-probe.rpx` in Cemu. The expected PASS lines are
animation start, animation interpolation, animation wrap and invalid, model
transform and radius, and vertex lighting. No SD fixture is needed. Report
any FAIL, crash, or freeze. HOME exit and hardware remain unverified.

The probe executes the shared core's model preparation and animation code
using a synthetic mesh and two-frame animation. Radius and lighting use a
floating-point tolerance; animation checkpoints use exactly representable
values. The portable mode uses libm for square root and trigonometry, while
the default PC mode keeps its existing standard-library operations. Matching
these checkpoints does not prove general numerical parity, rendering,
player movement, dinosaur AI, or a complete hunt.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/model-probe/Cargo.toml \
  --target wiiu-ver/model-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/model-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_model_probe.a target/wiiu-model.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Cemu result: the user reported all five checks passed. HOME exit and real
hardware remain unverified.
