# Texture detail: Rust against the original Windows game

Why Rust renders less fine texture detail than the original executable
(software renderer, run under Wine), one variable at a time. Target: the
original Carnivores 2 Windows game. Raw captures stay under the gitignored
`regression-sim-recordings/`.

## Hypotheses (from the original source)

| # | Hypothesis | Original | Rust |
|---|---|---|---|
| 1 | Sampling | `RenderSoft.cpp` reads `lpTextureAddr` through the asm fillers: nearest texel, no filter. D3D and 3dfx paths are bilinear. | Linear sampler for ground, models and sky (`gpu.rs`); `WORLD_MIPS` is Android only, so desktop minifies bilinearly with no mips. |
| 2 | Mip selection | One smaller level (`DataB`/`lpTexture2`) by distance: 10 cells ground, 12 models. | Android: full chain, per-pixel GPU choice. Desktop: none. |
| 3 | Dither | `DitherHi` is random and used only when building shaded mips and sky fades; 3dfx uses 4x4 dither. No software framebuffer dither found (grep only). | None. |
| 4 | Lighting precision | Software: model fade table from a scalar object light. | Per-vertex directional light, unquantised. See `GENESIS-VIBRANCE.md`. |
| 5 | Source texels | `BrightenTexture`/`GenerateMipMap` work in 5 bits per channel. | RGB555 expanded to RGBA8 by bit replication. |

Limit: the only drivable native engine is the software one, which is
point-sampled by design, so part of any detail gap is deliberate.

## Results

### T0. Noise floor

Re-rendering the same Rust or native capture is pixel-identical (confirmed
earlier for scene and sky captures; see `REGRESSION.md`). Not re-run here.
Cross-backend Rust comparison is still to do.

### T1. Source texels carry the detail (hypothesis 5 ruled out)

`cargo run -p c2-formats --example dump_texels -- <HUNTDAT> <out-dir>`
dumps every model skin, ground tile and object skin as raw RGB555.
Metrics per texture: Laplacian variance of the 5-bit luma (opaque texels
only) and the fraction of spectral power above half Nyquist.

| Set | Count | Laplacian variance, median (p10-p90) | HF power fraction, median (p90) |
|---|---|---|---|
| Animal skins (`.CAR`) | 19 | 14.1 (7.7-19.0) | 0.07 (0.10) |
| Ground tiles (`.RSC`) | 349 | 19.0 (6.0-33.0) | 0.37 (0.54) |
| Object skins (`.RSC`) | 148 | 19.5 (3.0-36.2) | 0.13 (0.38) |

Ground tiles are noisy (most power is high-frequency); animal skins are
smoother with fine speckle. The source is not missing detail, so detail is
lost in sampling or minification.

Rendered result, 15 isolated-model captures (`native-repair-c2-20261111`),
foreground pixels only (mask eroded 3 px):

- Laplacian variance of luma: Rust is 0.34x native (p10-p90: 0.30-0.46x);
  medians 371 against 955.
- Mean signed luma difference: 0.05 (median). The gap is spatial, not
  global colour, consistent with the earlier audit.
- Median per-pixel absolute luma error: 6.9.

This matches hypothesis 1: bilinear filtering of a point-sampled reference.
T2 and T3 isolate it.

### T4. Mip content (hypothesis 2, content only)

`mips::half` against `GenerateMipMap` on random RGB555 texels
(`matches_original_generate_mip_map_within_one_level` in
`c2-pc/src/mips.rs`): worst difference 1 level per channel in 5-bit,
5 in 8-bit, mean 2.1 in 8-bit. The averaging is not a meaningful source
of the gap. (Clear-texel handling of `CreateMipMapMT` is covered by the
existing tests.)

### T2/T3. Filter and mip matrix (isolated models)

