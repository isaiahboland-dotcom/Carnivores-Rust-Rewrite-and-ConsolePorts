# Genesis vibrance investigation

Controlled native validation on 2026-11-11 separates the reported difference
into sky projection/fade, distance fog and object directional lighting.
Fixed-camera captures now reproduce these differences independently of the
original tour's camera mismatch. No normal-play rendering, color adjustment,
baseline or regression tolerance was changed. GENESIS-VIBRANCE remains open
for implementation and visual acceptance, rather than for initial validation.

## Controlled native results

The new `c2-pc scene-review` and `tools/scene-review-original.py` paths capture
the same 800×600 camera, vertical half-angle tangent 0.6, day, brightness 128,
view distance and clock. The native hook calls the shipped Genesis
`DrawScene` and `RenderSkyPlane`, after initialization and without simulation
between captures. It removes animals to match Rust's area viewer and verifies
camera/clock/radius/viewport/herd state after each pair. Executable instructions
and writable globals are checked before calling native code; unsupported
layouts fail. This probe currently supports the Genesis software executable,
not vanilla, Ice Age, OpenGL or D3D renderers.

Eighteen native scene/sky pairs cover four pitches, three headings, two clocks,
a repeated input, view distances 32/74/128, a full authored day lightmap, and
flat gray terrain at light levels 128 and 255. The scene and sky repeated input
is pixel-identical in each engine. MAP/RSC hashes and effective float32 camera
inputs agree for every compared pair. Rust uses GL/llvmpipe in these runs.

| Control | Result |
| --- | --- |
| Level sky, fixed clock | Mean RGB across rows 200–209: native `(34.4, 197.5, 238.8)`, Rust `(85.8, 208.1, 250.0)`. Rust reaches the pale fade color much earlier. Native executable inspection confirms the 0.15-radian sky projection offset; Rust has no offset. UV scale, scrolling and fade also differ, so tilt alone is not established as the complete fix. |
| Flat gray, no water/objects, light 128 | Near-ground sample `(66, 66, 66)` in both engines. At light 255 both produce `(132, 132, 132)`. This does not support a global brightness/gamma correction. |
| Same flat terrain, distance fog | At view distance 74 and rows 305–306, native full-light terrain averages `(124.3, 124.3, 123.9)`; Rust averages `(112.9, 201.5, 238.0)`. Rust adds strong blue distance fog where the software reference remains nearly neutral. The view-distance sweep reproduces the difference without water or texture detail. |
| Authored sand, near camera | Native mean `(159.2, 161.7, 166.1)`, Rust `(153.6, 156.8, 160.8)`. These averages include sampling/quantization differences and do not establish a separate global lighting scale defect. |
| Palm trunk, normal rendering | Native mean `(75.3, 75.1, 71.1)`, Rust `(57.0, 56.5, 53.5)`. |
| Palm, Rust object-vertex-light ablation | Disabling only diagnostic object vertex lighting gives `(76.2, 75.6, 71.5)`, with ground unchanged. This identifies directional vertex lighting as the principal cause of this object's darkness relative to the software reference. It does not validate all object types or weapon lighting. |

The software source selects a model fade table from scalar object light;
`Resources.cpp` gates `CalcLights` on `HARD3D`. Rust applies hardware-style
per-vertex directional light. The native ablation supports this renderer-path
distinction; removing that lighting globally would also change the intended
hardware-style path and requires validation against that renderer.

Sand samples use rectangle `(200,420)..(600,580)`; trunk samples use
`(90,100)..(105,320)` at the fixed landing camera. Flat-ground samples use
columns 350–449. These are descriptive measurements, not acceptance thresholds.
The actual Genesis scene also exposes a water/terrain discrepancy at the
horizon (native water versus a dark Rust strip); it must not be scored as fog
parity. Flat-terrain controls remove that confounder.

Private report: `pc-ver/(testing)/genesis-vibrance/controlled/report/index.html`.
It links native/Rust/difference images and the directional-light ablation;
`measurements.json` records numerical results. Recipes, native state buffers,
hashes, the analysis script and temporary assets stay under `controlled/`.
Native MAP/RSC work-copy files were restored after the lighting/flat controls.

Validation: release PC build and native hook build pass; 11 Python native
adapter tests pass. The PC library suite passes 56 tests (7 GPU tests skipped);
the diagnostic captures themselves exercise the GPU pipelines.

## Initial tour evidence

The reported shotgun screenshots have identical content and scenario hashes
and both select day, but are not a matched scene comparison. The original
runs `v_soft.ren` at 800×600; Rust runs Vulkan at 960×540. Camera position/pitch,
animal placement and weapon animation differ. Checkpoint names identify script
stages, not equivalent rendered states.

