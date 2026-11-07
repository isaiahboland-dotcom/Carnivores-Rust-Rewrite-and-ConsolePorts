# Published reference baselines

Results of the regression scenarios on the **vanilla games**: the retail
Carnivores 2 and Carnivores: Ice Age data, unmodified, run through this
engine. Compare your own runs against them, for example before opening a
pull request:

```sh
cd pc-ver
cargo build --release -p c2-pc -p c2-regress
./target/release/c2-regress run --scenario crates/c2-regress/scenarios/vanilla_map2_four_dinos.toml \
    --game-id c2 --game <your Carnivores 2 folder> --repeat 3 --baseline published
```

`--baseline published` picks `baselines/<scenario>/<game>/` for the scenario
and game you run. See `docs/REGRESSION.md` for the scenario and the
comparison.

## What is here

| Folder | Game | Game files checked against |
|---|---|---|
| `vanilla_map2_four_dinos/c2/` | Carnivores 2 | `recipes/bases/c2-vanilla.toml` (all 25 files the scenario reads) |
| `vanilla_map2_four_dinos/ice-age/` | Carnivores: Ice Age | `recipes/bases/ice-age.toml` (all 22 files the scenario reads) |
| `vanilla_map2_four_dinos/c2-apk/` | Dinosaur Hunter (Android 2.0.1), rebuilt with `c2-pc mobile-base` | `pc-ver/crates/c2-regress/hash-lists/dinosaur-hunter-apk-2.0.1.toml` (79 files, the phone HUD included) |
| `vanilla_map2_four_dinos/ice-age-apk/` | Ice Age (Android 2.0.1), likewise | `pc-ver/crates/c2-regress/hash-lists/ice-age-apk-2.0.1.toml` (64 files) |

For the APK games, build the game folder first
(`c2-pc mobile-base <APK> --out <DIR>`) and pass it as `--game` with
`--game-id c2-apk` or `ice-age-apk`.

Each holds `manifest.json` (scenario, game content hashes, seed, settings,
build), `metrics.json` (stage results, weapons fired, animals, UI screens,
timings, memory, determinism, checkpoint state hashes) and `events.jsonl`
(every action and check by tick).

Not included: screenshots (they are pictures of the game's own art), logs,
and anything that identifies the machine (CPU, GPU, driver). There are no
game files here, only their SHA-256 hashes, as in the recipes.

## Reading a comparison against them

- **Functional** results are the reference: every stage should pass with
  the same counts. A failure here is a regression (or a change to explain
  in the pull request).
- **Content**: `content_matches_baseline: false` means your game files are
  not the same release. Expect differences for that reason alone.
- **State hashes**: identical when your build simulates exactly as the
  baseline build did. Differences on another OS or CPU architecture can
  come from floating-point maths alone, so treat them as a hint on the same
  `os`/`arch`, and as information elsewhere.
- **Performance** figures come from one reference machine and are shown
  for reference, never as warnings. Compare performance against your own
  baselines on your own machine.
- **Visual**: no published screenshots, so visual checks are local only.

## Updating

Baselines are approved by the maintainer and never updated by a run.
After a deliberate change (a new scenario `version`, an intended behaviour
change), re-run on the vanilla games and publish:

```sh
./target/release/c2-regress run ... --repeat 5 --label vanilla-baseline
./target/release/c2-regress publish <run folder> --hash-list ../recipes/bases/c2-vanilla.toml
```

APK baselines also need re-publishing when `c2-pc mobile-base` output
changes: regenerate their hash lists first (`docs/REGRESSION.md`, APK-built
bases).

`publish` refuses runs that failed, carry a deliberate fault (`--inject`),
or read any game file that is not in the hash list with the same hash.
`cargo test -p c2-regress` checks that the published baselines match the
current scenario version and hash lists.
