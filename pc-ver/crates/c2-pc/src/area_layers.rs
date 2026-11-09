//! `c2-pc area-layers`: joins copies of an area, each made in an ordinary
//! map editor with up to 254 kinds of object, into the side files that
//! lift that limit (`AREAn.OBX`, `AREAn.RSX`; docs/EXPANSION.md). The
//! base `.MAP` and `.RSC` are only read.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use c2_formats::{map, obx, rsc};

pub const USAGE: &str =
    "c2-pc area-layers <BASE.MAP> --layer <LAYER.MAP>... --out <DIR>   (each .MAP with its .RSC beside it)";

/// The `.RSC` beside a `.MAP`, in the same case style.
fn rsc_beside(map_path: &Path) -> Result<PathBuf> {
    let dir = map_path.parent().unwrap_or(Path::new("."));
    let stem = map_path
        .file_stem()
        .context("a .MAP path")?
        .to_string_lossy()
        .into_owned();
    std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_stem().is_some_and(|s| s.to_string_lossy() == stem)
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rsc"))
        })
        .with_context(|| format!("no {stem}.RSC beside {}", map_path.display()))
}

fn read_area(map_path: &Path) -> Result<(map::Map, rsc::Rsc)> {
    let m = map::parse(&std::fs::read(map_path)?)
        .with_context(|| format!("reading {}", map_path.display()))?;
    let rsc_path = rsc_beside(map_path)?;
    let r = rsc::parse(&std::fs::read(&rsc_path)?)
        .with_context(|| format!("reading {}", rsc_path.display()))?;
    Ok((m, r))
}

pub fn run(base: &str, layers: &[&str], out: &str) -> Result<()> {
    if layers.is_empty() {
        bail!("usage: {USAGE}");
    }
    let base_path = Path::new(base);
    let (bm, br) = read_area(base_path)?;
    let mut read = Vec::new();
    for l in layers {
        let (m, r) = read_area(Path::new(l))?;
        if m.height != bm.height {
            println!("note: {l}: terrain differs from the base; only its objects are used");
        }
        read.push((m, r));
    }
    let views: Vec<(&[u16], &[u8], &[rsc::Object])> = read
        .iter()
        .map(|(m, r)| {
            (
                m.objects.as_slice(),
                m.object_height.as_slice(),
                r.objects.as_slice(),
            )
        })
        .collect();
    let (grid, extra, report) =
        obx::join_layers(&bm.objects, &bm.object_height, &br.objects, &views)
            .context("more than 65533 kinds of object")?;
    let stem = base_path
        .file_stem()
        .context("a .MAP path")?
        .to_string_lossy()
        .into_owned();
    let out = Path::new(out);
    std::fs::create_dir_all(out)?;
    let obx_path = out.join(format!("{stem}.OBX"));
    let rsx_path = out.join(format!("{stem}.RSX"));
    std::fs::write(&obx_path, obx::write_obx(&grid))?;
    std::fs::write(&rsx_path, obx::write_rsx(&extra))?;
    println!(
        "{} kinds of object ({} in the .RSC, {} added)",
        report.models,
        br.objects.len(),
        extra.len()
    );
    for (i, l) in layers.iter().enumerate() {
        println!(
            "{l}: {} models shared with earlier ones, {} cells over another object",
            report.shared[i], report.conflicts[i]
        );
    }
    if report.landings_skipped > 0 {
        println!(
            "{} landing markers in layers left out (the base's are kept)",
            report.landings_skipped
        );
    }
    println!("{}\n{}", obx_path.display(), rsx_path.display());
    Ok(())
}
