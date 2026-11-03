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
failure/resource-lifetime audit remains open. Rendering measurements, cosmetic
module extraction and release publication are outside this security patch.
