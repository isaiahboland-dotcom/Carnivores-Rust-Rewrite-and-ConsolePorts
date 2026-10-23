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

const USAGE: &str = "usage:
  c2-cook --game <GAME_DIR> [--overlay DIR]... [--mod MOD]...
          [--recipe RECIPE.toml --mods DIR] [--balance FILE] [--areas N,N]
          [--mobile-survival FILE [--survival-modes ours|mobile|both]]
          [--target n3ds|o3ds] [--force] --out <PACK.c2pk>

  c2-cook --check <PACK.c2pk>   loads a pack as the 3DS does and plays a short
                                hunt for each animal (finds missing files);
                                with --bench NAME, that animal up close, killed

  --overlay  a game folder copied over GAME_DIR first (e.g. the Modder's
             Edition, which most add-ons are made for); repeatable
  --balance  prices and scores over the game (a balance file; see `c2-pc balance`)
  --menu-art the menu style: a game folder whose title art is used (Ice Age's)
  --mobile-survival  the mobile game's Survive mode (mobile-survival.txt from an
             APK import), and --survival-modes ours|mobile|both which are offered
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
    view: 42,
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
    view: 34,
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
            picked: u128::MAX,
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
    mods_dir: Option<PathBuf>,
    balance: Option<PathBuf>,
    /// Menu style: the game folder whose title art is used (Ice Age's).
    menu_art: Option<PathBuf>,
    /// The mobile game's Survive mode, and which survivals are offered.
    mobile_survival: Option<PathBuf>,
    survival_modes: Option<String>,
    areas: Option<String>,
    target: Option<String>,
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
            "--mods" => a.mods_dir = Some(val()?.into()),
            "--balance" => a.balance = Some(val()?.into()),
            "--menu-art" => a.menu_art = Some(val()?.into()),
            "--mobile-survival" => a.mobile_survival = Some(val()?.into()),
            "--survival-modes" => a.survival_modes = Some(val()?),
            "--areas" => a.areas = Some(val()?),
            "--target" => a.target = Some(val()?),
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
    Ok(vfs)
}

fn merged_game(a: &Args, game: &Path) -> Result<Vfs> {
    let bases: Vec<PathBuf> = std::iter::once(game.to_path_buf())
        .chain(a.overlays.iter().cloned())
        .collect();
    let game = &bases[..];
    if let Some(r) = &a.recipe {
        let mods_dir = a.mods_dir.as_deref().context("--recipe needs --mods DIR")?;
        let recipe = c2_mods::recipe::Recipe::load(r)?;
        let vanilla = c2_mods::vanilla(game)?;
        let found = c2_mods::recipe::locate(&recipe, mods_dir);
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
        return Ok(c2_mods::build_with(game, sources, &opts)?.vfs);
    }
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
    c.copy(vfs, &format!("HUNTDAT/AREAS/{name}.MAP"))?;
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
        let pcm = std::mem::take(&mut amb.pcm);
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
    /// Models of roaming add-ons (huntable, no menu slot), by `model_key`:
    /// they get a menu picture drawn from the model.
    roamers: std::collections::BTreeSet<String>,
    /// The menu: `_MENU.TXT` if there is one, else `_RES.TXT`.
    menu: c2_formats::menu_txt::MenuData,
}

