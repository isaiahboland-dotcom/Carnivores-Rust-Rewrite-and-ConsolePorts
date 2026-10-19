//! `c2-cook`: turns your game (vanilla, or with add-on mods / a recipe) into
//! one pack file for the 3DS: the chosen area, animals and weapons as game
//! files, plus their textures already converted for the 3DS GPU.
//!
//! ```text
//! c2-cook --game ../source/c2-original [--mod DIR]... [--recipe R.toml --mods DIR]
//!         [--area 1] [--time day|dusk|night] [--dinos NAME,NAME] [--weapons NAME,NAME]
//!         [--ambient] [--target n3ds|o3ds] [--force] --out c2rs.c2pk
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
use c2_formats::cooked::{tile, write_pack, write_texture, TexFormat};
use c2_vfs::{MemBackend, Vfs};

const USAGE: &str = "usage:
  c2-cook --game <GAME_DIR> [--mod MOD]... [--recipe RECIPE.toml --mods DIR]
          [--area N] [--time day|dusk|night] [--dinos NAME,NAME] [--weapons NAME,NAME]
          [--ambient] [--target n3ds|o3ds] [--force] --out <PACK.c2pk>

  --dinos    huntable animals to include, by _RES.TXT name (default: Parasaurolophus,Velociraptor)
  --weapons  weapons to include, by name (default: the first two)
  --ambient  also include the small ambient animals (Moschops, Gallimimus, ...)
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
    /// GPU (linear) memory for textures and vertex buffers, KiB.
    linear_kb: usize,
}

const N3DS: Profile = Profile {
    name: "n3ds",
    terrain_tile: 64,
    model_tex: 128,
    view: 42,
    object_view: 28,
    // ~124 MiB for the app: 16 MiB linear (`c2-3ds` sets it), code, slack.
    heap_kb: 96 * 1024,
    linear_kb: 12 * 1024,
};

