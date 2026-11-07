//! `c2-cook`: turns your game (vanilla, or with add-on mods / a recipe) into
//! one pack file for the 3DS: every area, animal and weapon as game files,
//! their textures and sounds already converted for the 3DS, and the menu
//! pictures. The 3DS loads only what is picked for a hunt.
//!
//! ```text
//! c2-cook --game ../source/c2-original [--overlay DIR]... [--mod DIR]...
//!         [--recipe R.toml --mods DIR] [--balance FILE] [--areas 1,2] [--target n3ds|o3ds]
//!         [--force] --out c2rs.c2pk
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{bail, Context, Result};
use c2_core::area::TimeOfDay;
use c2_core::color::image_to_rgba8;
use c2_core::dino::DinoInfo;
use c2_core::game::WeaponInfo;
use c2_core::model::PreparedModel;
use c2_core::sound::SAMPLE_RATE;
use c2_formats::cooked::{tile, write_pack, write_texture, TexFormat};
use c2_vfs::{MemBackend, Vfs};

mod check;
mod wiiu;

const USAGE: &str = "usage:
  c2-cook --game <GAME_DIR> [--overlay DIR]... [--mod MOD]...
          [--recipe RECIPE.toml --mods DIR...] [--balance FILE] [--areas N,N]
          [--phone-game DIR --phone-pictures weapons,animals,scope]
          [--target n3ds|o3ds] [--force] --out <PACK.c2pk>

  c2-cook --game <GAME_DIR> ... --target wiiu [--title NAME] --out <DIR>
                                the game as the Wii U reads it: a folder of the
                                merged game files (upper-case names), with
                                INFO.TXT and an index, for `wiiu/apps/<name>/`

  c2-cook --check <PACK.c2pk>   loads a pack as the 3DS does and plays a short
                                hunt for each animal (finds missing files);
                                with --bench NAME, that animal up close, killed

  --overlay  a game folder copied over GAME_DIR first (e.g. the Modder's
             Edition, which most add-ons are made for); repeatable
  --balance  prices and scores over the game (a balance file; see `c2-pc balance`)
  --menu-art the menu style: a game folder whose title art is used (Ice Age's)
  --phone-game  a game built from your APK (`c2-pc mobile-base`): with
             --phone-pictures, its weapon pictures, animal pictures (and call
             icons from them) and scope mask replace the game's own, matched
             by model file
  --areas    only these areas (default: all, plus the trophy room)
  --target   n3ds (New 3DS, default) or o3ds (Old 3DS / 2DS: smaller textures, shorter view)
  --force    write the pack even if it is over the target's memory budget

Copy the pack to sdmc:/3ds/c2rs/ on the SD card. Several packs (say
c2rs.c2pk and modded.c2pk) can sit side by side: the app asks which one.";

/// What a target can hold.
struct Profile {
    name: &'static str,
    /// Terrain tile size in the atlas (the originals are 128x128).
    terrain_tile: usize,
    /// Largest side for object / character / weapon textures (originals 256).
    model_tex: usize,
    /// View distance in cells.
    view: u32,
    /// Ground at full detail up to this many cells; past it, a quad per
    /// 2x2 cells (the original past `ctViewR1`, `renderd3d.cpp`).
    detail: u32,
    /// Objects are drawn up to this many cells away.
    object_view: u32,
    /// Main memory (heap) the app may use while loading, KiB.
    heap_kb: usize,
    /// GPU (linear) memory for textures, vertex buffers and sounds, KiB.
    linear_kb: usize,
    /// Sound sample rate (the originals are 22050 Hz).
    sound_rate: u32,
    /// A character's animations at most this big (KiB); bigger ones keep
    /// every other frame until they fit (add-ons can be huge).
    anim_kb: usize,
}

const N3DS: Profile = Profile {
    name: "n3ds",
    terrain_tile: 64,
    model_tex: 128,
    // The phone's 76 cells (`Game::Process`), drawn by the 3DS's frustum
    // culled chunks; full detail to the 42 drawn before.
    view: 76,
    detail: 42,
    object_view: 28,
    // ~124 MiB for the app: 24 MiB linear (`c2-3ds` sets it), code, slack.
    heap_kb: 96 * 1024,
    linear_kb: 22 * 1024,
    sound_rate: SAMPLE_RATE,
    anim_kb: 6 * 1024,
};

const O3DS: Profile = Profile {
    name: "o3ds",
    terrain_tile: 32,
    model_tex: 64,
    // The owner's Old 3DS ran 26 / 18 at 60 fps with time to spare.
    // Farther than the 34 drawn before (full detail to there): a guess
    // until measured on hardware.
    view: 56,
    detail: 34,
    object_view: 24,
    // 64 MiB for the app: 24 MiB linear, code, slack.
    heap_kb: 34 * 1024,
    linear_kb: 22 * 1024,
    sound_rate: SAMPLE_RATE,
    anim_kb: 2 * 1024,
};

/// Every other frame of each animation, timing kept (the engine blends
/// between frames, so motion stays smooth, with less detail).
fn halve_frames(car: &mut c2_formats::car::Car) {
    let n = car.mesh.vertices.len() * 3;
    for a in &mut car.animations {
        if a.frames < 4 || n == 0 {
            continue;
        }
        let keep = a.frames.div_ceil(2);
        let mut data = Vec::with_capacity(keep * n);
        for f in (0..a.frames).step_by(2) {
            if let Some(fr) = a.data.get(f * n..(f + 1) * n) {
                data.extend_from_slice(fr);
            }
        }
        // Same length: frames / kps.
        a.kps = ((i64::from(a.kps) * keep as i64 + a.frames as i64 / 2) / a.frames as i64).max(1)
            as i32;
        a.frames = data.len() / n;
        a.data = data;
    }
}

fn anim_bytes(car: &c2_formats::car::Car) -> usize {
    car.animations.iter().map(|a| a.data.len() * 2).sum()
}

/// Counts heap use, so the cooker can measure what loading a pack takes.
struct Counting;

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn note(grow: usize, shrink: usize) {
    let now = IN_USE.fetch_add(grow, Ordering::Relaxed) + grow;
    PEAK.fetch_max(now, Ordering::Relaxed);
    IN_USE.fetch_sub(shrink, Ordering::Relaxed);
}

// A GlobalAlloc must be unsafe; it only forwards to the system allocator.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            note(l.size(), 0);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        note(0, l.size());
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            // Old and new blocks briefly coexist.
            note(new, l.size());
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Peak heap bytes for loading the pack the way `c2-3ds` does
/// (`Game::load` in `c2-3ds/src/hunt.rs`; keep the two in step), with the
/// models in `models` (by `model_key`) and weapons in `weapons`. Measured
/// here on the PC, where pointers are wider, so it errs on the high side.
fn load_peak(
    entries: &[(String, Vec<u8>)],
    area: &str,
    dialect: bool,
    models: &[String],
    weapons: &[usize],
) -> Result<usize> {
    let mut mem = MemBackend::new();
    for (name, data) in entries {
        mem.insert(name, data.clone());
    }
    let mut vfs = Vfs::new();
    vfs.push("pack", Box::new(mem));
    let base = IN_USE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let area_data = c2_core::load::area_with(&vfs, area, TimeOfDay::Day, true)?;
    let res = if dialect {
        let raw = c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(
            &vfs.read("HUNTDAT/_RES.TXT")?,
        ))?;
        let h = c2_formats::mee::Hunt {
            area: c2_formats::mee::area_of(area),
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        };
        c2_core::mee::read(&raw, &h).res
    } else {
        c2_core::load::res(&vfs)?
    };
    let keys: Vec<String> = res
        .characters
        .iter()
        .map(|d| c2_formats::cooked::model_key(&DinoInfo::from_def(d).file))
        .collect();
    let s = c2_core::load::species_with(&vfs, &res, &|i| {
        keys.get(i).is_some_and(|k| models.contains(k))
    });
    let w = c2_core::load::weapons_with(&vfs, &res, &|i| weapons.contains(&i));
    let loaded = (area_data, res, s, w);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    drop(loaded);
    Ok(peak)
}

