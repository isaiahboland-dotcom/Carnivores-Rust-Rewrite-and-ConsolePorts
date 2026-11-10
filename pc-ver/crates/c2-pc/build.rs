//! Stamp the compiled engine, rather than whichever checkout runs the binary.
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")?;
    let root = Path::new(&manifest);
    for name in ["HEAD", "index"] {
        if let Some(path) = git(root, &["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    if let Some(reference) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(root, &["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    // Recompute provenance whenever a public source file changes. Private,
    // ignored captures and locally owned game data are never included.
    if let Some(repo) = git(root, &["rev-parse", "--show-toplevel"]) {
        if let Some(files) = git(root, &["ls-files", "-c", "-o", "--exclude-standard", "-z"]) {
            for file in files.split('\0').filter(|p| !p.is_empty()) {
                println!(
                    "cargo:rerun-if-changed={}",
                    Path::new(&repo).join(file).display()
                );
            }
        }
    }
    let commit = git(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(root, &["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
    println!("cargo:rustc-env=C2_ENGINE_COMMIT={commit}");
    println!("cargo:rustc-env=C2_ENGINE_DIRTY={dirty}");
    Ok(())
}