Fresh `standalone_map2_four_dinos` runs with the existing release binaries
passed for both engines at 800×600. The native shotgun image is pixel-identical
to the reported original. Rust still has the broad pale horizon band.
The fresh Rust capture uses GL/llvmpipe: reproduction on another backend,
not a controlled Vulkan/GL equivalence test. Both manifests identify build
`dc228c4715a8`, dirty, version 0.5.3. No engine rebuild was performed for this
documentation-only investigation.

Private evidence under `pc-ver/(testing)/genesis-vibrance/`:

- Native successful run: `native/standalone_map2_four_dinos/genesis-redux/original-wine/20261111T165018Z_dc228c47/`.
- Rust successful run: `rust/standalone_map2_four_dinos/genesis-redux/pc-linux/20261111T164919Z_dc228c47/` (15 screenshots).
- Measurements: `analysis/measurements.json` and `analysis/isolated-model-color-deltas.json`.
- Earlier native attempt `20261111T164909Z_dc228c47` failed to launch and provides
  no image evidence. Retrying with an absolute hook path succeeded.

In the clear upper-sky rectangle `(300, 0)..(600, 150)`, the most frequent RGB
is `(24, 198, 239)` in the native capture (26,104 pixels) and `(25, 197, 239)`
in Rust (15,547 pixels). Cloud phase/UVs are not matched, so this describes
the shared base color, not pixel parity. It does not support increasing
saturation throughout the image.

## Source review and initial hypotheses

| Subsystem | Evidence and limitation |
| --- | --- |
| Sky/fog color | AREA2.RSC day `FadeRGB` is `(111, 213, 255)`, a pale blue. Rust uses this authored color for the clear target and distance fog, distinct from the stronger cyan sky texture. |
| Sky projection | Reference `RenderSoft.cpp: RenderSkyPlane` and `renderd3d.cpp: RenderSkyPlane` use `CameraBeta - 0.15`. Rust `shaders/sky.wgsl` projects a level camera-relative plane without that offset. Controlled captures above validate the combined sky difference; a tilt-only ablation is still needed to quantify its individual contribution. |
| Sky fade | Rust uses `40240 / (40240 + max(distance - 100200, 0))`. Reference D3D uses camera-space depth and interpolated alpha at three screen rows. Reference software selects discrete `SkyFade` levels from projected texture span, with different UV scale and scrolling speed. The supplied executable is the software renderer; these are different reference paths. |
| Ambient/directional light | Fixed-day Rust ambient is 1.0. Terrain uses authored map lighting; models use scalar ground/vertex lighting derived from `CalcLights`. These affect local brightness without supplying a global blue directional-light tint. Shotgun/animal brightness still needs matched pose, location and light values. |
| Texture conversion | `c2-core/src/color.rs` expands RGB555 in RGB order using bit replication. `common.wgsl: grade_texel` applies the original brightness factor and five-bit quantization. Software dithering, sampling and quantization can still differ. |
| Gamma/color space | PC textures/offscreen output are `Rgba8Unorm`; windowed setup prefers non-sRGB. Shaders apply no gamma or saturation curve, consistent with lighting original encoded color values. Windowed fallback formats and display transforms were not measured. |
| Settings | Tour runners use fresh profiles, brightness 128 (factor 1), day and default options. Fresh runs match resolution, but do not synchronize cameras, cloud time or weapon animation. |

## Matched isolated-model cross-check

The existing local audit at
`regression-sim-recordings/audit-20261111T051738Z-92850f5e/texture-original-genesis/`
contains 252 valid captures against the shipped Genesis executable, with
matched serialized models, poses and camera recipes. Its executable hash
matches the reported gameplay reference. Neutral full lighting, brightness 1,
day and no fog isolate model rendering rather than reproduce hunt lighting.

The median of each capture's mean Rust-minus-native RGB difference is
approximately `(-0.066, -0.115, -0.151)` on the 0–255 scale. This calculation
intersects foreground masks: both images must differ from their recipe's
background by more than 12 in at least one channel. It does not align texels
or cancel rasterization/filtering differences and is descriptive, not a pass
criterion. **251 captures still fail the existing pixel-parity threshold.**
Those failures are not waived by the small mean color difference.

## Reproducing controlled captures

`c2-pc scene-review --manifest RECIPE.json --out NEW_DIR` accepts schema 1:

```json
{
  "schema": 1,
  "map": "PATH/TO/HUNTDAT/AREAS/AREA2.MAP",
  "captures": [
    {"camera": [133760, 4252, 52096, 0, 0], "clock_ms": 0, "radius": 74}
  ]
}
```

