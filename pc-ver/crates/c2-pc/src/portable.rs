//! No-argument launch of a packaged game, independent of Explorer's working directory.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn arguments(explicit: Vec<String>, exe: &Path) -> Result<(Vec<String>, Option<PathBuf>)> {
    if !explicit.is_empty() {
        return Ok((explicit, None));
    }
    let Some(root) = exe
        .parent()
        .filter(|p| p.join("c2-portable.toml").is_file())
    else {
        return Ok((explicit, None));
    };
    let config: toml::Table =
        toml::from_str(&std::fs::read_to_string(root.join("c2-portable.toml"))?)?;
    let extra = config
        .get("args")
        .and_then(toml::Value::as_array)
        .context("Portable config must contain an args array")?;
    let mut args = vec!["play".into(), "game".into()];
    for value in extra {
        args.push(
            value
                .as_str()
                .context("Portable args must be strings")?
                .into(),
        );
    }
    Ok((args, Some(root.to_path_buf())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_bootstrap_preserves_explicit_cli_and_handles_invalid_config() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../(testing)/portable-bootstrap");
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("c2-pc.exe");
        std::fs::write(
            root.join("c2-portable.toml"),
            "args = ['--balance', 'file with spaces.toml']",
        )
        .unwrap();
        let (args, cwd) = arguments(vec![], &exe).unwrap();
        assert_eq!(args, ["play", "game", "--balance", "file with spaces.toml"]);
        assert_eq!(cwd.as_deref(), Some(root.as_path()));
        std::fs::write(root.join("c2-portable.toml"), "args = [123]").unwrap();
        assert!(arguments(vec![], &exe).is_err());
        assert_eq!(
            arguments(vec!["model".into()], &exe).unwrap(),
            (vec!["model".into()], None)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
