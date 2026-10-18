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

/// Verifies a recipe; returns the located mods, printing a report.
fn verify(
    recipe_path: &Path,
    r: &c2_mods::recipe::Recipe,
    base: &Path,
    mods_dir: &Path,
) -> Result<Vec<c2_mods::recipe::Located>> {
    use c2_mods::recipe::{check_base, load_base, locate};
    let v = vanilla(base)?;
    println!(
        "recipe {} — {} {}",
        r.recipe.id, r.recipe.name, r.recipe.version
    );
    match load_base(recipe_path, r) {
        Some(b) => {
            let (missing, bad) = check_base(&b, &v);
            if missing.is_empty() && bad.is_empty() {
                println!("  base '{}': ok ({} files)", r.recipe.base, b.files.len());
            } else {
                println!(
                    "  base '{}': {} missing, {} different (a different game version or edited files)",
                    r.recipe.base,
                    missing.len(),
                    bad.len()
                );
                for p in missing.iter().chain(&bad).take(10) {
                    println!("    {p}");
                }
            }
        }
        None if r.recipe.base.is_empty() => {}
        None => println!(
            "  base '{}': no hash list found, not checked",
            r.recipe.base
        ),
    }
    let found = locate(r, mods_dir);
    for l in &found {
        let status = if l.complete() {
            "ok"
        } else {
            "NEEDS ATTENTION"
        };
        let at = l
            .root
            .as_ref()
            .map_or_else(|| "not found".to_string(), |p| p.display().to_string());
        println!("  mod {}: {status} ({at})", l.id);
        for p in &l.missing {
            println!("    missing {p}");
        }
        for (p, _, _) in &l.mismatched {
            println!("    different {p}");
        }
        if !l.complete() && !l.url.is_empty() {
            println!("    get it from {}", l.url);
        }
    }
    Ok(found)
}

/// Verifies and builds a recipe into `out`.
pub fn build_recipe(recipe_path: &Path, base: &Path, mods_dir: &Path, out: &Path) -> Result<()> {
    let r = c2_mods::recipe::Recipe::load(recipe_path)?;
    let found = verify(recipe_path, &r, base, mods_dir)?;
    let v = vanilla(base)?;
    let (sources, opts) = c2_mods::recipe::prepare(recipe_path, &r, &found, &v)?;
    let pack = c2_mods::build_with(base, sources, &opts)?;
    materialize(&pack, out).with_context(|| format!("writing {}", out.display()))?;
    tracing::info!(
        "built {} — see {}",
        out.display(),
        out.join(c2_mods::pack::REPORT_FILE).display()
    );
    Ok(())
}

/// Where `play --recipe` keeps its combined game.
pub fn recipe_pack_dir(recipe_path: &Path) -> PathBuf {
    let id = c2_mods::recipe::Recipe::load(recipe_path)
        .map(|r| r.recipe.id)
        .unwrap_or_default();
    let id = if id.is_empty() {
        "recipe".into()
    } else {
        c2_mods::manifest::slug(&id)
    };
    crate::save::data_dir()
        .join("packs")
        .join(format!("recipe-{id}"))
}

pub struct RecipeArgs<'a> {
    pub base: Option<&'a str>,
    pub mods: Option<&'a str>,
    pub out: Option<&'a str>,
    pub id: Option<&'a str>,
    pub name: Option<&'a str>,
}

/// `c2-pc recipe verify|build|new|base ...`.
pub fn run_recipe(args: &[String], a: &RecipeArgs<'_>) -> Result<()> {
    let Some((sub, rest)) = args.split_first() else {
        bail!("usage: c2-pc recipe verify|build|new|base ...");
    };
    let base = PathBuf::from(a.base.context("--base GAME_DIR is required")?);
    let first = || rest.first().map(PathBuf::from).context("which recipe?");
    match sub.as_str() {
        "verify" => {
            let p = first()?;
            let r = c2_mods::recipe::Recipe::load(&p)?;
            let mods = PathBuf::from(a.mods.context("--mods DIR is required")?);
            let found = verify(&p, &r, &base, &mods)?;
            if found.iter().all(|l| l.complete()) {
                println!("everything matches; `recipe build` will work");
                Ok(())
            } else {
                bail!("some mods are missing or different")
            }
        }
        "build" => {
            let mods = PathBuf::from(a.mods.context("--mods DIR is required")?);
            let out = PathBuf::from(a.out.context("--out DIR is required")?);
            build_recipe(&first()?, &base, &mods, &out)
        }
        "new" => {
            let id = a.id.context("--id ID is required")?;
            let v = vanilla(&base)?;
            let sources = rest
                .iter()
                .map(|m| load_mod(&v, Path::new(m)).with_context(|| format!("reading mod {m}")))
                .collect::<Result<Vec<_>>>()?;
            let r = c2_mods::recipe::new_recipe(id, a.name.unwrap_or(id), "c2-vanilla", &sources)?;
            write_new(a.out, &r.to_toml())
        }
        "base" => {
            let v = vanilla(&base)?;
            let b = c2_mods::recipe::base_profile(
                a.id.unwrap_or("c2-vanilla"),
                a.name
                    .unwrap_or("SHA-256 of every HUNTDAT file of the base game"),
                &v,
            )?;
            write_new(a.out, &toml::to_string_pretty(&b).unwrap_or_default())
        }
        _ => bail!("unknown recipe command {sub:?}"),
    }
}

/// Prints `text`, or writes it to a file that must not exist yet.
fn write_new(out: Option<&str>, text: &str) -> Result<()> {
    match out {
        None => {
            print!("{text}");
            Ok(())
        }
        Some(p) => {
            let p = Path::new(p);
            if p.exists() {
                bail!("{} exists; not overwriting it", p.display());
            }
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(p, text)?;
            eprintln!("wrote {}", p.display());
            Ok(())
        }
    }
}
