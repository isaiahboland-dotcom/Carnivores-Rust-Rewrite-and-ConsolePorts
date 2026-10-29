# Wii U shared-code runtime investigation

2026-10-28, Rust nightly 1.101.0 (2026-10-16). The boot probe proves that a
small Rust `no_std` function can be called from WUT in Cemu; it does not prove
the Linux ABI or standard library works on Wii U.

## Checks performed

From the repository root, with the cached dependencies and `rust-src`:

```sh
cargo +nightly check --offline --manifest-path pc-ver/Cargo.toml \
  -p c2-formats -p c2-vfs -p c2-core \
  --target powerpc-unknown-linux-gnu -Z build-std=std,panic_abort
```

This succeeded, with an existing unused-dependency warning in `c2-core`.
It checks the shared crates against a 32-bit, big-endian Linux target; `check`
does not link or run them, and Linux system calls and ABI are not Wii U APIs.
It is not evidence that filesystem, allocation, synchronization, or parsing
work on the console.

The locally supplied `tools/wut/powerpc-none-eabi.json` fails to load on this
nightly because `target-c-int-width` and `target-pointer-width` are strings
where the compiler expects numbers. In a temporary copy, converting those two
values to numbers let compilation proceed, but `-Z build-std=std,panic_abort`
failed in `std` with missing Unix platform and libc definitions (195 errors).
This target declares a Unix family without providing a supported Unix OS
backend. This experiment neither changes the supplied repository nor rules out
another target specification or a Wii U-specific Rust runtime.

The supplied Rust WUT repository describes itself as `no_std`, not a full
implementation of Rust `std`. A new experimental target with `os = "none"`
builds Rust `core`, and a WUT-linked runtime/ABI probe now builds as an RPX;
see [runtime-probe](runtime-probe/README.md). The user confirmed all three
displayed checks pass in Cemu. Its panic handler, HOME exit and real hardware
remain unverified. This is a foundation, not a working `std` backend and not
yet a runtime capable of running the shared engine.

Attempting `cargo +nightly check --offline --manifest-path pc-ver/Cargo.toml
-p c2-formats --target wiiu-ver/runtime-probe/powerpc-wiiu.json -Z
json-target-spec -Z build-std=std,panic_abort` also fails: Rust `std` has no
matching allocator or I/O error backend for this target, and the default
threaded configuration lacks matching synchronization and thread-local
implementations. Simply dropping the obsolete Unix target family does not
provide a Wii U standard library. A real platform backend or shared-code
portability work is required; inventing an OS identity would not establish
that its APIs exist on Wii U.

## `no_std` and `alloc` investigation

`c2-formats` operates primarily on in-memory slices; its explicit `std` uses
are formatting, `BTreeMap`, iteration and string decoding, for which `core`
or `alloc` equivalents exist. It also relies on the standard prelude for
`Vec`, `String`, macros and traits; a `no_std` version needs explicit imports.
Its `thiserror` dependency enables `std` by default; a standalone experiment
with `thiserror` 2.0.21 and default features disabled compiled that dependency
for the target. Cargo feature unification in the full workspace still needs
checking. The PC default should remain unchanged.

For a feasibility spike, a temporary copy of the complete formats crate was
changed to `no_std`, given broad `alloc` imports, and its `std` paths replaced
with equivalent `core`/`alloc` paths. Nine math call sites (`round`, `powi`,
`sin_cos`, `floor`, `ceil`, and `sqrt`) then failed to compile, since those
methods are not available from `core` on this target. Replacing them in that
copy with cached `libm` 0.2.16 calls made `cargo +nightly check --offline`
pass with `-Z build-std=core,alloc,compiler_builtins` on the custom
`powerpc-wiiu` target. This is a type-check only: the temporary math changes
are not a reviewed semantic conversion, and it does not link or execute a
parser, validate numeric parity, provide an allocator, or prove engine support.
No shared project source was changed by this experiment.

The next practical step is an optional, properly reviewed `no_std` mode for
`c2-formats`, preserving its existing PC default, with explicit imports and a
deliberate math implementation. Check dependency feature unification and the
minimum supported Rust version, run the existing PC format tests in both
modes where possible, and compare math and parsing results on PowerPC once an
allocator is available. `c2-vfs` has filesystem paths, `std::io` and a mutex;
it will need a platform-neutral data-access boundary or suitable platform
implementations. `c2-core` and its dependencies also need a separate audit;
success for the parser alone does not imply the game can run.

## Shared formats implementation

`c2-formats` now defaults to its original `std` mode and supports
`--no-default-features` for `no_std` plus `alloc`. It uses `core`/`alloc`
imports and `libm` for the floating-point operations unavailable in `core`;
the default mode retains the original standard-library math operations.
Both modes passed all 43 existing host tests. The Wii U target successfully
passed the same `cargo +nightly check` described above with
`--no-default-features` and `-Z build-std=core,alloc,compiler_builtins`.
This target check does not link or execute code, and host tests do not prove
PowerPC math parity. The standalone dependency feature tree does not enable
`thiserror/std`; Cargo can unify features when other crates are added, so the
final Wii U dependency graph must be checked again. The cached `libm` 0.2.16
and `thiserror` 2.0.21 declare minimum Rust versions 1.63 and 1.77 respectively,
below the workspace's 1.88; a full build on 1.88 has not been verified.

