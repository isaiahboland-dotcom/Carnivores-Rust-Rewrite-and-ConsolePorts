# Wii U tools (not in git)

The probes build against these. They are third-party, so they are fetched
here by hand rather than committed:

| Folder | What | Where from | Tested at |
| --- | --- | --- | --- |
| `wut/` | Rust bindings for WUT (archived), custom PowerPC target | <https://github.com/rust-wiiu/wut> | `1c3dc300f6de376370fb56df71f2bca1b6986b32` |
| `devkitpro-wut/` | devkitPro's WUT (Wii U SDK headers and libraries) | <https://github.com/devkitPro/wut> | `937f0d6901f5e2a72672154a674ec47b63e2f417` |
| `cafeglsl/` | CafeGLSL 0.3.0, GLSL to GX2 shader compiler (`bin/glslcompiler`) | its release (see its own README and LICENSE) | 0.3.0 |

```sh
git clone https://github.com/rust-wiiu/wut tools/wut
git -C tools/wut checkout 1c3dc300f6de376370fb56df71f2bca1b6986b32
git clone https://github.com/devkitPro/wut tools/devkitpro-wut
git -C tools/devkitpro-wut checkout 937f0d6901f5e2a72672154a674ec47b63e2f417
```

See `../runtime-investigation.md` for how they are used.
