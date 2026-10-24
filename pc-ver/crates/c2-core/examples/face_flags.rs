//! Face flag counts of `.CAR` files: `cargo run --example face_flags -- X.CAR...`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for p in std::env::args().skip(1) {
        let Ok(d) = std::fs::read(&p) else { continue };
        let Ok(car) = c2_formats::car::parse(&d) else {
            println!("{p}: not a CAR");
            continue;
        };
        let mut counts = std::collections::BTreeMap::new();
        for f in &car.mesh.faces {
            *counts.entry(f.flags).or_insert(0) += 1;
        }
        println!("{p}: {} faces, flags {counts:x?}", car.mesh.faces.len());
    }
    Ok(())
}
