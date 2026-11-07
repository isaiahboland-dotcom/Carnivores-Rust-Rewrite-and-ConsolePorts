//! `c2-pc regress`: a regression scenario through the PC renderer,
//! offscreen. Writes into the run folder `c2-regress run` made:
//! `screenshots/<checkpoint>.png`, `frame_times.csv`, `render.json`,
//! `render.log`. Started by `c2-regress run` (docs/REGRESSION.md).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use c2_core::area::TimeOfDay;
use c2_core::game::Hunt;
use c2_core::scenario::{self, Director, Frontend, Headless, Inject, Screen};
use c2_regress::output::{event_json, stage_json, write_json};
use c2_regress::scenario_file::ScenarioFile;
use serde_json::json;

use crate::gpu::{Frame, Gpu, Target};
use crate::hunt_view::{HuntSetup, HuntView};
use crate::scene::render;

/// The core's screens from the hunt; the pickers as the view reports them.
struct PcFrontend {
    open: Option<Screen>,
    request: Option<(Screen, bool)>,
}

impl Frontend for PcFrontend {
    fn showing(&self, hunt: &Hunt, s: Screen) -> Option<bool> {
        if s.frontend_only() {
            Some(self.open == Some(s))
        } else {
            Headless.showing(hunt, s)
        }
    }

    fn set(&mut self, s: Screen, open: bool) {
        self.request = Some((s, open));
    }
}

/// `name` in `dir`, ignoring case.
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

fn size(v: &str) -> Result<(u32, u32)> {
    let (w, h) = v.split_once('x').context("--size WxH")?;
    Ok((w.parse()?, h.parse()?))
}

pub struct Args<'a> {
    pub scenario: &'a str,
    pub game_id: &'a str,
    pub game: &'a str,
    pub out: &'a str,
    pub size: &'a str,
    pub inject: Option<&'a str>,
}

pub fn run(a: &Args) -> Result<()> {
    let file = ScenarioFile::load(Path::new(a.scenario))?;
    let def = file.game(a.game_id)?;
    let s = file.for_game(def)?;
    let area = def
        .area
        .as_deref()
        .context("the game section names no area")?;
    let out = PathBuf::from(a.out);
    let shots = out.join("screenshots");
    std::fs::create_dir_all(&shots)?;
    let game_dir = PathBuf::from(a.game);
    let map = find_ci(&game_dir, "HUNTDAT")
        .and_then(|h| find_ci(&h, "AREAS"))
        .and_then(|d| find_ci(&d, &format!("{area}.MAP")))
        .with_context(|| format!("no HUNTDAT/AREAS/{area}.MAP in {}", game_dir.display()))?;
    let size = size(a.size)?;

    let (gpu, target) = Gpu::offscreen(size)?;
    let Target::Offscreen { texture } = &target else {
        bail!("offscreen target expected")
    };
    let info = gpu.adapter_info();
    let frame = Frame::new(&gpu, size);
    let setup = HuntSetup {
        game_dir,
        map,
        time: TimeOfDay::Day,
        seed: Some(s.seed),
        targets: c2_formats::bits::Bits::NONE,
        weapons: u128::MAX,
        double_ammo: false,
        night_vision: false,
        flashlight: false,
        observer: false,
        survival: None,
        reg: 0,
        from_menu: false,
        // Defaults, not this machine's saved settings.
        pc: crate::settings::PcSettings::default(),
    };
    let mut view = HuntView::load(&gpu, setup)?;
    let (mut hold, names) = view.regress_setup(&s)?;
    let inject = a
        .inject
        .map(Inject::parse)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    if let Some(i) = &inject {
        i.adjust_hold(&mut hold);
    }
    let mut d = Director::new(&s, &names);
    let mut fe = PcFrontend {
        open: None,
        request: None,
    };
    let tex_view = texture.create_view(&Default::default());
    let mut csv = String::from("frame,tick,update_ms,render_ms,total_ms\n");
    let mut checkpoints = Vec::new();
    let mut log = String::new();
    let ceiling = s.max_ticks(names.len());
    let mut frame_no = 0u32;
    while !d.done() {
        if d.tick() >= ceiling {
            d.abort("scenario timeout: whole-run tick ceiling reached");
            break;
        }
        let t0 = Instant::now();
        let mut plan = d.plan(view.regress_hunt(), &mut fe);
        if let Some(i) = &inject {
            i.adjust_plan(&mut plan, view.regress_hunt());
            i.adjust_hunt(d.tick(), s.warmup_ticks, view.regress_hunt_mut());
        }
        if let Some((screen, open)) = fe.request.take() {
            view.regress_picker(screen, open);
        }
        fe.open = view.regress_open_picker();
        view.regress_tick(&gpu, &plan.input);
        let lost = view.regress_hold(&hold);
        let caps = d.observe(view.regress_hunt(), &mut fe, &lost);
        let update = t0.elapsed().as_secs_f64() * 1000.0;
        let t1 = Instant::now();
        render(&gpu, &mut view, &tex_view, &frame);
        if let Err(e) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
            let _ = writeln!(log, "WARN gpu wait: {e}");
        }
        let draw = t1.elapsed().as_secs_f64() * 1000.0;
        let _ = writeln!(
            csv,
            "{frame_no},{},{update:.4},{draw:.4},{:.4}",
            d.tick(),
            update + draw
        );
        frame_no += 1;
        for name in caps {
            let rgba = gpu.read_rgba(texture)?;
            let rgb: Vec<u8> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let png = c2_regress::image_diff::encode_rgb(size.0, size.1, &rgb)?;
            std::fs::write(shots.join(format!("{name}.png")), png)?;
            let hash = scenario::state_hash(view.regress_hunt());
            checkpoints.push(json!({
                "name": name, "tick": d.tick(), "state_hash": format!("{hash:016x}"),
                "file": format!("screenshots/{name}.png"),
            }));
        }
    }
    let (_, peak) = c2_regress::metrics::process_memory();
    let render = json!({
        "adapter": info.name,
        "backend": format!("{:?}", info.backend),
        "device_type": format!("{:?}", info.device_type),
        "driver": info.driver,
        "resolution": [size.0, size.1],
        "frames": frame_no,
        "process_peak_bytes": peak,
        "stages": d.stages.iter().map(|x| stage_json(x, "pc-render")).collect::<Vec<_>>(),
        "events": d.events.iter().map(|e| event_json(e, "pc-render")).collect::<Vec<_>>(),
        "checkpoints": checkpoints,
        "failed": d.failed(),
    });
    std::fs::write(out.join("frame_times.csv"), csv)?;
    std::fs::write(out.join("render.log"), log)?;
    write_json(&out.join("render.json"), &render)?;
    println!(
        "render pass: {frame_no} frames, {} screenshots, {}",
        render["checkpoints"].as_array().map_or(0, Vec::len),
        if d.failed() { "failed" } else { "passed" }
    );
    Ok(())
}
