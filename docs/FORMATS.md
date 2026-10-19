# Carnivores 2 file formats

Verified against the original source (`source/c2-source/`, mirror of the 2013
Assembla release) and against real files: every `.CAR`, `.3DF`, `.MAP`, `.RSC`
and `_RES.TXT` in the vanilla install parses
**and writes back byte-for-byte** (`c2-parity scan`, see bottom).

Conventions: little-endian; `i32`/`f32` are 4 bytes, `i16`/`u16` 2 bytes.
"Texture" means 16-bit pixels, `xRRRRRGGGGGBBBBB` (1-5-5-5, top bit unused on
disk). Parsers return on-disk values; the load-time transforms listed under
each format are the engine's job.

Source references are `File.cpp: Function`.

---

## Shared records (`Hunt.h`)

### Vertex — `TPoint3d`, 16 bytes
| off | type | field |
|---|---|---|
| 0 | f32 ×3 | x, y, z |
| 12 | i16 | owner (bone index) |
| 14 | i16 | hide |

### Face — `TFace`, 64 bytes
| off | type | field |
|---|---|---|
| 0 | i32 ×3 | v1, v2, v3 |
| 12 | i32 ×3 | tax, tbx, tcx (texel u) |
| 24 | i32 ×3 | tay, tby, tcy (texel v) |
| 36 | u16 | Flags (`sf*`) |
| 38 | u16 | DMask |
| 40 | i32 ×3 | Distant, Next, group |
| 52 | u8 ×12 | reserved |

Face flags: `DoubleSide 0x1`, `DarkBack 0x2`, `Opacity 0x4`, `Transparent 0x8`,
`Mortal 0x10`, `Phong 0x30`, `EnvMap 0x50`, `NeedVC 0x80`, `Dark 0x8000`.

Texture coordinates are integer texel positions on disk. `CorrectModel` converts
them: D3D build `float(i) / 256`, software build `(i << 16) + 0x8000`.

### Bone — `TObj`, 48 bytes
`char name[32]`, `f32 x, y, z`, `i16 owner`, `i16 hide`.

### Model load-time transforms (`LoadModel`, `LoadModelEx`, `LoadCharacterInfo`)
- vertices: `x *= 2`, `y *= 2`, `z *= -2`
- `BrightenTexture` on the texture
- `CorrectModel`: faces without `DoubleSide` get `NeedVC`; faces re-sorted
  opaque → `Opacity` → `Transparent`
- texture height = `TextureSize >> 9` (always 256 px wide); D3D/3dfx builds
  allocate 256 rows regardless

---

## `.3DF` — static model (`LoadModelEx`)
Used for `SUN2.3DF`, `MOON.3DF`, `COMPAS.3DF`, `BINOCUL.3DF`.

```
i32 VCount, FCount, OCount, TextureSize(bytes)
Face   [FCount]
Vertex [VCount]
Bone   [OCount]
u16    texture[TextureSize/2]
```
File ends exactly after the texture.

## `.CAR` — animated character (`LoadCharacterInfo`)
Dinosaurs, hunter, ship, weapons (`HUNTDAT/WEAPONS/*.CAR`).

```
char   name[32]            usually 24-byte name + editor tag like "msc: 3"
i32    AniCount, SfxCount
i32    VCount, FCount, TextureSize(bytes)       (no OCount, unlike .3DF)
Face   [FCount]
Vertex [VCount]
u16    texture[TextureSize/2]
AniCount × { char name[32]; i32 KPS; i32 Frames; i16 data[Frames × VCount × 3] }
SfxCount × { char name[32]; i32 length; u8 pcm[length] }
i32    Anifx[64]          OPTIONAL: if the read is short, all -1
```
- Animation data is frame-major, `(x, y, z)` per vertex. Original computes
  `AniTime = Frames * 1000 / KPS` (divides by zero if KPS is 0 — guard it).
- Sound PCM is headerless 16-bit mono; sample rate is fixed by the audio layer
  (to be confirmed in `Audio.cpp` during M3).
