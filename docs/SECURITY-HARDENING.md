# Input and install hardening

Implemented from the security and data-preservation findings in the local
November 2026 review. This records a bounded hardening pass, not a claim that
all parsers, native libraries or GPU/runtime lifetimes have been audited.

## Archive policy

- Desktop ZIP VFS reads reject entries declaring more than **512 MiB** and
  independently enforce that limit on actual decoded bytes. Allocation does not
  trust the declared size. Decoder and CRC failures propagate as errors.
- ZIP VFS layers and launcher extraction allow at most **100,000 entries**.
  Launcher extraction also enforces **512 MiB per file** and **8 GiB total**
  actual expanded output, including a one-byte probe beyond each allowance.
- ZIP layers reject unsafe original paths before normalization. Launcher
  extraction rejects traversal, absolute paths, Windows drive/stream paths,
  reserved device names, symlinks and special files. Duplicate output files
  fail instead of overwriting an earlier entry.
- Pack materialization validates every backend's output paths before removing
  an existing pack. Directory backend requests also reject unsafe paths.
- Mobile and Ice Age imports validate metadata-derived output folder/file names
  before writing or removing anything. Titles cannot escape the import folder.
- Automatic RAR/7z extraction is disabled: external tools did not provide an
  enforceable actual-output budget. Extract them separately and add their
  folders, or use ZIP. This supersedes earlier launcher support notes.

These are desktop policies, with no unbounded override. A supported asset that
exceeds them needs an explicit compatibility-policy review. They limit logical
entry/output bytes, not total process memory, central-directory parsing cost,
CPU time, or cumulative memory across multiple loaded assets. Console memory
budgets remain separate.

## Generated installs and recovery

Launcher ZIP extraction, mobile add-on import, Ice Age import and APK-base
rebuilds generate output in a fresh directory under a reserved transaction:

```
<parent>/.c2rs-install-<destination-name>/data
<parent>/.c2rs-install-<destination-name>/previous
<parent>/<destination-name>
```

Build errors remove incomplete staged output and preserve the live destination.
An import with no compatible add-ons also preserves the old install. Publication
renames the old destination to `previous`, then the staged `data` to the live
name. If the second rename fails, it attempts to restore the old install. If
restoration fails, both the error and the retained backup path are reported.
Cleanup failures after successful publication are warnings with recovery paths.

The transaction name reserves a destination against simultaneous launcher
writers. Existing transactions block replacement; startup also reports them.
Parents and existing destinations must be real directories, with no symlink
ancestors. These helpers are used only for launcher-generated destinations in
app-owned storage. They assume other local processes cannot maliciously mutate
that storage while an install runs.

For interrupted transactions:

1. Close all launchers and games before recovery.
2. Preserve both `data` (new output) and `previous` (old install), when present.
3. If the live destination is missing and `previous` exists, restore `previous`
   to the live destination name. Keep `data` until its usefulness is determined.
4. If the live destination exists, verify it before discarding any backup.
   Remove the reserved transaction only after recovering/retaining its contents.

Recovery is deliberately manual: a transaction may belong to another running
launcher. Publication is a recoverable rename sequence, not a power-loss-safe
atomic exchange. No automatic deletion follows paths loaded from a record.

A central launcher guard checks running jobs and undispatched import, extraction,
APK, instance-save and recipe-worker completion. Worker-start routes and dropped
files respect it. Archive add-ons are registered only after successful extraction.
A launched PC game holds its operation until exit, so the launcher cannot rebuild
its inputs while it runs; waiting happens on the worker thread.

## Other corrected boundaries

The resolver uses an explicit DFS stack and ID lookup map, preserving input and
dependency order and cycle-error identity. A synthetic 20,000-node chain verifies
that mod metadata cannot exhaust the native recursion stack.

Area filenames use checked parsing and reject numbers outside `u32`; invalid
names no longer fall back to AREA0. The original game's valid numbering behavior
is retained.

