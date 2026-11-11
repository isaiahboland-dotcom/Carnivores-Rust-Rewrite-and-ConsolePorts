//! Synthetic GPU coverage regression; no game assets.
use c2_formats::{
    car,
    model::{face_flags, Face, Mesh, Vertex},
};

#[test]
#[ignore = "requires an available wgpu adapter"]
fn shared_texture_key_only_cuts_flagged_faces() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!("c2-cutout-{}", std::process::id()));
    std::fs::create_dir_all(root.join("models"))?;
    let mut mesh = Mesh::default();
    // Three equal triangles in the Y/Z plane: black solid, black cutout,
    // white solid. The white triangle also makes capture validation useful.
    for (index, z) in [-90.0, 0.0, 90.0].into_iter().enumerate() {
        for (y, dz) in [(-25.0, -30.0), (-25.0, 30.0), (25.0, 0.0)] {
            mesh.vertices.push(Vertex {
                x: 0.0,
                y,
                z: z + dz,
                owner: 0,
                hide: 0,
            });
        }
        let v = index as i32 * 3;
        mesh.faces.push(Face {
            v: [v, v + 1, v + 2],
            tu: [if index == 2 { 200 } else { 20 }; 3],
            tv: [20; 3],
            flags: if index == 1 { face_flags::OPACITY } else { 0 },
            dmask: 0,
            distant: 0,
            next: 0,
            group: 0,
            reserved: [0; 12],
        });
    }
    // Keep a cutout flag in both models even when the second visible face
    // becomes solid: both uploads must retain the same texture alpha mask.
    let mut hidden = mesh.faces[1];
    hidden.v = [0; 3];
    mesh.faces.push(hidden);
    mesh.texture = (0..256 * 256)
        .map(|i| if i % 256 < 128 { 0 } else { 0x7fff })
        .collect();
    for (name, flag) in [("mixed", face_flags::OPACITY), ("solid", 0)] {
        mesh.faces[1].flags = flag;
        let model = car::Car {
            name_raw: [0; 32],
            mesh: mesh.clone(),
            animations: vec![],
            sounds: vec![],
            anim_sounds: None,
            trailing: vec![],
        };
        std::fs::write(root.join(format!("models/{name}.car")), car::write(&model))?;
    }
    let manifest = root.join("review.toml");
    std::fs::write(&manifest, "width = 320\nheight = 240\nanimations = [0]\nphases = [0.0]\n[[source]]\nid = 'fixture'\nkind = 'cars'\npath = 'models'\n")?;
    let out = root.join("review");
    super::texture_review::run(&manifest, &out, None, None, false)?;
    let review: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("review.json"))?)?;
    let mut black = std::collections::BTreeMap::new();
    for capture in review["captures"].as_array().expect("captures") {
        let path = out
            .join("screenshots")
            .join(capture["file"].as_str().expect("file"));
        let pixels = c2_regress::image_diff::load_png(&path)?;
        let count = pixels
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3].iter().all(|&v| v < 5))
            .count();
        black.insert(
            (
                capture["asset"].as_str().unwrap().to_owned(),
                capture["side"].as_str().unwrap().to_owned(),
            ),
            count,
        );
    }
    for side in ["left", "right"] {
        let mixed = black[&("mixed.car".into(), side.into())];
        let solid = black[&("solid.car".into(), side.into())];
        assert!(mixed > 100, "solid black face was cut out: {mixed}");
        assert!(
            (1.8..2.2).contains(&(solid as f32 / mixed as f32)),
            "cutout must remove only one of two black faces: mixed={mixed}, solid={solid}"
        );
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}
