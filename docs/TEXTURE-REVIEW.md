# Texture fidelity review

The public `c2-pc texture-review` command renders locally owned animals with
the engine's model renderer and vertex animation code. It captures both side
views at configured deterministic poses, with fixed bounds across the poses of
each clip. The compact example selects one clip and one pose; exhaustive mode
selects every animation at three phases. APKs are converted anew, serialized to CAR and
reloaded through the normal runtime preparation and GPU shader. This is an isolated animation
and texture test; it does not run animal AI, terrain, spawning or combat.

The runner and this configuration format are public. Your inventory, model
copies, screenshots, differences, hashes and HTML/JSON reports stay private.
Keep the local manifest in `source/` and outputs in `regression-sim-recordings/`
(both gitignored). Each new output directory also contains a `*` .gitignore.
There is no uploader or public baseline export for texture reviews. The
existing gameplay baseline publisher expects a different run format.
Do not force-add these generated files to Git or attach them to public CI.

```sh
cargo build --release --manifest-path pc-ver/Cargo.toml -p c2-pc
# Copy examples/texture-review.toml from c2-regress into source/ and edit it.
pc-ver/target/release/c2-pc texture-review \
  --manifest source/texture-review.toml \
  --out regression-sim-recordings/textures-before
# After a change and rebuild, compare against the original capture:
pc-ver/target/release/c2-pc texture-review \
  --manifest source/texture-review.toml \
  --out regression-sim-recordings/textures-after \
  --baseline regression-sim-recordings/textures-before
```

Open the new run's `report.html` locally. Filter by pack, animal or model path
and expand an animal to see its captures, baseline images and difference images.
Inspect feet, faces, seams and intentional cutouts on both sides;
click an image to inspect its full resolution. A successful initial capture
means images were generated, **not** that their textures have human approval.
Reports always retain `human_approval: pending` until your review is recorded
separately. Keep an approved run unchanged and pass it as `--baseline`.

Comparisons are tolerant, not byte-identical: by default a pixel changes only
if a channel differs by more than **12/255**, and a capture fails when more
than **0.5%** of pixels change. Both thresholds are configurable. Small noise
below tolerance passes; sufficiently large missing-texture regions fail.
Differences smaller than that fraction can still hide a defect, so inspect
the images. Compare on the same GPU/driver; thresholds are not a guarantee
of equivalence across renderers. Missing/unreadable images, changed dimensions
and removed baseline captures fail. New captures without a baseline also fail
comparison, requiring review of an expanded suite before adopting a new baseline.
Initial capture without a baseline is supported and never implies approval.
Empty or black-silhouette captures fail when the source model should be visible.
Some death/hidden clips deliberately shrink all vertices to a point: when the
posed mesh extent is below 0.1% of its rest mesh extent, the report marks
`source_pose_collapsed` instead. The screenshot is still saved and compared;
a changed hidden pose or unexpected colored pixels can still fail the diff.

Add a `[[source]]` for each pack or loose addon. See the public
[example manifest](../pc-ver/crates/c2-regress/examples/texture-review.toml).
`game` enumerates unique registry character files; `cars` recursively includes
all CAR variants, including ambient animals, even when map restrictions would
hide them in a hunt. It needs explicit filters to omit weapons and effects.
`apk` enumerates character descriptors, including stock mobile counterparts;
filter out nonanimals explicitly (ambient flyers may have fewer clips than
huntable animals). ZIP sources support `prefix`. Relative paths resolve from
the manifest, and include/exclude paths resolve within the selected backend.
For planar wings that vanish edge-on, a source may set `yaw_offset_degrees`
and `pitch_degrees` (each within ±45°). Put these models in a separate source
to retain exact side views for the rest. Source ids must remain stable; image
keys use the asset path, so adding an addon does not renumber existing animals. `--limit N` is a smoke test cap per
source; use it without a full baseline and omit it for coverage runs.

The synthetic data regression remains independent of GPU rendering:

```sh
cargo test --manifest-path pc-ver/Cargo.toml -p c2-core --test texture_fidelity
cargo test --manifest-path pc-ver/Cargo.toml -p c2-regress texture_review
```

## Compact sweeps and disk use

For routine texture review, use `animations = [0]`, `phases = [0.33]`:
that produces **two images per animal model**, one per side. This gives less
pose coverage than the exhaustive `animations = []` with three phases. Select
more clips or phases for a suspected animation-dependent defect. A baseline
must use the same asset type, selection and phases. Each capture records a
versioned recipe with integer animation time, camera target/distance/angles,
projection, clipping planes, background, fog and lighting. Comparisons replay
that recipe, including the baseline framing when model bounds change, rather
than recomputing a different view of the candidate. Legacy runs without recipes
must be recaptured before comparison; their images remain historical evidence. `keep_models = false` is the default:
the CAR is serialized and reloaded, then the temporary copy is removed after
the renderer owns it. Set it to true only for conversion diagnostics.
Animation archives always retain serialized CAR files, including every clip,
so restoring an archive retrieves the actual animation data as well as sampled
images. Compact sweeps honor `keep_models = false`.
Candidate comparisons save extra baseline/difference images only for misses.

