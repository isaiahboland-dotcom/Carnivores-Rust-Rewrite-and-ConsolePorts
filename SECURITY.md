# Security Policy

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security reports.

Use GitHub's private vulnerability reporting instead — the **Report a
vulnerability** button on the
[Security tab](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/security),
or open a
[private security advisory](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/security/advisories/new)
directly.

Reports go straight to the maintainer and are not visible publicly until
a fix is published.

## What to include

A good report helps us reproduce quickly. Please include:

- The affected crate or path (e.g. `pc-ver/crates/c2-vfs/src/zip.rs`,
  `pc-ver/crates/c2-launcher/src/jobs.rs`).
- The version or commit (tag, branch, or SHA).
- A minimal reproduction — a small ZIP, recipe TOML, or committed test
  fixture is ideal. **Do not attach original game or mod files**; see
  `CONTRIBUTING.md` and the `.gitignore`.
- The impact you observed or expect.
- Any suggested fix or pointer to an upstream advisory.

If you have already disclosed elsewhere, tell us where and when.

## Response timeline

This is a hobby project maintained outside working hours. We aim to:

- Acknowledge receipt within **5 business days**.
- Provide an initial assessment within **14 days**.
- Coordinate a fix and disclosure date with you before publishing.

These are targets, not guarantees. Critical issues get priority over
hardening follow-ups.

## Supported versions

Only the latest tagged release and the `main` branch receive security
fixes. Earlier releases are not patched separately.

| Version | Supported |
|---|---|
| `main` (HEAD) | Yes |
| Latest release tag ([releases](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/releases)) | Yes |
| Earlier release tags | No |

## Scope

**In scope:**

- Code in this repository: Rust sources, Python tools, CI workflows,
  build scripts, the launcher, the cooker, the VR frontend, the Wii U
  perf-probe, the Android frontend, and the 3DS port.
- File-format parsers and VFS layers under `pc-ver/crates/c2-formats/`
  and `pc-ver/crates/c2-vfs/` when fed untrusted inputs (ZIP archives,
  cooked packs, `.CAR`/`.MAP`/`.RSC`/`.3DF`/`.TGA` files, MEE folders,
  mobile APK-extracted metadata).
- The launcher's install/extract transaction code under
  `pc-ver/crates/c2-launcher/` (path traversal, atomicity, recovery).
- Native `unsafe` blocks in `pc-ver/crates/c2-vr/`,
  `pc-ver/crates/c2-mods/src/pack.rs`, `pc-ver/crates/c2-vfs/src/zip.rs`,
  and the 3DS/Wii U targets.
- CI supply-chain issues: action SHA pins, workflow permissions, secret
  handling.

**Out of scope:**

- The original **Carnivores 2** game or any mod files. Users supply
  their own game and mod files; this project contains neither. Reports
  about the original game's behaviour belong in the
  [DECISIONS](docs/DECISIONS.md) log, not as security reports.
- Homebrew console SDKs (devkitPro, wut, Aroma), Nintendo/Valve/Sony
  platform SDKs, or third-party runtimes (Wine, Monado). Report those
  upstream.
- Vulnerabilities in dependencies. Those belong in the upstream crate's
  advisory database and are picked up by `cargo audit`. Open an issue
  here only if our lockfile pins a known-vulnerable version we have not
  yet updated.
- Behaviour parity deviations from the original engine. Those are bugs,
  not security issues — see [docs/REGRESSION.md](docs/REGRESSION.md).

If in doubt, report privately. We will tell you if it is out of scope.

## Hardening record

This policy covers **disclosure**. The project's existing hardening work
— archive size and entry-count limits, path-traversal rejection,
transactional installs, parser bounds, VR unsafe-block invariants — is
documented in [docs/SECURITY-HARDENING.md](docs/SECURITY-HARDENING.md),
along with the audits that remain open.

## Acknowledgements

With your permission, reporters are credited in the release notes for
the fix's release. Anonymous reports stay anonymous.