VR mapped writes check offset/length before pointer arithmetic. Buffer capacity
is private and exposed through a read-only accessor. Raw buffer/image destruction
now has an unsafe API with explicit device, completion and single-destruction
obligations. The allocation-backed regression test exercises empty writes,
exact/end/beyond-end offsets and truncated in-range writes without a headset.
This does not establish all Vulkan/OpenXR ownership or synchronization invariants.

## Validation

Local Linux results: workspace tests **302 passed, 2 ignored, 0 failed**;
workspace Clippy and formatting passed; portable core compilation passed;
VFS without default features **4 passed**; host cooker **3 passed**; all **six**
Wii U host C programs passed. Ignored tests were not executed.

Commands (from `pc-ver/` unless specified):

- `cargo fmt --all --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace`
- `cargo check --locked -p c2-core --no-default-features --features portable-load,hunt-diagnostics`
- `cargo test --locked -p c2-vfs --no-default-features`
- From `3ds-ver/`: `cargo +stable test --locked -p c2-cook`
- Compile and execute every `wiiu-ver/perf-probe/tests/*_test.c` with
  `cc -std=c11 -Wall -Wextra -UNDEBUG ... -lm` (six programs).

Security fixtures are synthetic: forged ZIP size metadata, corrupt CRC,
metadata-independent output overrun, exact/over/total/count limits, traversal and
Windows aliases, duplicate output, rejected imports, rename/restore/cleanup
failures, restart discovery and blocked replacement, plus mapped buffer bounds.
The lifecycle C test stub was updated to supply the current `app_path` dependency.

CI now includes workspace Windows formatting/Clippy/tests (including VR), Linux
workspace checks, host cooker tests/build and Wii U C tests. Local Linux results
do not stand in for running Windows CI. Full 3DS/Wii U cross-build CI still needs
a validated reproducible toolchain environment; no container digest was invented.

Still requiring separate validation: GUI import/rebuild smoke tests with the
owner's assets, Windows execution, and real OpenXR startup, scene replacement,
session loss and shutdown with Vulkan diagnostics. The wider VR constructor
failure/resource-lifetime audit is recorded below (2026-11-15). Rendering measurements, cosmetic
module extraction and release publication are outside this security patch.

## 2026-11-03 audit follow-up

The follow-up was checked against the current source, including the hunt and
survival module extractions after v0.1.0. The local plan's patches were adjusted
where their assumptions differed from the implementation:

- `_RES.TXT` parsing rejects nesting beyond 64 levels below a section body with
  a syntax error and line number. The parser cap also bounds downstream walkers
  of parsed trees. The writer uses an explicit stack and borrows section bodies,
  avoiding recursive writing and subtree clones. Sub-block positions beyond the
  entry list are written at its end instead of silently omitted. Other public
  tree operations still assume callers construct reasonably shallow trees.
- Loose add-ons ignore zero-numbered menu art/call slot hints with a merge-report
  note, while retaining the files. Slot lookups/subtractions are checked; a bad
  hint does not erase an already discovered valid slot.
- PKM decoding validates ETC1 format, nonempty dimensions, 4x4 padding, the full
  encoded block payload and checked output size before allocating RGBA pixels.
- Desktop cooked-pack opening validates every payload range against the index
  boundary and file length, matching the portable backend.
- ELF section-table arithmetic and section scans are file-bounded. Character
  function scans use the bytes mapped at their virtual address, bounded by the
  section and symbol size; virtual addresses are not treated as file offsets.
- ZIP 9.0.5 already decodes legacy CP437 names. No second decoder was added.
  Regression fixtures verify listing, reading, root discovery and traversal
  rejection with non-UTF-8 names and valid CRCs.
- Texture tiling checks its documented dimension and RGBA-length preconditions
  in release builds as well as debug builds. File texture parsing remains
  fallible; `tile` is a programmer-contract API.

All workflow actions are pinned to full SHAs verified from their upstream refs,
with weekly GitHub Actions updates configured through Dependabot. The plan's
cache-action SHA was not used: it differed from the upstream ref. The Linux PR
suite now runs workspace tests with all features. A daily/manual Parity workflow
runs the same suite to compile and exercise the portable hunt/load tests too.