- `Anifx[a]` = sound index played with animation `a`, -1 = none.

## `.MAP` — area grids (`LoadResources`, "Load MAPs file")
Fixed size: **14,155,776 bytes**. All grids row-major `[y][x]`.

| order | grid | type | size |
|---|---|---|---|
| 1 | HMap — terrain height (×`ctHScale`=64 in engine) | u8 | 1024² |
| 2 | TMap1 — texture index | u16 | 1024² |
| 3 | TMap2 — second texture index | u16 | 1024² |
| 4 | OMap — object index (RSC model), 255 = none | u8 | 1024² |
| 5 | FMap — cell flags | u16 | 1024² |
| 6 | LMap ×3 — light map: day, dusk, night | u8 | 3×1024² |
| 7 | WMap — water index | u8 | 1024² |
| 8 | HMapO — object height | u8 | 1024² |
| 9 | FogsMap — fog zone | u8 | 512² |
| 10 | AmbMap — ambient zone | u8 | 512² |

Time of day (`OptDayNight`): 0 day, 1 dusk, 2 night. The engine reads only the
matching `LMap` layer. After loading, `FogsMap` cells are set to 1 where any
covered height is below fog 1's `YBegin` (if `YBegin > 1`).

## `.RSC` — area resources (`LoadResources`)
```
i32 tc, mc                               texture count, model count
i32 FadeRGB[3][3]                        [time of day][r,g,b]
i32 TransRGB[3][3]
tc × u16 texture[128×128]                terrain textures (LoadTexture)
mc × Object
u16 sky[3][256×256]                      day, dusk, night (LoadSky)
u8  skymap[128×128]                      (LoadSkyMap)
i32 FgCount;  TFogEntity[FgCount]        stored as FogsList[1..]; entry 0 = no fog
i32 RdCount;  RdCount × { i32 len; u8 pcm[len] }           random sounds
i32 AmbCount; AmbCount × Ambient
i32 WtrCount; TWaterEntity[WtrCount]
```
File ends exactly after the water list.

**Object**
```
TObjInfo (64 bytes):
  i32 Radius, YLo, YHi, linelenght, lintensity, circlerad, cintensity,
      flags, GrRad, DefLight, LastAniTime
  f32 BoundR
  u8  res[16]
<model block, same layout as a whole .3DF file>
u16 sprite[128×128]                      distance billboard (LoadBMPModel)
if flags & ofANIMATED (0x80000000):      (LoadAnimation)
  i32 vcount (ignored), i32 vcount, i32 KPS, i32 Frames
  i16 data[(Frames + 1) × vcount × 3]    ← original does FramesCount++ first
```
Object flags: `PLACEWATER 1`, `PLACEGROUND 2`, `PLACEUSER 4`, `CIRCLE 8`,
`BOUND 16`, `NOBMP 32`, `NOLIGHT 64`, `DEFLIGHT 128`, `GRNDLIGHT 256`,
`NOSOFT 512`, `NOSOFT2 1024`, `ANIMATED 0x80000000`.
Load-time: `Radius`, `YLo`, `YHi` ×2; `linelenght` rounded down to a multiple
of 128; `BoundR` recomputed from vertices.

**TFogEntity** (20 bytes): `i32 fogRGB (0x00RRGGBB)`, `f32 YBegin`,
`i32 Mortal`, `f32 Transp`, `f32 FLimit`.

**Ambient**: `i32 len; u8 pcm[len]; TRD rdata[16]; i32 RSFXCount; i32 AVolume`
where `TRD` (16 bytes) = `i32 RNumber, RVolume, RFreq; u16 REnvir, Flags`.
At night the engine drops `rdata` entries with non-zero `Flags`.

**TWaterEntity** (16 bytes): `i32 tindex, wlevel; f32 transp; i32 fogRGB`
(`fogRGB` is overwritten at load with the average colour of texture `tindex`).

---

