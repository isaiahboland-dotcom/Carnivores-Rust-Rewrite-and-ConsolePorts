//! Dumps the RGB555 texels of a game install to raw little-endian `u16`
//! files, for the texture-detail analysis (docs/TEXTURE-DETAIL.md, T1).
//!
//! `cargo run -p c2-formats --example dump_texels -- <HUNTDAT> <out-dir>`
//! Writes `car-<NAME>-256xH.u16` for each model and `rsc-<AREA>-tex<N>-128x128.u16`
//! for each ground tile.
use std::{fs, path::Path};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (huntdat, out) = (Path::new(&args[1]), Path::new(&args[2]));
    fs::create_dir_all(out).unwrap();
    let put = |name: String, px: &[u16]| {
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        fs::write(out.join(name), bytes).unwrap();
    };
    for e in fs::read_dir(huntdat).unwrap().flatten() {
        let p = e.path();
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        match p.extension().map(|x| x.to_ascii_uppercase()) {
            Some(x) if x == "CAR" => {
                if let Ok(car) = c2_formats::car::parse(&fs::read(&p).unwrap()) {
                    let h = car.mesh.texture_height();
                    put(format!("car-{stem}-256x{h}.u16"), &car.mesh.texture);
                }
            }
            _ => {}
        }
    }
    for e in fs::read_dir(huntdat.join("AREAS")).unwrap().flatten() {
        let p = e.path();
        if p.extension().map(|x| x.to_ascii_uppercase()) == Some("RSC".into()) {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            if let Ok(rsc) = c2_formats::rsc::parse(&fs::read(&p).unwrap()) {
                for (i, t) in rsc.textures.iter().enumerate() {
                    put(format!("rsc-{stem}-tex{i}-128x128.u16"), t);
                }
                for (i, o) in rsc.objects.iter().enumerate() {
                    let h = o.model.mesh.texture_height();
                    put(format!("obj-{stem}-{i}-256x{h}.u16"), &o.model.mesh.texture);
                }
            }
        }
    }
}
