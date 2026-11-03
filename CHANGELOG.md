# Changelog

## Unreleased

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
- Add contribution instructions and a structured bug report form.

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
