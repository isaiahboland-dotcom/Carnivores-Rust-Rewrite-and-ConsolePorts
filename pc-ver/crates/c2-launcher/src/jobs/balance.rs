//! Read the selected game through the shared launcher worker.
use std::path::PathBuf;

use c2_mods::standalone::{Ambients, Skip};

/// The game over `bases` with `mods`; `skip`: the base game's own parts
/// left out.
fn plain(bases: &[PathBuf], mods: &[PathBuf], skip: &Skip) -> Result<c2_vfs::Vfs, String> {
    let v = c2_mods::vanilla(bases).map_err(|e| e.to_string())?;
    let sources = mods
        .iter()
        .map(|m| c2_mods::load_mod(&v, m).map_err(|e| format!("{}: {e}", m.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let mut pack = c2_mods::build(bases, sources).map_err(|e| e.to_string())?;
    c2_mods::standalone::leave_out::push(&mut pack.vfs, skip).map_err(|e| e.to_string())?;
    Ok(pack.vfs)
}

/// An instance's game as the recipe editor knows it (entries whose folder
/// isn't known are left out).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Instance {
    /// The game, then its overlays.
    pub bases: Vec<PathBuf>,
    /// The base game's own parts left out.
    pub base_skip: Skip,
    /// Standalone games and what of each is left out.
    pub standalones: Vec<(PathBuf, Skip)>,
    /// The recipe source ID of each of `standalones`.
    pub standalone_ids: Vec<String>,
    /// Per standalone: its huntables that variants take off the menu
    /// (`variants::unlisted`), as the build does.
    pub unlisted: Vec<Vec<c2_mods::standalone::Unlisted>>,
    /// Add-ons in load order.
    pub mods: Vec<PathBuf>,
    /// The recipe source ID of each add-on in `mods`.
    pub mod_ids: Vec<String>,
    pub random_ambients: bool,
    /// Where the standalones are combined to be read (a folder this
    /// launcher owns: replaced on each read).
    pub scratch: PathBuf,
}

/// The instance's game: its standalones combined (as Play does, with
/// their parts left out), the add-ons over that, the base's own parts left
/// out.
fn game(i: Instance) -> Result<c2_vfs::Vfs, String> {
    if i.standalones.is_empty() {
        return plain(&i.bases, &i.mods, &i.base_skip);
    }
    use c2_mods::standalone::{add_ons, build, Options};
    let (games, skip): (Vec<PathBuf>, Vec<Skip>) = i.standalones.into_iter().unzip();
    let o = Options {
        ambients: if i.random_ambients {
            Ambients::Random
        } else {
            Ambients::Tied
        },
        skip,
        unlisted: i.unlisted,
    };
    let built = build(&i.bases, &games, &i.scratch, &o).map_err(|e| e.to_string())?;
    let mut pack = add_ons(&i.scratch, &i.mods, &built, &o).map_err(|e| e.to_string())?;
    c2_mods::standalone::leave_out::push(&mut pack.vfs, &i.base_skip).map_err(|e| e.to_string())?;
    Ok(pack.vfs)
}

/// The instance's values (areas, animals and weapons with their prices).
pub(crate) fn read_instance(i: Instance) -> Result<super::Completion, String> {
    let values = c2_mods::balance::read(&game(i)?).map_err(|e| e.to_string())?;
    Ok(super::Completion::RecipeBalance(Box::new(values)))
}

/// The overview picture of the instance's area called `area` (as the
/// balance editor names it), for the map editor: RGBA8,
/// [`c2_formats::overview::SIZE`] a side.
pub(crate) fn read_overview(i: Instance, area: String) -> Result<super::Completion, String> {
    let vfs = game(i)?;
    let rgba = overview(&vfs, &area)?;
    Ok(super::Completion::AreaOverview(Box::new((area, rgba))))
}

fn overview(vfs: &c2_vfs::Vfs, area: &str) -> Result<Vec<u8>, String> {
    let maps = c2_mods::balance::area_maps(vfs).map_err(|e| e.to_string())?;
    let stem = maps
        .iter()
        .find(|(name, _)| name.trim().eq_ignore_ascii_case(area.trim()))
        .map(|(_, stem)| stem.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("the game has no area named {area:?}"))?;
    let read = |ext: &str| {
        let path = format!("HUNTDAT/AREAS/{stem}.{ext}");
        vfs.read(&path).map_err(|e| format!("{path}: {e}"))
    };
    let map = c2_formats::map::parse(&read("MAP")?).map_err(|e| format!("{stem}.MAP: {e}"))?;
    let rsc = c2_formats::rsc::parse(&read("RSC")?).map_err(|e| format!("{stem}.RSC: {e}"))?;
    Ok(c2_formats::overview::image_rgba8(&map, &rsc))
}

/// The instance's dinosaurs and maps for the Variant manager: the base
/// game's, each standalone's (as named in that game) and each add-on's.
pub(crate) fn read_catalog(i: Instance) -> Result<super::Completion, String> {
    let addons: Vec<(String, PathBuf)> = i.mod_ids.into_iter().zip(i.mods).collect();
    let standalones: Vec<(String, PathBuf, Skip)> = i
        .standalone_ids
        .into_iter()
        .zip(i.standalones)
        .map(|(id, (dir, skip))| (id, dir, skip))
        .collect();
    let catalog =
        c2_mods::variants::catalog(&i.bases, &standalones, &addons).map_err(|e| e.to_string())?;
    Ok(super::Completion::VariantCatalog(Box::new(catalog)))
}
