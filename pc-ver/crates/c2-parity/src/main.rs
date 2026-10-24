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
        ..Default::default()
    };
    herd.place(
        &area,
        (hx, hz),
        &mut rng,
        PlaceOptions {
            density: 128,
            opt_agres: 128,
            targets: c2_formats::bits::Bits::NONE,
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
            render_r: 74,
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

/// The Modder's Edition tables a hunt would use: spawn groups, pack groups,
/// and which characters spawn where.
fn mee_report(game: &Path, area: u32, time: u8, picked: u128) -> Result<()> {
    use c2_formats::mee::{is_dialect, Hunt, Tables};
    let res_path = find_ci(&game.join("HUNTDAT"), "_RES.TXT").context("no HUNTDAT/_RES.TXT")?;
    let res =
        c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(res_path)?))?;
    println!("dialect: {}", is_dialect(&res));
    let h = Hunt {
        area: Some(area),
        time,
        picked: c2_formats::bits::Bits::from(picked),
    };
    let t = Tables::read(&res, &h);
    println!("area {area}, time {time}, picked slots {picked:#b}");
    for (i, g) in t.spawn_groups.iter().enumerate() {
        println!(
            "spawn group {i}: {}..{} rate {} dens {} fwd {} rnd {} regions {} avoid {}",
            g.min,
            g.max,
            g.rate,
            g.density_multi,
            g.move_forward,
            g.randomise,
            g.regions.len(),
            g.avoid.len()
        );
    }
    for (i, p) in t.pack_groups.iter().enumerate() {
        println!(
            "pack group {i}: {}..{} density {} spawn {:?}",
            p.min, p.max, p.density, p.spawn
        );
    }
    for (i, c) in t.characters.iter().enumerate() {
        if c.spawn.is_empty() && c.packs.is_empty() {
            continue;
        }
        let name = c.block.get(&["name"]).unwrap_or("?");
        let ai = c.block.get(&["ai"]).unwrap_or("?");
        println!(
            "char {i} {name} ai {ai}: spawn {:?} packs {:?}",
            c.spawn, c.packs
        );
    }
    Ok(())
}

/// `path` (either slash, any case) under `base`.
fn find_path(base: &Path, path: &str) -> Option<PathBuf> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .try_fold(base.to_path_buf(), |d, c| find_ci(&d, c))
}