#[derive(Default)]
struct Args {
    game: Option<PathBuf>,
    check: Option<PathBuf>,
    bench: Option<String>,
    overlays: Vec<PathBuf>,
    mods: Vec<PathBuf>,
    recipe: Option<PathBuf>,
    /// `--mods` (repeatable): downloads folders, or a mod's own folder.
    mods_dirs: Vec<PathBuf>,
    balance: Option<PathBuf>,
    /// Menu style: the game folder whose title art is used (Ice Age's).
    menu_art: Option<PathBuf>,
    areas: Option<String>,
    target: Option<String>,
    /// The game's name on the Wii U's title screen.
    title: Option<String>,
    /// A game built from the APK, and which of its pictures to use.
    phone_game: Option<PathBuf>,
    phone_pictures: Option<String>,
    force: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || {
            it.next()
                .with_context(|| format!("{arg} needs a value\n\n{USAGE}"))
        };
        match arg.as_str() {
            "--game" => a.game = Some(val()?.into()),
            "--mod" => a
                .mods
                .extend(c2_mods::expand_addons(std::path::Path::new(&val()?))),
            "--overlay" => a.overlays.push(val()?.into()),
            "--check" => a.check = Some(val()?.into()),
            "--bench" => a.bench = Some(val()?),
            "--recipe" => a.recipe = Some(val()?.into()),
            "--mods" => a.mods_dirs.push(val()?.into()),
            "--balance" => a.balance = Some(val()?.into()),
            "--menu-art" => a.menu_art = Some(val()?.into()),
            // Older launchers' survival choices: survival is always
            // Classic / Modded now.
            "--mobile-survival" | "--survival-modes" => {
                val()?;
            }
            "--areas" => a.areas = Some(val()?),
            "--target" => a.target = Some(val()?),
            "--title" => a.title = Some(val()?),
            "--phone-game" => a.phone_game = Some(val()?.into()),
            "--phone-pictures" => a.phone_pictures = Some(val()?),
            "--out" => a.out = Some(val()?.into()),
            "--force" => a.force = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}\n\n{USAGE}"),
        }
    }
    Ok(a)
}

/// The game as the engine would see it: vanilla (with any overlays), plus
/// mods or a recipe.
/// The game, add-ons and recipe merged, with the balance file's prices and
/// scores over them (`c2_mods::balance`, as `c2-pc play --balance`).
fn merged(a: &Args, game: &Path) -> Result<Vfs> {
    let mut vfs = merged_game(a, game)?;
    if let Some(f) = &a.balance {
        let b = c2_mods::balance::Balance::load(f)?;
        let layer = c2_mods::balance::apply(&vfs, &b)
            .with_context(|| format!("applying {}", f.display()))?;
        vfs.push("balance", Box::new(layer));
    }
    if let (Some(dir), Some(want)) = (&a.phone_game, &a.phone_pictures) {
        let layer = phone_pictures(&vfs, dir, want)
            .with_context(|| format!("taking pictures from {}", dir.display()))?;
        vfs.push("phone pictures", Box::new(layer));
    }
    Ok(vfs)
}

/// `--phone-pictures`: the phone game's pictures in place of the game's
/// own (owner, 2026-11-01): weapons (`MENU/PICS/WEAPONn.TGA`) and animals
/// (`DINOn.TGA`) where both games have the model, matched by its file
/// (the names differ: "Moshops", "Moschops"); the call icons from the
/// animals' pictures (the phone has none); the scope's mask
/// (`c2_mods::mobile::SCOPE`).
fn phone_pictures(vfs: &Vfs, dir: &Path, want: &str) -> Result<MemBackend> {
    let phone = c2_mods::vanilla(&[dir.to_path_buf()])?;
    let want = |k: &str| want.split(',').any(|w| w.trim().eq_ignore_ascii_case(k));
    let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    let pics = "HUNTDAT/MENU/PICS";
    let mut layer = MemBackend::new();
    let (mine, theirs) = (rules(vfs)?, rules(&phone)?);
    if want("weapons") {
        let files = |r: &Rules| -> Vec<String> {
            r.res
                .weapons
                .iter()
                .map(|w| WeaponInfo::from_def(w).file)
                .collect()
        };
        let theirs = files(&theirs);
        for (i, f) in files(&mine).iter().enumerate() {
            let Some(j) = theirs.iter().position(|g| same(f, g)) else {
                continue;
            };
            if let Ok(d) = phone.read(&format!("{pics}/WEAPON{}.TGA", j + 1)) {
                layer.insert(&format!("{pics}/WEAPON{}.TGA", i + 1), d);
            }
        }
    }
    if want("animals") {
        for (k, f) in &mine.slot_files {
            let Some((m, _)) = theirs.slot_files.iter().find(|(_, g)| same(f, g)) else {
                continue;
            };
            let Ok(d) = phone.read(&format!("{pics}/DINO{}.TGA", m + 1)) else {
                continue;
            };
            if let Some(icon) = square_middle(&d) {
                layer.insert(&format!("{pics}/CALL{}.TGA", k + 1), icon);
            }
            layer.insert(&format!("{pics}/DINO{}.TGA", k + 1), d);
        }
    }
    if want("scope") {
        if let Some(m) = c2_mods::mobile::scope_mask(&phone) {
            layer.insert(c2_mods::mobile::SCOPE, m);
        }
    }
    Ok(layer)
}

/// A menu picture's middle square, as a `.TGA` (a call icon).
fn square_middle(tga: &[u8]) -> Option<Vec<u8>> {
    let p = c2_formats::tga::parse(tga).ok()?;
    let side = p.width.min(p.height);
    let x0 = (p.width - side) / 2;
    let pixels = (0..side)
        .flat_map(|y| {
            let i = y * p.width + x0;
            p.pixels
                .get(i..i + side)
                .unwrap_or_default()
                .iter()
                .copied()
        })
        .collect::<Vec<u16>>();
    (pixels.len() == side * side).then(|| {
        c2_formats::tga::write(&c2_formats::tga::Picture {
            width: side,
            height: side,
            pixels,
        })
    })
}

/// Whether the game has add-ons: given here (`--mod`, a recipe's mods), or
/// built in by `c2-pc` (`MERGE_REPORT.md` lists layers past the base's).
/// The 3DS offers "random ambients" only then (owner, 2026-11-01).
fn has_addons(a: &Args, game: &Path) -> bool {
    if !a.mods.is_empty() {
        return true;
    }
    if let Some(r) = &a.recipe {
        if c2_mods::recipe::Recipe::load(r).is_ok_and(|r| !r.mods.is_empty()) {
            return true;
        }
    }
    let report = std::fs::read_to_string(game.join("MERGE_REPORT.md")).unwrap_or_default();
    report
        .split("## ")
        .find(|s| s.starts_with("Load order"))
        .is_some_and(|s| {
            s.lines().any(|l| {
                l.split_once(". ")
                    .is_some_and(|(n, _)| n.parse::<u32>().is_ok_and(|n| n > 0))
            })
        })
}