/// A dialect character with a menu slot (`callNo`): a hunt target. Without
/// one, AI under 10 (the hunter, small ambient animals, flyers) is loaded
/// for every hunt; the rest (add-ons that roam, brachiosaurs, fish) only
/// when picked. `c2-3ds` (`hunt::Role`) decides the same way.
fn has_slot(b: &c2_formats::res_txt::Block) -> bool {
    b.get(&["callNo"])
        .is_some_and(|v| c2_formats::res_txt::c_atoi(v) >= 0)
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
            picked: u128::MAX,
        };
        let m = c2_core::mee::read(&raw, &h);
        let slots: Vec<bool> = m
            .tables
            .characters
            .iter()
            .map(|c| has_slot(&c.block))
            .collect();
        (m.res, slots)
    } else {
        (c2_core::load::res(vfs)?, Vec::new())
    };
    let mut models = BTreeMap::new();
    let mut roamers = std::collections::BTreeSet::new();
    for (i, def) in res.characters.iter().enumerate() {
        let info = DinoInfo::from_def(def);
        let key = c2_formats::cooked::model_key(&info.file);
        if dialect && !slots.get(i).copied().unwrap_or(false) && (10..30).contains(&info.ai) {
            roamers.insert(key.clone());
        }
        let always = if dialect {
            !slots.get(i).copied().unwrap_or(false) && (0..10).contains(&info.ai)
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
        Err(_) => raw,
    };
    let menu = c2_formats::menu_txt::MenuData::from_res(&menu_src);
    Ok(Rules {
        dialect,
        res,
        models,
        roamers,
        menu,
    })
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
    let profile = match a.target.as_deref().unwrap_or("n3ds") {
        "n3ds" => &N3DS,
        "o3ds" => &O3DS,
        t => bail!("unknown target {t:?} (n3ds or o3ds)"),
    };
    let vfs = merged(&a, &game)?;
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

    // Everything a hunt can use, whatever is picked.
    c.copy(&vfs, "HUNTDAT/_RES.TXT")?;
    if vfs.resolve("HUNTDAT/_MENU.TXT").is_some() {
        c.copy(&vfs, "HUNTDAT/_MENU.TXT")?;
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
    // hunter (6 corners per cell, 24 bytes each).
    c.count("screens", 3 << 20);
    let side = (2 * profile.view + 32) as usize;
    c.count("vertex buffers", side * side * 6 * 24);

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
        c.menu_picture(
            &vfs,
            &format!("DINO{n}.TGA"),
            &format!("C2RS/MENU/DINO{n}.TX"),
            128,
        );
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
    let ambient: usize = always.iter().map(|k| model_cost(k)).sum();
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
    let mut over = false;
    let mut worst = false;
    let mut base_heap = BTreeMap::new();
    for name in &areas {
        let base = load_peak(&c.entries, name, rules.dialect, &always, &[])? / 1024;
        let lin = sum("hunt") + sum(name) + ambient;
        base_heap.insert(name.clone(), base);
        println!(
            "    {name:<8} heap {:>5.1} + {:>4.1} MiB, linear {:>5.1} + {:>4.1} MiB",
            mib(base),
            mib(picks_heap),
            mib(lin),
            mib(picks_linear)
        );
        over |= base > profile.heap_kb || lin > profile.linear_kb;
        worst |= base + picks_heap > profile.heap_kb || lin + picks_linear > profile.linear_kb;
    }
    println!(
        "    budget: heap {:.0} MiB, linear {:.0} MiB; menu pictures {:.1} MiB (menu only)",
        mib(profile.heap_kb),
        mib(profile.linear_kb),
        mib(sum("menu"))
    );
    if worst && !over {
        println!(
            "  The costliest picks don't fit together; the 3DS menu says when picks are too much."
        );
    }
    if over && !a.force {
        bail!(
            "over the {} budget; use --target o3ds for smaller textures, or --force",
            profile.name
        );
    }
    // What each piece costs, for the 3DS menu to check a hunt fits:
    // linear memory (`cost.`) and main memory (`heap.`), KiB.
    let mut info = format!(
        "pack = 4\ntarget = {}\ndialect = {}\nareas = {}\nview = {}\nobject_view = {}\n\
         sound_rate = {}\nlinear_kb = {}\nheap_kb = {}\n",
        profile.name,
        u8::from(rules.dialect),
        areas.join(","),
        profile.view,
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
    // Areas whose `.c2map` says it snows (Ice Age's beside C2's).
    let snowy: Vec<&String> = areas
        .iter()
        .filter(|n| {
            vfs.read(&format!("HUNTDAT/AREAS/{n}.c2map"))
                .ok()
                .and_then(|d| c2_formats::menu_txt::parse_c2map(&String::from_utf8_lossy(&d)))
                .is_some_and(|a| a.snow)
        })
        .collect();
    if !snowy.is_empty() {
        let names: Vec<&str> = snowy.iter().map(|s| s.as_str()).collect();
        info += &format!("snow = {}\n", names.join(","));
    }
    // The mobile Survive mode, from the player's own APK import.
    if let Some(f) = &a.mobile_survival {
        let t = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        c.entries.push(("C2RS/SURVIVE.TXT".into(), t.into_bytes()));
        info += &format!(
            "survival = {}\n",
            a.survival_modes.as_deref().unwrap_or("both")
        );
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
}