## `_RES.TXT` — script (`LoadResourcesScript`, `ReadWeapons`, `ReadCharacters`)
CRLF text. Top-level sections `name { ... }`, each containing `{ ... }` blocks
of `key = value` lines, or (for `prices`) bare `key = value` lines. A line
starting with `.` ends the file. `//` starts a comment.

Original parsing rules (and how `c2-formats` matches them):
- Values: C `atoi`/`atof` prefix parsing (`"255.0.0"` → 255). **Matched.**
- Strings: from the first `'` with the last two chars chopped. We take the
  text between the first and last quote. **Equivalent on all known data.**
- Keys are matched with `strstr` (substring). We match exact keys plus the
  aliases the substring rule produced in practice (`hearK`→`hear`,
  `lookK`→`look`). **Deviation**, see `DECISIONS.md`. `c2-parity` includes a
  substring-faithful copy of the original reader and diffs every field; no
  known file differs.
- `danger` is true if the line exists at all (`danger = FALSE` is still true).
  **Matched.**
- `trace` is stored as `value - 1` (`TraceC`). We keep the written value.
- Unset fields: the engine has defaults in `Game.cpp` (e.g. `Scale0 = 800`,
  `ScaleA = 600`). Parser fields are `Option` so `c2-core` can apply them.

**weapons**: `name file pic power prec loud rate shots reload trace optic fall`
**characters**: `name file pic ai health mass length radius basescore smell
hear look shipdelta scale0 scaleA danger`
**prices**: `start`, then repeated `area`, `dino`, `weapon`, `acces` (sic).
The released source **does not parse `prices`** (its menu code is missing),
but every retail and mod `_RES.TXT` has it.

Keys the original engine doesn't read are kept in each def's `extra` list, so
mods that add their own keys survive a parse/merge/write cycle.

---

## `.TGA` — images (`LoadPictureTGA`)
The original reads `u16 width, height` at offset 12, then `width × height`
raw `u16` pixels from offset 18, and always flips rows (assumes bottom-up). It
ignores the ID length, image type, bit depth and origin bit.

All 64 vanilla files are type 2 (uncompressed), 16-bit, no ID field,
bottom-up (descriptor 0 or 1). For those, `c2_formats::tga` produces exactly
the original's pixels (checked by `c2-parity scan`). It also honours the
header for other encodings (ID field, top-left origin, RLE type 10, 24/32-bit),
which the original would have misread. Pixels are `ARRRRRGGGGGBBBBB`; the top
bit is kept as stored (the D3D path drops it in `conv_565`).

## `.WAV` — sounds (`LoadWav`)
The original seeks to offset 36, scans byte-by-byte for `data`, reads an
`i32` length and that many PCM bytes. It never reads `fmt `; the audio layer
assumes **16-bit mono 22050 Hz**, which all 41 vanilla files are.
`c2_formats::wav` does the same scan, clamps an over-long length to the bytes
present (as `ReadFile` would), and also reports the `fmt ` chunk so the cooker
can flag or resample anything else.

## `trophy0N.sav` — trophy room + options (`LoadTrophy`, `SaveTrophy`)
1660 bytes: `TTrophyRoom` (1516: `char PlayerName[128]`, `i32 RegNumber, Score,
Rank`, `TStats Last, Total` (`i32 smade, success; f32 path, time`), 24 ×
`TTrophyItem` (`i32 ctype, weapon, phase, height, weight, score, date, time;
f32 scale, range; i32 r1..r4`)), then 10 option ints (`OptAgres, OptDens,
OptSens, OptRes, FOGENABLE, OptText, OptViewR, SHADOWS3D, OptMsSens,
OptBrightness`), `KeyMap` (17 virtual-key ints), `REVERSEMS`, `ScentMode,
CamoMode, RadarMode, Tranq, OPT_ALPHA_COLORKEY, OptSys, OptSound, OptRender`.
Dates are `(year<<20)+(month<<10)+day`, times `(hour<<10)+minute`.
`KeyMap` entries are Windows virtual-key codes (`1`/`2` = left/right mouse).

