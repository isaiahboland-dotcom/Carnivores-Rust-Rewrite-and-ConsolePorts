//! `c2-pc mod ...`: detecting and combining add-on mods (`c2-mods`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use c2_mods::{build, load_mod, materialize, vanilla};

/// The base game folder (`--base`, or `play`'s game folder) and the
/// `--overlay` folders copied over it.
pub fn bases(base: Option<&str>, overlays: &[&str]) -> Result<Vec<PathBuf>> {
    let base = base.context("--base GAME_DIR is required")?;
    Ok(std::iter::once(base)
        .chain(overlays.iter().copied())
        .map(PathBuf::from)
        .collect())
}

/// The base + `mods`, combined in memory.
pub fn combine(bases: &[PathBuf], mods: &[PathBuf]) -> Result<c2_mods::Pack> {
    let v = vanilla(bases)?;
    let sources = mods
        .iter()
        .map(|m| load_mod(&v, m).with_context(|| format!("reading mod {}", m.display())))
        .collect::<Result<Vec<_>>>()?;
    Ok(build(bases, sources)?)
}

/// Puts a balance file's prices and scores over `pack` (a layer of
/// rewritten `_RES.TXT` / `_MENU.TXT` / `.c2map` files).
fn balance(pack: &mut c2_mods::Pack, file: Option<&Path>) -> Result<()> {
    let Some(file) = file else { return Ok(()) };
    let b = c2_mods::balance::Balance::load(file)?;
    let layer = c2_mods::balance::apply(&pack.vfs, &b)
        .with_context(|| format!("applying {}", file.display()))?;
    pack.vfs.push("balance", Box::new(layer));
    pack.notes
        .push(format!("balance file {} applied", file.display()));
    Ok(())
}

/// `c2-pc balance`: the combined game's prices and scores as a balance
/// file (every value), for the launcher's editor.
pub fn print_balance(bases: &[PathBuf], mods: &[PathBuf]) -> Result<()> {
    let pack = combine(bases, mods)?;
    print!("{}", c2_mods::balance::read(&pack.vfs)?.to_toml());
    Ok(())
}

/// Builds the base + `mods` (and a balance file) into `out` and prints a
/// summary.
pub fn build_pack(
    bases: &[PathBuf],
    mods: &[PathBuf],
    balance_file: Option<&Path>,
    out: &Path,
) -> Result<()> {
    let mut pack = combine(bases, mods)?;
    balance(&mut pack, balance_file)?;
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

/// Where `play --mod` keeps its combined games (named after the overlays
/// and mods).
pub fn pack_dir(overlays: &[PathBuf], mods: &[PathBuf]) -> PathBuf {
    let names: Vec<String> = overlays
        .iter()
        .chain(mods)
        .map(|m| {
            c2_mods::manifest::slug(
                &m.file_name()
                    .map_or_else(|| "mod".into(), |n| n.to_string_lossy().into_owned()),
            )
        })
        .collect();
    let name = if names.is_empty() {
        "base".to_string()
    } else {
        names.join("+")
    };
    crate::save::data_dir().join("packs").join(name)
}

/// `c2-pc mod detect|build ...`.
pub fn run(args: &[String], bases: Vec<PathBuf>, out: Option<&str>, write: bool) -> Result<()> {
    let Some((sub, rest)) = args.split_first() else {
        bail!("usage: c2-pc mod detect|build ...");
    };
    match sub.as_str() {
        "detect" => {
            let v = vanilla(&bases)?;
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
            build_pack(&bases, &mods, None, &out)
        }
        _ => bail!("unknown mod command {sub:?}"),
    }
}

/// Verifies a recipe; returns the located mods, printing a report.
fn verify(
    recipe_path: &Path,
    r: &c2_mods::recipe::Recipe,
    bases: &[PathBuf],
    mods_dir: &Path,
) -> Result<Vec<c2_mods::recipe::Located>> {
    use c2_mods::recipe::{check_base, load_base, locate};
    let v = vanilla(bases)?;
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
pub fn build_recipe(
    recipe_path: &Path,
    bases: &[PathBuf],
    mods_dir: &Path,
    balance_file: Option<&Path>,
    out: &Path,
) -> Result<()> {
    let r = c2_mods::recipe::Recipe::load(recipe_path)?;
    let found = verify(recipe_path, &r, bases, mods_dir)?;
    let v = vanilla(bases)?;
    let (sources, opts) = c2_mods::recipe::prepare(recipe_path, &r, &found, &v)?;
    let mut pack = c2_mods::build_with(bases, sources, &opts)?;
    balance(&mut pack, balance_file)?;
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
    pub bases: Vec<PathBuf>,
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
    let bases = &a.bases;
    let first = || rest.first().map(PathBuf::from).context("which recipe?");
    match sub.as_str() {
        "verify" => {
            let p = first()?;
            let r = c2_mods::recipe::Recipe::load(&p)?;
            let mods = PathBuf::from(a.mods.context("--mods DIR is required")?);
            let found = verify(&p, &r, bases, &mods)?;
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
            build_recipe(&first()?, bases, &mods, None, &out)
        }
        "new" => {
            let id = a.id.context("--id ID is required")?;
            let v = vanilla(bases)?;
            let sources = rest
                .iter()
                .map(|m| load_mod(&v, Path::new(m)).with_context(|| format!("reading mod {m}")))
                .collect::<Result<Vec<_>>>()?;
            let r = c2_mods::recipe::new_recipe(id, a.name.unwrap_or(id), "c2-vanilla", &sources)?;
            write_new(a.out, &r.to_toml())
        }
        "base" => {
            let v = vanilla(bases)?;
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
