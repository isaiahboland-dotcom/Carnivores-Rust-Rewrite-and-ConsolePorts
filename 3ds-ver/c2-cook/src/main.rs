//! `c2-cook`: turns your game (vanilla, or with add-on mods / a recipe) into
//! one pack file for the 3DS: every area, animal and weapon as game files,
//! their textures and sounds already converted for the 3DS, and the menu
//! pictures. The 3DS loads only what is picked for a hunt.
//!
//! ```text
//! c2-cook --game ../source/c2-original [--mod DIR]... [--recipe R.toml --mods DIR]
//!         [--areas 1,2] [--target n3ds|o3ds] [--force] --out c2rs.c2pk
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

const USAGE: &str = "usage:
  c2-cook --game <GAME_DIR> [--mod MOD]... [--recipe RECIPE.toml --mods DIR]
          [--areas N,N] [--target n3ds|o3ds] [--force] --out <PACK.c2pk>

  --areas    only these areas (default: all, plus the trophy room)
  --target   n3ds (New 3DS, default) or o3ds (Old 3DS / 2DS: smaller textures, shorter view)
  --force    write the pack even if it is over the target's memory budget

Copy the pack to sdmc:/3ds/c2rs/ on the SD card.";

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
};

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
/// (`Game::load` in `c2-3ds/src/hunt.rs`; keep the two in step). Measured
/// here on the PC, where pointers are wider, so it errs on the high side.
fn load_peak(entries: &[(String, Vec<u8>)], area: &str) -> Result<usize> {
    let mut mem = MemBackend::new();
    for (name, data) in entries {
        mem.insert(name, data.clone());
    }
    let mut vfs = Vfs::new();
    vfs.push("pack", Box::new(mem));
    let base = IN_USE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let loaded = (
        c2_core::load::area_with(&vfs, area, TimeOfDay::Day, true)?,
        c2_core::load::res(&vfs).map(|res| {
            let s = c2_core::load::species(&vfs, &res);
            let w = c2_core::load::weapons(&vfs, &res);
            (res, s, w)
        })?,
    );
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    drop(loaded);
    Ok(peak)
}

