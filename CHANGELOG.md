# Changelog

## Unreleased

- Launcher: all add-ons (and standalones) on/off at once; **Pick parts** per standalone game
  (areas, animals, weapons left out on PC, `c2-pc --skip`, recipe `skip`); a Menu choice for
  Android and each standalone's own menus; **Stop using** moved to the Instances page; "Use my
  APK as the game" moved into the Base game box; Instances as wrapping cards with an always
  visible scroll bar; an Interface size setting that fits the window and full screen under
  Steam's game mode / Big Picture (Steam Deck).
- Launcher: editing an instance now covers its balance (a new hunter's points, each area's,
  animal's and weapon's price and score) and **Pick parts** for each of its standalone games.

- Bound `_RES.TXT` nesting, preserve sub-blocks during writing, and reject invalid
  menu slot hints without discarding their files.
- Validate PKM payloads before allocation, bound mobile ELF scans, and reject
  cooked-pack payload ranges outside the file.
- Check texture tiling preconditions in every build and cover legacy ZIP names
  with synthetic regression tests.
- Pin CI actions, configure Dependabot, and run all-feature tests on Linux PRs
  and in a daily/manual workflow.
- Split shared hunt and survival logic into focused modules while preserving
  their public paths.

## 0.1.0 — 2026-11-02

First tagged source release of the Rust rewrite and console-port tools.

- PC gameplay, mod merging, saved recipes/instances, Ice Age and mobile conversion,
  plus 3DS cooking/build tools, a Wii U demo and experimental OpenXR support.
- Hardened archive extraction, bounded untrusted input and runtime writes, and
  staged generated-install publication. Automatic RAR/7z extraction is disabled.
- Extracted launcher workers into typed jobs with one shared handle; pending
  results cannot be overwritten. Split presentation into focused modules.
- Published a roadmap and documented the release process.

See [release notes](docs/releases/v0.1.0.md) for platform limits and validation scope.
