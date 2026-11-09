# Gameplay regression scenarios

Scripted, deterministic hunts that check a build still loads a map,
places the animals, fires every weapon and opens every screen, and that
it looks and performs as it did. The engine behind it is in this
repository. Recordings, screenshots and reports stay local
(`regression-sim-recordings/` is gitignored) because they are made from the
player's own game files.

## Validating a change

Much of this project's code is written with AI assistance. A fix or an
optimization can compile, pass the unit tests and still change how the game
behaves. The scenarios exist to catch that before it is merged. Use them for
every change to gameplay, AI, rendering, file loading or performance-sensitive
code, whoever wrote it.

### What a passing run proves, and what it does not

| Check | Catches | Strength |
|---|---|---|
| Functional (fails the comparison) | The map not loading, animals missing or dead, a weapon not firing exactly once, a screen that no longer opens, timeouts, panics, set-up errors | Strong, on any machine |
| Determinism (`--repeat 3` or more) | A change that makes the simulation depend on timing, threads or uninitialised state | Strong, on any machine |
| Checkpoint state hashes (manual, below) | Any change at all to the simulation: the hunter, the weapon, every animal's pose, the random stream, at every checkpoint | Strongest; only meaningful between runs on the same platform |
| Visual (fails the comparison) | A rendering change larger than the tolerance | Only against your own baseline, same GPU and driver. Published baselines have no screenshots |
| Performance (warns) | Slower frames or ticks, more memory | Only between runs on the same machine and settings. Published figures are reference only |

By default the comparison reports, but does not fail on, checkpoint state
hashes that differ from the baseline's: a change that moves an animal but
keeps all four alive still passes. For an optimization or refactor that must
not change behaviour, add `--same-state` (step 4 below).

Not covered by any scenario: the menus and hunt setup, the launcher, mod
merging and recipes, combined packs, sound, save files, animals stalking the
player over time (the kill scenario sets each killer down beside him),
Modder's Edition games beyond loading and the kills below, and every console
build. Changes there still need the unit tests and a hand-played check, as
described in the pull request.

### Steps

1. **Before the change**, on the base commit, record your own baseline (same
   machine, same game folder):
   ```sh
   cd pc-ver
   cargo build --release -p c2-pc -p c2-regress
   ./target/release/c2-regress run --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
       --game-id c2 --game <DIR> --repeat 5 --label before
   ```
2. **After the change**, rebuild both binaries and run against it:
   ```sh
   cargo build --release -p c2-pc -p c2-regress
   ./target/release/c2-regress run --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
       --game-id c2 --game <DIR> --repeat 5 --label after --baseline <BEFORE_RUN_DIR>
   ```
   Exit code 0 is required (5 means the visual comparison did not run: one
   of the two runs has no screenshots). Also run `--baseline published` to
   check against the public reference.
3. **Read `report.html`** of the candidate: every stage passed,
   `determinism.identical: true`, and the screenshots look right.
4. **Compare the state hashes**: add `--same-state` to the step 2 run (or to
   `c2-regress compare`), which fails on any difference and lists the
   checkpoints that differ in `comparison.json`; or by hand:
   ```sh
   diff <(jq -c '.checkpoints[]' <BEFORE_RUN_DIR>/metrics.json) \
        <(jq -c '.checkpoints[]' <AFTER_RUN_DIR>/metrics.json)
   ```
   - An optimization, refactor or cleanup must give **no output**: the same
     checkpoints at the same ticks with the same hashes. Any difference
     means behaviour changed; find out why before merging.
   - A bug fix that changes behaviour on purpose will differ. Say which
     checkpoints changed and why in the pull request, and check the
     screenshots for those checkpoints.
5. **Performance claims**: with `--repeat 3` or more on both runs, on the same
   machine, the comparison's performance section gives the change in median
   and p95 tick and frame times. Quote those figures; do not quote a single
   run or a different machine.
6. Repeat for every game the change can affect: Ice Age (`--game-id ice-age`)
   and the APK-built bases (`c2-apk`, `ice-age-apk`) when the change touches
   their code paths (mobile HUD, conversion, `HUNTDAT/MOBILE`).