Camera entries are world X/Y/Z and Rust yaw/pitch in radians. Native alpha/beta
are their negatives. Optional `disable_object_vertex_light: true` is an
explicit diagnostic ablation, never a normal-play setting. Both terrain and
object animation use the fixed clock. Outputs require a new directory.

Build `original/build.sh HOOK_DIR`, then run:

```sh
python3 tools/scene-review-original.py --manifest RECIPE.json \
  --work DEDICATED_INITIALIZED_REGRESSION_WORK --hook-dir HOOK_DIR --out NEW_DIR
```

The work directory must contain the runner's copied `game/` and Wine `prefix/`,
not the owned installation. The adapter resets the regression profile and pins
software rendering. It requires AREA2 MAP/RSC hashes to match the Rust recipe;
it does not automatically replace game resources. Prior probe captures in the
work copy cause a refusal. Wine requires local socket/display access.

## Invisible water (2026-11-12)

In-game, Genesis AREA2 water is missing over large areas and the player walks
on visible seabed. The cause is the water alpha, not water coverage.

- **Coverage is correct.** `create_tmap` reproduces the `CreateTMap` 8-neighbour
  `fmWater2` shoreline expansion, and `vs_water` applies the `ProcessMapW`
  four-corner rule. No Genesis area has a `WMap` index outside its RSC water
  list. The only rejected water quads are the map's final row and column.
- **Alpha is wrong.** AREA2's single water record has `transp = 0.001`.
  Production uses `transp.clamp(0, 1)` as the final fragment alpha, so that
  sea is drawn at 0.1% opacity. Records with `transp >= 1` clamp to opaque,
  which hides the defect on vanilla-style maps.
- **Reference rule** (`Hunt2.cpp`, hardware path): `transp` is a depth
  coefficient. Per water vertex,
  `Alpha = ((wlevel - HMap) * 2 + 4) * transp + dist / 256 + wdelta * 2`,
  clamped at 0, then multiplied by `6 / (down + 0.1)`, where `down` is the
  downward component of the unit camera-to-ground-vertex direction, and
  clamped to 255. Rings at or beyond `ctViewR1 + 2` are opaque. Underwater
  views use `160 - dist * 160 / 220 / ctViewR`, minimum 10. The software
  renderer draws water opaque.

The sky/water candidate's `native-water-alpha` switch implements the
above-water depth/distance/incidence term. In the matched landing views it
restores the sea band where native shows water and production shows dark
seabed. A shallow-water probe (camera standing in the AREA2 sea, eye 220 units
above seabed) shows only seabed in production and a visible surface with the
switch. These probe captures used the Vulkan backend because the GL/llvmpipe path
currently fails to create a compute-capable context on the test machine.

Production PC rendering now applies this above-water term
(`shaders/terrain.wgsl: water_alpha`; see `DECISIONS.md`, 2026-11-12). Its
shallow-probe output matches the candidate switch except at the horizon,
where production keeps water distance fog.

Production PC rendering also applies the underwater alpha branch
(`DECISIONS.md`, 2026-11-15).

Not yet implemented in the candidate: the opaque far cutoff and the
underwater alpha branch. The vanilla cutoff is `ctViewR1 + 2` (28, clamped to
`ctViewR - 10`). The modernized MEE source that Genesis is built from has no
`ctViewR1` and tests `r < ctViewR + 2`, so no ring inside the view is forced
opaque there (see "Software coverage rules"). The formula is from source; no
native hardware-renderer capture has validated it.

## Software coverage rules (2026-11-14)

**The reference source for Genesis is the modernized MEE renderer, not the
vanilla `RenderSoft.cpp`.** The shipped `v_soft.ren` carries MEE 1.11 source
paths, a `SoftRenderer` class and a `ViewR = %d BMP at %d` status string with
no near-radius field; its view-distance routine takes two arguments and
clamps two values. The matching source is `Hunt/Renderer/SoftTerrain.cpp`,
`SoftWater.cpp`, `Game/Effects.cpp` and `Loaders/MapLoader.cpp` of the
modernized MEE tree. Identity of that source revision with the shipped
binary is not proven; it is supported by those strings and by the model
agreement below.

That renderer differs from vanilla where this task was looking:

- `RenderGround` walks every cell of every ring with `ProcessMap`. There is
  no `ctViewR1`, no 2-cell far ring (`ProcessMap2`/`ProcessMapW2` are never
  called) and therefore no far `GlassL` terrain fade.
- `ProcessMap` and `ProcessMapW` skip a cell whose diagonal midpoint is
  farther than `ctViewR * 256` from the camera: the drawn area is a disc,
  not the window's square.
