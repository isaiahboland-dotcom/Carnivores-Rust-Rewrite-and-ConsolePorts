# Decisions and deviations

Newest first. Each entry: what, why, and what it affects.

## 2026-10-18 — M2 viewer

**Stack: `winit` 0.30 + `wgpu` 30** (owner's choice), plus `glam` (maths),
`bytemuck`, `pollster`. Versions picked at least two weeks old. No image crate:
screenshots use a ~60-line stored-deflate PNG writer.

**Colour maths stay in the original's space.** The original lights
(`D3DTBLEND_MODULATE` with a grey vertex colour) and fogs raw 16-bit colour
values. We use non-sRGB textures and surfaces so `texel * light` gives the same
numbers. The owner's saves use `OptBrightness = 128`, which makes
`BrightenTexture` a no-op, so it's skipped for now.

**Terrain is generated in the vertex shader** from a per-cell storage buffer
(height, light, water, texture, flags) instead of building meshes on the CPU.
Each cell is 6 vertices; triangulation follows `fmReverse`, and the 4
`TDirection` UV tables in `DrawTPlane` reduce to rotating the corner, which
is how the shader does it. The PC draws a square window around the camera;
the 3DS will need cooked meshes instead (M6).

**Load-time fixups live in `c2-core::area`** (`CreateTMap` hardware path,
`GetLandH`, `GetObjectH`, landing list, trophy ring), so the cooker can reuse them.

**Known approximations in the viewer** (to check against the retail game):
objects use the flat `_RenderObject` light without `RandomMap` jitter or
per-vertex `VLight`; `sfTransparent` faces blend at 50%; object quarter-turn
direction is unverified; no sky dome, sun/moon or distant billboards yet.

## 2026-10-17 — TGA/WAV: read the header instead of assuming it

The original TGA loader assumes 16-bit uncompressed bottom-up and ignores the
header; the WAV loader assumes 16-bit mono 22050 Hz and ignores `fmt `. For
every vanilla file our output is identical to the original's (checked by
`c2-parity scan`). For files in other encodings (seen in some mods: 24/32-bit,
RLE and ID-field TGAs, a stereo 16 kHz WAV) we decode correctly instead of
reproducing the original's garbage, and report the format so the cooker can
convert it. This only changes behaviour for files the original misread.

## 2026-10-17 — Add-on scope; MIT license

**The public build targets add-on mods**: small, data-only mods that layer
on vanilla and merge cleanly. Whole-game total conversions are out of scope.

**License: MIT** (`LICENSE`, `license = "MIT"` in the workspace). Covers our
code only; game and mod data are excluded.

## 2026-10-17 — M0/M1 kickoff

**Repository layout differs from README §3 (owner's request).**
- `pc-ver/` is the Cargo workspace for all shared crates plus the PC backend.
- `3ds-ver/` will hold the 3DS backend as its own workspace (nightly toolchain,
  `armv6k-nintendo-3ds` target) that depends on `../pc-ver/crates/*` by path.
  A separate workspace keeps the nightly pin and devkitPro requirements from
  leaking into the stable PC build.
- `source/` (gitignored) replaces `reference/`:
  - `source/c2-original/` — the user's Carnivores 2 **install** (README called
    this `reference/game/`).
  - `source/c2-source/` — original C++ source (README's
    `reference/c2-original/`), cloned from
    `github.com/videogamepreservation/carnivores2` (mirror of the 2013 Assembla release).
  - `source/mods/` — mod installs.

**ReShade is out of scope.** The user's install includes ReShade
(`reshade-shaders/`, `dxgi.dll/.ini`, `d3d9.dll/.ini`, `opengl32.dll/.ini`).
These are third-party post-processing wrappers, not game data; they are
ignored by every tool and will not be ported to PC or 3DS.

**The released C++ source is not the retail 1.0.4 build.** It has no menu
code (`Interface.cpp` is a fragment) and never reads the `prices` section that
every `_RES.TXT` contains. Where the source is missing, behaviour will be
matched against the retail `carn2.exe` by observation, and noted here.

**`_RES.TXT` keys are matched exactly, not by substring.** The original uses
`strstr(line, "ai")` etc., so any line containing the substring sets the field
— e.g. a character named `'Maiasaura'` would have `ai` reset to 0. We match
whole keys, plus the aliases the substring rule actually made work in shipped
data (`hearK`, `lookK`). Verified: `c2-parity` carries a faithful `strstr`
copy of the original reader and diffs every typed field; no `_RES.TXT` we
have produces any difference, and none would trigger the original's halt. Unknown keys are preserved (`extra`) so mod data survives merge/write.

**Parsers return on-disk values; transforms live in the engine.** Vertex
`*2`/z-flip, `BrightenTexture`, face re-sorting, `TraceC = trace - 1`, and
`_RES.TXT` defaults are applied by `c2-core`, not `c2-formats`. This is what
makes byte-exact round-tripping possible, which the cooker and mod merger need.

**Counts are bounded by input size, not engine limits.** The original has
fixed arrays (1024 vertices/faces, 256 objects, 64 animations…). We reject a
count only if the remaining bytes can't possibly hold it, so mods that exceed
the old limits still load and no malformed file can trigger a huge allocation.

**Synthetic fixtures are generated inside the tests** (`c2-formats/tests/`)
rather than by a separate tool in `tools/`. Same guarantee (no real game data
in the repo), less machinery. A standalone generator can be added if the
parity shim needs files on disk.

**Format parity is checked by byte-exact round-trip instead of a C++ JSON
shim (for now).** Building the original requires MSVC 6-era DirectX/Glide
SDKs. Parse-then-write reproducing every real file byte-for-byte, with the
parser consuming the whole file, is a strong check of the layouts. A shim that
compiles just the loader functions with modern GCC remains an option if a
field's *meaning* (rather than layout) needs confirming.

**License:** superseded — MIT, see the entry above.