/// `c2-parity mee-sim <GAME_DIR> [AREA] [seconds]`: a Modder's Edition hunt
/// placed as the game would (day, nothing picked), run headlessly with the
/// hunter standing at the first landing; checks positions stay finite and
/// reports each species' phase and distance, and the slowest step.
fn mee_sim(game: &Path, area_n: u32, seconds: u32) -> Result<()> {
    use c2_core::area::{Area, TimeOfDay};
    use c2_core::dino::{Ctx, DinoInfo, Herd, HunterView, Species};
    use c2_core::rng::MsRand;
    use c2_formats::mee::Hunt;
    let huntdat = find_ci(game, "HUNTDAT").context("no HUNTDAT")?;
    let res_path = find_ci(&huntdat, "_RES.TXT").context("no _RES.TXT")?;
    let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(res_path)?))?;
    let h = Hunt {
        area: Some(area_n),
        time: 1,
        picked: c2_formats::bits::Bits::NONE,
    };
    let m = c2_core::mee::read(&res, &h);
    let areas = find_ci(&huntdat, "AREAS").context("no AREAS")?;
    let map_path = find_ci(&areas, &format!("AREA{area_n}.MAP")).context("no map")?;
    let rsc_path = find_ci(&areas, &format!("AREA{area_n}.RSC")).context("no rsc")?;
    let area = Area::new(
        map::parse(&std::fs::read(map_path)?)?,
        rsc::parse(&std::fs::read(rsc_path)?)?,
        TimeOfDay::Day,
    );
    let mut species = Vec::new();
    for d in &m.res.characters {
        let info = DinoInfo::from_def(d);
        let car = find_path(&huntdat, &info.file)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|d| car::parse(&d).ok());
        let empty = car::Car {
            name_raw: [0; 32],
            mesh: Default::default(),
            animations: vec![],
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        };
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty)));
    }
    c2_core::mee::tune(&mut species, &m.tables);
    for (i, s) in species.iter().enumerate() {
        if let Some(mi) = &s.mee {
            let n = s.ani_time.len();
            let bad: Vec<_> = mi.anim_index.iter().filter(|&&a| a >= n).collect();
            if !bad.is_empty() {
                println!(
                    "char {i} {}: anims {bad:?} beyond the model's {n}",
                    s.info.name
                );
            }
        }
    }
    let mut rng = MsRand::new(12345);
    let (hx, hz) = area
        .landings
        .first()
        .map_or((512.0 * 256.0, 512.0 * 256.0), |l| {
            (
                f32::from(l.0) * 256.0 + 128.0,
                f32::from(l.1) * 256.0 + 128.0,
            )
        });
    let mut herd = Herd {
        species,
        ..Default::default()
    };
    herd.place_mee(&area, (hx, hz), &mut rng, &m.tables, 128);
    let native = herd
        .chars
        .iter()
        .filter(|c| herd.species[c.ctype].mee.is_some())
        .count();
    println!(
        "placed {} ({native} on the dialect's AI), {} packs",
        herd.chars.len(),
        herd.packs.len()
    );
    let start: Vec<[f32; 3]> = herd.chars.iter().map(|c| c.pos).collect();
    let hunter = HunterView {
        x: hx,
        y: area.land_height(hx, hz),
        z: hz,
        head_y: 220.0,
        alive: true,
        ..Default::default()
    };
    let mut events = Vec::new();
    let mut slowest = std::time::Duration::ZERO;
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
            render_r: 74,
            run_k: 1.0,
            events: &mut events,
        };
        let t0 = std::time::Instant::now();
        herd.step(&mut ctx);
        slowest = slowest.max(t0.elapsed());
        if let Some(c) = herd
            .chars
            .iter()
            .find(|c| !c.pos.iter().all(|v| v.is_finite()))
        {
            bail!("t={}ms: non-finite position, ai {}", t * 10, c.ai);
        }
    }
    let mut by_species: std::collections::BTreeMap<usize, (usize, f32, Vec<usize>)> =
        Default::default();
    for (i, c) in herd.chars.iter().enumerate() {
        let d = ((c.pos[0] - start[i][0]).powi(2) + (c.pos[2] - start[i][2]).powi(2)).sqrt();
        let e = by_species.entry(c.ctype).or_default();
        e.0 += 1;
        e.1 += d;
        e.2.push(c.phase);
    }
    for (ct, (n, d, phases)) in by_species {
        let s = &herd.species[ct];
        println!(
            "  {:<24} ai {:>2} {}x  avg moved {:>6.0}  phases {:?}",
            s.info.name,
            s.info.ai,
            n,
            d / n as f32,
            phases
        );
    }
    println!(
        "{} events, slowest step {:.2} ms",
        events.len(),
        slowest.as_secs_f64() * 1000.0
    );
    Ok(())
}