Diagnostic switches in `gpu.rs` (default behaviour unchanged):
`C2_DIAG_FILTER=nearest` point-samples ground and model textures;
`C2_DIAG_MIPS=0|1` forces the mip chain off or on. Each variant is a
`texture-review` run with `source/texture-review-original-c2.toml` (15 models,
both sides, 960x720), compared with the native captures over foreground pixels
(mask eroded 3 px, 30 captures):

| Filter | Mips | Laplacian variance vs native, median (p10-p90) | Abs luma error, median | Signed luma diff, median |
|---|---|---|---|---|
| linear (shipped desktop) | off | 0.34 (0.29-0.50) | 7.0 | 0.05 |
| linear | on | 0.28 (0.20-0.34) | 7.0 | 0.05 |
| nearest | off | 0.96 (0.94-0.99) | 7.1 | 0.29 |
| nearest | on | 0.92 (0.54-0.96) | 7.1 | 0.22 |

- Linear/no-mips reproduces the saved Rust captures, so the matrix is
  comparable with T1.
- Point sampling restores the detail (0.34 -> 0.96). Hypothesis 1 is the
  cause of the gap; hypothesis 5 stays ruled out.
- Mips on top of linear reduce detail further at this framing; with nearest
  they cost a little and widen the spread, since a GPU-chosen level differs
  from the original's single switch.
- Mean absolute luma error does not move (about 7). It is dominated by
  something other than sampling (edges, pose or light), so detail variance
  and pixel error must be judged separately. T6 covers lighting.

Not yet measured: the near / below / beyond-switch distance sweep. The review
runner fits the model to the frame, so a camera-distance option is needed
first. Nearest is a native-parity diagnostic, not a recommendation: the
original's point sampling shimmers in motion, which a still capture cannot
show.

### T6. Where the remaining pixel error sits

The native adapter accepts only neutral full-light, no-fog recipes, so
lighting is identical on both sides in these captures and cannot explain the
~7 luma error. Quantising light to the fade-table steps would change nothing
here; it needs a lit scene (the regression tours) to test.

Error split on the same 30 captures (median per capture, foreground pixels):

| Variant | Interior (eroded 3 px) | Edge band | Silhouette XOR | Interior, best +/-2 px shift | Interior, 5x5 box-blurred |
|---|---|---|---|---|---|
| linear, no mips | 7.0 | 11.8 | 2.8% | 6.1 | 2.7 |
| nearest, no mips | 7.1 | 12.4 | 2.8% | 6.2 | 2.9 |

- The interior error collapses once texel-scale detail is blurred away
  (7 -> 2.7), so colour and lighting agree; the difference is which texel
  lands on which pixel.
- A whole-pixel shift barely helps (6.1), so the offset is sub-pixel: sample
  position or UV rounding differs between the software filler and the GPU
  rasteriser. With high-frequency skins, neighbouring texels differ a lot, so
  even nearest sampling cannot match per-pixel. Treat ~7 as the floor for
  this comparison; detail variance (T2/T3) is the meaningful metric.
- Edges are worse (12) with a 2.8% silhouette mismatch: rasterisation rules
  and pose rounding, not texture content.

## Still to run

- T3 distance sweep, plus a single-switch variant using `mips::half` once.
- T5 gradient texture for native dither (expected none; do not add any).
- T6b light quantised to the fade-table steps, diagnostic only, in a lit scene (not the neutral-light review).
- Sub-pixel UV/sample-position offset test (shift model UVs by a fraction of a texel) to see whether the interior floor moves.
- T7 second native renderer, only if a D3D/OpenGL hook becomes possible.

## Classic texture filter (Video option)

`classic_filter` (Options > Video > "Classic texture filter", default off)
point-samples ground and model textures, as the original's software renderer.
It is read when an area loads, so a change applies from the next hunt. After
play-testing, the shimmer in motion was judged minimal. `C2_DIAG_FILTER=nearest`
still forces it for diagnostics. Mips stay a diagnostic (`C2_DIAG_MIPS`).
