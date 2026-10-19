//! `c2-parity`: checks our parsers against real game data.
//!
//! `c2-parity scan <dir>...` walks each directory, parses every file with a
//! known extension, and checks that writing it back reproduces the input
//! byte for byte. Exit status is non-zero if anything fails.

use std::path::{Path, PathBuf};

mod strstr;

use anyhow::{bail, Context, Result};
use c2_formats::{car, map, res_txt::ResTxt, rsc, sav, tga, three_df, wav};

#[derive(Default)]
struct Tally {
    ok: usize,
    failed: Vec<(PathBuf, String)>,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        if p.is_dir() {
            // ReShade ships with some installs; it is not game data.
            if p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("reshade-shaders"))
            {
                continue;
            }
            walk(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// Parses and round-trips one file. `Ok(None)` means the extension is not ours.
fn check(path: &Path) -> Result<Option<String>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let data = || std::fs::read(path).with_context(|| format!("reading {}", path.display()));
    let roundtrip = |orig: &[u8], written: Vec<u8>| -> Result<()> {
        if orig != written.as_slice() {
            let at = orig
                .iter()
                .zip(&written)
                .position(|(a, b)| a != b)
                .unwrap_or(orig.len().min(written.len()));
            bail!(
                "round-trip mismatch at byte {at:#x} (orig {} bytes, written {})",
                orig.len(),
                written.len()
            );
        }
        Ok(())
    };
    let summary = match ext.as_deref() {
        Some("car") => {
            let d = data()?;
            let c = car::parse(&d)?;
            roundtrip(&d, car::write(&c))?;
            format!(
                "{} verts, {} faces, {} anims, {} sounds",
                c.mesh.vertices.len(),
                c.mesh.faces.len(),
                c.animations.len(),
                c.sounds.len()
            )
        }
        Some("3df") => {
            let d = data()?;
            let m = three_df::parse(&d)?;
            roundtrip(&d, three_df::write(&m))?;
            format!(
                "{} verts, {} faces, {} bones",
                m.mesh.vertices.len(),
                m.mesh.faces.len(),
                m.bones.len()
            )
        }
        Some("map") => {
            let d = data()?;
            let m = map::parse(&d)?;
            roundtrip(&d, map::write(&m))?;
            "1024x1024".to_string()
        }
        Some("rsc") => {
            let d = data()?;
            let r = rsc::parse(&d)?;
            roundtrip(&d, rsc::write(&r))?;
            format!(
                "{} textures, {} objects, {} ambients",
                r.textures.len(),
                r.objects.len(),
                r.ambients.len()
            )
        }
        Some("tga") => {
            let d = data()?;
            let p = tga::parse(&d)?;
            let mut s = format!("{}x{}", p.width, p.height);
            // For the vanilla encoding, our pixels must equal exactly what
            // LoadPictureTGA reads: raw u16s from offset 18, rows flipped.
            if d.get(..3) == Some(&[0, 0, 2]) && d.get(16) == Some(&16) && d[17] & 0x20 == 0 {
                let ours = tga::write(&p);
                if d.get(18..ours.len()) != ours.get(18..) {
                    bail!("pixels differ from the original loader's reading");
                }
            } else {
                s.push_str(&format!(
                    " (NON-VANILLA encoding: type {}, {}-bit)",
                    d[2], d[16]
                ));
            }
            s
        }
        Some("sav") => {
            let d = data()?;
            let s = sav::parse(&d)?;
            roundtrip(&d, sav::write(&s))?;
            format!(
                "player {:?}, score {}, {} trophies",
                s.player_name(),
                s.score,
                s.bodies.iter().filter(|b| b.ctype != 0).count()
            )
        }
        Some("wav") => {
            let d = data()?;
            let w = wav::parse(&d)?;
            match w.format {
                Some(f) if f == wav::WavFormat::VANILLA => format!("{} bytes PCM", w.pcm.len()),
                f => format!("{} bytes PCM, NON-VANILLA format {f:?}", w.pcm.len()),
            }
        }
        Some("txt")
            if path
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("_res.txt")) =>
        {
            let d = data()?;
            let r = ResTxt::parse(&String::from_utf8_lossy(&d))?;
            // Writing normalises whitespace, so compare the re-parsed tree instead of bytes.
            let again = ResTxt::parse(&r.write())?;
            if again.sections != r.sections {
                bail!("re-parse of written _RES.TXT differs");
            }
            let data = r.to_data();
            let faithful = strstr::parse(&String::from_utf8_lossy(&d));
            let mut s = format!(
                "{} weapons, {} characters",
                data.weapons.len(),
                data.characters.len()
            );
            for w in strstr::diff(&data, &faithful) {
                s.push_str(&format!("\n      strstr-diff: {w}"));
            }
            for w in &r.warnings {
                s.push_str(&format!("\n      warning: {w}"));
            }
            s
        }
        _ => return Ok(None),
    };
    Ok(Some(summary))
}

