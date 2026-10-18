//! `c2-pc mod ...`: detecting and combining add-on mods (`c2-mods`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use c2_mods::{build, load_mod, materialize, vanilla};

/// Builds vanilla + `mods` into `out` and prints a summary.
pub fn build_pack(base: &Path, mods: &[PathBuf], out: &Path) -> Result<()> {
    let v = vanilla(base)?;
    let sources = mods
        .iter()
        .map(|m| load_mod(&v, m).with_context(|| format!("reading mod {}", m.display())))
        .collect::<Result<Vec<_>>>()?;
    let pack = build(base, sources)?;
    materialize(&pack, out).with_context(|| format!("writing {}", out.display()))?;
    let conflicts = pack.file_conflicts().len() + pack.merged.conflicts.len();
    tracing::info!(
        "built {} ({} mods, {} conflicts) — see {}",
        out.display(),
        pack.mods.len(),
        conflicts,
        out.join(c2_mods::pack::REPORT_FILE).display()
    );
    Ok(())
}

/// Where `play --mod` keeps its combined games.
pub fn pack_dir(mods: &[PathBuf]) -> PathBuf {
    let names: Vec<String> = mods
        .iter()
        .map(|m| {
            c2_mods::manifest::slug(
                &m.file_name()
                    .map_or_else(|| "mod".into(), |n| n.to_string_lossy().into_owned()),
            )
        })
        .collect();
    crate::save::data_dir().join("packs").join(names.join("+"))
}

/// `c2-pc mod detect|build ...`.
pub fn run(args: &[String], base: Option<&str>, out: Option<&str>, write: bool) -> Result<()> {
    let Some((sub, rest)) = args.split_first() else {
        bail!("usage: c2-pc mod detect|build ...");
    };
    let base = PathBuf::from(base.context("--base GAME_DIR is required")?);
    match sub.as_str() {
        "detect" => {
            let v = vanilla(&base)?;
            for m in rest {
                let dir = PathBuf::from(m);
                let src = load_mod(&v, &dir)?;
                let text = src.manifest.to_toml();
                eprintln!(
                    "{}: {} files changed, {} identical to vanilla, {} _RES.TXT changes{}",
                    dir.display(),
                    src.files.len(),
                    src.identical,
                    src.res_changes.len(),
                    if src.inferred { "" } else { " (from mod.toml)" }
                );
                if write {
                    let p = dir.join("mod.toml");
                    if p.exists() {
                        bail!("{} exists; not overwriting it", p.display());
                    }
                    std::fs::write(&p, &text)?;
                    eprintln!("wrote {}", p.display());
                } else {
                    println!("{text}");
                }
            }
            Ok(())
        }
        "build" => {
            let out = PathBuf::from(out.context("--out DIR is required")?);
            let mods: Vec<PathBuf> = rest.iter().map(PathBuf::from).collect();
            build_pack(&base, &mods, &out)
        }
        _ => bail!("unknown mod command {sub:?}"),
    }
}