fn merged_game(a: &Args, game: &Path) -> Result<Vfs> {
    let mut bases: Vec<PathBuf> = std::iter::once(game.to_path_buf())
        .chain(a.overlays.iter().cloned())
        .collect();
    if let Some(r) = &a.recipe {
        let recipe = c2_mods::recipe::Recipe::load(r)?;
        if !recipe.standalones.is_empty() {
            bail!(
                "{} combines standalone games: build it first (c2-pc recipe build ... --out DIR), then --game DIR",
                r.display()
            );
        }
        if a.mods_dirs.is_empty() && !(recipe.mods.is_empty() && recipe.overlays.is_empty()) {
            bail!("--recipe needs --mods DIR");
        }
        // The recipe's own overlays, when it lists them, over the game.
        if !recipe.overlays.is_empty() {
            bases.truncate(1);
            for l in c2_mods::recipe::locate_games(&recipe.overlays, &a.mods_dirs) {
                match (&l.root, l.complete()) {
                    (Some(root), true) => bases.push(root.clone()),
                    _ => bail!(
                        "overlay '{}' is missing or doesn't match (get it from {})",
                        l.id,
                        if l.url.is_empty() { "?" } else { &l.url }
                    ),
                }
            }
        }
        let game = &bases[..];
        let vanilla = c2_mods::vanilla(game)?;
        let found = c2_mods::recipe::locate(&recipe, &a.mods_dirs);
        for l in found.iter().filter(|l| !l.complete()) {
            eprintln!(
                "mod '{}': {} missing, {} different — get it from {}",
                l.id,
                l.missing.len(),
                l.mismatched.len(),
                if l.url.is_empty() { "?" } else { &l.url }
            );
        }
        let (sources, opts) = c2_mods::recipe::prepare(r, &recipe, &found, &vanilla)?;
        let mut vfs = c2_mods::build_with(game, sources, &opts)?.vfs;
        // The base game's own parts left out.
        c2_mods::standalone::leave_out::push(&mut vfs, &recipe.recipe.skip)?;
        // Predation (an advanced instance setting): `_PREDATION.TXT`.
        if let Some(p) = &recipe.settings.predation {
            p.push(&mut vfs);
        }
        // Times of day, locked settings and spawn rules: `_RULES.TXT`.
        recipe.settings.rules.push(&mut vfs)?;
        return Ok(vfs);
    }
    let game = &bases[..];
    if a.mods.is_empty() {
        return Ok(c2_mods::vanilla(game)?);
    }
    let vanilla = c2_mods::vanilla(game)?;
    let mods = a
        .mods
        .iter()
        .map(|m| c2_mods::load_mod(&vanilla, m))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(c2_mods::build(game, mods)?.vfs)
}

/// Box-filters a square-ish RGBA8 image down by an integer factor. Alpha is
/// kept binary (cut-out textures): a texel is opaque if most of its block
/// is, and takes the average colour of the opaque ones.
fn downscale(rgba: &[u8], w: usize, h: usize, nw: usize, nh: usize) -> Vec<u8> {
    let (fx, fy) = ((w / nw).max(1), (h / nh).max(1));
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        for x in 0..nw {
            let (mut sum, mut all, mut opaque) = ([0u32; 3], [0u32; 3], 0u32);
            for dy in 0..fy {
                for dx in 0..fx {
                    let i = ((y * fy + dy) * w + x * fx + dx) * 4;
                    let Some(p) = rgba.get(i..i + 4) else {
                        continue;
                    };
                    for k in 0..3 {
                        all[k] += u32::from(p[k]);
                        if p[3] >= 128 {
                            sum[k] += u32::from(p[k]);
                        }
                    }
                    opaque += u32::from(p[3] >= 128);
                }
            }
            let n = (fx * fy) as u32;
            let (c, d) = if opaque > 0 { (sum, opaque) } else { (all, n) };
            out.extend(c.map(|v| (v / d) as u8));
            out.push(if opaque * 2 >= n { 255 } else { 0 });
        }
    }
    out
}

/// Area-averaged resize of an RGBA8 image to `nw` x `nh`, placed at the
/// top left of a transparent `cw` x `ch` canvas (power-of-two for the GPU).
fn fit(rgba: &[u8], w: usize, h: usize, nw: usize, nh: usize, cw: usize, ch: usize) -> Vec<u8> {
    let mut out = vec![0u8; cw * ch * 4];
    for y in 0..nh.min(ch) {
        for x in 0..nw.min(cw) {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
            let (mut sum, mut n) = ([0u32; 4], 0u32);
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let i = (sy * w + sx) * 4;
                    if let Some(p) = rgba.get(i..i + 4) {
                        for k in 0..4 {
                            sum[k] += u32::from(p[k]);
                        }
                        n += 1;
                    }
                }
            }
            let d = (y * cw + x) * 4;
            for k in 0..4 {
                out[d + k] = (sum[k] / n.max(1)) as u8;
            }
        }
    }
    out
}

/// The 8x8 font (ASCII 32..=127, 16 per row) as white-on-clear, 128x64.
fn font() -> Vec<u8> {
    use font8x8::{UnicodeFonts, BASIC_FONTS};
    let mut px = vec![0u8; 128 * 64 * 4];
    for code in 32u32..128 {
        let Some(glyph) = char::from_u32(code).and_then(|c| BASIC_FONTS.get(c)) else {
            continue;
        };
        let (cx, cy) = ((code - 32) as usize % 16 * 8, (code - 32) as usize / 16 * 8);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits & (1 << col) != 0 {
                    let i = ((cy + row) * 128 + cx + col) * 4;
                    px[i..i + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    px
}

/// 16-bit mono PCM at `rate` from `pcm` in format (`rate0`, `channels`,
/// `bits`): channels averaged, resampled linearly.
fn to_mono16(pcm: &[u8], rate0: u32, channels: u16, bits: u16, rate: u32) -> Vec<u8> {
    let ch = usize::from(channels.max(1));
    let samples: Vec<f32> = if bits == 8 {
        pcm.iter()
            .map(|&b| (f32::from(b) - 128.0) * 256.0)
            .collect()
    } else {
        pcm.as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)))
            .collect()
    };
    let mono: Vec<f32> = samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / f.len() as f32)
        .collect();
    if mono.is_empty() {
        return Vec::new();
    }
    let step = f64::from(rate0.max(1)) / f64::from(rate.max(1));
    let n = (mono.len() as f64 / step) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f64 * step;
        let (k, f) = (t as usize, (t.fract()) as f32);
        let a = mono[k.min(mono.len() - 1)];
        let b = mono[(k + 1).min(mono.len() - 1)];
        let v = (a + (b - a) * f).clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// An area's ambient loop at most this long on the 3DS. The originals'
/// are seconds long; one standalone (Fallen Kings) loops minutes of music,
/// 13 MiB of the 22 MiB of video memory a hunt has, so that no hunt on
/// those areas fit ("Too much for this console", owner 2026-11-01).
const MAX_AMBIENT_SECS: u32 = 45;

/// A 16-bit mono loop of `rate` Hz cut to `secs`, its first half second
/// cross-faded from what followed the cut, so the loop runs on without a
/// click. Shorter ones as they are.
fn loop_cut(pcm: &[u8], rate: u32, secs: u32) -> Vec<u8> {
    let s: Vec<i16> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    let len = (rate * secs) as usize;
    let fade = (rate / 2) as usize;
    if s.len() <= len + fade {
        return pcm.to_vec();
    }
    let mut out: Vec<i16> = s[..len].to_vec();
    for (i, o) in out.iter_mut().take(fade).enumerate() {
        // At the seam the loop goes from s[len - 1] to out[0] = s[len].
        let a = i as f32 / fade as f32;
        *o = (f32::from(s[i]) * a + f32::from(s[len + i]) * (1.0 - a)) as i16;
    }
    out.iter().flat_map(|v| v.to_le_bytes()).collect()
}

struct Cooker {
    profile: &'static Profile,
    entries: Vec<(String, Vec<u8>)>,
    /// Linear memory by scope ("hunt" = every hunt, "AREAn", "char{i}",
    /// "wpn{i}", "menu") and kind, KiB.
    linear: BTreeMap<String, BTreeMap<&'static str, usize>>,
    /// The scope `add_linear` counts into.
    scope: String,
    /// Sounds go in as 16-bit mono at this rate.
    sound_rate: u32,
    /// Main memory a loaded model takes, by scope, KiB (about its cooked
    /// file).
    heap: BTreeMap<String, usize>,
}

impl Cooker {
    fn add(&mut self, name: impl Into<String>, data: Vec<u8>) {
        self.entries.push((name.into(), data));
    }

    /// A game file, copied as the engine reads it.
    fn copy(&mut self, vfs: &Vfs, path: &str) -> Result<()> {
        let d = vfs.read(path).with_context(|| format!("reading {path}"))?;
        self.add(path, d);
        Ok(())
    }

