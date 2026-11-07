## What changed

<!-- What the change does and why. Link the issue if there is one. -->

## Kind of change

- [ ] Bug fix (behaviour changes on purpose)
- [ ] Optimization or refactor (behaviour must not change)
- [ ] New feature or option
- [ ] Documentation, recipes or tooling only

## Checks

- [ ] `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`
      and `cargo test --locked --workspace --all-features` pass in `pc-ver/`
- [ ] Gameplay, AI, rendering or loading changes: regression scenario run against a
      baseline from the base commit and against `--baseline published`
      ([docs/REGRESSION.md](../docs/REGRESSION.md#validating-a-change)), exit code 0
- [ ] Optimization or refactor: checkpoint state hashes identical to the baseline
- [ ] Bug fix: the checkpoints whose hashes changed, and why, are listed below
- [ ] Deviations from the original game are recorded in `docs/DECISIONS.md`
- [ ] No game files, mod files, original source, logs or machine identifiers are included

## Testing

<!-- Games and platforms run (PC Linux/Windows, 3DS hardware or emulator, Wii U, Android, VR),
     what was played by hand, and anything not tested. Performance claims: median and p95 from
     the comparison, both runs with repeat 3 or more on the same machine. -->
