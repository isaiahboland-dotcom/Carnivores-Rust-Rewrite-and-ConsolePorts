# Releasing

1. Choose a stable revision. Update the PC, 3DS and Android workspace versions and Android manifest
   versionName/versionCode together. Update local package versions in all four
   lockfiles, including `wiiu-ver/perf-probe/Cargo.lock`. Keep dependency versions unchanged
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
   remote tag target and release page. Attach the validated `windows.zip` and `linux.tar` desktop bundles to every
   release, including prereleases. Follow the bundle validation below.

Continue development under `Unreleased`; keep each release's notes and changelog
entry as its historical record.

## Desktop bundles

`release-bundles.yml` builds the exact release tag on Windows x64 (static MSVC
runtime) and Ubuntu 22.04 x64. It includes the launcher, PC/VR engines, cooker,
builder interfaces, tracked TOML recipes/base hashes, platform guides and a
SHA-256 manifest. It never copies `source/`, a whole build directory or private
recipe instances. Console payloads and third-party tools remain separate;
[the bundle guide](BUNDLE.md) lists their requirements.

Before publication, run **Release bundles** manually with the existing tag and
`attach` disabled. Download both workflow artifacts and validate their enclosed
archives. The workflow can also attach to an existing draft release with `attach`
enabled after validation. On publication it builds and attaches both archives;
the attachment job waits for both platforms to succeed. Uploads refuse to replace
existing assets. If validated archives are already attached, skip that redundant
job; never replace public binaries silently. Older releases require a deliberate
backfill from their own tagged revision; do not label current binaries as old ones.
A release created with `GITHUB_TOKEN` may not trigger another workflow: use the
manual workflow before publication in that case.

For each archive, record the tag/revision, archive SHA-256, OS version, and results
in the release notes and local validation queue:

1. Extract on a clean machine/VM with no checkout, Rust, Cargo, Visual Studio,
   devkitPro or remembered c2rs settings. Use an extraction path containing spaces.
2. Check the manifest hashes and inspect archive contents for assets/private files.
   On Windows inspect PE imports: no non-system runtime DLL may be missing.
   On Linux use `ldd` on all six binaries: no `not found`; verify the documented
   system runtime packages suffice. Graphics/audio drivers must work.
3. Start the launcher by double-click (Windows) or `./c2-launcher` (Linux), with a
   working directory outside the extracted folder. Confirm bundled recipes appear.
4. Create a New instance, select a user-supplied base folder, save and reopen it.
   Check a wrong base is rejected, then choose the matching base and press Play.
5. Register/select a hunter, enter a hunt, move/aim/fire, check sound, return to the
   menu and restart. Confirm settings/saves persist and the base is unchanged.
6. Exercise cooker/builders with separately supplied console applications/tools
   where advertised; distinguish unavailable optional tools from desktop failures.

Automated archive tests and build success do not establish fresh-machine startup
or playable-hunt acceptance. Do not mark RELEASE-BUNDLES complete without those
results for both systems. Once validated archives are publicly downloadable,
change README GUI installation to recommend the Releases page with `windows.zip`
and `linux.tar`, their runtime requirements and launch steps; retain manual build
instructions. Until then retain the current installation recommendation.

To package already-built host tools locally (Windows: use the target-specific
release directories and `--platform windows`):

```sh
python tools/release_bundle.py --platform linux \
  --pc pc-ver/target/release --cooker 3ds-ver/target/release \
  --output out/bundles
```