    /// A model file without what the 3DS gets elsewhere: its texture (now a
    /// `.TX`) and its sounds' samples (kept as empty sounds, so indices
    /// hold; the samples go to `C2RS/SND/{snd}{k}.PCM`).
    fn car(&mut self, vfs: &Vfs, path: &str, snd: &str) -> Result<Vec<u8>> {
        let mut car =
            c2_formats::car::parse(&vfs.read(path)?).with_context(|| format!("parsing {path}"))?;
        let rgba = PreparedModel::from_mesh(&car.mesh).texture_rgba;
        car.mesh.texture = Vec::new();
        let before = anim_bytes(&car);
        while anim_bytes(&car) > self.profile.anim_kb * 1024 {
            let was = anim_bytes(&car);
            halve_frames(&mut car);
            if anim_bytes(&car) >= was {
                break;
            }
        }
        if anim_bytes(&car) < before {
            println!(
                "  {path}: animations {:.1} -> {:.1} MiB (fewer frames)",
                before as f32 / 1048576.0,
                anim_bytes(&car) as f32 / 1048576.0
            );
        }
        for (k, s) in car.sounds.iter_mut().enumerate() {
            let pcm = std::mem::take(&mut s.pcm);
            self.sound(&format!("{snd}{k}"), &pcm, SAMPLE_RATE, 1, 16);
        }
        let bytes = c2_formats::car::write(&car);
        // Parsed, a model is a little bigger than its file.
        *self.heap.entry(self.scope.clone()).or_default() += bytes.len() * 5 / 4 / 1024;
        self.add(path, bytes);
        Ok(rgba)
    }

    /// A sound as `C2RS/SND/{name}.PCM` (raw 16-bit mono at `sound_rate`).
    fn sound(&mut self, name: &str, pcm: &[u8], rate: u32, channels: u16, bits: u16) {
        let out = to_mono16(pcm, rate, channels, bits, self.sound_rate);
        if out.is_empty() {
            return;
        }
        self.count("sounds", out.len());
        self.add(format!("C2RS/SND/{name}.PCM"), out);
    }

    /// A `.WAV` from the game, if there.
    fn wav(&mut self, vfs: &Vfs, path: &str, name: &str) {
        let Some(w) = vfs
            .read(path)
            .ok()
            .and_then(|d| c2_formats::wav::parse(&d).ok())
        else {
            return;
        };
        let f = w.format.unwrap_or(c2_formats::wav::WavFormat::VANILLA);
        self.sound(name, &w.pcm, f.sample_rate, f.channels, f.bits);
    }

    /// A 2D picture, already sized, as an RGBA5551 texture.
    fn picture(&mut self, name: &str, rgba: &[u8], w: usize, h: usize) {
        let t = tile(rgba, w, h, TexFormat::Rgba5551);
        self.count("textures", t.data.len());
        self.add(name, write_texture(&t));
    }

    /// Counts `bytes` of linear memory of `kind` in the current scope.
    fn count(&mut self, kind: &'static str, bytes: usize) {
        *self
            .linear
            .entry(self.scope.clone())
            .or_default()
            .entry(kind)
            .or_default() += bytes / 1024;
    }

    /// An RGB565 texture (terrain, sky, map) from RGBA8.
    fn opaque(&mut self, name: &str, rgba: &[u8], w: usize, h: usize) {
        let t = tile(rgba, w, h, TexFormat::Rgb565);
        self.count("textures", t.data.len());
        self.add(name, write_texture(&t));
    }

    /// A menu picture (357x208 in the original) fitted to `w` wide, on a
    /// `w` x `w` canvas.
    fn menu_picture(&mut self, vfs: &Vfs, file: &str, name: &str, w: usize) {
        let Some(p) = vfs
            .read(&format!("HUNTDAT/MENU/PICS/{file}"))
            .ok()
            .and_then(|d| c2_formats::tga::parse(&d).ok())
        else {
            return;
        };
        let px = image_to_rgba8(&p.pixels, false);
        let h = (w * p.height / p.width.max(1)).min(w);
        self.picture(name, &fit(&px, p.width, p.height, w, h, w, w), w, w);
    }

    /// A menu picture of a model (as `menu_picture` lays one out: 128 wide,
    /// 357:208, on a 128 x 128 canvas), drawn at twice the size and
    /// averaged down.
    fn portrait(&mut self, vfs: &Vfs, path: &str, name: &str) {
        let Some(car) = vfs
            .read(path)
            .ok()
            .and_then(|d| c2_formats::car::parse(&d).ok())
        else {
            return;
        };
        let (w, h) = (256, 256 * 208 / 357);
        let Some(px) = c2_formats::portrait::render(&car.mesh, w, h, 0.6) else {
            return;
        };
        self.picture(name, &fit(&px, w, h, 128, h / 2, 128, 128), 128, 128);
    }

    /// A model texture (256x256 RGBA8) at the profile's size.
    fn model_texture(&mut self, name: String, rgba: &[u8]) {
        let side = self.profile.model_tex.min(256);
        let px = downscale(rgba, 256, 256, side, side);
        self.picture(&name, &px, side, side);
    }
}