- Projected vertices are whole pixels: `VideoC + (int)(v / z * Camera)`,
  truncated toward the screen centre. Rows `[y1, y3)` are filled.
- Ground triangles of an all-water cell are skipped when any vertex is below
  the water level; water triangles are skipped when any terrain vertex is
  above it. Software `CreateTMap` levels cardinal bank vertices to the water
  level, so both tests pass on the bank.
- Each water vertex takes the level of its own `WMap` record.
- Water vertices within `ctViewR - 4` rings get a three-component wave
  displacement in X, Y and Z.

A CPU model of these rules (coverage only: sky, ground or water per pixel; no
objects, texturing or waves) was scored against the native captures, with
object pixels excluded. Figures are the share of classified pixels whose
class differs from native, for the six distinct camera cases.

| Rule set | Mismatch |
| --- | --- |
| All rules above | 0.04–0.23% |
| Without software shore levelling | 0.04–2.10% |
| Square window instead of the disc | 0.04–0.94% |
| Rounded instead of truncated vertices | 0.04–0.65% |
| Vanilla near/far split, `ctViewR1` = 28 | 0.06–0.35% |
| Without either triangle skip rule | 0.04–0.23% (at most 16 pixels from the full rules) |
| Full rules, nearest surface wins instead of ring order | within 3 pixels of the full rules |

Conclusions:

- Shore levelling, the disc and truncated projection are each confirmed.
- The vanilla near/far split is not supported: it is no better on the sea
  views and worse where terrain forms the skyline.
- **The triangle skip rules are not observable in a depth-tested renderer**
  once the shore is levelled. They exist because the software renderer has
  no depth buffer. Porting them would add shader work for no measured change.
- Per-vertex water levels cannot be tested on Genesis: no area has a water
  quad whose corners reference records with different levels (AREA1–3, 5, 6
  checked after `CreateTMap`). The rule matters only for maps that do.

Two candidate switches implement the confirmed rules, `software-radius` and
`software-projection`. With the earlier scoped set (software sky, shore,
water and terrain), on Vulkan:

| Case | Baseline | Scoped | Scoped + both |
| --- | --- | --- | --- |
| Pitch −0.3 | 2.64% | 0.44% | 0.04% |
| Pitch 0 | 2.44% | 0.13% | 0.06% |
| Pitch 0.3 | 2.84% | 0.23% | 0.03% |
| Yaw π/2 | 1.10% | 0.43% | 0.18% |
| Yaw π | 3.92% | 1.31% | 0.71% |

**The one-scanline horizon residual is explained by these two rules
together.** On flat full-light terrain the first ground row matches native
in 762, 793 and 797 of 800 columns at view distances 32, 74 and 128 (238,
434 and 654 before); the remaining columns differ by one row. Either rule
alone does not reach this. No colour or view-distance adjustment is involved.

Observed and not addressed here:

- The yaw π view's remaining mismatch is objects. Native software draws
  distant palms unfogged and omits some objects that Rust draws (software
  `CreateTMap` removes `ofNOSOFT` objects and half of `ofNOSOFT2`).
- Native water shows isolated sky-coloured pixels, consistent with the
  unmodelled wave displacement.
- These captures use Vulkan. The earlier nine-variant report used
  GL/llvmpipe, which no longer creates a device on the test machine; the two
  are not mixed in any comparison above. Vulkan and GL baselines differ by a
  mean of 0.1–0.3 per channel.

The same modernized tree contains the OpenGL renderer (`Hunt/Renderer/GL*.cpp`),
the reference for any future "modern MEE style" rendering option.

Private evidence: `pc-ver/(testing)/sky-water-candidate-20261111/model/`
(model and scoring scripts) and `probe-20261114/` (captures, report,
`measurements.json`).

## Remaining implementation and acceptance

The software reference is now validated for sky, flat-terrain distance fog,
neutral terrain light levels and the sampled object's directional lighting.
Choose renderer fidelity explicitly when implementing a fix: Rust currently
mixes D3D-style sky/fog/object lighting with comparisons against software.
Native hardware-renderer captures remain necessary before changing behavior
claimed to match that path. The shipped Genesis folder provides `v_gl.ren`,
not a D3D executable; the new software ABI adapter cannot validate it.

Weapon lighting/animation, local fog volumes, underwater rendering and
dawn/night are not covered by this fixed-day environment probe. Add matched
weapon and local-fog cases before claiming their parity.

Test projection, UV/clock and fade independently while preserving authored
colors. Add synthetic projection/fade coverage with the selected fix, rerun
native comparisons, and verify dawn/night and other maps before closing this
task. Do not approve baselines or add global grading to compensate for an
unmatched scene.
