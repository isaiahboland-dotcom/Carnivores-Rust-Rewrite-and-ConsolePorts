# 3ds-ver — Nintendo 3DS backend

Starts at milestone M6, once the PC port (`../pc-ver`) is playable.

This will be a separate Cargo workspace (nightly, `armv6k-nintendo-3ds`) that
depends on the shared crates in `../pc-ver/crates/` by path, and loads only
data cooked by `c2-cook`. See `../docs/3DS.md`.
