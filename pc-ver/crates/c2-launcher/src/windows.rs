//! Materialize a saved instance for Windows, without links to the build machine.
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MARKER: &str = ".c2rs-windows-package";

pub fn validate_exe(path: &Path) -> Result<(), String> {
    let check = || -> io::Result<bool> {
        let mut f = fs::File::open(path)?;
        let mut header = [0; 64];
        f.read_exact(&mut header)?;
        if &header[..2] != b"MZ" {
            return Ok(false);
        }
        let offset = u32::from_le_bytes([header[60], header[61], header[62], header[63]]);
        f.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut pe = [0; 6];
        f.read_exact(&mut pe)?;
        Ok(&pe[..4] == b"PE\0\0" && pe[4..] == [0x64, 0x86])
    };
    if check().unwrap_or(false) {
        Ok(())
    } else {
        Err(format!(
            "{} is not a Windows x64 executable.",
            path.display()
        ))
    }
}

/// Copy files rather than preserving the merge output's source symlinks.
/// Directory links are rejected to avoid cycles; merged packs only link files.
fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else if fs::metadata(&from)?.is_file() {
            fs::copy(from, to)?;
        } else {
            return Err(io::Error::other(
                "Unsupported directory link or special file in package",
            ));
        }
    }
    Ok(())
}

pub fn package(game: &Path, exe: &Path, out: &Path, mut args: Vec<String>) -> io::Result<()> {
    validate_exe(exe).map_err(io::Error::other)?;
    let parent = out
        .parent()
        .ok_or_else(|| io::Error::other("Missing output parent"))?;
    fs::create_dir_all(parent)?;
    let destination_parent = fs::canonicalize(parent)?;
    let mut inputs = vec![game.to_path_buf()];
    for pair in args.windows(2) {
        if pair[0] == "--menu-art" {
            inputs.push(pair[1].clone().into());
        }
    }
    for input in inputs {
        if destination_parent.starts_with(fs::canonicalize(input)?) {
            return Err(io::Error::other(
                "Choose an output folder outside the source game folders",
            ));
        }
    }
    // Only replace our own output, never a user game folder or symlink.
    if out.symlink_metadata().is_ok() && (out.is_symlink() || !out.join(MARKER).is_file()) {
        return Err(io::Error::other(
            "Output exists and is not a generated Windows package",
        ));
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let stage = parent.join(format!(".c2rs-windows-{nonce}"));
    let backup = parent.join(format!(".c2rs-windows-old-{nonce}"));
    fs::create_dir(&stage)?;
    let result = (|| {
        copy_tree(game, &stage.join("game"))?;
        fs::copy(exe, stage.join("c2-pc.exe"))?;
        fs::write(
            stage.join("LICENSE-engine.txt"),
            include_str!("../../../../LICENSE"),
        )?;
        // Preserve runtime overrides with portable paths; TOML handles spaces and Unicode.
        let mut i = 0;
        while i < args.len() {
            let relative = match args[i].as_str() {
                "--balance" => Some("balance.toml"),
                "--mobile-survival" => Some("mobile-survival.txt"),
                "--menu-art" => Some("menu-art"),
                _ => None,
            };
            if let Some(relative) = relative {
                let source = args
                    .get(i + 1)
                    .ok_or_else(|| io::Error::other("Missing option value"))?;
                if args[i] == "--menu-art" {
                    let source = Path::new(source);
                    if let Some(menu) = c2_mods::pack::find_path(source, "HUNTDAT/MENU")
                        .or_else(|| c2_mods::pack::find_path(source, "MENU"))
                    {
                        copy_tree(&menu, &stage.join(relative).join("HUNTDAT/MENU"))?;
                    } else {
                        fs::create_dir_all(stage.join(relative))?;
                    }
                } else {
                    fs::copy(source, stage.join(relative))?;
                }
                args[i + 1] = relative.into();
                i += 1;
            }
            i += 1;
        }
        let mut config = toml::Table::new();
        config.insert(
            "args".into(),
            toml::Value::Array(args.into_iter().map(toml::Value::String).collect()),
        );
        fs::write(
            stage.join("c2-portable.toml"),
            toml::to_string(&config).map_err(io::Error::other)?,
        )?;
        fs::write(
            stage.join(MARKER),
            "Generated by c2rs. Rebuild replaces this folder.\n",
        )?;
        fs::write(stage.join("README.txt"), "Double-click c2-pc.exe. Keep the entire folder together. Saves and settings are stored in your Windows user profile (APPDATA/c2rs). This package contains your own game/mod data; engine downloads do not include it.\n")?;
        let had_old = out.exists();
        if had_old {
            fs::rename(out, &backup)?;
        }
        if let Err(e) = fs::rename(&stage, out) {
            if had_old {
                fs::rename(&backup, out)?;
            }
            return Err(e);
        }
        if had_old {
            fs::remove_dir_all(&backup)?;
        }
        Ok(())
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_copy_rebuild_and_protect_unrelated_output() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../(testing)/windows-package");
        let _ = fs::remove_dir_all(&root);
        let game = root.join("source");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("synthetic.txt"), "first").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(game.join("synthetic.txt"), game.join("linked.txt")).unwrap();
        let exe = root.join("engine.exe");
        let mut pe = vec![0; 70];
        pe[..2].copy_from_slice(b"MZ");
        pe[60..64].copy_from_slice(&64u32.to_le_bytes());
        pe[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        fs::write(&exe, pe).unwrap();
        let out = root.join("output with spaces");
        package(&game, &exe, &out, vec![]).unwrap();
        assert!(!out.join("game/linked.txt").is_symlink());
        assert!(package(&game, &exe, &game.join("nested/output"), vec![]).is_err());
        // Remove the empty parent created by the rejected nested output.
        fs::remove_dir(game.join("nested")).unwrap();
        fs::write(game.join("synthetic.txt"), "second").unwrap();
        assert_eq!(
            fs::read_to_string(out.join("game/synthetic.txt")).unwrap(),
            "first"
        );
        assert!(package(
            &game,
            &exe,
            &out,
            vec![
                "--balance".into(),
                root.join("missing.toml").display().to_string()
            ]
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(out.join("game/synthetic.txt")).unwrap(),
            "first"
        );
        fs::write(out.join("stale.txt"), "old").unwrap();
        package(&game, &exe, &out, vec![]).unwrap();
        assert!(!out.join("stale.txt").exists());
        assert_eq!(
            fs::read_to_string(out.join("game/synthetic.txt")).unwrap(),
            "second"
        );
        assert!(package(&game, &exe, &game, vec![]).is_err());
        fs::write(&exe, b"ELF").unwrap();
        assert!(package(&game, &exe, &out, vec![]).is_err());
        assert!(out.join("game/synthetic.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
