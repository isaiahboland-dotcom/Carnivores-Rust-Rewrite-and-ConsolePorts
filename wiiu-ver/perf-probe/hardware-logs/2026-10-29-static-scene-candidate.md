# Combined static-scene candidate (no new hardware result)

The owner deferred additional console trips and requested further local
optimization. HOME-exit-v1 has not been run; its shutdown/resume fix remains
unverified. This candidate retains the fix and lifecycle trace.

Package: `build/carnivores-static-scene-v1.wuhb`, 450,976 bytes, SHA-256
`b1d87cb21f099740675852aea830fe8a56d6274558b7bde668b1be06e7f9c758`.
Copied to the standard SD app path, flushed with `sync -f`, and read-back hash
matched. Earlier WUHB candidates remain archived locally.

Schema 6; tiled sampling and exact unchanged-view reuse default on. Precomputed
terrain bounds add 8 MiB, rotated UVs add a small per-texture table. Each visible
cell projects four corners before emitting the original six vertices. Scenery
occupied-cell candidates retain the previous near-first order and rebuild on
player cell changes. No population, view-distance or resolution changes.

Verification passed:

- Rust powerpc release archive and Wii U C/RPX build.
- Original-data regression: bit-identical terrain vertex counts and every
  emitted position/UV/light float against the previous renderer, 100 views on
  all five stock maps (including map edges, pitch/rotation and landing points).
- Independent scenery candidate membership/order checks on the same five maps.
- Host ASan/UBSan static-view, map-view and lifecycle checks (LSan disabled in
  the ptrace sandbox); stream-range and schema-6 CSV/close/size-limit checks.
- `git diff --check`.

Hardware frame-time improvement, moving-view appearance, actual reuse rate,
HOME overlay/resume and system-menu exit remain to be checked in a later run.
D-pad Right toggles reuse (TC enabled / TB bypass); D-pad Left toggles atlas
layout. There is no need for an immediate hardware test to preserve this build.
