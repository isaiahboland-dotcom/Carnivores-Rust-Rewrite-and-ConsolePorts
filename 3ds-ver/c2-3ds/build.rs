//! Assembles the PICA200 vertex shaders with devkitPro's `picasso`, into one
//! file: `world` is program 0, `flat` program 1.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let devkitpro = std::env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".into());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let srcs = ["shaders/world.v.pica", "shaders/flat.v.pica"];
    for src in srcs {
        println!("cargo:rerun-if-changed={src}");
    }
    println!("cargo:rerun-if-env-changed=DEVKITPRO");
    let status = Command::new(format!("{devkitpro}/tools/bin/picasso"))
        .arg("-o")
        .arg(out.join("shaders.shbin"))
        .args(srcs)
        .status();
    match status {
        Ok(s) if s.success() => {}
        other => panic!("picasso failed on {srcs:?}: {other:?} (is devkitPro installed?)"),
    }
}