#[derive(Default)]
struct Args {
    game: Option<PathBuf>,
    mods: Vec<PathBuf>,
    recipe: Option<PathBuf>,
    mods_dir: Option<PathBuf>,
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
            "--mod" => a.mods.push(val()?.into()),
            "--recipe" => a.recipe = Some(val()?.into()),
            "--mods" => a.mods_dir = Some(val()?.into()),
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

/// The game as the engine would see it: vanilla, plus mods or a recipe.
fn merged(a: &Args, game: &Path) -> Result<Vfs> {
    let game = &[game.to_path_buf()][..];
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
        for (k, s) in car.sounds.iter_mut().enumerate() {
            let pcm = std::mem::take(&mut s.pcm);
            self.sound(&format!("{snd}{k}"), &pcm, SAMPLE_RATE, 1, 16);
        }
        self.add(path, c2_formats::car::write(&car));
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

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let a = parse_args()?;
    let game = a.game.clone().context(USAGE)?;
    let out = a.out.clone().context(USAGE)?;
    let profile = match a.target.as_deref().unwrap_or("n3ds") {
        "n3ds" => &N3DS,
        "o3ds" => &O3DS,
        t => bail!("unknown target {t:?} (n3ds or o3ds)"),
    };
    let vfs = merged(&a, &game)?;
    let res = c2_core::load::res(&vfs)?;
    let mut c = Cooker {
        profile,
        entries: Vec::new(),
        linear: BTreeMap::new(),
        scope: "hunt".into(),
        sound_rate: profile.sound_rate,
    };
    let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
    let winfos: Vec<WeaponInfo> = res.weapons.iter().map(WeaponInfo::from_def).collect();

    // Areas: AREA1.. as present (or as asked), then the trophy room.
    let wanted: Option<Vec<u32>> = a
        .areas
        .as_deref()
        .map(|l| l.split(',').filter_map(|v| v.trim().parse().ok()).collect());
    let mut areas: Vec<String> = (1..=9)
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
    for (i, info) in infos.iter().enumerate() {
        let path = format!("HUNTDAT/{}", info.file);
        if vfs.resolve(&path).is_none() {
            continue;
        }
        c.scope = format!("char{i}");
        let rgba = c.car(&vfs, &path, &format!("C{i}_"))?;
        c.model_texture(format!("C2RS/CHAR{i}.TX"), &rgba);
        // Its call (MENU/PICS/CALLn.TGA, SOUNDFX/CALLS/CALLn_*.WAV,
        // n = AI - 9).
        let ai = info.ai;
        if ai < 10 {
            continue;
        }
        for (v, l) in ["A", "B", "C"].iter().enumerate() {
            let f = format!("HUNTDAT/SOUNDFX/CALLS/CALL{}_{l}.WAV", ai - 9);
            c.wav(&vfs, &f, &format!("K{ai}_{v}"));
        }
        if let Some(p) = vfs
            .read(&format!("HUNTDAT/MENU/PICS/CALL{}.TGA", ai - 9))
            .ok()
            .and_then(|d| c2_formats::tga::parse(&d).ok())
        {
            let px = image_to_rgba8(&p.pixels, false);
            let px = fit(&px, p.width, p.height, 64, 52, 64, 64);
            c.picture(&format!("C2RS/CALL{ai}.TX"), &px, 64, 64);
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
    let huntable: Vec<i32> = infos.iter().map(|i| i.ai).filter(|&ai| ai >= 10).collect();
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
    for n in 1..=huntable.len() {
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
    if let Some(p) = vfs
        .read("HUNTDAT/MENU/MENUM.TGA")
        .ok()
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
    // Linear memory per piece, for the menu to check a hunt fits.
    let mut info = format!(
        "pack = 3\ntarget = {}\nareas = {}\nview = {}\nobject_view = {}\nsound_rate = {}\n\
         linear_kb = {}\n",
        profile.name,
        areas.join(","),
        profile.view,
        profile.object_view,
        profile.sound_rate,
        profile.linear_kb,
    );
    for (scope, kinds) in &c.linear {
        if scope != "menu" {
            info += &format!("cost.{scope} = {}\n", kinds.values().sum::<usize>());
        }
    }
    c.entries.push(("C2RS/INFO.TXT".into(), info.into_bytes()));

    // Memory: the loading peak (each area, every animal and weapon); linear
    // memory for each area with the hunter, the ambient animals, the two
    // costliest huntable ones and weapons. The menu checks real picks.
    let mib = |kb: usize| kb as f32 / 1024.0;
    let sum = |s: &str| c.linear.get(s).map_or(0, |m| m.values().sum::<usize>());
    let ambient: usize = (0..infos.len())
        .filter(|&i| infos[i].ai < 10)
        .map(|i| sum(&format!("char{i}")))
        .sum();
    let top2 = |mut v: Vec<usize>| {
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.iter().take(2).sum::<usize>()
    };
    let dinos2 = top2(
        (0..infos.len())
            .filter(|&i| infos[i].ai >= 10)
            .map(|i| sum(&format!("char{i}")))
            .collect(),
    );
    let weapons2 = top2((0..winfos.len()).map(|i| sum(&format!("wpn{i}"))).collect());
    println!(
        "{} pack: {} ({} animals, {} weapons)",
        profile.name,
        areas.join(", "),
        infos.len(),
        winfos.len()
    );
    println!("  3DS memory (linear: 2 animals, 2 weapons; the menu checks real picks):");
    let mut over = false;
    for name in &areas {
        let heap = load_peak(&c.entries, name)? / 1024;
        let linear = sum("hunt") + sum(name) + ambient + dinos2 + weapons2;
        println!(
            "    {name:<8} loading peak {:>5.1} MiB, linear {:>5.1} MiB",
            mib(heap),
            mib(linear)
        );
        over |= heap > profile.heap_kb || linear > profile.linear_kb;
    }
    println!(
        "    budget: heap {:.0} MiB, linear {:.0} MiB; menu pictures {:.1} MiB (menu only)",
        mib(profile.heap_kb),
        mib(profile.linear_kb),
        mib(sum("menu"))
    );
    if over && !a.force {
        bail!(
            "over the {} budget; use --target o3ds for smaller textures, or --force",
            profile.name
        );
    }
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
