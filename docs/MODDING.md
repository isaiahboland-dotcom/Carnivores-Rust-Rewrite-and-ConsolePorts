# Modding

> Status: design only. The mod system lands in milestone M4, recipes in M5.
> The schemas are specified in the top-level `README.md` (§5 `mod.toml`,
> §6 recipes); this page will become the user-facing guide once they exist.

## Scope
This release supports **add-on mods**: data-only mods that add or replace a
limited set of files on top of vanilla (new dinos, weapons, areas, textures).
Whole-game total conversions are out of scope.

## What already works
`c2-parity scan <dir>` parses every `.CAR`, `.3DF`, `.MAP`, `.RSC` and
`_RES.TXT` under a directory and reports anything malformed — a quick way to
check a mod's files before the mod system exists.

## Planned workflow
1. Put each mod in its own folder under `source/mods/` (if a mod ships as a
   full game copy, files identical to vanilla are detected and ignored).
2. `c2rs mod detect <dir>` writes an inferred `mod.toml` you can edit.
3. Combine mods with a recipe (`recipes/*.toml`), which lists mods by URL and
   file hash and never contains mod files.
