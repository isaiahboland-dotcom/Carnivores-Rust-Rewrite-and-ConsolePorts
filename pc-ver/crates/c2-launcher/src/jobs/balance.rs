//! Read the selected game through the shared launcher worker.
use std::path::PathBuf;
/// The game (base, overlays, add-ons) combined in memory, and its values.
pub(crate) fn read(bases: Vec<PathBuf>, mods: Vec<PathBuf>) -> Result<super::Completion, String> {
    let v = c2_mods::vanilla(&bases).map_err(|e| e.to_string())?;
    let sources = mods
        .iter()
        .map(|m| c2_mods::load_mod(&v, m).map_err(|e| format!("{}: {e}", m.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let pack = c2_mods::build(&bases, sources).map_err(|e| e.to_string())?;
    let values = c2_mods::balance::read(&pack.vfs).map_err(|e| e.to_string())?;
    Ok(super::Completion::Balance(Box::new(values)))
}