## `MENU/*_MAP.RAW` — menu hit-maps
120000 bytes: 400 × 300, top-down, one byte per pixel = clickable region id
(0 = none), at half the 800 × 600 resolution of the screen picture it
belongs to. A region's lit look is the same pixels of the screen's `*_ON.TGA`.
Regions in vanilla: `MAIN_MAP` 1 Hunt, 2 Options, 3 Trophy, 4 Credits, 5 Quit,
6 Statistics; `M2_MAP` (hunt setup) 1-3 Dawn/Day/Night, 4 (blank box),
5 Tranquilizer, 6 Observer mode, 7 Back, 8 Hunt; `OPT_MAP` 1 Game, 2 Controls,
3 Video, 4 Back; `MQ_MAP` (quit) / `ML_MAP` (waiver) / `MR_MAP` (register) /
`MD_MAP` (delete) 1-2 = the two buttons, left to right. Read by
`c2-pc/src/menu.rs`; no parser in `c2-formats` (it's raw bytes).

## `.c2pk` — cooked 3DS pack (ours, `c2_formats::cooked`)
Written by `c2-cook`, read by `c2-3ds` through `c2_vfs::PackBackend`. Not an
original format. All values little-endian.

| Offset | Type | Field |
|---|---|---|
| 0 | `[u8; 4]` | `"C2PK"` |
| 4 | `u32` | version (1) |
| 8 | `u32` | entry count |
| 12 | `u32` | index length in bytes |
| 16 | index | per entry: `u16` name length, name (UTF-8, `/`-separated), `u32` offset from file start, `u32` size |
| … | data | entries, each starting on a 16-byte boundary |

Entries:
- `HUNTDAT/...`: game files in their original formats: the merged
  `_RES.TXT` and the area's `.MAP` as they are; the area's `.RSC` and the
  chosen `.CAR`s slimmed (pack version 2): no terrain textures (texture
  count 0), empty model textures, and sounds with no samples (the entries
  stay, so indices hold).
- `C2RS/INFO.TXT`: `key = value` lines: `pack` (2), `target`, `area` (e.g.
  `AREA1`), `time`, `dinos` / `weapons` (`_RES.TXT` indices),
  `terrain_tile`, `atlas_cols`, `terrain_count`, `view`, `object_view`
  (cells).
- `C2RS/TERRAIN.TX`: every terrain texture in one atlas, `terrain_tile`
  pixels square, `atlas_cols` per row, in texture order.
- `C2RS/OBJn.TX`, `C2RS/CHARn.TX`, `C2RS/WPNn.TX`: the texture of RSC object
  `n`, `_RES.TXT` character `n`, weapon `n`.

### `.TX` — cooked texture
| Offset | Type | Field |
|---|---|---|
| 0 | `[u8; 4]` | `"C2TX"` |
| 4 | `u16`, `u16` | width, height (powers of two, 8..=1024) |
| 8 | `u8` | PICA200 format: 2 = RGBA5551, 3 = RGB565 (`GPU_TEXCOLOR`) |
| 9 | 3 bytes | padding |
| 12 | texels | 2 bytes each, in the GPU's tiled order |

Tiled order: 8x8 tiles left to right, tile rows starting from the **bottom**
of the image; inside a tile, texels in Morton (Z) order (bit 0 = x0, bit 1 =
y0, bit 2 = x1, …, y counted upwards). The GPU then samples it with `t` equal to the image's
`v` (0 = top), as checked in an emulator. Cut-out alpha is 1 bit (texel colour 0 in an `sfOpacity`
model, as `GenerateAlphaFlags`).

## Not yet parsed
- Map-editor side files in some mods (`.grp .lai .mtc .prj`) — not loaded by the game.

## Verifying against your own data
```
cd pc-ver
cargo run --release -p c2-parity -- scan ../source/c2-original ../source/mods
```
Files a mod ships that the game never loads (editor leftovers, backups) may
fail; only paths the game loads (`HUNTDAT/...`) need to pass.