Fixtures remain synthetic and generated by committed test code, including the
portable hunt, mod merge and malformed-input cases. Tests requiring
`C2_ORIGINAL_HUNTDAT`, `C2_GAMES_DIR` or `C2_WIIU_INSTALLS` still return early
without original game data; ignored stress/manual tests are not run. These CI results
must not be described as original-asset or hardware parity. Do not populate
those variables with synthetic substitutes for original game files.

Follow-up validation on Linux: all-features workspace tests **345 passed,
6 ignored**, workspace Clippy with warnings denied and formatting passed;
no-default-feature format/VFS tests **70 + 4 passed**, portable-load core check
passed, host cooker **3 passed**, and all **six** Wii U C tests passed. Workflow
YAML and full-SHA pins were checked locally. Windows execution, hosted workflow
runs and console hardware were not revalidated in this pass.

## 2026-11-15 VR constructor-failure and session-lifetime audit

Scope: `c2-vr` from OpenXR instance creation to device destruction: every
Vulkan object made by `xr`, `gpu` and the scenes (test scene, terrain,
objects, sky, animals, menu panel), on success, on a constructor error, on a
scene swap (`Next::Load`) and on a frame-loop error.

Corrected:

- **Hang on exit after a frame error.** The frame loop reset a slot's fence
  before recording; an error from the scene's `record`, `wait_image`, or the
  submit left that fence unsignalled, and the exit path's
  `wait_for_fences(u64::MAX)` never returned. The fence is now reset
  immediately before the submit that signals it, and the exit path waits
  with `device_wait_idle`, which a failed submit cannot block.
- **Objects leaked by failing constructors.** Each constructor returned on
  its first error without destroying what it had already made, so a later
  `vkDestroyDevice` ran with live child objects (a Vulkan valid-usage
  violation). A `Cleanup` guard in `gpu.rs` now records each object as it is
  made and destroys them newest-first unless the constructor reaches
  `keep()`; temporaries (shader modules, staging buffers) use a guard that is
  never kept. This covers `Gpu::buffer`/`image`/`texture`, the shared model
  set and pipelines, every scene constructor, the frame loop's command pool
  and fences, and the depth targets. Composite scenes destroy completed parts
  when a later part fails (terrain objects when the sky fails, the terrain
  when the menu panel or the animals fail).
- **Vulkan instance leaked** when no GPU, no graphics queue or no device
  could be obtained after the runtime made the instance.
- **Half-made framebuffer pair leaked** when the second eye's view or
  framebuffer failed; the pair is now listed before it is filled.
- **Exit request refused.** A failed `xrRequestExitSession` was ignored, so
  a `--seconds` run could loop with nothing left to end it; the loop now
  ends directly.

Reviewed and unchanged:

- Teardown order: OpenXR swapchain, spaces, actions and session drop before
  the device is idled and the scene's and targets' objects are destroyed;
  the device and then the Vulkan instance go last, before the OpenXR
  instance.
- Scene swap: all frame fences are waited before the old scene and its
  framebuffers are destroyed. If the new scene fails to build, the error
  ends the session (the old scene is already gone); data-loading errors are
  caught earlier, before the swap, and keep the current scene.
- `LOSS_PENDING` and `InstanceLossPending` end the run rather than
  recreating the session. Reconnecting after session loss is not supported
  in the draft.
- An error between `xrBeginFrame` and `xrEndFrame` ends the session without
  ending the frame; destroying the session is valid in any state.
- `Animals::fit` replaces a slot's buffer only after that slot's fence was
  waited.

Validation: `c2-vr` Clippy with warnings denied and its 16 tests pass. An
ignored test (`gpu::tests::constructors_and_cleanup_on_a_device`) builds the
test scene and menu panel and drops a guard without `keep` on a plain Vulkan
device; it passed locally on a desktop GPU without validation layers. Still
pending: a headset run (startup, scene swaps, quit, runtime-initiated exit)
with `VK_LAYER_KHRONOS_validation` enabled, which reports leaked objects on
device destruction, and Windows.