After human review, archive a Rust run with the public Python utility:

```sh
python3 tools/texture-review-archive.py archive \
  regression-sim-recordings/textures-after \
  --out regression-sim-recordings/archives/textures-after.tar.xz \
  --reviewed-by 'YOUR NAME' --notes 'Review notes' --prune
```

`--reviewed-by` records an explicit human review; the assistant must not supply
one on your behalf. Without `--prune`, all local files remain. With it, the
utility removes Rust model copies and matching screenshot media **only after**
it has verified every archived file against SHA-256 and checked that the local
files have not changed. Captures marked changed, invalid or missing a reference
remain on disk with their baseline/difference images. Add `--keep FILENAME.png`
for any additional capture needing review, including a suspicious image below
the automated tolerance. The small JSON metadata and archive receipt remain;
the local HTML report becomes an index of retained misses. The original full
report, source copies and captures are preserved verbatim inside the archive.

The archive uses solid, lossless tar.xz compression with content deduplication
for identical files. PNG is already compressed, so do not expect dramatic
savings on unique PNGs; avoiding unnecessary poses and CAR copies saves more.
Original-engine references must remain on disk: `--role original` (or metadata
with an original engine) rejects `--prune`.

Keep the `.tar.xz` and adjacent `.tar.xz.index.json` together when moving them
to private cloud storage. The index contains filenames and hashes and is also
private. There is no upload or automatic deletion after upload. To check or
restore after downloading both files:

```sh
python3 tools/texture-review-archive.py verify path/to/run.tar.xz
python3 tools/texture-review-archive.py restore path/to/run.tar.xz \
  --out regression-sim-recordings/restored-reference
```

Restore requires a new destination and checks the archive hash and every file's
bytes. Pass the restored directory as `--baseline` when needed. Do not move a
live baseline out of a folder that the next comparison expects to read without
restoring it first. Run the lifecycle tests with:

```sh
python3 -m unittest discover -s tools -p 'test_texture_review_archive.py'
```

## Stable approved baseline identities

Keep these as separate assets, with separate output directories and archives:

- `texture-sweep`: the compact routine check.
- `animation-archive`: every animation clip, at the configured phases. This is
  exhaustive clip coverage, not a recording of every animation frame.
- `original-parity`: matched captures from the original executable. Existing
  unmatched gameplay screenshots do not qualify.

A Rust manifest may set `asset_type = "texture-sweep"` or
`asset_type = "animation-archive"`. By default an empty animation selection
means `animation-archive`; an explicit clip selection means `texture-sweep`.
The archive type requires `animations = []` to prevent labelling a compact
selection as exhaustive. Rust cannot generate an `original-parity` asset.

To register an approved baseline when archiving, add both flags:

```sh
python3 tools/texture-review-archive.py archive PRIVATE_RUN \
  --out PRIVATE_ARCHIVES/sweep-v1.tar.xz --reviewed-by 'YOUR NAME' \
  --asset-type texture-sweep --baseline-version v1
python3 tools/texture-review-archive.py find PRIVATE_ARCHIVES \
  --game pc-carnivores-2 --animation 0 --baseline-version v1 \
  --asset-type texture-sweep
```

Each index entry carries `game / model / animation / scenario / engine_commit /
baseline_version`, plus asset type, image filename, phase, side, CAR hash and
capture recipe. `game` is the manifest's stable source id and `model` is the
asset path within that source. Keep those source ids stable, including separate
ids for variant inventories. `find` uses exact matches and returns the archive
index and capture metadata; restore that archive and use it as `--baseline`.
No media decompression is needed to search indexes.

The engine commit and dirty status are stamped into the binary at build time.
Approved identities require a clean recorded commit, valid capture recipes,
zero failures and a matching recorded asset type. Build and recapture from a
clean checkout before registering a baseline; a commit alone cannot identify
uncommitted engine changes. Ordinary archival remains available for dirty or
historical runs without baseline registration. Neither capture nor search
supplies human approval. The original report stays unchanged inside the archive;
the review attestation and approved identities live in its adjacent index and
local receipt. Keep version names immutable within each baseline lineage. Registration rejects
identities already present in adjacent archive indexes.

## Matched original-engine parity references

`tools/texture-review-original.py` renders isolated animals **inside the shipped
Windows software-renderer executable**, using the existing Wine injection hook.
It calls that executable's native `CreateMorphedModel` and `RenderModelClip`;
it does not recompile the original renderer or use Rust to produce its images.
The adapter recognizes the stock Carnivores 2 and Ice Age builds and the
Genesis Redux/MEE build. It checks executable code signatures, model layouts,
viewport instructions and writable global addresses before starting the game.
Unknown builds are rejected rather than used with guessed addresses.

