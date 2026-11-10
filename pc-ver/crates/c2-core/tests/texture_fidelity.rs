//! Synthetic APK -> CAR -> runtime regression; no proprietary assets.
use c2_core::model::PreparedModel;
use c2_formats::mobile::Assets;
use c2_formats::{car, mobile, model::face_flags};

struct Fixture;
impl mobile::Assets for Fixture {
    fn get(&self, path: &str) -> Option<Vec<u8>> {
        match path {
            "assets/models/characters/fidelity.3dn" => {
                let mut d = Vec::new();
                for n in [3u32, 1, 0] {
                    d.extend(n.to_le_bytes());
                }
                d.extend([0; 36]);
                for p in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
                    for v in p {
                        d.extend(v.to_le_bytes());
                    }
                    d.extend([0; 4]);
                }
                for i in [0i32, 1, 2] {
                    d.extend(i.to_le_bytes());
                }
                for (u, v) in [(0u16, 0u16), (255, 0), (0, 255)] {
                    d.extend(u.to_le_bytes());
                    d.extend(v.to_le_bytes());
                }
                d.extend(face_flags::OPACITY.to_le_bytes());
                d.extend([0; 26]);
                Some(d)
            }
            "assets/textures/characters/fidelity.crt" => {
                let mut d = vec![0, 1, 0, 1, 32];
                for y in 0..256 {
                    for x in 0..256 {
                        d.extend(match (x, y) {
                            (0, 0) => [0, 0, 0, 255],
                            (1, 0) => [7, 7, 7, 255],
                            (2, 0) => [0, 0, 0, 0],
                            (3, 0) => [255, 0, 0, 255],
                            (255, 255) => [0, 255, 0, 255],
                            _ => [64, 128, 192, 255],
                        });
                    }
                }
                Some(d)
            }
            _ => None,
        }
    }
}

#[test]
fn texture_fidelity_apk_car_runtime_preserves_coverage_colors_and_uvs() {
    let imported = mobile::car_from(
        &mobile::Cat {
            model: "fidelity.3dn".into(),
            ..Default::default()
        },
        &Fixture,
    )
    .expect("synthetic APK import");
    let loaded = car::parse(&car::write(&imported)).expect("exported CAR");
    assert_eq!(loaded, imported, "CAR export must preserve texture and UVs");
    let prepared = PreparedModel::from_mesh(&loaded.mesh);
    assert_eq!(prepared.faces.len(), 1);
    assert_eq!(
        prepared.faces[0].uv,
        [[0., 0.], [255. / 256., 0.], [0., 255. / 256.]]
    );
    assert_ne!(prepared.faces[0].flags & face_flags::OPACITY, 0);
    let source = Fixture
        .get("assets/textures/characters/fidelity.crt")
        .unwrap();
    for (i, (src, dst)) in source[5..]
        .as_chunks::<4>()
        .0
        .iter()
        .zip(prepared.texture_rgba.as_chunks::<4>().0.iter())
        .enumerate()
    {
        assert_eq!(dst[3], src[3], "coverage changed at texel {i}");
        if src[3] == 255 {
            for c in 0..3 {
                assert!(
                    (i16::from(src[c]) - i16::from(dst[c])).abs() <= 8,
                    "RGB555 color error at texel {i}, channel {c}: {src:?} -> {dst:?}"
                );
            }
        }
    }
}
