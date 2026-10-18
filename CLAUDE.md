Read `README.md` fully before doing anything — it is the spec, roadmap and status.
Follow its conventions (§8) and legal rules (§2). Record decisions in `docs/DECISIONS.md`.

Quick map:
- `pc-ver/`  — Cargo workspace: shared crates + PC backend (stable Rust).
- `3ds-ver/` — 3DS backend (nightly, separate workspace, reuses `pc-ver/crates/*`).
- `source/`  — gitignored local reference: `c2-original/` (game install), `c2-source/` (C++ source), `mods/`.
