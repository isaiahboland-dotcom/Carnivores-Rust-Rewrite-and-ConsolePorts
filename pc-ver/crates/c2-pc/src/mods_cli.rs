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

/// Whether `game` is Carnivores: Ice Age (played through a combined game
/// folder: its rules rewritten in the dialect, `c2_mods::ice_age`).
pub fn is_ice_age(game: &Path) -> bool {
    c2_mods::ice_age::is_ice_age_dir(game)
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

/// Standalone games combined beside the base (README M16) into `out`, then
/// add-ons and a balance file over that.
pub fn build_combined(
    bases: &[PathBuf],
    standalones: &[PathBuf],
    mods: &[PathBuf],
    balance_file: Option<&Path>,
    ambients: c2_mods::standalone::Ambients,
    out: &Path,
) -> Result<()> {
    use c2_mods::standalone::{add_ons, build, Options};
    let o = Options { ambients };
    let games = if mods.is_empty() && balance_file.is_none() {
        out.to_path_buf()
    } else {
        PathBuf::from(format!("{}.games", out.display()))
    };
    let built = build(bases, standalones, &games, &o)?;
    print!("{}", built.report);
    if games != out {
        let mut pack = add_ons(&games, mods, &built, &o)?;
        balance(&mut pack, balance_file)?;
        materialize(&pack, out).with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(())
}

/// Where `play --standalone` keeps its combined game (named after the
/// standalones; add-ons, if any, after them).
pub fn combined_dir(standalones: &[PathBuf], mods: &[PathBuf]) -> PathBuf {
    let p = pack_dir(standalones, mods);
    let name = p
        .file_name()
        .map_or_else(|| "base".into(), |n| n.to_string_lossy().into_owned());
    p.with_file_name(format!("combined-{name}"))
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
    let mut name = if names.is_empty() {
        "base".to_string()
    } else {
        names.join("+")
    };
    // File names stop at 255 bytes: a long list keeps its start and a hash
    // of the whole (the full list is in its MERGE_REPORT.md).
    if name.len() > 120 {
        let hash = c2_mods::recipe::sha256(name.as_bytes());
        let hash = hash.trim_start_matches("sha256:");
        let start: String = name.chars().take(80).collect();
        name = format!(
            "{start}+{}-mods-{}",
            names.len(),
            &hash[..12.min(hash.len())]
        );
    }
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
            let mods: Vec<PathBuf> = rest
                .iter()
                .flat_map(|m| c2_mods::expand_addons(Path::new(m)))
                .collect();
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
) -> Result<Verified> {
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
    let overlays = c2_mods::recipe::locate_games(&r.overlays, mods_dir);
    let games = c2_mods::recipe::locate_games(&r.standalones, mods_dir);
    let kinds = overlays
        .iter()
        .map(|l| ("overlay", l))
        .chain(games.iter().map(|l| ("standalone", l)))
        .chain(found.iter().map(|l| ("mod", l)));
    for (kind, l) in kinds {
        let status = if l.complete() {
            "ok"
        } else {
            "NEEDS ATTENTION"
        };
        let at = l
            .root
            .as_ref()
            .map_or_else(|| "not found".to_string(), |p| p.display().to_string());
        println!("  {kind} {}: {status} ({at})", l.id);
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
    Ok(Verified {
        mods: found,
        overlays,
        games,
    })
}

/// What [`verify`] found: the recipe's mods, overlays and standalones.
pub struct Verified {
    pub mods: Vec<c2_mods::recipe::Located>,
    pub overlays: Vec<c2_mods::recipe::Located>,
    pub games: Vec<c2_mods::recipe::Located>,
}

impl Verified {
    fn complete(&self) -> bool {
        self.mods
            .iter()
            .chain(&self.overlays)
            .chain(&self.games)
            .all(|l| l.complete())
    }

    /// The game folders of `list`, or an error naming one to download.
    fn roots(list: &[c2_mods::recipe::Located]) -> Result<Vec<PathBuf>> {
        list.iter()
            .map(|l| match (&l.root, l.complete()) {
                (Some(r), true) => Ok(r.clone()),
                _ => bail!(
                    "'{}' is missing or doesn't match (get it from {})",
                    l.id,
                    if l.url.is_empty() { "?" } else { &l.url }
                ),
            })
            .collect()
    }
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
    // The recipe's own overlays, when it lists them, over the given game.
    let bases: Vec<PathBuf> = if r.overlays.is_empty() {
        bases.to_vec()
    } else {
        std::iter::once(bases.first().context("which game?")?.clone())
            .chain(Verified::roots(&found.overlays)?)
            .collect()
    };
    let bases = &bases[..];
    let mut pack = if r.standalones.is_empty() {
        let v = vanilla(bases)?;
        let (sources, opts) = c2_mods::recipe::prepare(recipe_path, &r, &found.mods, &v)?;
        c2_mods::build_with(bases, sources, &opts)?
    } else {
        // Standalones combined beside the base; the add-ons over that.
        use c2_mods::standalone::{add_on_sources, build, Ambients, Options};
        let o = Options {
            ambients: match r.combine.as_ref().map(|c| c.ambients.as_str()) {
                Some("random") => Ambients::Random,
                _ => Ambients::Tied,
            },
        };
        let games = Verified::roots(&found.games)?;
        let games_dir = PathBuf::from(format!("{}.games", out.display()));
        let built = build(bases, &games, &games_dir, &o)?;
        print!("{}", built.report);
        let v = vanilla(std::slice::from_ref(&games_dir))?;
        let (sources, opts) = c2_mods::recipe::prepare(recipe_path, &r, &found.mods, &v)?;
        add_on_sources(&games_dir, sources, &built, &o, &opts)?
    };
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
    /// `recipe new --standalone DIR`: standalone games to pin.
    pub standalones: Vec<PathBuf>,
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
            if found.complete() {
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
            let mut r =
                c2_mods::recipe::new_recipe(id, a.name.unwrap_or(id), "c2-vanilla", &sources)?;
            // Overlays and standalones, pinned whole (credits and URLs to
            // fill in).
            for o in bases.iter().skip(1) {
                r.overlays.push(c2_mods::recipe::pin_game(o)?);
            }
            for g in &a.standalones {
                r.standalones.push(c2_mods::recipe::pin_game(g)?);
            }
            if !r.standalones.is_empty() {
                r.combine = Some(c2_mods::recipe::Combine {
                    ambients: "tied".into(),
                });
            }
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