/// One area: its map and slimmed RSC, terrain atlas, object textures, the
/// three skies, the overview map and its sounds, under `C2RS/{name}/`.
fn cook_area(c: &mut Cooker, vfs: &Vfs, name: &str) -> Result<()> {
    c.scope = name.to_string();
    let map_path = format!("HUNTDAT/AREAS/{name}.MAP");
    // Forests denser than the original's densest are thinned, the same for
    // everyone with this pack (`thin_objects`): over the square the 3DS
    // draws objects in, as many faces at most as the original's 48 cells.
    let rsc_only = c2_core::load::area(vfs, name, TimeOfDay::Day)?.rsc;
    let mut raw = c2_formats::map::parse(&vfs.read(&map_path)?)
        .with_context(|| format!("parsing {map_path}"))?;
    let window = 2 * c.profile.object_view as usize;
    let budget = c2_core::area::ORIGINAL_FACES_48 * (window * window) as u64 / (48 * 48);
    let thinned = c2_core::area::thin_objects(&mut raw, &rsc_only, window, budget);
    drop(rsc_only);
    if thinned > 0 {
        eprintln!(
            "  {name}: {thinned} trees and plants thinned (a forest denser than the original's)"
        );
        c.add(&map_path, c2_formats::map::write(&raw));
    } else {
        c.copy(vfs, &map_path)?;
    }
    drop(raw);
    // A trophy room's own layout, an area's own snow (the phone game's,
    // ours), when it has them.
    for ext in ["SPOT", "SNOW"] {
        let f = format!("HUNTDAT/AREAS/{name}.{ext}");
        if vfs.resolve(&f).is_some() {
            c.copy(vfs, &f)?;
        }
    }
    let area = c2_core::load::area(vfs, name, TimeOfDay::Day)?;
    let profile = c.profile;
    // Terrain textures in one atlas, square, power-of-two.
    let n = area.rsc.textures.len().max(1);
    let mut t = profile.terrain_tile;
    while (1024 / t) * (1024 / t) < n && t > 8 {
        t /= 2;
    }
    let mut side = t;
    while (side / t) * (side / t) < n {
        side *= 2;
    }
    let cols = side / t;
    let mut atlas = vec![0u8; side * side * 4];
    for (k, tex) in area.rsc.textures.iter().enumerate() {
        let px = downscale(&image_to_rgba8(tex, false), 128, 128, t, t);
        let (ox, oy) = ((k % cols) * t, (k / cols) * t);
        for y in 0..t {
            let d = ((oy + y) * side + ox) * 4;
            atlas[d..d + t * 4].copy_from_slice(&px[y * t * 4..(y + 1) * t * 4]);
        }
    }
    c.opaque(&format!("C2RS/{name}/TERRAIN.TX"), &atlas, side, side);
    // The overview map (GenerateMapImage) and the cloud planes.
    let map = image_to_rgba8(&c2_core::game::Hunt::map_image(&area), false);
    c.opaque(&format!("C2RS/{name}/MAP.TX"), &map, 256, 256);
    let sky_side = (profile.model_tex * 2).min(256);
    for (i, sky) in area.rsc.skies.iter().enumerate() {
        let px = downscale(&image_to_rgba8(sky, false), 256, 256, sky_side, sky_side);
        c.add(
            format!("C2RS/{name}/SKY{i}.TX"),
            write_texture(&tile(&px, sky_side, sky_side, TexFormat::Rgb565)),
        );
    }
    // One sky is loaded per hunt.
    c.count("textures", sky_side * sky_side * 2);
    for (k, o) in area.rsc.objects.iter().enumerate() {
        let p = PreparedModel::from_mesh(&o.model.mesh);
        c.model_texture(format!("C2RS/{name}/OBJ{k}.TX"), &p.texture_rgba);
        // Vertex buffer: 24 bytes per face corner.
        c.count("vertex buffers", p.faces.len() * 3 * 24);
        // Its far sprite (`LoadBMPModel`; none for ofNOBMP), at the model
        // texture's sharpness: 128x128 of the 256 is half its side.
        if o.info.flags & c2_formats::rsc::object_flags::NO_BMP == 0 {
            let sp =
                PreparedModel::from_mesh(&c2_core::model::sprite_mesh(&o.model.mesh, &o.sprite));
            let s = c2_formats::rsc::SPRITE_SIZE;
            let crop: Vec<u8> = sp
                .texture_rgba
                .chunks(256 * 4)
                .take(s)
                .flat_map(|row| row[..s * 4].iter().copied())
                .collect();
            let side = (profile.model_tex / 2).clamp(16, s);
            let px = downscale(&crop, s, s, side, side);
            c.picture(&format!("C2RS/{name}/SPR{k}.TX"), &px, side, side);
        }
    }
    // The RSC without what is now in the .TX files or only the PC uses:
    // terrain and object textures, sounds (kept as empty sounds).
    let mut rsc = area.rsc;
    rsc.textures = Vec::new();
    for o in &mut rsc.objects {
        o.model.mesh.texture = Vec::new();
    }
    for (k, s) in rsc.random_sounds.iter_mut().enumerate() {
        let pcm = std::mem::take(s);
        c.sound(&format!("{name}/R{k}"), &pcm, SAMPLE_RATE, 1, 16);
    }
    for (k, amb) in rsc.ambients.iter_mut().enumerate() {
        let pcm = loop_cut(&std::mem::take(&mut amb.pcm), SAMPLE_RATE, MAX_AMBIENT_SECS);
        c.sound(&format!("{name}/A{k}"), &pcm, SAMPLE_RATE, 1, 16);
    }
    c.add(
        format!("HUNTDAT/AREAS/{name}.RSC"),
        c2_formats::rsc::write(&rsc),
    );
    c.add(
        format!("C2RS/{name}/INFO.TXT"),
        format!("terrain_tile = {t}\natlas_cols = {cols}\nterrain_count = {n}\n").into_bytes(),
    );
    Ok(())
}

/// The pack's rules, read the way the 3DS reads them.
struct Rules {
    /// Modder's Edition `_RES.TXT` (resolved per hunt on the 3DS).
    dialect: bool,
    /// Characters and weapons for a neutral hunt (area 1, day, every slot
    /// picked); in the dialect the 3DS resolves each hunt itself.
    res: c2_formats::res_txt::ResData,
    /// Every model a character can use, by `model_key`: (file, loaded
    /// for every hunt). The others load only when picked.
    models: BTreeMap<String, (String, bool)>,
    /// The dialect's `_RES.TXT` as written (each area's ambients).
    raw: Option<c2_formats::res_txt::ResTxt>,
    /// Models of roaming add-ons (huntable, no menu slot), by `model_key`:
    /// they get a menu picture drawn from the model.
    roamers: std::collections::BTreeSet<String>,
    /// The menu: `_MENU.TXT` if there is one, else `_RES.TXT`.
    menu: c2_formats::menu_txt::MenuData,
    /// The model of each menu slot's animal (slot 0 = `DINO1.TGA`), for
    /// menu pictures that are an empty frame.
    slot_files: BTreeMap<usize, String>,
}

/// A dialect character with a menu slot (`callNo`): a hunt target. Without
/// one, AI under 10 (the hunter, small ambient animals, flyers) is loaded
/// for every hunt; the rest (add-ons that roam, brachiosaurs, fish) only
/// when picked. `c2-3ds` (`hunt::Role`) decides the same way.
fn has_slot(b: &c2_formats::res_txt::Block) -> bool {
    b.get(&["callNo"])
        .is_some_and(|v| c2_formats::res_txt::c_atoi(v) >= 0)
}

/// The models of the hunter and ambient animals a hunt on `area` loads
/// (`hunt::Role` on the 3DS): a dialect ambient whose tables spawn it only
/// on other maps (a combined game's) isn't, at any time of day.
fn ambients_on(
    raw: &c2_formats::res_txt::ResTxt,
    area: &str,
) -> std::collections::BTreeSet<String> {
    let blocks = raw.section("characters").map_or(&[][..], |s| &s.blocks);
    let mut out = std::collections::BTreeSet::new();
    for time in 0..3 {
        let h = c2_formats::mee::Hunt {
            area: c2_formats::mee::area_of(area),
            time,
            picked: c2_formats::bits::Bits::ALL,
        };
        let m = c2_core::mee::read(raw, &h);
        for (i, c) in m.tables.characters.iter().enumerate() {
            let info = DinoInfo::from_def(&m.res.characters[i]);
            let elsewhere = blocks.get(i).is_some_and(c2_formats::mee::has_spawns) && !c.spawns();
            if (0..10).contains(&info.ai) && !has_slot(&c.block) && !elsewhere {
                out.insert(c2_formats::cooked::model_key(&info.file));
            }
        }
    }
    out
}

/// The `file` of every block under `b`, quotes removed.
fn files_in(b: &c2_formats::res_txt::Block, out: &mut Vec<String>) {
    for e in &b.entries {
        if e.key.eq_ignore_ascii_case("file") {
            let v = e.value.trim().trim_matches('\'').trim().to_string();
            if !v.is_empty() {
                out.push(v);
            }
        }
    }
    for s in &b.subs {
        files_in(&s.block, out);
    }
}

fn rules(vfs: &Vfs) -> Result<Rules> {
    use c2_formats::res_txt::ResTxt;
    let text = String::from_utf8_lossy(&vfs.read("HUNTDAT/_RES.TXT")?).into_owned();
    let raw = ResTxt::parse(&text)?;
    let dialect = c2_formats::mee::is_dialect(&raw);
    let (res, slots) = if dialect {
        let h = c2_formats::mee::Hunt {
            area: Some(1),
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        };
        let m = c2_core::mee::read(&raw, &h);
        let raw_chars = raw.section("characters").map_or(&[][..], |s| &s.blocks);
        let slots: Vec<Option<usize>> = m
            .tables
            .characters
            .iter()
            .enumerate()
            .map(|(i, c)| c2_formats::mee::menu_slot(raw_chars.get(i), &c.block))
            .collect();
        (m.res, slots)
    } else {
        (c2_core::load::res(vfs)?, Vec::new())
    };
    let mut models = BTreeMap::new();
    let mut roamers = std::collections::BTreeSet::new();
    let mut slot_files = BTreeMap::new();
    for (i, def) in res.characters.iter().enumerate() {
        let info = DinoInfo::from_def(def);
        let key = c2_formats::cooked::model_key(&info.file);
        let slot = if dialect {
            slots.get(i).copied().flatten()
        } else {
            // The original's slot is the AI - 10.
            usize::try_from(info.ai - 10).ok()
        };
        if let Some(k) = slot {
            slot_files.entry(k).or_insert_with(|| info.file.clone());
        }
        if dialect && slot.is_none() && (10..30).contains(&info.ai) {
            roamers.insert(key.clone());
        }
        let always = if dialect {
            slot.is_none() && (0..10).contains(&info.ai)
        } else {
            info.ai < 10
        };
        let e = models.entry(key).or_insert((info.file.clone(), always));
        e.1 &= always;
    }
    if dialect {
        // Models only an area or time override uses.
        let mut all = Vec::new();
        for b in raw.section("characters").map_or(&[][..], |s| &s.blocks) {
            files_in(b, &mut all);
        }
        for f in all {
            models
                .entry(c2_formats::cooked::model_key(&f))
                .or_insert((f, false));
        }
    }
    let menu_src = match vfs.read("HUNTDAT/_MENU.TXT") {
        Ok(d) => ResTxt::parse(&String::from_utf8_lossy(&d))?,
        Err(_) => raw.clone(),
    };
    let menu = c2_formats::menu_txt::MenuData::from_res(&menu_src);
    Ok(Rules {
        dialect,
        res,
        models,
        raw: dialect.then_some(raw),
        roamers,
        menu,
        slot_files,
    })
}