/// `c2-parity walk <AREA.MAP> <seconds>`: headless hunter simulation that
/// walks/runs forward from the first landing spot, turning gently, and
/// prints the trajectory. A quick sanity check for movement and collision.
fn walk_sim(map_path: &Path, seconds: u32) -> Result<()> {
    use c2_core::area::{Area, TimeOfDay};
    use c2_core::player::{Controls, Player};
    let stem = map_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = map_path.parent().unwrap_or(Path::new("."));
    let rsc_path = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&stem))
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rsc"))
        })
        .context("no matching .RSC")?;
    let area = Area::new(
        map::parse(&std::fs::read(map_path)?)?,
        rsc::parse(&std::fs::read(rsc_path)?)?,
        TimeOfDay::Day,
    );
    let mut p = Player::new(&area, area.landings[0]);
    let mut events = Vec::new();
    let mut c = Controls {
        forward: true,
        run: true,
        mouse_sens: 128.0,
        ..Default::default()
    };
    let (mut blocked, mut last) = (0u32, (p.x, p.z));
    for t in 0..seconds * 100 {
        c.mouse = (if (t / 300) % 2 == 0 { 1.0 } else { -1.0 }, 0.0);
        c.jump = t % 700 == 0;
        p.tick(&area, &c, 10, &mut events);
        let ground = area.land_qh_no_obj(p.x, p.z);
        if p.y < ground - 81.0 {
            bail!(
                "t={}ms: fell through the ground (y {:.0} < ground {:.0})",
                t * 10,
                p.y,
                ground
            );
        }
        if t % 100 == 99 {
            let moved = ((p.x - last.0).powi(2) + (p.z - last.1).powi(2)).sqrt();
            if moved < 50.0 {
                blocked += 1;
            }
            last = (p.x, p.z);
            println!(
                "t={:>3}s pos=({:>7.0},{:>6.0},{:>7.0}) yaw={:.2} cell=({},{}) speed={:.2} moved={:>5.0}/s head={:.0}{}{}",
                (t + 1) / 100, p.x, p.y, p.z, p.alpha, (p.x / 256.0) as i32, (p.z / 256.0) as i32,
                p.vspeed, moved, p.head_y,
                if p.swim { " swim" } else { "" },
                if p.underwater { " underwater" } else { "" }
            );
        }
    }
    let n: std::collections::HashMap<String, usize> =
        events.iter().fold(Default::default(), |mut m, e| {
            *m.entry(format!("{e:?}")).or_default() += 1;
            m
        });
    println!("events: {n:?}; seconds mostly blocked: {blocked}");
    Ok(())
}

