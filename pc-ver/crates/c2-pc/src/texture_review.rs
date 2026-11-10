//! Private visual artifacts from public, deterministic model-rendering software.
use crate::{
    gpu::{Frame, Gpu, Target},
    input::Input,
    model_view::ModelView,
    scene::{self, Scene},
};
use anyhow::{bail, ensure, Context, Result};
use c2_core::model::{car_anim_duration_ms, pose_car};
use c2_formats::{car, mobile, res_txt::ResTxt};
use c2_regress::{image_diff, output::write_json, texture_review::Config};
use c2_vfs::{Backend, DirBackend, ZipBackend};
use serde_json::json;
use std::path::Path;

struct Apk(ZipBackend);
impl mobile::Assets for Apk {
    fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read(path).ok()
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
/// No public baseline format or upload path: all output lives in a new ignored directory.
pub fn run(
    manifest: &Path,
    out: &Path,
    baseline: Option<&Path>,
    limit: Option<usize>,
    keep_models: bool,
) -> Result<()> {
    let engine_commit = env!("C2_ENGINE_COMMIT");
    let engine_dirty = env!("C2_ENGINE_DIRTY") != "false";
    let text = std::fs::read_to_string(manifest)?;
    let mut config = Config::parse(&text)?;
    config.keep_models |= keep_models;
    let root = manifest.parent().unwrap_or(Path::new("."));
    let (width, height) = (config.width, config.height);
    let tolerance = config.channel_tolerance;
    let allowed = config.changed_fraction;
    let phases = &config.phases;
    ensure!(limit != Some(0), "--limit must be positive");
    let previous: Option<serde_json::Value> = baseline
        .map(|base| -> Result<_> {
            let prior: serde_json::Value =
                serde_json::from_slice(&std::fs::read(base.join("review.json"))?)?;
            ensure!(
                prior["kind"] == "private-texture-review" && prior["failures"] == 0,
                "baseline must be a complete texture review without failures"
            );
            let old = Config::parse(
                prior["manifest"]
                    .as_str()
                    .context("baseline lacks manifest")?,
            )?;
            ensure!(
                old.asset_type() == config.asset_type()
                    && old.width == width
                    && old.height == height
                    && old.phases == config.phases
                    && old.animations == config.animations,
                "baseline asset type, resolution, animation selection and phase schedule must match"
            );
            ensure!(
                prior["captures"].as_array().is_some_and(|c| !c.is_empty()),
                "baseline lacks captures"
            );
            for capture in prior["captures"]
                .as_array()
                .context("baseline lacks captures")?
            {
                ensure!(
                    capture["scenario"] == "isolated-model-v1",
                    "baseline is not a matched isolated model capture"
                );
                let recipe: c2_regress::texture_review::CaptureRecipe =
                    serde_json::from_value(capture["recipe"].clone())
                        .context("baseline lacks a supported capture recipe; recapture it")?;
                recipe.validate()?;
            }
            Ok(prior)
        })
        .transpose()?;
    // Never overwrite a baseline or a previous review, even after a failed run.
    ensure!(
        !out.exists(),
        "output already exists; choose a new run directory"
    );
    std::fs::create_dir_all(
        out.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    std::fs::create_dir(out)?;
    std::fs::write(out.join(".gitignore"), "*\n")?;
    let shots = out.join("screenshots");
    std::fs::create_dir(&shots)?;
    let models = out.join("models");
    std::fs::create_dir(&models)?;
    if config.keep_models {
        std::fs::create_dir(out.join("poses"))?;
    }
    let size = (width, height);
    let (gpu, target) = Gpu::offscreen(size)?;
    let Target::Offscreen { texture } = target else {
        bail!("offscreen target expected")
    };
    let target_view = texture.create_view(&Default::default());
    let frame = Frame::new(&gpu, size);
    let mut records = Vec::new();
    let mut errors = Vec::new();
    let mut gallery = String::from("<!doctype html><meta charset=utf-8><title>Private texture review</title><style>body{background:#222;color:#eee;font:16px sans-serif}img{width:46%;max-width:960px}summary{cursor:pointer;padding:1em;background:#333}input{padding:.7em;width:90%}section{border-top:1px solid #777;padding:1em}a{color:#9cf}</style><h1>Private texture review</h1><p>Human approval pending. Isolated animated models through the engine renderer; no AI or terrain. Differences are measured with a per-channel tolerance and an allowed fraction of changed pixels.</p><p><input placeholder=\"Filter by pack, animal or model path\" oninput=\"const q=this.value.toLowerCase();document.querySelectorAll('details.animal').forEach(d=>d.hidden=!d.dataset.search.includes(q))\"></p>");
    gallery.push_str(&format!(
        "<p>Asset: {} · Engine commit: {}{} · <a href=\"review.json\">Capture recipes and provenance</a></p>",
        escape(config.asset_type()), escape(engine_commit),
        if engine_dirty { " (uncommitted changes; approval identity unavailable)" } else { "" }
    ));
    let mut animals = 0;
    let mut failures = 0;
    for source in &config.source {
        let id = &source.id;
        let path = root.join(&source.path);
        let kind = source.kind.as_str();
        let include = &source.include;
        let exclude = &source.exclude;
        let backend: Box<dyn Backend> = if kind == "apk"
            || path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            Box::new(ZipBackend::new(&path, &source.prefix)?)
        } else {
            Box::new(DirBackend::new(&path)?)
        };
        let mut entries: Vec<(String, String)> = match kind {
            "apk" => backend
                .files()
                .into_iter()
                .filter(|p| {
                    p.to_ascii_lowercase().starts_with("assets/characters/")
                        && p.to_ascii_lowercase().ends_with(".cat")
                })
                .map(|p| (p.clone(), p))
                .collect(),
            "game" => {
                let data = backend.read("HUNTDAT/_RES.TXT")?;
                let res = ResTxt::parse(&String::from_utf8_lossy(&data))?.to_data();
                res.characters
                    .into_iter()
                    .filter_map(|c| {
                        c.file.map(|p| {
                            (
                                c.name.unwrap_or_else(|| p.clone()),
                                format!("HUNTDAT/{}", p.replace('\\', "/")),
                            )
                        })
                    })
                    .collect()
            }
            "cars" => backend
                .files()
                .into_iter()
                .filter(|p| p.to_ascii_lowercase().ends_with(".car"))
                .map(|p| (p.clone(), p))
                .collect(),
            _ => bail!("kind must be apk, game or cars"),
        };
        entries.sort_by_key(|a| a.1.to_ascii_lowercase());
        entries.dedup_by(|a, b| a.1.eq_ignore_ascii_case(&b.1));
        for requested in include {
            ensure!(
                entries
                    .iter()
                    .any(|(_, p)| p.eq_ignore_ascii_case(requested)),
                "{id}: included asset does not exist: {requested}"
            );
        }
        entries.retain(|(_, p)| {
            (include.is_empty() || include.iter().any(|i| p.eq_ignore_ascii_case(i)))
                && !exclude.iter().any(|i| p.eq_ignore_ascii_case(i))
        });
        ensure!(!entries.is_empty(), "source {id} selects no models");
        let apk = if kind == "apk" {
            Some(Apk(ZipBackend::new(&path, &source.prefix)?))
        } else {
            None
        };
        let mut selected = 0;
        for (name, asset) in entries {
            if limit.is_some_and(|n| selected >= n) {
                break;
            }
            let result = (|| -> Result<()> {
                let data = backend.read(&asset)?;
                let car = if let Some(apk) = &apk {
                    let cat = mobile::parse_cat(&String::from_utf8_lossy(&data))?;
                    mobile::car_from(&cat, apk)?
                } else {
                    car::parse(&data)?
                };
                ensure!(
                    !car.mesh.vertices.is_empty()
                        && !car.mesh.faces.is_empty()
                        && !car.mesh.texture.is_empty(),
                    "empty model/texture"
                );
                let key = format!(
                    "{id}-{}",
                    &c2_regress::texture_review::sha(asset.to_ascii_lowercase().as_bytes())[..16]
                );
                let model_path = models.join(format!("{key}.car"));
                let car_bytes = car::write(&car);
                let car_hash = c2_regress::texture_review::sha(&car_bytes);
                let reference = c2_core::model::PreparedModel::from_mesh(&car.mesh).positions;
                // Exercise CAR serialization/reload for APK imports as for addons.
                std::fs::write(&model_path, &car_bytes)?;
                let animations: Vec<usize> = if config.animations.is_empty() {
                    (0..car.animations.len().max(1)).collect()
                } else {
                    config.animations.clone()
                };
                ensure!(
                    animations.iter().all(|a| *a < car.animations.len().max(1)),
                    "selected animation is absent from {asset}"
                );
                let mut view = ModelView::new(&gpu, vec![model_path.clone()], 0, 0., 0.);
                // The renderer owns the reloaded data; keep source copies only
                // when explicitly requested for conversion diagnostics.
                if !config.keep_models {
                    std::fs::remove_file(&model_path)?;
                }
                gallery.push_str(&format!("<details class=animal data-search=\"{}\"><summary>{}: {} — {} animations</summary>", escape(&format!("{id} {name} {asset}").to_lowercase()), escape(id), escape(&name), animations.len()));
                for &anim in &animations {
                    let bounds = animation_bounds(&car, anim, width as f32 / height as f32)?;
                    view.review_frame(bounds.0, bounds.1)?;
                    for (phase_index, phase) in phases.iter().enumerate() {
                        let time = car
                            .animations
                            .get(anim)
                            .map_or(0., |a| car_anim_duration_ms(a) as f32 * *phase as f32);
                        let time = time.floor();
                        for (side, yaw) in [
                            ("left", std::f32::consts::FRAC_PI_2),
                            ("right", -std::f32::consts::FRAC_PI_2),
                        ] {
                            let shot = format!("{key}-a{anim:02}-p{phase_index}-{side}.png");
                            let yaw = yaw + source.yaw_offset_degrees.to_radians();
                            let pitch = source.pitch_degrees.to_radians();
                            let mut recipe = json!({"schema":1,"animation":anim,"time_ms":time as u32,
                                "center":bounds.0,"distance":bounds.1,"yaw":yaw,"pitch":pitch,
                                "vertical_half_angle_tangent":0.6,"near":16.0,
                                "far":bounds.1 * 10.0 + 1000.0,
                                "background":[0.18,0.2,0.24],"ambient":1.0,
                                "instance_light":1.0,"brightness":1.0,"daylight":[0.0,1.0,0.0],
                                "fog":[1e9,2e9],"resolution":[width,height]});
                            if let Some(prior) = &previous {
                                if let Some(old) =
                                    prior["captures"].as_array().and_then(|captures| {
                                        captures.iter().find(|c| c["file"] == shot)
                                    })
                                {
                                    ensure!(!old["recipe"].is_null(), "baseline lacks a capture recipe; recapture it before comparison");
                                    recipe = old["recipe"].clone();
                                }
                            }
                            let replay: c2_regress::texture_review::CaptureRecipe =
                                serde_json::from_value(recipe.clone())?;
                            ensure!(
                                replay.animation == anim && replay.resolution == [width, height],
                                "baseline recipe does not match capture selection"
                            );
                            let posed = if let Some(clip) = car.animations.get(anim) {
                                pose_car(clip, car.mesh.vertices.len(), replay.time_ms)
                                    .context("invalid animation pose")?
                            } else {
                                reference.clone()
                            };
                            let collapsed =
                                c2_regress::texture_review::collapsed_pose(&posed, &reference);
                            let pose_file = config
                                .keep_models
                                .then(|| format!("poses/{key}-a{anim:02}-p{phase_index}.f32"));
                            if let Some(file) = &pose_file {
                                let bytes: Vec<u8> = posed
                                    .iter()
                                    .flatten()
                                    .flat_map(|v| v.to_le_bytes())
                                    .collect();
                                std::fs::write(out.join(file), bytes)?;
                            }
                            view.review_recipe(replay)?;
                            view.update(&gpu, 0., &Input::default());
                            scene::render(&gpu, &mut view, &target_view, &frame);
                            let rgba = gpu.read_rgba(&texture)?;
                            let background = &rgba[..3];
                            let foreground = rgba
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .filter(|p| p[..3] != *background)
                                .count();
                            let capture_problem =
                                if collapsed {
                                    None
                                } else if foreground <= 8 {
                                    Some("empty or invisible model capture")
                                } else if car.mesh.texture.iter().any(|p| {
                                    [p & 31, (p >> 5) & 31, (p >> 10) & 31]
                                        .iter()
                                        .any(|c| *c > 4)
                                }) && !rgba.as_chunks::<4>().0.iter().any(|p| {
                                    p[..3] != *background && p[..3].iter().any(|c| *c > 24)
                                }) {
                                    Some("model rendered as a black silhouette")
                                } else {
                                    None
                                };
                            if let Some(problem) = capture_problem {
                                failures += 1;
                                errors.push(format!("{shot}: {problem}"));
                            }
                            let rgb: Vec<u8> = rgba
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .flat_map(|p| [p[0], p[1], p[2]])
                                .collect();
                            std::fs::write(
                                shots.join(&shot),
                                image_diff::encode_rgb(size.0, size.1, &rgb)?,
                            )?;
                            let mut ratio = None;
                            let mut status = "unreviewed";
                            let mut diff_file = None;
                            let mut baseline_file = None;
                            if let Some(base) = baseline {
                                let before =
                                    image_diff::load_png(&base.join("screenshots").join(&shot));
                                match before {
                                    Ok(before) => {
                                        let after = image_diff::Rgba {
                                            width: size.0,
                                            height: size.1,
                                            data: rgba,
                                        };
                                        let diff =
                                            image_diff::diff(&before, &after, tolerance, &[])?;
                                        ratio = Some(diff.ratio());
                                        status = if diff.ratio() > allowed {
                                            failures += 1;
                                            "changed"
                                        } else {
                                            "within tolerance"
                                        };
                                        // Passing comparisons need metrics, not two
                                        // additional copies of every screenshot.
                                        if diff.ratio() > allowed || capture_problem.is_some() {
                                            let filename = format!("baseline-{shot}");
                                            std::fs::copy(
                                                base.join("screenshots").join(&shot),
                                                shots.join(&filename),
                                            )?;
                                            baseline_file = Some(filename);
                                            let filename = format!("diff-{shot}");
                                            std::fs::write(
                                                shots.join(&filename),
                                                image_diff::encode_rgb(
                                                    size.0,
                                                    size.1,
                                                    &diff.image,
                                                )?,
                                            )?;
                                            diff_file = Some(filename);
                                        }
                                    }
                                    Err(e) => {
                                        failures += 1;
                                        status = "baseline missing/unreadable";
                                        errors.push(format!("{shot}: {e:#}"));
                                    }
                                }
                            }
                            if collapsed && status == "unreviewed" {
                                status = "source pose collapsed (see geometry)";
                            }
                            if capture_problem.is_some() {
                                status = "invalid capture";
                            }
                            let clip = car
                                .animations
                                .get(anim)
                                .map(|a| a.name())
                                .unwrap_or_else(|| "rest".into());
                            gallery.push_str(&format!("<section><p>{} · {} · phase {} · {} · {} ({:?})</p><a href=\"screenshots/{shot}\"><img loading=lazy src=\"screenshots/{shot}\"></a>", escape(&key), escape(&clip), phase, side, status, ratio));
                            if let Some(f) = &baseline_file {
                                gallery.push_str(&format!("<a href=\"screenshots/{f}\"><img loading=lazy title=\"Baseline\" src=\"screenshots/{f}\"></a>"));
                            }
                            if let Some(f) = &diff_file {
                                gallery.push_str(&format!(
                                    "<img loading=lazy src=\"screenshots/{f}\">"
                                ));
                            }
                            gallery.push_str("</section>");
                            records.push(json!({"scenario":"isolated-model-v1","recipe":recipe,"pose_file":pose_file,"model_file":config.keep_models.then(|| format!("models/{key}.car")),"source":id,"asset":asset,"name":name,"animation":anim,"clip":clip,"phase":phase,"side":side,"file":shot,"status":status,"changed_fraction":ratio,"car_sha256":car_hash,"foreground_pixels":foreground,"capture_error":capture_problem,"source_pose_collapsed":collapsed}));
                        }
                    }
                }
                gallery.push_str("</details>");
                animals += 1;
                selected += 1;
                println!("{id}: {name} ({} animations)", animations.len());
                Ok(())
            })();
            if let Err(e) = result {
                failures += 1;
                eprintln!("{id}/{asset}: {e:#}");
                errors.push(format!("{id}/{asset}: {e:#}"));
            }
        }
        if selected == 0 {
            failures += 1;
            errors.push(format!("{id}: no animals captured"));
        }
    }
    if let Some(previous) = previous {
        for old in previous["captures"]
            .as_array()
            .context("baseline lacks captures")?
        {
            if !records.iter().any(|r| r["file"] == old["file"]) {
                failures += 1;
                errors.push(format!("baseline capture removed: {}", old["file"]));
            }
        }
    }
    for error in &errors {
        gallery.push_str(&format!("<p>Error: {}</p>", escape(error)));
    }
    gallery.push_str(&format!(
        "<p>{animals} animals, {} captures, {failures} failures. Human approval pending.</p>",
        records.len()
    ));
    std::fs::write(out.join("report.html"), gallery)?;
    let adapter = gpu.adapter_info();
    write_json(
        &out.join("review.json"),
        &json!({"kind":"private-texture-review","asset_type":config.asset_type(),"engine":"rust","engine_commit":engine_commit,"engine_dirty":engine_dirty,"schema":2,"human_approval":"pending","manifest":text,"adapter":adapter.name,"driver":adapter.driver,"resolution":[width,height],"channel_tolerance":tolerance,"allowed_changed_fraction":allowed,"animals":animals,"failures":failures,"errors":errors,"captures":records}),
    )?;
    println!("Review: {}", out.join("report.html").display());
    ensure!(
        animals > 0 && failures == 0,
        "texture review has {failures} failures ({animals} animals)"
    );
    Ok(())
}

fn animation_bounds(car: &car::Car, animation: usize, aspect: f32) -> Result<([f32; 3], f32)> {
    let prepared = c2_core::model::PreparedModel::from_mesh(&car.mesh);
    let mut lo = glam::Vec3::splat(f32::INFINITY);
    let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
    let mut add = |positions: &[[f32; 3]]| {
        for p in positions {
            let p = glam::Vec3::from(*p);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    };
    if car.animations.is_empty() {
        add(&prepared.positions);
    }
    if let Some(anim) = car.animations.get(animation) {
        for f in 0..anim.frames {
            let time = (f as u64 * 1000 / anim.kps.max(1) as u64) as u32;
            if let Some(p) = pose_car(anim, car.mesh.vertices.len(), time) {
                add(&p);
            }
        }
    }
    ensure!(
        lo.is_finite() && hi.is_finite(),
        "non-finite animation bounds"
    );
    Ok((((lo + hi) * 0.5).to_array(), {
        let span = hi - lo;
        (span.y.max(span.z / aspect) + span.x * 0.5).max(100.)
    }))
}