When a change breaks a scenario on purpose (a new stage, a new check, a
different layout), bump the scenario's `version` and make new baselines
([Making and approving a baseline](#making-and-approving-a-baseline)). Never
edit a baseline to make a candidate pass.

To confirm the checks still catch problems after changing the runner itself,
run with `--inject` ([Deliberate faults](#deliberate-faults)): each fault must
make the comparison fail.

## Pieces

| Where | What |
|---|---|
| `pc-ver/crates/c2-core/src/scenario/` | The scenario run, shared by every frontend: set-up (`build`), the scripts (`Director`; the kill script's steps and checks in `kills.rs`), pinning (`Hold`), state hashes, deliberate faults (`Inject`). No I/O. Compiles for the consoles' `portable-hunt` target. |
| `pc-ver/crates/c2-regress/` | `c2-regress` CLI: scenario files, the headless runner, recordings, comparison, `report.html` and the screenshot PDF. |
| `pc-ver/crates/c2-pc/src/regress.rs`, `hunt_view/regress.rs` | `c2-pc regress`: the same script through the PC renderer, offscreen. Started by `c2-regress run`. |
| `pc-ver/crates/c2-regress/scenarios/` | Versioned scenario files. |
| `pc-ver/crates/c2-regress/thresholds.toml` | Default comparison thresholds. |

## The canonical scenario: `vanilla_map2_four_dinos`

File: `scenarios/vanilla_map2_four_dinos.toml` (version 2).

1. Loads an area, daytime: the game's second (`HUNTDAT/AREAS/AREA2.MAP` /
   `.RSC`) for Carnivores 2, the fourth (`AREA4`) for Ice Age. Version 1
   used area 2 for Ice Age too, where terrain hid some of the animals from
   the first landing. The scenario id keeps its `map2` name so recordings
   and baselines stay where they are.
2. Seed 20261107. The hunter stands at the area's first landing
   (`Area::landings[0]`), looking along yaw 0, level.
3. Places four animals around one anchor, 12 cells out and 27° right of the
   line of fire, at fixed offsets. Set-up refuses a layout where any
   animal's edge is within 4° of the line of fire. Shots go straight ahead,
   so no animal can be hit.
4. Warm-up: 500 ticks (5 s).
5. Every weapon in `_RES.TXT` order: select, wait until ready, fire exactly
   once (one tick of trigger), checkpoint mid fire animation, wait for it to
   finish, put it away.
6. Binoculars, the map, the change-call icon (the original's call switch:
   press, switch, wait until it closes), the PC's weapon picker and call
   picker: each opened, checkpointed and closed.
7. Checks that the four animals are still there.

A game section may override `landing`, `yaw_deg` and `anchor_distance`
(an area whose first landing hides the animals, or larger animals).
Modder's Edition games are resolved for the scenario's area, by day, with
no menu slots picked, as the PC hunt does; their names and AI numbers
repeat, so their species are picked by character index (`index:N`).
Set-up refuses an animal with no model, or with a Modder's Edition
`blank.car` placeholder (the model such games give an animal on maps it
does not live on), since it would pass every check unseen.

Every wait has a timeout (600 ticks). A stage that times out fails and the
script goes on with the next stage. The whole run also has a tick ceiling.

### Per game

| Game | Status | Animals |
|---|---|---|
| Carnivores (C1) | **Unsupported**: the engine has no C1 loader (DEVELOPMENT.md §1 non-goal). `run` reports it and exits 3. | - |
| Carnivores 2 | **Supported** | T-Rex (AI 18), Allosaurus (13), Stegosaurus (12), Chasmosaurus (14). C2 has no Triceratops; Chasmosaurus stands in as its ceratopsian. 6 weapons. |
| Carnivores: Ice Age | **Supported** | Mammoth (17), Smilodon (16), Bear (18), Woolly rhinoceros (13): the four largest huntables, at 20 cells since they are up to twice the C2 animals' size. 6 weapons. |
| Carnivores: Dinosaur Hunter (Android 2.0.1), `c2-apk` | **Supported**, from a folder built with `c2-pc mobile-base` (below). Same four animals as Carnivores 2 (the rebuilt `_RES.TXT` keeps their AI numbers). 7 weapons (the phone adds the Revolver). The render pass draws the phone's HUD. |
| Carnivores: Ice Age (Android 2.0.1), `ice-age-apk` | **Supported**, likewise. Same four animals as Ice Age, at 20 cells. 7 weapons. |
| Fallen Kings (Beta 3.35, standalone) | **Planned**: Modder's Edition `_RES.TXT`, which the runner loads. Not yet baselined, so not part of public runs. | - |

### Test mode

Only a scenario run does any of this; nothing in normal play calls it.

- **Hold**: after every tick the hunter's place and view and the animals'
  places are restored, and the hunter's health is topped up. Animals still
  think, animate, hear the shots and react; they just stay put. Rendering
  and the sound logic run as normal. The render pass uses a silent audio
  output.
- **Fresh profile**: a new in-memory player save, not the player's own. The
  render pass uses default PC settings, not the saved ones.
- Simulation time advances by fixed 10 ms ticks counted by the script. No
  wall-clock sleeps.

### APK-built bases

The Android games have no `HUNTDAT/` of their own. `c2-pc mobile-base`
rebuilds one from the APK, with `HUNTDAT/MOBILE/` holding the phone's HUD
files, and that folder is the `--game`:

```sh
./target/release/c2-pc mobile-base Carnivores_+Dinosaur+Hunter_2.0.1.apk --out <DIR>
./target/release/c2-regress run --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
    --game-id c2-apk --game <DIR> --repeat 3 --baseline published
```

Build the folder somewhere outside the repository (it holds the game's
files). `HuntView` loads the phone HUD when `HUNTDAT/MOBILE` exists, so
these runs are the only ones that render the phone's compass and wind,
binocular mask, scope and painted map. Every `HUNTDAT/MOBILE` file is in
the run's content hashes.

The rebuild is deterministic: the same APK and the same `mobile-base`
code give byte-identical folders. Their hash lists are generated from it
(`pc-ver/crates/c2-regress/hash-lists/*-apk-2.0.1.toml`, with the APK's own
hash). `recipes/bases/*-apk.toml` stay APK-hash only, as the launcher
checks the APK, not the rebuilt files. When `mobile-base` output changes,
regenerate the lists (`c2-pc recipe base --base <DIR> --id ID --out FILE`,
keeping the header) and re-publish the APK baselines.

### Standalone games: `standalone_map2_four_dinos`

The same tour for Modder's Edition standalone games, in a file of its own
so that adding a game doesn't change the canonical scenario's text (and
invalidate its published baselines). Runs stay local; nothing from these
games is published. Genesis Redux (Beta 2.0) passes on our engine, headless
and rendered: four animals by character index (Tyrannosaurus, Carnotaurus,
Stegosaurus, Triceratops on area 2) and all 8 weapons. It also passes
under `--engine original`, with the game's own `v_soft.ren`: every weapon
fires, and binoculars, map and call switch open and close.

For Modder's Edition executables the driver:
- picks animals by menu slot: the hunt's command line is the stock one
  (`din` a bit per slot), and a character index is turned into its slot
  as our engine reads the game's `_RES.TXT`;
- pins `config.cfg` in the game copy to 800x600, exclusive fullscreen,
  the software renderer and no frame limit (the file's `resolution`
  overrides the profile's).

Open: in the original run the hunter's view tilts up during warm-up and
ends facing the sky; weapon and screen checks still pass. Ruled out so
far: mouse input (the game re-centres the cursor every frame and reads
back the centre, through the same window), the window's title bar (the
same frames in borderless and exclusive fullscreen), the profile (the
fork's `trophy0N.sav` layout is the stock one), and a drop-ship intro
(the game has none).

## Kill animations: `kill_animations`

File: `scenarios/kill_animations.toml` (version 1, `script = "kills"`).
Kills have broken before: a body frozen on one frame, a kill type naming
a clip the hunter model does not have, a synchronized body left behind by
its killer. This scenario plays each kill and checks it.

1. Loads the area by day, seed 20261108. The hunter stands at the first
   landing, pinned, looking along yaw 0.
2. Parks the animals 30 cells to the hunter's right, 6 cells apart,
   pinned (the first `[anchor] offsets`, one per animal).
3. Warm-up: 200 ticks (2 s).
4. For each animal, one stage `kill_NN_<name>`:
   - **Release**: the animal is set down 600 units ahead, facing the
     hunter, hunting him (as `c2-pc --die`), and is no longer pinned.
     The hunter's health is no longer topped up once he is dead.
   - **Kill**: waits up to 1500 ticks (a T-Rex finishes a sniff and roars
     before it charges). `killed_by`: the death names this animal.
   - Checkpoints `_start` (the tick of the kill), `_mid` (1 s later) and
     `_late` (3 s later). The state hash includes the body's and the
     killer's clip and frame time, so any change to either shows up in
     the hashes; the render pass takes a picture at each.
   - Checks over the 300 watched ticks:
     - `hunter_clip`: the body is a hunter model playing one of its own
       clips, with a length above zero.
     - `hunter_animates`, `killer_animates`: each one's (clip, frame time)
       changes on at least 20 ticks.
     - `body_with_killer`: the body falls within reach of the killer
       (3 x the killer's size x scale, at least 1024 units). After that,
       it may leave the spot where it fell only by staying with the
       killer (carried, or synchronized to the killer's clip). Killers
       that gore and run on (the Chasmosaurus AI, which Ice Age's Diatryma
       uses) leave the body where it fell, and that passes.
   - **Revive**: the body is removed, the death camera ends, the hunter
     gets full health, and the animal is calmed and pinned at its parking
     spot again. A kill that fails or times out is still revived, and the
     next stage runs.
5. Checks that every animal is still alive and in place.

| Game | Killers | Area |
|---|---|---|
| `c2` | T-Rex (scripted grab and carry), Allosaurus | AREA2 |
| `ice-age` | Woolly rhinoceros, Yeti, Diatryma | AREA4 |
| `c2-addons` | Diabloceratops (synchronized toss), Saurophaganax (its own hunter model) | AREA1 |
| `jphl` | JPHL Special Edition's own T-Rex (`t-rex.car`, not the Carnivores 2 one beside it) | AREA1 |
| `triassic-mee` | Gojirasaurus (Triassic, Modder's Edition) | AREA1 |

`c2-addons` takes any pack that holds both add-ons, for example one built
from `recipes/c2-everything.toml`. The modded games pick their animals by
model file (`model:diablo.car`), since an add-on pack's character indices
change whenever its mod list changes. They have local baselines only:
add-on and standalone packs are never published.

```sh
./target/release/c2-regress run --scenario crates/c2-regress/scenarios/kill_animations.toml \
    --game-id c2-addons --game <PACK DIR> --repeat 3 [--baseline RUN_DIR]
```

Use it for any change to the AI's attacks, kill types, hunter-model
merging, the death camera, `.CAR` animation loading or the renderer's
character drawing. The functional comparison requires every kill to pass
(`functional.kills`). The state hashes and screenshots show *how* a kill
changed. `--inject freeze-body` holds the body on its first frame, and
every kill must then fail `hunter_animates`.

Results (2026-11-08, PC, Linux): all ten kills pass on all five games.
The original engines run the same scenario for `c2` and `ice-age`
([Kill animations in the original](#kill-animations-in-the-original)).
Three repeats are identical, and the render pass gives the same state
hashes as the headless run.

## Commands

Build both binaries (the render pass runs `c2-pc` from beside `c2-regress`;
rebuild both after changing either):

```sh
cd pc-ver
cargo build --release -p c2-pc -p c2-regress
```

```sh
# Games the scenario covers, and their status
./target/release/c2-regress list --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml

# A game's characters as the runner loads them for an area: index (for
# index:N), AI number after Modder's Edition tuning, model, name
./target/release/c2-regress species --game <DIR> --area AREA1

# Run: headless (timed) N times, then the render pass once
./target/release/c2-regress run --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
    --game-id c2 --game ../source/c2-original --repeat 3 [--label TEXT] [--size 960x540] \
    [--baseline RUN_DIR] [--thresholds FILE] [--same-state] [--no-render] [--inject FAULT]

# Compare a candidate with a baseline (writes comparison.json and diffs into the candidate)
./target/release/c2-regress compare BASELINE_DIR CANDIDATE_DIR [--thresholds FILE] [--same-state] [--fail-on-warn]

# Screenshot PDF: runs side by side per checkpoint, then metrics by run
./target/release/c2-regress pdf --out history.pdf RUN_DIR [RUN_DIR...]

# Rebuild report.html of a run
./target/release/c2-regress report RUN_DIR
```

`--game` is the folder that holds `HUNTDAT/`. No game path is built in.

Exit codes: 0 passed, 1 failed (functional, visual, or checkpoint state
with `--same-state`), 2 usage or set-up error (missing `_RES.TXT`,
unreadable scenario, an existing baseline without `--replace`), 3 the game
is unsupported or planned for the scenario, 4 performance warnings with
`--fail-on-warn`, 5 passed but the visual comparison did not run though it
could have (the baseline or the candidate was recorded with `--no-render`
or without `c2-pc`). A visual check that cannot run by design (a published
baseline, which has no screenshots, or the original engine against ours)
is noted and exits 0. `--no-render` with a `--baseline` other than
`published` warns at the start. `c2-regress compare` prints the JSON report
on stdout and the verdict on stderr.

### Deliberate faults

`--inject` proves the checks catch real problems: `drop-shot:N` (weapon N,
0-based, is never fired), `kill-animal:N` (animal N dies after warm-up),
`nudge-view:DEG` (the view turns, so screenshots differ), `freeze-body`
(kill script: the hunter's body is held on its first frame). The fault is
recorded in the manifest, and such a run is never used for performance
comparisons.

## Recordings

`regression-sim-recordings/<scenario>/<game>/<platform>/<UTC time>_<commit>[_<label>]/`:

| File | Contents |
|---|---|
| `manifest.json` | Scenario id, version and file hash; game and area; platform; build (version, commit, dirty, profile); content hash and the hash of every game file read; seed; settings; hardware; renderer (adapter, backend, resolution). |
| `metrics.json` | Result; functional counts; UI screens; simulation tick and per-section timings; render frame statistics; memory; determinism across repeats; checkpoint state hashes. |
| `events.jsonl` | One line per stage start/end, action, check and checkpoint, by tick, from the headless run and the render pass (`source`). |
| `tick_times.csv` | Headless: each tick's sections (`prof::SECTIONS`) and total, ms. |
| `frame_times.csv` | Render pass: each frame's update, render (including the GPU wait) and total, ms. |
| `engine.log` | The engine's log during the run, set-up errors and panics, the render pass's log. |
| `screenshots/` | `<checkpoint>.png` from the render pass. |
| `comparison.json`, `comparison/` | Against a baseline, with difference images. |
| `report.html` | All of the above, readable in a browser. |
| `render.json`, `render.log` | The render pass's own results, folded into the files above. |

No video is recorded (`recording.mp4`): the renderer has no encoder, and
adding one would add a heavy dependency.

### Metrics

Times are milliseconds. Percentiles are nearest-rank. "1% low FPS" is the
rate of the slowest 1% of frames (1000 / their mean time). A value that is
not measured is written as `{"unavailable": "why"}`, never as zero:

- **GPU memory and GPU time**: wgpu exposes neither without timestamp
  queries. The render frame time includes waiting for the GPU.
- **Process memory** (resident, peak): Linux only (`/proc/self/status`).
  The headless runner and the render pass are separate processes, measured
  separately.
- **AI update**: the `dinos` section of `Hunt::tick`'s profiler.
- **Render frames**: one scripted tick and one offscreen frame each, with
  no presentation or vsync. They compare builds; they are not the frame
  rate a player sees.

### Determinism

Each checkpoint records a hash of the hunt (tick, hunter, weapon, every
animal's pose, the random stream). `--repeat N` runs the headless hunt N
times; `metrics.json` `determinism.identical` says whether every repeat
reached every checkpoint at the same tick with the same hash; repeats that
disagree fail the run (exit code 1). One run reports determinism as
unavailable and does not fail on it. The render
pass runs the same script through `HuntView`; `metrics.json` `render_state`
compares its checkpoints with the headless run's by name, and a checkpoint
reached at another tick or with another hash, or missing from the render
pass, fails the run. Checkpoints only the render pass reaches (the weapon
and call pickers, which headless can't open) are listed under
`render_only` and not compared; `end` is then skipped too, since the render
pass ends later.

## Published vanilla baselines

`pc-ver/crates/c2-regress/baselines/` holds the scenario's results on the
vanilla Carnivores 2 and Ice Age data, verified file by file against
`recipes/bases/c2-vanilla.toml` and `recipes/bases/ice-age.toml`, and on
the two APK-built bases, verified against
`pc-ver/crates/c2-regress/hash-lists/`. These are
the public reference for pull requests: `run ... --baseline published`.
Only scores are published (manifest, metrics, events), with machine
details removed. Their performance figures are reference only (one
machine, not identified). See the README there.

`c2-regress publish RUN_DIR --hash-list LIST.toml [--to DIR]` makes one. The
list must be a file of this repository; the manifest names it by its path
from the repository root. It refuses failed runs, runs with `--inject`, runs
without agreeing repeats (`--repeat` of 2 or more, `determinism.identical`
true), runs of another text of the scenario than the one in
`crates/c2-regress/scenarios/`, and runs that read any file not matching the list.
It does not overwrite a baseline already in the folder unless given
`--replace`; either way it lists what changes against it (scenario, game
content, build, results, determinism, checkpoint state). A test in the `pc` CI job checks the
published baselines (current scenario version, hash lists, no machine
details) without game files.

## Comparing with a baseline

- **Functional** (fails): different scenario/game/version, a different
  scenario file text (its SHA-256: edited without a version bump), failed run, a
  different map, animals missing at the end, a weapon not fired exactly
  once (none, for the kill script: `functional.weapons_to_fire`), a kill
  that did not pass (`functional.kills`), timeouts, panics, set-up errors,
  a screen that passed in the baseline and no longer does.
- **Visual** (fails): each baseline screenshot against the candidate's
  (`visual.ran` false and `visual.not_run` say why when none were compared). A
  pixel differs when a channel is more than `tolerance` (24) apart; a
  checkpoint fails when more than `max_differing_ratio` (1%) of its
  unmasked pixels differ. Masks are rectangles in fractions of the picture,
  per checkpoint or `*`. Difference images: red = differs, dimmed =
  same, blue = masked. Different GPUs and drivers rasterise differently,
  so keep baselines per renderer.
- **Performance** (warns): only between runs with the same platform, mode,
  OS, CPU, logical CPUs, GPU adapter, resolution, build profile, scenario
  version, game content and timing settings; otherwise it is skipped with
  the reason. Provisional limits: median frame time +10%, p95 frame time
  +15%, peak memory +15% (the headless tick times use the median and p95
  limits too). Use `--repeat` of 3 or more for both runs.

### Making and approving a baseline

Nothing is ever approved automatically, and no run writes to a baseline.

1. Run the scenario on the commit to approve, on the machine and settings
   the baseline is for:
   `c2-regress run ... --repeat 5 --label baseline`.
2. Check the run by eye: `report.html`, every screenshot, `PASSED`, and
   `determinism.identical: true`.
3. Keep the folder where your candidates can find it (for example
   `regression-sim-recordings/baselines/<scenario>/<game>/<platform>/`),
   and record why it was approved (commit, date, what was checked) in a
   note beside it.
4. Later runs: `c2-regress run ... --baseline <that folder>`, or
   `c2-regress compare`.
5. To replace a baseline, repeat 1–3 into a new folder. A changed scenario
   file needs a new `version` and new baselines.

## CI

The existing `pc` job's `cargo test --workspace` runs the scenario unit
tests (synthetic area and animals, no game data): parsing, the script,
determinism, injected faults, refused set-ups, metrics, image comparison,
the PDF writer and the CLI's exit codes. Full runs need the game files and
a GPU, so they run locally (or on a self-hosted runner with the files):
`C2_ORIGINAL_DIR=<folder with HUNTDAT> cargo test -p c2-regress` adds a
headless run of the canonical scenario on Carnivores 2.

## Platforms

| Platform | State |
|---|---|
| PC, Linux: Carnivores 2 | **Implemented and run**: headless and render pass, comparison, reports. |
| PC, Linux: Ice Age | **Implemented and run**: area 4 (scenario version 2). All four animals are in view in the screenshots. |
| PC, Linux: Dinosaur Hunter and Ice Age APKs (phone HUD) | **Implemented and run**: both pass, 7 weapons, 14 screenshots, headless x5 identical. |
| PC, Windows | Should build as on Linux; not run. Process memory is unavailable there. |
| PC: Fallen Kings | **Planned** (no reference baseline yet). |
| Original engines (Wine): Carnivores 2, Ice Age | **Implemented and run**: `--engine original`; see [Original engines](#original-engines). |
| 3DS (Azahar) | **Planned.** `c2_core::scenario` builds for the consoles' `portable-hunt` target, so the 3DS app can run the same script. How Azahar would start it, read its log and capture screens has not been investigated yet. `c2-cook --check` already plays cooked packs headless on the PC. |
| Wii U (Cemu) | **Planned.** Same core module; Cemu's automation and logging not yet investigated. |
| Android | **Planned.** The app hosts `c2-pc`, so `c2-pc regress` is the likely route; storage paths and adb collection not yet worked out. |

Emulator timings would describe the emulator on the host, not the console.
They must never be compared with hardware runs or read as console
performance.

## Original engines

The same scenario in the original Carnivores 2 and Ice Age, under Wine:

```sh
pc-ver/crates/c2-regress/original/build.sh        # c2launch.exe, c2hook.dll (clang, lld-link, Wine headers)
./target/release/c2-regress run --engine original --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
    --game-id c2 --game ../source/c2-original --repeat 3 [--hook-dir DIR] [--baseline RUN_DIR]
```

How it works (`pc-ver/crates/c2-regress/original/`, `src/original/`):

- The original's menu (`carn2.exe`, `IceAge.exe`) runs the hunt as a
  second program, the renderer's `.ren` (`v_soft.ren`: software), with
  `prj=huntdat\areas\area2 reg=0 din=284 wep=63 dtm=1`: area, profile,
  a bit per menu animal slot (AI - 10), a bit per weapon, time of day. The
  runner starts `v_soft.ren` that way directly, with a fresh profile in
  the original's save format (`trophy00.sav`: 5000 points, 800x600).
- `c2launch.exe` starts it suspended and loads `c2hook.dll`, which
  re-points the game's own imports in memory (nothing on disk changes):
  - `timeGetTime`: a virtual clock, a fixed start (the game seeds `rand()`
    from it, `Game.cpp: srand(timeGetTime())`, and picks the landing from
    it) and 20 ms per frame. Frames are counted on `PeekMessageA`.
  - Keyboard and mouse (`GetKeyboardState`, `GetAsyncKeyState`,
    `GetKeyState`, `GetCursorPos`, `SetCursorPos`, and posted window
    messages): a script of key presses by frame (`script.c`). The real
    devices are never read. A key polled over and over within one frame
    is let go (the original waits for a click's release in such a loop).
  - `MessageBoxA`: the text is logged and the run ends (no hang).
  - `HeapCreate`: the game asks for a fixed-size 64 MB heap, which Ice
    Age's data does not fit under Wine ("Heap allocation error!" with or
    without the hook); it is made growable. Memory only, no game logic.
  - `CreateProcessA`: if the menu is driven instead, the hunt it starts is
    hooked too (that is how the command line above was found).
  - `BitBlt`: the software renderer presents each frame with one blit
    from its own DIB (`RenderSoft.cpp`). The hook keeps a copy of the last
    presented frame, and screenshots are taken from it, not from the
    screen. Screen captures came out black when the monitor slept or the
    session was locked. The screen is still read before the first frame.
- A copy of the game folder (without wrapper DLLs, logs and saves) and a
  Wine prefix with an 800x600 virtual desktop are kept in
  `regression-sim-recordings/_original/<game>/`. A run whose screenshots
  are not 800x600 fails: the game did not run in the virtual desktop.
- The stages are ours, as key presses with the original's default keys,
  with screenshots at the same checkpoint names. The tour does not read
  the original's memory, so its checks use the pictures: a weapon fired when its ammo
  display (top left) changed between its "ready" and "after" frames, and a
  screen opened when the frame with it open differs from the frame after
  closing it. The fire key is held two frames (40 ms), shorter than any
  fire animation, so each weapon fires once.
- Determinism: `--repeat N` compares every screenshot byte for byte.

Results (Wine 11): both originals pass every stage. All six weapons fire,
and binoculars, map and call switch open and close. Three repeats give
identical screenshots. The original has no picker screens (unsupported).

Comparing with our engine (`compare ORIGINAL_RUN OUR_RUN`, or both in
`pdf`): functional results are compared; pictures differ by design
(different renderers, and the original places its own animals), so the
visual check is skipped and the PDF shows them side by side; performance
is not comparable (virtual clock). Not yet published: the original-engine
dataset waits for approval.

### Kill animations in the original

`kill_animations.toml` also runs with `--engine original`, for the stock
games (`c2`, `ice-age`; killers by AI number). A kill can't be told
from pictures alone, and the original places its hunt animals 40 or
more cells away, so here the hook reads and writes a few of the game's
globals (`original/kill.c`):

- **Finding them.** `c2-regress` finds them in the copied `v_soft.ren`'s
  own code (`src/original/addrs.rs`), without hard-coded addresses.
  `MyHealth` is the variable most often set to `MAX_HEALTH` (128000).
  `AddDeadBody` starts by reading it. In that function the scan finds
  `ChCount`, `CameraAlpha`, `&Characters[0]` and the three `PlayerX/Y/Z`
  globals (the stores of the body's position), and it checks what it
  found. Both shipped renderers match `Hunt.h`'s `TCharacter` (156
  bytes). The addresses go to the hook as `addr` lines, and it checks
  each one against the executable's image before using it.
- **One hunt per killer**, with only that animal picked in the menu
  (`din`). After the warm-up, `release AI 600` sets the first living
  animal of that AI 600 units ahead of the hunter and sets it hunting
  (`State` 1, `AfraidTime` 20000, its target on the hunter), as our
  engine's kill script does.
- **The death** is `MyHealth` reaching 0. From then on, the hook logs the
  body (`Characters[ChCount-1]`) and the killer every frame: clip, frame
  time, position and scale. Script lines with a `D+N` frame run N frames
  after the death: the `_start`, `_mid` and `_late` screenshots (1 s and
  3 s, in 20 ms frames), then `exit`. An absolute `exit` ends a hunt in
  which nobody kills.
- **Checks** are our engine's (`KillWatch`) on the logged poses. The
  movement counts are in frames, so `min_moving_ticks` is halved.
  `--repeat` compares the screenshots and the logged poses.

Each run records `functional.kills_detail`: per kill, the time from
release to kill, the body's and the killer's clips, and whether the body
lies on the killer. Ours records the same. `compare ORIGINAL_RUN OUR_RUN`
lists their differences under `kill_parity` in `comparison.json`. They
don't fail the comparison, because each kill's own checks already do
that.

Results (2026-11-08, Wine 11, `--repeat 2`, identical repeats): all
five kills pass in both originals.

| Kill | Original: body clip, killer clip, body | Ours |
|---|---|---|
| C2 T-Rex | 3, 9, carried; 7.9 s to kill | same; 8.0 s |
| C2 Allosaurus | 0, 6, 179 units from the killer | same, 182 |
| Ice Age Woolly rhinoceros | 6, 7, on the killer | 0, 6, 150 units away |
| Ice Age Yeti | 12, 7, on the killer | 0, 6, 148 units away |
| Ice Age Diatryma | 7, 8, on the killer; stays | 0, runs on (0, 1) |

Ice Age's `AddDeadBody` differs from Carnivores 2's. The body takes the
killer's position, heading and scale, and the hunter clip `AI - 7`. The
killer then plays a kill clip of its own. Our engine gives these killers
Carnivores 2's generic kill. See `docs/DECISIONS.md` (2026-11-08).

## Limitations

- C1 is unsupported (no loader). Fallen Kings is not yet baselined.
- The kill scenario has no published baselines yet. Under the original it
  runs the stock games only (the original's menu picks animals by AI
  number; add-on and Modder's Edition packs need their own executables).
  Killers are set down beside the hunter, so how an animal finds and
  stalks him is not covered.
- Screenshots and frame times come from the PC renderer only.
- The change-gun and change-call pickers exist only in the PC frontend:
  the headless run reports them as unsupported, and the render pass checks
  them.
- Pinned animals still run their AI. Frightened ones run on the spot,
  which is fine for checks and pictures but does not show natural motion.
- Render frame time includes the GPU wait but not presentation. GPU memory
  and GPU time are not measured.
- A render pass from a `c2-pc` older than `c2-regress` can disagree with it
  (for example, an older scenario parser). Build both together.