/// `c2-parity dinos <AREA.MAP> [seconds]`: places the herd and runs the AI
/// headlessly with the hunter standing still, logging herbivore behaviour.
fn dinos_sim(map_path: &Path, seconds: u32) -> Result<()> {
    use c2_core::area::{Area, TimeOfDay};
    use c2_core::dino::{ai, Ctx, DinoInfo, Herd, HunterView, PlaceOptions, Species};
    use c2_core::rng::MsRand;
    let dir = map_path.parent().unwrap_or(Path::new("."));
    let huntdat = dir.parent().unwrap_or(Path::new("."));
    let find = |d: &Path, name: &str| -> Option<PathBuf> {
        std::fs::read_dir(d)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
            })
    };
    let stem = map_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rsc_path = find(dir, &format!("{stem}.rsc")).context("no .RSC")?;
    let area = Area::new(
        map::parse(&std::fs::read(map_path)?)?,
        rsc::parse(&std::fs::read(rsc_path)?)?,
        TimeOfDay::Day,
    );
    let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(
        find(huntdat, "_RES.TXT").context("no _RES.TXT")?,
    )?))?
    .to_data();
    let mut species = Vec::new();
    for d in &res.characters {
        let info = DinoInfo::from_def(d);
        let c = car::parse(&std::fs::read(
            find(huntdat, &info.file).with_context(|| info.file.clone())?,
        )?)?;
        species.push(Species::new(info, &c));
    }
    let mut rng = MsRand::new(12345);
    let (hx, hz) = (
        f32::from(area.landings[0].0) * 256.0 + 128.0,
        f32::from(area.landings[0].1) * 256.0 + 128.0,
    );
    let mut herd = Herd {
        species,
        chars: Vec::new(),
        demo_index: None,
    };
    herd.place(
        &area,
        (hx, hz),
        &mut rng,
        PlaceOptions {
            density: 128,
            opt_agres: 128,
            targets: 0,
            max_ambient: None,
        },
    );
    println!("placed {}:", herd.chars.len());
    for (i, c) in herd.chars.iter().enumerate() {
        println!(
            "  #{i} {} (ai {}) at cell ({}, {})",
            herd.species[c.ctype].info.name,
            c.ai,
            (c.pos[0] / 256.0) as i32,
            (c.pos[2] / 256.0) as i32
        );
    }
    let ported = |a: i32| a != ai::HUNTER;
    let start: Vec<[f32; 3]> = herd.chars.iter().map(|c| c.pos).collect();
    let mut events = Vec::new();
    let hunter = HunterView {
        alpha: 0.0,
        x: hx,
        y: area.land_height(hx, hz),
        z: hz,
        head_y: 220.0,
        alive: true,
        underwater: false,
    };
    for t in 0..seconds * 100 {
        let mut ctx = Ctx {
            area: &area,
            rng: &mut rng,
            dt: 10,
            real_time: (t * 10) as i32,
            hunter,
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            run_k: 1.0,
            events: &mut events,
        };
        herd.step(&mut ctx);
        for c in &herd.chars {
            if !c.pos.iter().all(|v| v.is_finite()) {
                bail!("t={}ms: non-finite position for ai {}", t * 10, c.ai);
            }
        }
        if (t + 1) % 1000 == 0 {
            println!("t={}s", (t + 1) / 100);
            for (i, c) in herd.chars.iter().enumerate().filter(|(_, c)| ported(c.ai)) {
                let d =
                    ((c.pos[0] - start[i][0]).powi(2) + (c.pos[2] - start[i][2]).powi(2)).sqrt();
                println!(
                    "  #{i:<2} {:<16} phase {} speed {:.2} alpha {:.2} beta {:+.2} gamma {:+.2} travelled {:>6.0} state {}",
                    herd.species[c.ctype].info.name, c.phase, c.vspeed, c.alpha, c.beta, c.gamma, d, c.state
                );
            }
        }
    }
    println!(
        "sound events: {}",
        events
            .iter()
            .filter(|e| matches!(e, c2_core::dino::DinoEvent::Sound { .. }))
            .count()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("dinos") {
        let path = args
            .get(1)
            .context("usage: c2-parity dinos <AREA.MAP> [seconds]")?;
        let secs = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(30);
        return dinos_sim(Path::new(path), secs);
    }
    if args.first().map(String::as_str) == Some("walk") {
        let path = args
            .get(1)
            .context("usage: c2-parity walk <AREA.MAP> [seconds]")?;
        let secs = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(30);
        return walk_sim(Path::new(path), secs);
    }
    let (Some("scan"), dirs) = (args.first().map(String::as_str), &args[1.min(args.len())..])
    else {
        bail!("usage: c2-parity scan <dir>...");
    };
    if dirs.is_empty() {
        bail!("usage: c2-parity scan <dir>...");
    }
    let verbose = std::env::var_os("VERBOSE").is_some();
    let mut t = Tally::default();
    for dir in dirs {
        let mut files = Vec::new();
        walk(Path::new(dir), &mut files)?;
        files.sort();
        for f in files {
            match check(&f) {
                Ok(None) => {}
                Ok(Some(s)) => {
                    t.ok += 1;
                    if verbose
                        || s.contains("NON-VANILLA")
                        || s.contains("warning")
                        || s.contains("strstr-diff")
                    {
                        println!("ok   {}: {s}", f.display());
                    }
                }
                Err(e) => {
                    println!("FAIL {}: {e:#}", f.display());
                    t.failed.push((f, format!("{e:#}")));
                }
            }
        }
    }
    println!("{} ok, {} failed", t.ok, t.failed.len());
    if !t.failed.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
