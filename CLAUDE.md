Read before doing anything:
- `README.md`: what the project is, platform status, install.
- `docs/DEVELOPMENT.md`: the spec. Follow its conventions (§8) and legal rules (§2).
- `TODO.md` (gitignored, local only): the roadmap, milestone status and known issues. Update its Status as you go.

Record decisions in `docs/DECISIONS.md`. If the original source disagrees with the spec, the original wins:
note it in `DECISIONS.md`.

Quick map:
- `pc-ver/`  — Cargo workspace: shared crates + PC backend (stable Rust).
- `3ds-ver/` — 3DS backend (nightly, separate workspace, reuses `pc-ver/crates/*`).
- `wiiu-ver/` — Wii U probes and demo; `psvr2-ver/` — PS VR2 research.
- `source/`  — gitignored local reference: `c2-original/` (game install), `c2-source/` (C++ source), `mods/`.
