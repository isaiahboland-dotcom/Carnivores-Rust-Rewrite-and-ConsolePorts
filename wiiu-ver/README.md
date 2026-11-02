# Wii U port

The playable port lives in [perf-probe](perf-probe/README.md). Its existing
name is retained because build commands, the launcher, and shared host tests
use that path. Build and install instructions are in the
[repository README](../README.md#wii-u-from-the-command-line-demo).

## Layout

- `perf-probe/`: current C/WUT frontend, Rust integration, shaders, and host tests.
- `tools/`: performance summary script and instructions for external SDK tools.
- `gameplan.md`: development priorities and validation requirements.
- `(testing)/`: ignored local backups, earlier probes, and investigation notes.

## Local archives and captures

Before cleanup, the complete Wii U directory (including ignored builds, tools,
screenshots, and hardware captures) was saved in
`(testing)/wiiu-ver-before-cleanup-2026-11-02.tar.gz`. The archive excludes
`(testing)` itself. Extract it into a separate directory to recover the original
layout; its top-level directory is `wiiu-ver/`.

The 19 earlier probes now live locally in `(testing)/historical-probes/`.
Their original relative paths and build caches refer to the old layout;
restore them from the archive before rebuilding. Original roadmap and runtime
investigation notes are in `(testing)/historical-notes/`. These local files
are not supplied by a fresh clone. Earlier source remains in Git history.

Hardware captures remain locally in `perf-probe/hardware-logs/`, ignored by
Git. Store future capture CSVs, startup/lifecycle traces, and session notes
there, or under `(testing)/`. Keep reusable technical conclusions in maintained
documentation. The [hardware review](../docs/WIIU-HARDWARE-REVIEW.md) records
historical findings; its original captures are local only.
