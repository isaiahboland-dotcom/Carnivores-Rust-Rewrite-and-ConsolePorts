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
mod mobile_anims;
pub mod mobile_base;
pub mod pack;
pub mod patch;
pub mod predation;
pub mod recipe;
pub mod res_merge;
pub mod resolve;
pub mod rules;
pub mod standalone;
mod triassic;

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
    #[error("standalone games: {0}")]
    Standalone(String),
}

/// An I/O error on `path` as [`ModError::Io`], for `map_err`.
fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A path from mod metadata must stay inside the generated output directory.
fn output_path(root: &std::path::Path, relative: &str) -> Result<PathBuf, ModError> {
    if !c2_vfs::is_safe_relative_path(relative) {
        return Err(ModError::Manifest {
            path: root.to_path_buf(),
            message: format!("unsafe generated path: {relative}"),
        });
    }
    Ok(root.join(relative.replace('\\', "/")))
}

fn output_child(root: &std::path::Path, name: &str) -> Result<PathBuf, ModError> {
    if name == "." || name.contains(['/', '\\']) {
        return Err(ModError::Manifest {
            path: root.to_path_buf(),
            message: format!("unsafe import folder name: {name}"),
        });
    }
    output_path(root, name)
}

#[cfg(test)]
mod output_tests {
    use super::*;
    #[test]
    fn metadata_cannot_escape_an_import_directory() {
        let root = std::path::Path::new("output");
        for name in [
            "..",
            "../old-install",
            "/tmp/install",
            "..\\old-install",
            "C:install",
            "",
            "NUL",
            ".",
        ] {
            assert!(output_child(root, name).is_err(), "{name}");
        }
        assert_eq!(
            output_child(root, "Ice Age animal").ok(),
            Some(root.join("Ice Age animal"))
        );
        assert!(output_path(root, "WEAPONS/../../outside").is_err());
        assert_eq!(
            output_path(root, "WEAPONS\\X.CAR").ok(),
            Some(root.join("WEAPONS/X.CAR"))
        );
    }
}
