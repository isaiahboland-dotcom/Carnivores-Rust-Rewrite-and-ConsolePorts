# Releasing

1. Choose a stable revision. Update both workspace versions (`pc-ver/Cargo.toml`
   and `3ds-ver/Cargo.toml`) and local package versions in all three lockfiles,
   including `wiiu-ver/perf-probe/Cargo.lock`. Keep dependency versions unchanged
   unless an update is part of the release.
2. Add a dated entry to the root `CHANGELOG.md`, notes in
   `docs/releases/vX.Y.Z.md`, and update `docs/ROADMAP.md` and README links/status.
   Describe platform evidence and unresolved limits explicitly.
3. Run PC workspace formatting, Clippy with warnings denied, and locked tests;
   portable core/VFS checks; stable host cooker tests; release builds for the PC,
   launcher and cooker; and the portable Wii U C tests. Commands are documented
   in CI and [SECURITY-HARDENING.md](SECURITY-HARDENING.md).
4. Commit and push the release preparation. Require the CI and Windows workflows
   to pass for that exact revision. Review any untested hardware/runtime paths in
   the notes. Never include game/mod assets, private logs or machine identifiers.
5. Create an annotated `vX.Y.Z` tag on the validated revision, push the tag, then
   publish GitHub release notes from the versioned Markdown file. Verify the
   remote tag target and release page. Attach only distributable, validated
   artifacts; a source release need not include platform binaries.

Continue development under `Unreleased`; keep each release's notes and changelog
entry as its historical record.
