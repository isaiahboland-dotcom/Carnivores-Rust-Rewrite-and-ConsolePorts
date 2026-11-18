//! Dumps the RGB555 texels of a game install to raw little-endian `u16`
//! files, for the texture-detail analysis (docs/TEXTURE-DETAIL.md, T1).
//!
//! `cargo run -p c2-formats --example dump_texels -- <HUNTDAT> <out-dir>`
//! Writes `car-<NAME>-256xH.u16` for each model and `rsc-<AREA>-tex<N>-128x128.u16`
//! for each ground tile.
use std::{error::Error, fs, path::Path};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result {
    let args: Vec<String> = std::env::args().collect();
    let (Some(huntdat), Some(out)) = (args.get(1), args.get(2)) else {
        return Err("usage: dump_texels <HUNTDAT> <out-dir>".into());
    };
    let (huntdat, out) = (Path::new(huntdat), Path::new(out));
    fs::create_dir_all(out)?;
    let put = |name: String, px: &[u16]| -> Result {
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        fs::write(out.join(name), bytes)?;
        Ok(())
    };
    for e in fs::read_dir(huntdat)?.flatten() {
        let p = e.path();
        let stem = file_stem(&p)?;
        match p.extension().map(|x| x.to_ascii_uppercase()) {
            Some(x) if x == "CAR" => {
                if let Ok(car) = c2_formats::car::parse(&fs::read(&p)?) {
                    let h = car.mesh.texture_height();
                    put(format!("car-{stem}-256x{h}.u16"), &car.mesh.texture)?;
                }
            }
            _ => {}
        }
    }
    for e in fs::read_dir(huntdat.join("AREAS"))?.flatten() {
        let p = e.path();
        if p.extension().map(|x| x.to_ascii_uppercase()) == Some("RSC".into()) {
            let stem = file_stem(&p)?;
            if let Ok(rsc) = c2_formats::rsc::parse(&fs::read(&p)?) {
                for (i, t) in rsc.textures.iter().enumerate() {
                    put(format!("rsc-{stem}-tex{i}-128x128.u16"), t)?;
                }
                for (i, o) in rsc.objects.iter().enumerate() {
                    let h = o.model.mesh.texture_height();
                    put(format!("obj-{stem}-{i}-256x{h}.u16"), &o.model.mesh.texture)?;
                }
            }
        }
    }
    Ok(())
}

fn file_stem(p: &Path) -> Result<String> {
    let stem = p.file_stem().ok_or("path has no file name")?;
    Ok(stem.to_string_lossy().into_owned())
}
