# 3DS port

> Status: not started (milestones M6–M8). Notes collected so far.

## Toolchain
- devkitPro with devkitARM + libctru (`$DEVKITPRO=/opt/devkitpro`)
- Rust nightly with `rust-src`, target `armv6k-nintendo-3ds`, `cargo-3ds`
- Crates from the rust3ds org (`ctru-rs`, `citro3d-rs`)

## Verified so far
- `c2-formats` builds for the 3DS target:
  `cargo +nightly build -p c2-formats --target armv6k-nintendo-3ds -Z build-std=std,panic_abort`
  (run from `pc-ver/`). CI runs the same check.

## Memory budget
- Old 3DS ~64 MB app RAM, New 3DS ~124 MB (extended mode).
- A single vanilla area is ~14 MB `.MAP` + 5–13 MB `.RSC` raw, before
  characters. The cooker must downscale and drop unused time-of-day layers
  (only one of the three light maps / skies is needed per hunt).
- Real numbers go here once `c2-cook` emits budget reports.