First capture the desired Rust sweep with its model and pose data retained:

```sh
pc-ver/target/debug/c2-pc texture-review \
  --manifest source/my-pc-textures.toml \
  --out regression-sim-recordings/pc-texture-input --keep-models
pc-ver/crates/c2-regress/original/build.sh
python3 tools/texture-review-original.py \
  --reference regression-sim-recordings/pc-texture-input \
  --game /path/to/owned/original-game --source MY_SOURCE_ID \
  --out regression-sim-recordings/original-textures \
  --work regression-sim-recordings/_original_texture/MY_GAME
```

Python requires Pillow and NumPy; Wine and the hook build dependencies are the
same as the existing [original gameplay runner](REGRESSION.md). Repeat
`--source` for variants or critters that use the same executable. Use a
separate original run and `--work` for each original game. Without `--source`,
the adapter selects every model in the reference. Choose a compatible owned
executable for those models: the stock engines support 1024 vertices/faces;
the recognized Genesis build uses dynamic buffers with an adapter limit of
8192. The original model texture limit remains 256×256.

The runner uses its own game copy and Wine prefix and never alters the owned
installation. Each run writes to a new private output directory. It renders
into a separate surface, with the **same integer animation time, camera target,
distance, yaw, pitch, projection and background** as the Rust capture. Models
are centered without terrain, AI, spawning, weapons or combat. The original
loader's face ordering, UV offsets and software texture representation are
retained, including wrapped UV coordinates. Lighting is controlled: neutral
texture brightness and full model light (native 240 / fade row 60), without
scene fog or directional vertex lighting. The game's surface stride and
viewport scissor are set explicitly for both stock and Genesis layouts.

Supported recipes use even resolutions within 1024×768 and neutral full light.
Every vertex must remain within the shared view and beyond the original
renderer’s 64-unit near limit, so neither engine applies different clipping
planes. Unsupported recipes or models fail. Widen the shared framing and
recapture both engines if a model would intersect native clipping planes.

`report.html` shows original, Rust and difference images side by side.
`review.json` records two **separate** results:

- Capture validity: native poses must agree with Rust within
  `max(0.001, maximum coordinate magnitude × 0.000001)` engine units. Missing
  outputs, mismatched poses, invisible models and incomplete native batches
  fail the run. The maximum pose error is recorded for every side.
- Pixel parity: `parity_changed_fraction` and `parity_failures` use the reference
  sweep's tolerance. Agreement on pose and scene does **not** imply identical
  pixels. The original integer software rasterizer and Rust GPU renderer can
  still disagree in sampling, edges, cutouts and blending. Those differences
  remain visible for human review; the runner does not automatically approve
  them or loosen the tolerance.

Reports also show the average changed-pixel fraction, median, nearest-rank
95th percentile, maximum, configured pass threshold and number exceeding it,
overall and by source. Every valid capture has equal weight; invalid or missing
comparisons are excluded and counted separately. These descriptive statistics
do not change thresholds, hide individual failures or automatically approve
renderer differences. New original captures include the summary in HTML and
`comparison_summary` in JSON. Refresh existing unapproved reports, including a
combined original index, without rerendering or modifying screenshots:

```sh
python3 tools/texture_review_summary.py PRIVATE_RUN PRIVATE_COMBINED_INDEX
```

Archived or approved reports cannot be refreshed in place.

The original's RGB555 surface is converted losslessly to visible RGB colours.
Clear background pixels use the exact shared RGB background to avoid counting
background quantization as an animal defect. Native vertex outputs, CAR files,
input jobs, hook log, executable SHA-256, hook build provenance and runner hash
are preserved locally. Each capture records its original executable hash in
addition to the stable baseline identity, so a stock and a modified executable
cannot silently stand in for one another. No run is approved or pruned by the
capture command. Register an approved original reference with
`--asset-type original-parity --baseline-version VERSION` in the archive tool;
original references cannot be pruned. Clean hook build provenance and checked
native pose agreement are required for approved registration.

To compare a future Rust build against these original images, pass the
original output directory as `c2-pc texture-review --baseline`. Use a manifest
covering exactly the original run's selected sources/models; expanded or
removed coverage fails as in ordinary Rust comparisons. The saved original
recipes are replayed. A strict pixel comparison may fail on known rasterizer
differences, which are distinct from a failed or unmatched capture.

Existing `c2-regress run --engine original` gameplay captures remain separate
historical evidence. They are not substituted for the isolated texture sweep.
The existing gameplay baseline publisher does not accept this private format.

Synthetic adapter and archive checks:

```sh
python3 -m unittest discover -s tools -p 'test_texture_review*.py'
```
