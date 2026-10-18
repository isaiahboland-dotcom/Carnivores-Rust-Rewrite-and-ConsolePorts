//! `c2-parity`: checks our parsers against real game data.
//!
//! `c2-parity scan <dir>...` walks each directory, parses every file with a
//! known extension, and checks that writing it back reproduces the input
//! byte for byte. Exit status is non-zero if anything fails.

use std::path::{Path, PathBuf};

mod strstr;

use anyhow::{bail, Context, Result};
use c2_formats::{car, map, res_txt::ResTxt, rsc, tga, three_df, wav};

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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