A [linked Wii U probe](alloc-probe/README.md) now uses a newlib-backed Rust
allocator and exercises the shared TGA parser and writer with a synthetic
fixture. The user confirmed all five checks passed in Cemu. HOME exit and
hardware remain unverified. A separate [SD read and seek probe](sd-probe/README.md)
passed mount, open, read, Rust parsing, and seek/read in Cemu after placing
the fixture in the emulated SD directory. HOME exit and hardware are unverified;
A `no_std` shared VFS mode and a fixed-fixture Rust-initiated SD backend
are now linked in a [VFS probe](vfs-probe/README.md). The user reports its
all six checks, including SD mount, passed in Cemu. HOME exit could not be
verified in the current Cemu setup.
The real SD directory/pack backends still need implementation.

## Next experiments

1. Pin and validate a modern Wii U target against WUT's actual calling
   convention and code generation, rather than relying on the Linux target.
2. The `no_std`/`alloc` formats mode and allocator passed the synthetic TGA
   probe in Cemu; test hardware and additional formats. Keep a
   maintained or locally implemented Wii U `std` backend as a fallback if the
   shared-code and dependency adaptations prove too costly. Choose only after
   proving a small linked and running example.
3. The WUT SD mount/read/seek probe passed in Cemu with bytes passed into
   shared Rust parsing. The Rust-initiated VFS probe also passed all six
   checks in Cemu. Test on hardware; general SD access and panic
   handling need investigation.
4. Run synthetic little-endian parser and pack fixtures on PowerPC and compare
   results with the host, including signed integers and floating-point fields.
   Follow with representative simulation checkpoints before starting rendering.

## Shared VFS experiment

`c2-vfs` now has an optional `no_std` mode with layered lookup, memory
backend, and platform I/O errors. Its default `std` mode retains directory
and cooked pack backends; zip explicitly enables `std`. Host tests passed
with default features (4), zip (5), and no default features (2), and the
no-default mode type-checks for the Wii U target. The linked VFS probe
passed all six mount, Rust-initiated read, parser, error and layer checks
in Cemu with a limited fixed fixture; it does not yet provide a reusable, thread-safe SD backend or on-demand cooked pack access.

## Portable pack reader

`RangePackBackend` uses a platform-provided random-access source and reads the
C2PK index at open, then entry data on demand. It validates index and entry
ranges against the file size. Host tests cover lazy reads, case-insensitive
lookup, truncated indices and invalid entry ranges. A Wii U [SD pack probe](pack-probe/README.md) links this reader to a serialized WUT FS bridge; its
user confirmed all seven checks passed in Cemu; hardware remains unverified.
The source opens a file per
request, so performance, media changes, memory limits, lifecycle, and
concurrency require further work. The C2PK container does not make 3DS PICA
textures directly usable on Wii U.

## Shared core audit and initial probe

The full `c2-core` depends on formats, VFS, platform, thiserror and tracing.
`c2-platform` already declares `no_std`. Gameplay uses standard-library paths,
allocation prelude types and many floating-point methods unavailable in `core`
(a broad search finds over 200 math call sites, including in tests). Porting
those operations requires deliberate math choices and parity checks, not just
changing imports. Dependency features and platform logging also need review.

`c2-core` now defaults to the unchanged full `std` feature, while disabling
default features exposes `rng`, `color`, `model`, `area`, `collide` and
`player` as a portable subset. The
[core probe](core-probe/README.md) links this subset and compares deterministic
random and color checkpoints. The user confirmed that all five checks passed in Cemu; hardware remains
untested. It does not run a hunt or establish floating-point simulation parity. Extend
portability incrementally with targeted host and PowerPC checkpoints before
claiming gameplay support.

Host validation: the portable subset passed four unit tests. The full PC
core passed 66 library tests with `friendly_and_stealth_shorten_noticing`
filtered out; a full run was stopped after that existing test ran for more
than 60 seconds. The test has not been verified in this run.

## Portable model and animation

The model preparation and animation module is available without `std`. Its
portable mode uses libm for square root and trigonometry, while the default
PC configuration retains its previous standard-library calculations. Host
tests pass in both modes, including animation checkpoints, model radius and
vertex lighting with tolerances. The [model probe](model-probe/README.md)
links the module for Wii U; the user confirmed all five checks passed in
Cemu. HOME exit and hardware remain unverified. This synthetic test does
not exercise player movement, dinosaur AI, rendering or a complete simulation.

## First-map movement simulation

The shared player, area and collision modules now build without `std`. Their
portable math methods use libm; the default PC build retains its prior math.
Thirteen portable unit tests and a separate integration test of walk, sprint,
jump and crouch passed on the host; the integration test exercises the library
compiled without `std`. Four PC player tests also passed. An actual AREA1 host
run produced the same scripted landing, walk and sprint positions in normal
and portable release builds.

The [movement simulation](movement-probe/README.md) loads the user's base-game
AREA1.MAP and AREA1.RSC, performs those checkpoints and offers interactive
GamePad movement with text coordinates. In Cemu the user reported both files
loaded, all three checkpoints passed, and the interactive screen reached READY.
The user corrected the initial Y to 5248. The initial GamePad-only build did
not respond to their Pro Controller profile. After Pro support was added, the
user reported working four-direction movement, yaw, jumping, sprint toggle and
crouch toggle in Cemu. Pitch, simultaneous inputs, GamePad input, HOME exit
and hardware remain unverified. It does not render the area or include guns or
dinosaurs. The tests do not establish general simulation, controls, timing,
memory usage or float parity on Wii U.

## GX2 presentation probe

A [GX2 probe](gx2-probe/README.md) builds an RPX that uses WUT's sample
position/color shader and Rust-produced vertex attributes to draw an animated
triangle on TV and GamePad. It awaits Cemu and hardware testing. This is a
graphics foundation; it does not yet connect the first-map movement simulation
or demonstrate texture formats, depth, GPU synchronization or performance.