/// Areas whose `.c2map` says it snows (Ice Age's beside C2's).
fn snowy<'a>(vfs: &Vfs, areas: &'a [String]) -> Vec<&'a str> {
    areas
        .iter()
        .filter(|n| {
            vfs.read(&format!("HUNTDAT/AREAS/{n}.c2map"))
                .ok()
                .and_then(|d| c2_formats::menu_txt::parse_c2map(&String::from_utf8_lossy(&d)))
                .is_some_and(|a| a.snow)
        })
        .map(String::as_str)
        .collect()
}

/// Where each roaming add-on spawns (per area its tables), so the menus
/// needn't resolve the rules for every area when they open (seconds on an
/// Old 3DS for a combined game's many): the areas by character.
fn roam_areas<'a>(
    raw: &c2_formats::res_txt::ResTxt,
    areas: &'a [String],
) -> BTreeMap<usize, Vec<&'a str>> {
    let mut by_char: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for name in areas.iter().filter(|a| !a.eq_ignore_ascii_case("TROPHY")) {
        let h = c2_formats::mee::Hunt {
            area: c2_formats::mee::area_of(name),
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        };
        let t = c2_formats::mee::Tables::read(raw, &h);
        let raw_chars = raw.section("characters").map_or(&[][..], |s| &s.blocks);
        for (i, ch) in t.characters.iter().enumerate() {
            let ai = ch.block.get(&["ai"]).map(c2_formats::res_txt::c_atoi);
            let slot = c2_formats::mee::menu_slot(raw_chars.get(i), &ch.block);
            let roams = slot.is_none() && ai.is_some_and(|a| (10..30).contains(&a));
            if roams && (!ch.spawn.is_empty() || !ch.packs.is_empty()) {
                by_char.entry(i).or_default().push(name);
            }
        }
    }
    by_char
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let a = parse_args()?;
    if let Some(p) = &a.check {
        return check::check(p, a.bench.as_deref());
    }
    let game = a.game.clone().context(USAGE)?;
    let out = a.out.clone().context(USAGE)?;
    let wiiu_target = a.target.as_deref() == Some("wiiu");
    let profile = match a.target.as_deref().unwrap_or("n3ds") {
        "n3ds" | "wiiu" => &N3DS,
        "o3ds" => &O3DS,
        t => bail!("unknown target {t:?} (n3ds, o3ds or wiiu)"),
    };
    let vfs = merged(&a, &game)?;
    let addons = has_addons(&a, &game);
    let rules = rules(&vfs)?;
    let res = &rules.res;
    let mut c = Cooker {
        profile,
        entries: Vec::new(),
        linear: BTreeMap::new(),
        scope: "hunt".into(),
        sound_rate: profile.sound_rate,
        heap: BTreeMap::new(),
    };
    let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
    let winfos: Vec<WeaponInfo> = res.weapons.iter().map(WeaponInfo::from_def).collect();

    // Areas: AREA1.. as present (or as asked), then the trophy room.
    let wanted: Option<Vec<u32>> = a
        .areas
        .as_deref()
        .map(|l| l.split(',').filter_map(|v| v.trim().parse().ok()).collect());
    let mut areas: Vec<String> = (1..=99)
        .filter(|n| wanted.as_ref().is_none_or(|w| w.contains(n)))
        .map(|n| format!("AREA{n}"))
        .filter(|n| vfs.resolve(&format!("HUNTDAT/AREAS/{n}.MAP")).is_some())
        .collect();
    if areas.is_empty() {
        bail!("no areas found (HUNTDAT/AREAS/AREAn.MAP)");
    }
    if wanted.is_none() && vfs.resolve("HUNTDAT/AREAS/TROPHY.MAP").is_some() {
        areas.push("TROPHY".into());
    }
    if wiiu_target {
        let info = wiiu::Info {
            name: a.title.as_deref().unwrap_or("Carnivores 2"),
            areas: &areas,
            snow: snowy(&vfs, &areas),
            roams: rules
                .raw
                .as_ref()
                .map(|raw| roam_areas(raw, &areas))
                .unwrap_or_default(),
        };
        return wiiu::install(&vfs, &info, &out);
    }

    // Everything a hunt can use, whatever is picked.
    c.copy(&vfs, "HUNTDAT/_RES.TXT")?;
    if vfs.resolve("HUNTDAT/_MENU.TXT").is_some() {
        c.copy(&vfs, "HUNTDAT/_MENU.TXT")?;
    }
    for p in [c2_formats::predation::PATH, c2_formats::rules::PATH] {
        if vfs.resolve(p).is_some() {
            c.copy(&vfs, p)?;
        }
    }
    for (file, tx) in [("SHIP2A.CAR", "SHIP"), ("WCIRCLE2.CAR", "RING")] {
        let path = format!("HUNTDAT/{file}");
        if vfs.resolve(&path).is_some() {
            let rgba = c.car(&vfs, &path, &format!("{tx}_"))?;
            c.model_texture(format!("C2RS/{tx}.TX"), &rgba);
        }
    }
    // Binoculars, sun and moon (`Hunt2.cpp`: MOON.3DF at night).
    for (file, tx) in [("BINOCUL", "BINOC"), ("SUN2", "SUN"), ("MOON", "MOON")] {
        let path = format!("HUNTDAT/{file}.3DF");
        if let Ok(d) = vfs.read(&path) {
            let m = c2_formats::three_df::parse(&d).with_context(|| format!("parsing {path}"))?;
            c.add(format!("C2RS/{tx}.3DF"), d);
            c.model_texture(
                format!("C2RS/{tx}.TX"),
                &PreparedModel::from_mesh(&m.mesh).texture_rgba,
            );
        }
    }
    // The phone's scope mask (`--phone-pictures scope`): its left half on a
    // 256x256 canvas. One-bit alpha: the faint cross-hairs (alpha ~70)
    // and the ring's soft edge kept by a low cut, in black.
    if let Some((w, h, px)) = vfs
        .read(c2_mods::mobile::SCOPE)
        .ok()
        .as_deref()
        .and_then(c2_mods::mobile::parse_scope)
    {
        let mut canvas = vec![0u8; 256 * 256 * 4];
        for y in 0..h.min(256) {
            for x in 0..w.min(256) {
                let a = px[(y * w + x) * 4 + 3];
                let d = (y * 256 + x) * 4;
                canvas[d + 3] = if a >= 32 { 255 } else { 0 };
            }
        }
        c.scope = "hunt".into();
        c.picture("C2RS/SCOPE.TX", &canvas, 256, 256);
    }
    // Character models by file (`model_key`): texture `C2RS/M/{key}.TX`,
    // sounds `C2RS/SND/M/{key}/{k}.PCM`.
    let mut cooked_models = Vec::new();
    for (key, (file, always)) in &rules.models {
        let path = format!("HUNTDAT/{file}");
        if vfs.resolve(&path).is_none() {
            continue;
        }
        c.scope = format!("m:{key}");
        let rgba = c.car(&vfs, &path, &format!("M/{key}/"))?;
        c.model_texture(format!("C2RS/M/{key}.TX"), &rgba);
        cooked_models.push((key.clone(), *always));
        if rules.roamers.contains(key) {
            c.scope = "menu".into();
            c.portrait(&vfs, &path, &format!("C2RS/MENU/R/{key}.TX"));
            c.scope = format!("m:{key}");
        }
    }
    // Calls by menu slot n (call id n + 9; in the original the slot is the
    // AI - 10): MENU/PICS/CALLn.TGA, SOUNDFX/CALLS/CALLn_*.WAV.
    for n in 1..=rules.menu.huntables.len().max(1) {
        let id = n + 9;
        for (v, l) in ["A", "B", "C"].iter().enumerate() {
            let f = format!("HUNTDAT/SOUNDFX/CALLS/CALL{n}_{l}.WAV");
            c.wav(&vfs, &f, &format!("K{id}_{v}"));
        }
        let pic = |f: &str| {
            vfs.read(&format!("HUNTDAT/MENU/PICS/{f}"))
                .ok()
                .and_then(|d| c2_formats::tga::parse(&d).ok())
        };
        if let Some(p) = pic(&format!("CALL{n}.TGA")) {
            let px = image_to_rgba8(&p.pixels, false);
            let px = fit(&px, p.width, p.height, 64, 52, 64, 64);
            c.picture(&format!("C2RS/CALL{id}.TX"), &px, 64, 64);
        } else if let Some(p) = pic(&format!("DINO{n}.TGA")) {
            // Add-ons often ship calls without an icon: the animal's menu
            // picture (its middle, where the animal is) stands in.
            let px = image_to_rgba8(&p.pixels, false);
            let (w, h) = (p.width, p.height);
            let side = h.min(w);
            let x0 = (w - side) / 2;
            let mut sq = Vec::with_capacity(side * side * 4);
            for y in 0..side {
                let i = (y * w + x0) * 4;
                sq.extend_from_slice(px.get(i..i + side * 4).unwrap_or_default());
            }
            if sq.len() == side * side * 4 {
                let px = fit(&sq, side, side, 64, 64, 64, 64);
                c.picture(&format!("C2RS/CALL{id}.TX"), &px, 64, 64);
            }
        }
    }
    for (i, w) in winfos.iter().enumerate() {
        let path = format!("HUNTDAT/WEAPONS/{}", w.file);
        if vfs.resolve(&path).is_none() {
            continue;
        }
        c.scope = format!("wpn{i}");
        let rgba = c.car(&vfs, &path, &format!("W{i}_"))?;
        c.model_texture(format!("C2RS/WPN{i}.TX"), &rgba);
        c.menu_picture(
            &vfs,
            &format!("WEAPON{}.TGA", i + 1),
            &format!("C2RS/WPNPIC{i}.TX"),
            64,
        );
    }
    c.scope = "hunt".into();
    // SOUNDFX: underwater, steps, screams, every call.
    c.wav(&vfs, "HUNTDAT/SOUNDFX/A_UNDERW.WAV", "U");
    for n in 1..=3 {
        let steps = "HUNTDAT/SOUNDFX/STEPS";
        c.wav(
            &vfs,
            &format!("{steps}/HWALK{n}.WAV"),
            &format!("F{}", n - 1),
        );
        c.wav(
            &vfs,
            &format!("{steps}/FOOTW{n}.WAV"),
            &format!("FW{}", n - 1),
        );
    }
    for n in 1..=4 {
        let f = format!("HUNTDAT/SOUNDFX/HUM_DIE{n}.WAV");
        c.wav(&vfs, &f, &format!("H{}", n - 1));
    }
    c.picture("C2RS/FONT.TX", &font(), 128, 64);
    // Framebuffers, depth buffer, command buffer; terrain chunks around the
    // hunter, those with their middle within the view and a chunk (6
    // corners per cell, 24 bytes each; past `detail`, a quarter of the cells
    // and their skirts, 3/8 of the corners in all).
    c.count("screens", 3 << 20);
    let disc = |r: u32| (std::f64::consts::PI * f64::from(r + 16).powi(2)) as usize;
    let (near, all) = (disc(profile.detail), disc(profile.view.max(profile.detail)));
    c.count("vertex buffers", (near + (all - near) * 3 / 8) * 6 * 24);

    // The menu: pictures of areas, animals, weapons, accessories, and the
    // descriptions (MENU/TXT).
    c.scope = "menu".into();
    for n in 1..=areas.iter().filter(|a| a.starts_with("AREA")).count() {
        c.menu_picture(
            &vfs,
            &format!("AREA{n}.TGA"),
            &format!("C2RS/MENU/AREA{n}.TX"),
            256,
        );
    }
    for n in 1..=rules.menu.huntables.len() {
        let name = format!("C2RS/MENU/DINO{n}.TX");
        // An empty frame (Triassic's 10th animal): drawn from the model.
        let blank = vfs
            .read(&format!("HUNTDAT/MENU/PICS/DINO{n}.TGA"))
            .ok()
            .and_then(|d| c2_formats::tga::parse(&d).ok())
            .is_some_and(|t| c2_formats::portrait::blank_picture(&t.pixels, t.width, t.height));
        match rules.slot_files.get(&(n - 1)) {
            Some(file) if blank => c.portrait(&vfs, &format!("HUNTDAT/{file}"), &name),
            _ => c.menu_picture(&vfs, &format!("DINO{n}.TGA"), &name, 128),
        }
    }
    for n in 1..=winfos.len() {
        c.menu_picture(
            &vfs,
            &format!("WEAPON{n}.TGA"),
            &format!("C2RS/MENU/WEAPON{n}.TX"),
            128,
        );
    }
    for n in 1..=4 {
        c.menu_picture(
            &vfs,
            &format!("EQUIP{n}.TGA"),
            &format!("C2RS/MENU/EQUIP{n}.TX"),
            128,
        );
    }
    // The title screen: the main menu's art (MENUM.TGA, 800x600), its
    // 800x480 middle at 400x240 on a 512x256 canvas.
    // The menu style's (`--menu-art`), else the game's.
    let style_title = a.menu_art.as_ref().and_then(|d| {
        let mut v = Vfs::new();
        v.push("menu style", Box::new(c2_vfs::DirBackend::new(d).ok()?));
        let hd = v.files().find_map(|(_, r)| {
            let l = r.path.to_ascii_lowercase();
            l.ends_with("menu/menum.tga").then(|| r.path.clone())
        })?;
        v.read(&hd).ok()
    });
    if let Some(p) = style_title
        .or_else(|| vfs.read("HUNTDAT/MENU/MENUM.TGA").ok())
        .and_then(|d| c2_formats::tga::parse(&d).ok())
    {
        let px = image_to_rgba8(&p.pixels, false);
        let (y0, h) = (p.height.saturating_sub(480) / 2, p.height.min(480));
        let band = &px[(y0 * p.width * 4).min(px.len())..((y0 + h) * p.width * 4).min(px.len())];
        let fitted = fit(band, p.width, h, 400, 240, 512, 256);
        c.opaque("C2RS/MENU/TITLE.TX", &fitted, 512, 256);
    }
    let txt: Vec<String> = vfs
        .files()
        .filter(|(_, r)| r.path.to_ascii_uppercase().starts_with("HUNTDAT/MENU/TXT/"))
        .map(|(_, r)| r.path.clone())
        .collect();
    for path in txt {
        c.copy(&vfs, &path)?;
    }

    for name in &areas {
        cook_area(&mut c, &vfs, name)?;
    }
    // Memory. For each area, the base: the area, the hunter and the
    // ambient animals (measured loading peak; linear memory). Then each
    // pick's share, which the 3DS menu adds up for the real picks; here the
    // three costliest huntable animals and weapons are shown (the menu
    // takes three of each).
    const PICKS: usize = 3;
    let mib = |kb: usize| kb as f32 / 1024.0;
    let sum = |s: &str| c.linear.get(s).map_or(0, |m| m.values().sum::<usize>());
    let heap_of = |s: &str| c.heap.get(s).copied().unwrap_or(0);
    let model_cost = |k: &str| sum(&format!("m:{k}"));
    let always: Vec<String> = cooked_models
        .iter()
        .filter(|m| m.1)
        .map(|m| m.0.clone())
        .collect();
    let mut hunted: Vec<String> = cooked_models
        .iter()
        .filter(|m| !m.1)
        .map(|m| m.0.clone())
        .collect();
    hunted.sort_by_key(|k| std::cmp::Reverse(model_cost(k) + heap_of(&format!("m:{k}"))));
    hunted.truncate(PICKS);
    let mut guns: Vec<usize> = (0..winfos.len()).collect();
    guns.sort_by_key(|&i| std::cmp::Reverse(sum(&format!("wpn{i}"))));
    guns.truncate(PICKS);
    if std::env::var_os("C2COOK_DEBUG").is_some() {
        for (k, h) in &cooked_models {
            let kinds = c.linear.get(&format!("m:{k}"));
            eprintln!(
                "  {k:<28} always {h:<5} heap {:>6} KiB  linear {:?}",
                heap_of(&format!("m:{k}")),
                kinds
            );
        }
        for (s, k) in &c.linear {
            if !s.starts_with("m:") {
                eprintln!("  {s:<28} linear {k:?}");
            }
        }
    }
    let picks_linear: usize = hunted.iter().map(|k| model_cost(k)).sum::<usize>()
        + guns.iter().map(|&i| sum(&format!("wpn{i}"))).sum::<usize>();
    let picks_heap: usize = hunted
        .iter()
        .map(|k| heap_of(&format!("m:{k}")))
        .sum::<usize>()
        + guns
            .iter()
            .map(|&i| heap_of(&format!("wpn{i}")))
            .sum::<usize>();
    println!(
        "{} pack{}: {} ({} animals, {} models, {} weapons)",
        profile.name,
        if rules.dialect {
            " (Modder's Edition)"
        } else {
            ""
        },
        areas.join(", "),
        infos.len(),
        cooked_models.len(),
        winfos.len()
    );
    println!(
        "  3DS memory: base (area, hunter, ambient animals) + the 3 costliest animals and weapons"
    );
    // Areas whose base (area, hunter, every ambient animal there) is over.
    let mut over_areas: Vec<String> = Vec::new();
    let mut worst = false;
    let mut base_heap = BTreeMap::new();
    for name in &areas {
        let here: Vec<String> = match &rules.raw {
            // The trophy room loads the hunter and what is on display.
            _ if name.eq_ignore_ascii_case("TROPHY") => rules
                .res
                .characters
                .iter()
                .map(DinoInfo::from_def)
                .filter(|d| d.ai == 0)
                .map(|d| c2_formats::cooked::model_key(&d.file))
                .filter(|k| always.contains(k))
                .collect(),
            Some(raw) => {
                let on = ambients_on(raw, name);
                always.iter().filter(|k| on.contains(*k)).cloned().collect()
            }
            None => always.clone(),
        };
        let ambient: usize = here.iter().map(|k| model_cost(k)).sum();
        let base = load_peak(&c.entries, name, rules.dialect, &here, &[])? / 1024;
        let lin = sum("hunt") + sum(name) + ambient;
        base_heap.insert(name.clone(), base);
        println!(
            "    {name:<8} heap {:>5.1} + {:>4.1} MiB, linear {:>5.1} + {:>4.1} MiB",
            mib(base),
            mib(picks_heap),
            mib(lin),
            mib(picks_linear)
        );
        if base > profile.heap_kb || lin > profile.linear_kb {
            over_areas.push(name.clone());
        }
        worst |= base + picks_heap > profile.heap_kb || lin + picks_linear > profile.linear_kb;
    }
    println!(
        "    budget: heap {:.0} MiB, linear {:.0} MiB; menu pictures {:.1} MiB (menu only)",
        mib(profile.heap_kb),
        mib(profile.linear_kb),
        mib(sum("menu"))
    );
    if worst && over_areas.is_empty() {
        println!(
            "  The costliest picks don't fit together; the 3DS menu says when picks are too much."
        );
    }
    // The console checks each hunt as picked (the ambient animals it places,
    // the picks) and refuses one that doesn't fit ("Too much for this
    // console"): a pack with some areas over plays the rest (JPHL Special
    // Edition on Old 3DS, owner's hardware). Only a pack where no area fits
    // is refused.
    if !over_areas.is_empty() {
        if over_areas.len() == areas.len() && !a.force {
            bail!(
                "over the {} budget in every area; use --target o3ds for smaller textures, or --force",
                profile.name
            );
        }
        println!(
            "  Over the budget with every ambient animal: {}. The 3DS menu refuses a hunt there that \
             doesn't fit (\"Too much for this console\"); fewer ambient animals or picks may fit.",
            over_areas.join(", ")
        );
    }
    // What each piece costs, for the 3DS menu to check a hunt fits:
    // linear memory (`cost.`) and main memory (`heap.`), KiB.
    let mut info = format!(
        "pack = 4\ntarget = {}\ndialect = {}\nareas = {}\nview = {}\ndetail = {}\n\
         object_view = {}\nsound_rate = {}\nlinear_kb = {}\nheap_kb = {}\n",
        profile.name,
        u8::from(rules.dialect),
        areas.join(","),
        profile.view,
        profile.detail,
        profile.object_view,
        profile.sound_rate,
        profile.linear_kb,
        profile.heap_kb,
    );
    for (scope, kinds) in &c.linear {
        if scope != "menu" {
            info += &format!("cost.{scope} = {}\n", kinds.values().sum::<usize>());
        }
    }
    for (scope, kb) in &c.heap {
        info += &format!("heap.{scope} = {kb}\n");
    }
    for (area, kb) in &base_heap {
        info += &format!("heap.{area} = {kb}\n");
    }
    info += &format!("addons = {}\n", u8::from(addons));
    let snowy = snowy(&vfs, &areas);
    if !snowy.is_empty() {
        info += &format!("snow = {}\n", snowy.join(","));
    }
    // Where each roaming add-on spawns (per area its tables), so the 3DS
    // menu needn't resolve the rules for every area when it opens (seconds
    // on an Old 3DS for a combined game's many).
    if let Some(raw) = &rules.raw {
        info += "roam_map = 1\n";
        for (i, list) in roam_areas(raw, &areas) {
            info += &format!("roam.{i} = {}\n", list.join(","));
        }
    }
    c.entries.push(("C2RS/INFO.TXT".into(), info.into_bytes()));
    let bytes = write_pack(&c.entries);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} ({:.1} MiB, {} files) — copy it to sdmc:/3ds/c2rs/",
        out.display(),
        bytes.len() as f32 / (1024.0 * 1024.0),
        c.entries.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_keeps_cutouts() {
        // 2x2 -> 1x1: three opaque red texels, one transparent.
        let px = [255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0];
        assert_eq!(downscale(&px, 2, 2, 1, 1), vec![255, 0, 0, 255]);
        let clear = [0, 0, 0, 0, 0, 0, 0, 0, 9, 9, 9, 255, 0, 0, 0, 0];
        assert_eq!(downscale(&clear, 2, 2, 1, 1)[3], 0);
    }

    #[test]
    fn long_loops_are_cut_without_a_seam() {
        let pcm = |v: &[i16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        // 10 Hz, 3 s cap, half a second (5 samples) of fade.
        let short: Vec<i16> = (0..35).collect();
        assert_eq!(loop_cut(&pcm(&short), 10, 3), pcm(&short));
        let long: Vec<i16> = (0..100).map(|i| i * 10).collect();
        let out = loop_cut(&pcm(&long), 10, 3);
        assert_eq!(out.len(), 30 * 2);
        let first = i16::from_le_bytes([out[0], out[1]]);
        // The loop's first sample follows its last as the source did.
        assert_eq!(first, long[30]);
        assert_eq!(&out[10..], &pcm(&long[5..30])[..]);
    }
}