/// `c2-parity tick-bench <GAME_DIR> [AREA]`: the owner's 3DS report
/// (sim time jumps once a Stegosaurus is in view, and stays up after
/// killing it). A vanilla hunt with the 3DS's options, the hunter parked
/// near a Stegosaurus for 20 s, then the animal killed and 60 s more; the
/// slowest and mean tick of every 5 s.
fn tick_bench(game: &Path, area_n: u32) -> Result<()> {
    use c2_core::area::{Area, TimeOfDay};
    use c2_core::dino::{ai, DinoInfo, Herd, PlaceOptions, Species};
    use c2_core::game::{Hunt, HuntInput, HuntOptions, WeaponInfo, Weapons};
    use c2_core::player::Player;
    use c2_core::rng::MsRand;
    let huntdat = find_ci(game, "HUNTDAT").context("no HUNTDAT")?;
    let res_path = find_ci(&huntdat, "_RES.TXT").context("no _RES.TXT")?;
    let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(res_path)?))?.to_data();
    let areas = find_ci(&huntdat, "AREAS").context("no AREAS")?;
    let map_path = find_ci(&areas, &format!("AREA{area_n}.MAP")).context("no map")?;
    let rsc_path = find_ci(&areas, &format!("AREA{area_n}.RSC")).context("no rsc")?;
    let area = Area::new(
        map::parse(&std::fs::read(map_path)?)?,
        rsc::parse(&std::fs::read(rsc_path)?)?,
        TimeOfDay::Day,
    );
    let mut species = Vec::new();
    let mut cars = Vec::new();
    for d in &res.characters {
        let info = DinoInfo::from_def(d);
        let car = find_path(&huntdat, &info.file)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|d| car::parse(&d).ok());
        let empty = car::Car {
            name_raw: [0; 32],
            mesh: Default::default(),
            animations: vec![],
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        };
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty)));
        cars.push(car);
    }
    let cars: Vec<Option<&car::Car>> = cars.iter().map(Option::as_ref).collect();
    let mut rng = MsRand::new(4242);
    let cell = area.landings.first().copied().unwrap_or((512, 512));
    let mut player = Player::new(&area, cell);
    let mut herd = Herd {
        species,
        ..Default::default()
    };
    herd.place(
        &area,
        (player.x, player.z),
        &mut rng,
        PlaceOptions {
            density: 128,
            opt_agres: 128,
            targets: c2_formats::bits::Bits::bit(ai::STEGO as usize),
            max_ambient: Some(4),
        },
    );
    let stego = herd
        .chars
        .iter()
        .position(|c| c.ai == ai::STEGO)
        .context("no Stegosaurus placed")?;
    // Park the hunter 1500 units from it, on the ground.
    let s = herd.chars[stego].pos;
    player.x = s[0] + 1500.0;
    player.z = s[2];
    player.y = area.land_height(player.x, player.z);
    println!("{} animals; Stegosaurus #{stego}", herd.chars.len());
    let opts = HuntOptions {
        targets: c2_formats::bits::Bits::bit(ai::STEGO as usize),
        radar: false,
        opt_agres: 128,
        opt_sens: 128,
        view_r: 42,
        render_r: 42,
        camo: false,
        scent: false,
        tranq: false,
        observer: false,
        trophy: false,
        score_factor: 1.0,
        dino_run_k: 1.0,
        blood: true,
        death_anim: true,
        afraid_every: 1,
        no_call_bonus: c2_formats::bits::Bits::NONE,
        notice_k: 1.0,
        stealth: false,
    };
    // The first weapon, drawn and aimed at the animal: real hits (blood
    // trail, hit reactions) rather than a scripted kill.
    let infos: Vec<_> = res
        .weapons
        .iter()
        .take(1)
        .map(WeaponInfo::from_def)
        .collect();
    let n = infos.len();
    let weapons = Weapons::new(
        infos,
        vec![[300; 4]; n],
        c2_formats::bits::Bits::bit(0),
        false,
    );
    let room = c2_core::profile::new_player("bench", 0, 0);
    let mut hunt = Hunt::new(&area, player, herd, weapons, rng, opts, room);
    let mut out = Vec::new();
    let (mut worst, mut sum, mut n) = (0f64, 0f64, 0u32);
    let mut hits = 0;
    for t in 0..8000u32 {
        let mut input = HuntInput {
            toggle_weapon: t == 10,
            ..Default::default()
        };
        let alive = hunt.herd.chars.get(stego).is_some_and(|c| c.health > 0);
        if (1500..6000).contains(&t) && t % 150 == 0 && alive {
            let c = &hunt.herd.chars[stego];
            let p = &mut hunt.player;
            // Keep up with it: 800 units behind, on the ground.
            p.x = c.pos[0] + 800.0;
            p.z = c.pos[2];
            p.y = area.land_height(p.x, p.z);
            let d = [
                c.pos[0] - p.x,
                c.pos[1] + 120.0 - (p.y + p.head_y),
                c.pos[2] - p.z,
            ];
            p.alpha = d[0].atan2(-d[2]);
            p.beta = (-d[1]).atan2(d[0].hypot(d[2]));
            input.fire = true;
        }
        let before = hunt.herd.chars.get(stego).map_or(0, |c| c.health);
        let t0 = std::time::Instant::now();
        hunt.tick(&area, &cars, &input, 10, &mut out);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if input.fire {
            let imp: Vec<_> = out
                .iter()
                .filter(|e| matches!(e, c2_core::game::GameEvent::Impact { .. }))
                .collect();
            println!(
                "fire at {}s: weapon {:?} shots {:?} {imp:?}",
                t / 100,
                hunt.weapons.pose(),
                hunt.weapons.shots_left
            );
        }
        out.clear();
        let after = hunt.herd.chars.get(stego).map_or(0, |c| c.health);
        if after < before {
            hits += 1;
            println!(
                "-- hit {hits} at {}s: health {after}, tick {ms:.3} ms",
                t / 100
            );
        }
        worst = worst.max(ms);
        sum += ms;
        n += 1;
        if (t + 1) % 500 == 0 {
            let c = hunt.herd.chars.get(stego);
            // Subnormal floats in the state: free on PC, but each operation
            // on one can trap to software on the 3DS's VFP.
            let dump = format!("{:?} {:?} {:?}", hunt.herd.chars, hunt.player, hunt.weapons);
            let subnormal = dump
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ".-+".contains(ch)))
                .filter(|t| t.parse::<f32>().is_ok_and(f32::is_subnormal))
                .count();
            println!(
                "t={:>3}s worst {:.3} ms mean {:.4} ms  subnormal {subnormal}  chars {} blood {} bursts {} ship {} stego state {:?}",
                (t + 1) / 100,
                worst,
                sum / f64::from(n),
                hunt.herd.chars.len(),
                hunt.effects.blood.len(),
                hunt.effects.bursts.len(),
                hunt.ship.state,
                c.map(|c| (c.state, c.health, c.afraid_time)),
            );
            (worst, sum, n) = (0.0, 0.0, 0);
        }
    }
    Ok(())
}

