//! c2rs PC frontend. For now: the M2 model and area viewers.

mod app;
mod area_view;
mod camera;
mod dinos;
mod gpu;
mod hud;
mod hunt_view;
mod input;
mod model_view;
mod png;
mod render;
mod save;
mod scene;
mod weapons;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use c2_core::area::TimeOfDay;

const USAGE: &str = "usage:
  c2-pc model <file.car|file.3df|dir> [--anim N] [--time MS] [--yaw RAD] [--shot out.png]
  c2-pc area  <AREA.MAP> [--dusk|--night] [--cam X,Y,Z,YAW,PITCH] [--view CELLS] [--shot out.png]
  c2-pc hunt  <AREA.MAP> [--dusk|--night] [--sim MS,AI,DIST,AFTER] [--weapon] [--shot out.png]

model: drag to orbit, wheel zoom, [ ] next/prev file, N/P next/prev animation, Space pause
area:  WASD/QE move, drag to look, wheel = speed, Shift = fast, +/- view distance
hunt:  click to capture mouse, WASD move, mouse look, LMB fire, E show/hide weapon, 1-6 choose weapon,
       Shift toggles run, X crouch, Space jump/swim, arrows turn/look,
       Esc exit prompt (Y leave and save, R restart)";

struct Opts {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut o = Opts {
            positional: Vec::new(),
            flags: Vec::new(),
        };
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            if let Some(f) = a.strip_prefix("--") {
                let takes_value = matches!(
                    f,
                    "anim" | "time" | "yaw" | "shot" | "cam" | "view" | "sim" | "ship"
                );
                let v = if takes_value { it.next() } else { None };
                o.flags.push((f.to_string(), v));
            } else {
                o.positional.push(a);
            }
        }
        o
    }

    fn has(&self, f: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == f)
    }

    fn value(&self, f: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(k, _)| k == f)
            .and_then(|(_, v)| v.as_deref())
    }

    fn num<T: std::str::FromStr>(&self, f: &str, default: T) -> Result<T> {
        match self.value(f) {
            None => Ok(default),
            Some(v) => v
                .parse()
                .map_err(|_| anyhow::anyhow!("--{f}: invalid value {v:?}")),
        }
    }
}

fn time_of_day(o: &Opts) -> TimeOfDay {
    if o.has("night") {
        TimeOfDay::Night
    } else if o.has("dusk") {
        TimeOfDay::Dusk
    } else {
        TimeOfDay::Day
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn,naga=warn".into()),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let o = Opts::parse(args);
    let Some(path) = o.positional.first().map(PathBuf::from) else {
        bail!("{USAGE}");
    };
    let shot = o.value("shot").map(PathBuf::from);

    let make: app::MakeScene = match cmd.as_str() {
        "model" => {
            let files = model_view::list_models(&path)?;
            let (anim, time, yaw) = (
                o.num("anim", 0usize)?,
                o.num("time", 0f32)?,
                o.num("yaw", 0.6f32)?,
            );
            Box::new(move |g| {
                Ok(
                    Box::new(model_view::ModelView::new(g, files, anim, time, yaw))
                        as Box<dyn scene::Scene>,
                )
            })
        }
        "hunt" => {
            let time = time_of_day(&o);
            let weapon = o.has("weapon");
            // --ship AI,MS[,DIST]: bag an animal, wait, look at the ship.
            let ship: Option<Vec<f32>> = o
                .value("ship")
                .map(|v| v.split(',').map(str::parse).collect::<Result<_, _>>())
                .transpose()
                .context("--ship AI,MS[,DIST]")?;
            // --sim MS[,AI[,DIST[,AFTER]]]: fixed seed, run the AI, then watch an
            // animal; AFTER ms more pass with the camera fixed (screenshots).
            let sim: Option<Vec<f32>> = o
                .value("sim")
                .map(|v| v.split(',').map(str::parse).collect::<Result<_, _>>())
                .transpose()
                .context("--sim MS[,AI[,DIST]]")?;
            Box::new(move |g| {
                let seed = sim.as_ref().map(|_| 1);
                let mut h = hunt_view::HuntView::load(g, &path, time, seed)?;
                if let Some(s) = sim {
                    let ai = s.get(1).copied().unwrap_or(12.0) as i32;
                    let dist = s.get(2).copied().unwrap_or(1500.0);
                    h.spectate(
                        s[0] as u32,
                        ai,
                        dist,
                        s.get(3).copied().unwrap_or(0.0) as u32,
                    );
                }
                if weapon {
                    h.draw_weapon();
                }
                if let Some(v) = &ship {
                    h.watch_ship(
                        v[0] as i32,
                        v[1] as u32,
                        v.get(2).copied().unwrap_or(3000.0),
                    );
                }
                Ok(Box::new(h) as Box<dyn scene::Scene>)
            })
        }
        "area" => {
            let time = time_of_day(&o);
            let cam = o
                .value("cam")
                .map(|c| {
                    let v: Vec<f32> = c
                        .split(',')
                        .map(str::parse)
                        .collect::<Result<_, _>>()
                        .context("--cam X,Y,Z,YAW,PITCH")?;
                    anyhow::ensure!(v.len() == 5, "--cam needs 5 numbers");
                    Ok(v)
                })
                .transpose()?;
            // 74 = ctViewR for the default OptViewR of 128 (Game.cpp: 42 + OptViewR/8*2).
            let view = o.num("view", 74u32)?;
            Box::new(move |g| {
                let mut a = area_view::AreaView::load(g, &path, time)?;
                if let Some(c) = cam {
                    a.cam.pos = glam::Vec3::new(c[0], c[1], c[2]);
                    a.cam.yaw = c[3];
                    a.cam.pitch = c[4];
                }
                a.radius = view.clamp(16, 512);
                Ok(Box::new(a) as Box<dyn scene::Scene>)
            })
        }
        _ => bail!("{USAGE}"),
    };
    match shot {
        Some(out) => app::screenshot((1280, 720), &out, make),
        None => app::run(make),
    }
}
