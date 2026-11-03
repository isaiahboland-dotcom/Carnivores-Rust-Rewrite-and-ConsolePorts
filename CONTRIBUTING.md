# Contributing

All communication happens on GitHub Issues/PRs; don't email maintainers.

Search existing issues before reporting a bug, and use the bug report form.
Keep pull requests focused, explain what changed, and describe how you tested it.
Discuss major architectural or gameplay changes in an issue first. Follow the
conventions in [the development guide](docs/DEVELOPMENT.md).

## Build and check

Install stable Rust (1.88 or newer). On Linux, install the ALSA and udev
development headers listed in the [README](README.md#what-you-need).

```sh
git clone https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts.git c2rs
cd c2rs/pc-ver
cargo build --locked --release
cargo run --locked --release -p c2-launcher --bin c2-launcher
```

Before submitting Rust changes, run formatting, Clippy and the tests from `pc-ver/`:

```sh
cargo fmt --all
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-features
```

Console builds need separate toolchains. Follow the [3DS guide](docs/3DS.md)
or the [Wii U build instructions](README.md#wii-u-from-the-command-line-demo)
for nightly Rust and devkitPro setup. State which hardware or emulator you tested.

## Files you must not share

Do not commit or attach SDK files, SDK binaries, original game assets, mod files,
or the original C++ source to issues or pull requests. Users supply their own
game and mod files; keep generated packs local. Tests must use synthetic fixtures.
Recipes may share credits, download links and hashes, as described in the
[distribution rules](docs/DEVELOPMENT.md#2-legal--distribution-rules-hard-rules).

Contributions are licensed under the project's [MIT license](LICENSE).
