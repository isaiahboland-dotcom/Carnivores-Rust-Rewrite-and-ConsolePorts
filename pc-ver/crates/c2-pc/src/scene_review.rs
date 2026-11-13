//! Private fixed-camera scene/sky diagnostic; no simulation or wall clock.
use crate::scene::Scene;
use crate::{
    area_view::AreaView,
    gpu::{Frame, Gpu, Target},
    scene,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
struct Capture {
    camera: [f32; 5],
    clock_ms: u32,
    radius: u32,
}
#[derive(Deserialize, Serialize)]
struct Recipe {
    schema: u32,
    map: PathBuf,
    captures: Vec<Capture>,
    #[serde(default)]
    disable_object_vertex_light: bool,
}

pub fn run(manifest: &Path, out: &Path) -> Result<()> {
    let recipe: Recipe = serde_json::from_slice(&std::fs::read(manifest)?)?;
    ensure!(
        recipe.schema == 1 && !recipe.captures.is_empty() && recipe.captures.len() <= 100,
        "unsupported scene recipe"
    );
    for c in &recipe.captures {
        ensure!(
            c.camera.iter().all(|x| x.is_finite())
                && c.camera[4].abs() < 1.5
                && (16..=128).contains(&c.radius),
            "invalid camera/radius"
        );
    }
    ensure!(!out.exists(), "output must be a new private directory");
    let (gpu, target) = Gpu::offscreen((800, 600))?;
    let Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let frame = Frame::new(&gpu, (800, 600));
    let target_view = texture.create_view(&Default::default());
    let mut area = AreaView::load(&gpu, &recipe.map, c2_core::area::TimeOfDay::Day)?;
    if recipe.disable_object_vertex_light {
        area.capture_without_vertex_light(&gpu);
    }
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join(".gitignore"), "*\n")?;
    for (i, c) in recipe.captures.iter().enumerate() {
        area.cam.pos = glam::Vec3::new(c.camera[0], c.camera[1], c.camera[2]);
        area.cam.yaw = c.camera[3];
        area.cam.pitch = c.camera[4];
        area.radius = c.radius;
        for (kind, isolated) in [("scene", false), ("sky", true)] {
            area.capture = Some((c.clock_ms, isolated));
            area.update(&gpu, 0.0, &crate::input::Input::default());
            scene::render(&gpu, &mut area, &target_view, &frame);
            let rgba = gpu.read_rgba(&texture)?;
            std::fs::write(
                out.join(format!("{kind}-{i:03}.png")),
                crate::png::encode_rgba(800, 600, &rgba),
            )?;
        }
    }
    let adapter = gpu.adapter_info();
    std::fs::write(
        out.join("review.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "recipe": recipe, "adapter": adapter.name, "backend": format!("{:?}", adapter.backend),
            "resolution": [800,600], "vertical_half_angle_tangent": 0.6,
            "time_of_day": "day", "brightness": 1, "ambient": 1,
            "map_sha256": c2_regress::texture_review::sha(&std::fs::read(&recipe.map)?),
            "rsc_sha256": c2_regress::texture_review::sha(&std::fs::read(crate::area_view::sibling(&recipe.map, "rsc")?)?),
            "executable_sha256": c2_regress::texture_review::sha(&std::fs::read(std::env::current_exe()?)?),
        }))?,
    )?;
    Ok(())
}