const O3DS: Profile = Profile {
    name: "o3ds",
    terrain_tile: 32,
    model_tex: 64,
    view: 26,
    object_view: 18,
    // 64 MiB for the app: 16 MiB linear, code, slack.
    heap_kb: 40 * 1024,
    linear_kb: 12 * 1024,
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
fn load_peak(entries: &[(String, Vec<u8>)], area: &str, time: TimeOfDay) -> Result<usize> {
    let mut mem = MemBackend::new();
    for (name, data) in entries {
        mem.insert(name, data.clone());
    }
    let mut vfs = Vfs::new();
    vfs.push("pack", Box::new(mem));
    let base = IN_USE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let loaded = (
        c2_core::load::area_with(&vfs, area, time, true)?,
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
    area: u32,
    time: Option<String>,
    dinos: Option<String>,
    weapons: Option<String>,
    ambient: bool,
    target: Option<String>,
    force: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut a = Args {
        area: 1,
        ..Default::default()
    };
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
            "--area" => a.area = val()?.parse().context("--area takes a number")?,
            "--time" => a.time = Some(val()?),
            "--dinos" => a.dinos = Some(val()?),
            "--weapons" => a.weapons = Some(val()?),
            "--target" => a.target = Some(val()?),
            "--out" => a.out = Some(val()?.into()),
            "--ambient" => a.ambient = true,
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

/// `BrightenTexture` at night: every texel becomes `(G/8, G, G/8)`.
fn night_vision(rgba: &mut [u8]) {
    for p in rgba.as_chunks_mut::<4>().0 {
        let lo = (p[1] >> 3) & !7;
        p[0] = lo;
        p[2] = lo;
    }
}

struct Cooker {
    profile: &'static Profile,
    night: bool,
    entries: Vec<(String, Vec<u8>)>,
    /// Estimated 3DS GPU memory by kind, KiB.
    linear: BTreeMap<&'static str, usize>,
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
    /// `.TX`) and its sounds' samples (kept as empty sounds, so indices hold).
    fn car(&mut self, vfs: &Vfs, path: &str) -> Result<Vec<u8>> {
        let mut car =
            c2_formats::car::parse(&vfs.read(path)?).with_context(|| format!("parsing {path}"))?;
        let rgba = PreparedModel::from_mesh(&car.mesh).texture_rgba;
        car.mesh.texture = Vec::new();
        for s in &mut car.sounds {
            s.pcm = Vec::new();
        }
        self.add(path, c2_formats::car::write(&car));
        Ok(rgba)
    }

    /// A model texture (256x256 RGBA8) at the profile's size.
    fn model_texture(&mut self, name: String, rgba: &[u8]) {
        let side = self.profile.model_tex.min(256);
        let mut px = downscale(rgba, 256, 256, side, side);
        if self.night {
            night_vision(&mut px);
        }
        let t = tile(&px, side, side, TexFormat::Rgba5551);
        *self.linear.entry("textures").or_default() += t.data.len() / 1024;
        self.add(name, write_texture(&t));
    }
}

fn pick(names: &[String], want: &str, what: &str) -> Result<Vec<usize>> {
    want.split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(|w| {
            names
                .iter()
                .position(|n| n.eq_ignore_ascii_case(w))
                .with_context(|| format!("no {what} named {w:?}; there are: {}", names.join(", ")))
        })
        .collect()
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            // `c2_core::load` warns about every species whose model is not in
            // the pack, which the cooker leaves out on purpose.
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,c2_core::load=error".into()),
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
    let time = match a.time.as_deref().unwrap_or("day") {
        "day" => TimeOfDay::Day,
        "dusk" => TimeOfDay::Dusk,
        "night" => TimeOfDay::Night,
        t => bail!("unknown time {t:?} (day, dusk or night)"),
    };
    let vfs = merged(&a, &game)?;
    let res = c2_core::load::res(&vfs)?;
    let mut c = Cooker {
        profile,
        night: time == TimeOfDay::Night,
        entries: Vec::new(),
        linear: BTreeMap::new(),
    };

    // Which animals: the hunter (his body after a death), the chosen
    // huntable species and, optionally, the ambient ones (CType 1..=5).
    let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
    let names: Vec<String> = infos.iter().map(|i| i.name.clone()).collect();
    let mut chars = vec![0];
    if a.ambient {
        chars.extend(1..=5.min(infos.len().saturating_sub(1)));
    }
    let dinos = pick(
        &names,
        a.dinos.as_deref().unwrap_or("Parasaurolophus,Velociraptor"),
        "animal",
    )?;
    chars.extend(&dinos);
    chars.sort_unstable();
    chars.dedup();
    let winfos: Vec<WeaponInfo> = res.weapons.iter().map(WeaponInfo::from_def).collect();
    let wnames: Vec<String> = winfos.iter().map(|w| w.name.clone()).collect();
    let weapons = match &a.weapons {
        Some(w) => pick(&wnames, w, "weapon")?,
        None => (0..wnames.len().min(2)).collect(),
    };

    c.copy(&vfs, "HUNTDAT/_RES.TXT")?;
    let area_name = format!("AREA{}", a.area);
    c.copy(&vfs, &format!("HUNTDAT/AREAS/{area_name}.MAP"))?;
    // The pickup ship and the water ring (`WCIRCLE2.CAR`, RenderWCircles).
    for (file, tx) in [("SHIP2A.CAR", "SHIP"), ("WCIRCLE2.CAR", "RING")] {
        let path = format!("HUNTDAT/{file}");
        if vfs.resolve(&path).is_some() {
            let rgba = c.car(&vfs, &path)?;
            c.model_texture(format!("C2RS/{tx}.TX"), &rgba);
        }
    }
    // Sun or moon (`Hunt2.cpp`: MOON.3DF at night, SUN2.3DF otherwise).
    let sun = if time == TimeOfDay::Night {
        "HUNTDAT/MOON.3DF"
    } else {
        "HUNTDAT/SUN2.3DF"
    };
    if let Ok(d) = vfs.read(sun) {
        let m = c2_formats::three_df::parse(&d).with_context(|| format!("parsing {sun}"))?;
        c.add("C2RS/SUN.3DF", d);
        // Not night-vision tinted: the PC draws it as it is.
        let night = std::mem::replace(&mut c.night, false);
        c.model_texture(
            "C2RS/SUN.TX".into(),
            &PreparedModel::from_mesh(&m.mesh).texture_rgba,
        );
        c.night = night;
    }
    for &i in &chars {
        let rgba = c.car(&vfs, &format!("HUNTDAT/{}", infos[i].file))?;
        c.model_texture(format!("C2RS/CHAR{i}.TX"), &rgba);
    }
    for &i in &weapons {
        let rgba = c.car(&vfs, &format!("HUNTDAT/WEAPONS/{}", winfos[i].file))?;
        c.model_texture(format!("C2RS/WPN{i}.TX"), &rgba);
    }

    // The area: its terrain textures in one atlas, one texture per object.
    let area = c2_core::load::area(&vfs, &area_name, time)?;
    let n = area.rsc.textures.len().max(1);
    let mut t = profile.terrain_tile;
    while (1024 / t) * (1024 / t) < n && t > 8 {
        t /= 2;
    }
    // Square: the smallest power-of-two side that holds every tile.
    let mut side = t;
    while (side / t) * (side / t) < n {
        side *= 2;
    }
    let cols = side / t;
    let (aw, ah) = (side, side);
    let mut atlas = vec![0u8; aw * ah * 4];
    for (k, tex) in area.rsc.textures.iter().enumerate() {
        let mut px = downscale(&image_to_rgba8(tex, false), 128, 128, t, t);
        if c.night {
            night_vision(&mut px);
        }
        let (ox, oy) = ((k % cols) * t, (k / cols) * t);
        for y in 0..t {
            let d = ((oy + y) * aw + ox) * 4;
            atlas[d..d + t * 4].copy_from_slice(&px[y * t * 4..(y + 1) * t * 4]);
        }
    }
    // The cloud plane (RenderSkyPlane), as on the PC: no night vision.
    let sky_side = (profile.model_tex * 2).min(256);
    let sky = downscale(
        &image_to_rgba8(&area.rsc.skies[time as usize], false),
        256,
        256,
        sky_side,
        sky_side,
    );
    let sky = tile(&sky, sky_side, sky_side, TexFormat::Rgb565);
    *c.linear.entry("textures").or_default() += sky.data.len() / 1024;
    c.add("C2RS/SKY.TX", write_texture(&sky));
    let tex = tile(&atlas, aw, ah, TexFormat::Rgb565);
    *c.linear.entry("textures").or_default() += tex.data.len() / 1024;
    c.add("C2RS/TERRAIN.TX", write_texture(&tex));
    for (k, o) in area.rsc.objects.iter().enumerate() {
        let p = PreparedModel::from_mesh(&o.model.mesh);
        c.model_texture(format!("C2RS/OBJ{k}.TX"), &p.texture_rgba);
        // Vertex buffer: 24 bytes per face corner.
        *c.linear.entry("vertex buffers").or_default() += p.faces.len() * 3 * 24 / 1024;
    }
    // Terrain chunks around the hunter: 6 corners per cell, 24 bytes each.
    let side = (2 * profile.view + 32) as usize;
    *c.linear.entry("vertex buffers").or_default() += side * side * 6 * 24 / 1024;
    // Framebuffers, depth buffer, command buffer.
    *c.linear.entry("screens").or_default() += 3 * 1024;
    // The RSC without what is now in the .TX files or only the PC uses:
    // terrain and object textures, sounds (kept as empty sounds).
    let mut rsc = area.rsc;
    rsc.textures = Vec::new();
    for o in &mut rsc.objects {
        o.model.mesh.texture = Vec::new();
    }
    for s in &mut rsc.random_sounds {
        *s = Vec::new();
    }
    for amb in &mut rsc.ambients {
        amb.pcm = Vec::new();
    }
    c.add(
        format!("HUNTDAT/AREAS/{area_name}.RSC"),
        c2_formats::rsc::write(&rsc),
    );
    drop(rsc);

    let list = |v: &[usize]| v.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
    let linear: usize = c.linear.values().sum();
    let info = format!(
        "pack = 2\ntarget = {}\narea = {area_name}\ntime = {}\ndinos = {}\nweapons = {}\n\
         terrain_tile = {t}\natlas_cols = {cols}\nterrain_count = {n}\nview = {}\n\
         object_view = {}\n",
        profile.name,
        a.time.as_deref().unwrap_or("day"),
        list(&dinos),
        list(&weapons),
        profile.view,
        profile.object_view,
    );
    c.entries.push(("C2RS/INFO.TXT".into(), info.into_bytes()));

    println!("{} pack: {area_name}, {:?}", profile.name, time);
    println!(
        "  animals: {}",
        chars
            .iter()
            .map(|&i| names[i].as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  weapons: {}",
        weapons
            .iter()
            .map(|&i| wnames[i].as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let heap = load_peak(&c.entries, &area_name, time)? / 1024;
    let mib = |kb: usize| kb as f32 / 1024.0;
    println!("  3DS memory:");
    println!(
        "    loading peak    {:>6.1} MiB (budget {:.0} MiB)",
        mib(heap),
        mib(profile.heap_kb)
    );
    for (k, v) in &c.linear {
        println!("    {k:<15} {:>6.1} MiB", mib(*v));
    }
    println!(
        "    GPU total       {:>6.1} MiB (budget {:.0} MiB)",
        mib(linear),
        mib(profile.linear_kb)
    );
    if (heap > profile.heap_kb || linear > profile.linear_kb) && !a.force {
        bail!(
            "over the {} budget; choose fewer animals, use --target o3ds for smaller textures, or --force",
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
