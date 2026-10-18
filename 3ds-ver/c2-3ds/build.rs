//! Assembles the PICA200 vertex shader with devkitPro's `picasso`.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let devkitpro = std::env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".into());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let src = "shaders/world.v.pica";
    println!("cargo:rerun-if-changed={src}");
    println!("cargo:rerun-if-env-changed=DEVKITPRO");
    let status = Command::new(format!("{devkitpro}/tools/bin/picasso"))
        .arg("-o")
        .arg(out.join("world.shbin"))
        .arg(src)
        .status();
    match status {
        Ok(s) if s.success() => {}
        other => panic!("picasso failed on {src}: {other:?} (is devkitPro installed?)"),
    }
}
