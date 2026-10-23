//! Add-on mods: manifests (`mod.toml`), auto-detection against vanilla,
//! load order, file and `_RES.TXT` merging, the merge report, combined game
//! folders, and recipes (mod combinations shared as URLs and file hashes).

pub mod balance;
pub mod detect;
pub mod hunter;
pub mod ice_age;
pub mod loose;
pub mod manifest;
pub mod mobile;
pub mod pack;
pub mod patch;
pub mod recipe;
pub mod res_merge;
pub mod resolve;

use std::path::PathBuf;

pub use manifest::Manifest;
pub use pack::{
    build, build_with, expand_addons, load_mod, materialize, vanilla, BuildOptions, ModSource, Pack,
};

#[derive(Debug, thiserror::Error)]
pub enum ModError {
    #[error("{0}: no HUNTDAT folder found")]
    NoHuntdat(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Vfs(#[from] c2_vfs::VfsError),
    #[error(transparent)]
    Format(#[from] c2_formats::FormatError),
    #[error("{path}: {message}")]
    Manifest { path: PathBuf, message: String },
    #[error("{0}: listed in mod.toml but missing")]
    MissingFile(PathBuf),
    #[error(transparent)]
    Resolve(#[from] resolve::ResolveError),
    #[error("recipe: {0}")]
    Recipe(String),
    #[error("{0} is not empty and was not built by c2rs; refusing to overwrite it")]
    NotAPack(PathBuf),
}