/// `name` in `dir`, ignoring case.
fn find_ci(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
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
    if args.first().map(String::as_str) == Some("mee") {
        let usage = "usage: c2-parity mee <GAME_DIR> [AREA] [TIME 0-2] [PICKED_SLOTS_MASK]";
        let dir = args.get(1).context(usage)?;
        let num = |i: usize, d: u32| -> Result<u32> {
            Ok(args.get(i).map(|s| s.parse()).transpose()?.unwrap_or(d))
        };
        return mee_report(
            Path::new(dir),
            num(2, 1)?,
            num(3, 1)? as u8,
            u128::from(num(4, 1)?),
        );
    }
    if args.first().map(String::as_str) == Some("mee-sim") {
        let usage = "usage: c2-parity mee-sim <GAME_DIR> [AREA] [seconds]";
        let dir = args.get(1).context(usage)?;
        let num = |i: usize, d: u32| -> Result<u32> {
            Ok(args.get(i).map(|s| s.parse()).transpose()?.unwrap_or(d))
        };
        return mee_sim(Path::new(dir), num(2, 1)?, num(3, 60)?);
    }
    if args.first().map(String::as_str) == Some("tick-bench") {
        let dir = args
            .get(1)
            .context("usage: c2-parity tick-bench <GAME_DIR> [AREA]")?;
        let a = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1);
        return tick_bench(Path::new(dir), a);
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
