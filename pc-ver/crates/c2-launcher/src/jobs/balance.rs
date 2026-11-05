//! Read the selected game through the shared launcher worker.
use std::path::PathBuf;

use c2_mods::balance::Balance;
use c2_mods::standalone::{Ambients, Skip};

/// The game (base, overlays, add-ons) combined in memory, and its values.
pub(crate) fn read(bases: Vec<PathBuf>, mods: Vec<PathBuf>) -> Result<super::Completion, String> {
    Ok(super::Completion::Balance(Box::new(values(&bases, &mods)?)))
}

fn values(bases: &[PathBuf], mods: &[PathBuf]) -> Result<Balance, String> {
    let v = c2_mods::vanilla(bases).map_err(|e| e.to_string())?;
    let sources = mods
        .iter()
        .map(|m| c2_mods::load_mod(&v, m).map_err(|e| format!("{}: {e}", m.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let pack = c2_mods::build(bases, sources).map_err(|e| e.to_string())?;
    c2_mods::balance::read(&pack.vfs).map_err(|e| e.to_string())
}

/// An instance's game as the recipe editor knows it (entries whose folder
/// isn't known are left out).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Instance {
    /// The game, then its overlays.
    pub bases: Vec<PathBuf>,
    /// Standalone games and what of each is left out.
    pub standalones: Vec<(PathBuf, Skip)>,
    /// Add-ons in load order.
    pub mods: Vec<PathBuf>,
    pub random_ambients: bool,
    /// Where the standalones are combined to be read (a folder this
    /// launcher owns: replaced on each read).
    pub scratch: PathBuf,
}

/// The instance's values: its standalones combined (as Play does, with
/// their parts left out), the add-ons over that.
pub(crate) fn read_instance(i: Instance) -> Result<super::Completion, String> {
    let values = if i.standalones.is_empty() {
        values(&i.bases, &i.mods)?
    } else {
        use c2_mods::standalone::{add_ons, build, Options};
        let (games, skip): (Vec<PathBuf>, Vec<Skip>) = i.standalones.into_iter().unzip();
        let o = Options {
            ambients: if i.random_ambients {
                Ambients::Random
            } else {
                Ambients::Tied
            },
            skip,
        };
        let built = build(&i.bases, &games, &i.scratch, &o).map_err(|e| e.to_string())?;
        let pack = add_ons(&i.scratch, &i.mods, &built, &o).map_err(|e| e.to_string())?;
        c2_mods::balance::read(&pack.vfs).map_err(|e| e.to_string())?
    };
    Ok(super::Completion::RecipeBalance(Box::new(values)))
}
