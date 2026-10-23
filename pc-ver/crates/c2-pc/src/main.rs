//! c2rs PC frontend: the game (`play`) plus model, area and hunt viewers.

mod app;
mod area_view;
mod audio;
mod camera;
mod dinos;
mod effects_view;
mod gpu;
mod hud;
mod hunt_view;
mod input;
mod keys;
mod menu;
mod model_view;
mod mods_cli;
mod png;
mod render;
mod save;
mod scene;
mod settings;
mod sounds;
mod weapons;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use c2_core::area::TimeOfDay;
use c2_core::game::SurvivalKind;

const USAGE: &str = "usage:
  c2-pc play  <GAME_DIR> [--overlay DIR]... [--mod MOD_DIR]... [--recipe RECIPE.toml --mods DIR] [--balance FILE]
              [--screen main|stats|hunt|options|credits|register|quit]
              [--script 'X,Y;'TEXT;enter;esc'] [--shot out.png]\n              [--survival-modes classic,modded,ours] [--mobile-survival FILE]
  c2-pc balance <GAME_DIR> [--overlay DIR]... [--mod MOD_DIR]...   (prints prices and scores as a balance file)
  c2-pc mobile <APK> --base <GAME_DIR> [--overlay DIR]... --out <DIR>   (the mobile game's animals as add-ons)
  c2-pc mobile <APK> --values                                         (its tuning values)
  c2-pc iceage <ICE_AGE_DIR> --base <GAME_DIR> [--overlay DIR]... --out <DIR>
                                        (Ice Age's animals and areas as add-ons beside C2's)
  c2-pc mod detect <MOD_DIR>... --base <GAME_DIR> [--write]
  c2-pc mod build  <MOD_DIR>... --base <GAME_DIR> --out <DIR>
  c2-pc recipe verify <RECIPE.toml> --base <GAME_DIR> --mods <DIR>
  c2-pc recipe build  <RECIPE.toml> --base <GAME_DIR> --mods <DIR> --out <DIR>
  c2-pc recipe new    <MOD_DIR>... --base <GAME_DIR> --id ID [--name NAME] [--out RECIPE.toml]
  c2-pc recipe base   --base <GAME_DIR> [--out recipes/bases/c2-vanilla.toml]
  c2-pc model <file.car|file.3df|dir> [--anim N] [--time MS] [--yaw RAD] [--shot out.png]
  c2-pc area  <AREA.MAP> [--dusk|--night] [--cam X,Y,Z,YAW,PITCH] [--view CELLS] [--models] [--shot out.png]
  c2-pc hunt  <AREA.MAP|TROPHY.MAP> [--dusk|--night] [--sim MS,AI,DIST,AFTER] [--target AI,..|all]
              [--weapon N] [--bino] [--observer] [--player N] [--survival normal|hardcore|custom|mobile]\n              [--mobile-survival FILE]\n              [--shot out.png]

model: drag to orbit, wheel zoom, [ ] next/prev file, N/P next/prev animation, Space pause
area:  WASD/QE move, drag to look, wheel = speed, Shift = fast, +/- view distance
play:  the menus; a hunt returns to them. --mod (repeatable) plays vanilla plus those add-on mods,
       combined into the data dir (see MERGE_REPORT.md there); --balance applies a balance file's
       prices and scores (see `c2-pc balance`); --menu-art GAME_DIR takes the menu frames from another
       game (the original's, a mod's, Ice Age's); --mobile-menu the experimental
       mobile layout of the hunt setup
mod:   detect prints (or --write saves) an inferred mod.toml; build writes a combined game folder
hunt:  click to capture mouse; keys follow the player's bindings (default WASD move, mouse look,
       LMB fire, E show/hide weapon, 1-6 choose weapon, Shift toggles run, X crouch, Space jump/swim,
       arrows turn/look, B binoculars, Tab map, Alt call, C change call),
       Pause/P pause, Esc exit prompt (Y leave and save, R restart); trophy room: R removes a trophy, Esc leaves";

/// Flags followed by a value (`--flag VALUE`).
const VALUE_FLAGS: &[&str] = &[
    "anim",
    "time",
    "yaw",
    "shot",
    "cam",
    "view",
    "sim",
    "ship",
    "weapon",
    "target",
    "fire",
    "screen",
    "player",
    "script",
    "mod",
    "base",
    "out",
    "recipe",
    "mods",
    "id",
    "name",
    "overlay",
    "balance",
    "menu-art",
    "survival",
    "survival-modes",
    "mobile-survival",
];

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
                let takes_value = VALUE_FLAGS.contains(&f);
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

    /// Every value of a repeatable flag.
    fn values(&self, f: &str) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(k, _)| k == f)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
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

/// `--mobile-survival FILE`: the mobile game's Survive mode.
fn mobile_survival(o: &Opts) -> Result<Option<c2_formats::mobile_arcade::Survive>> {
    o.value("mobile-survival")
        .map(|f| {
            let t = std::fs::read_to_string(f).with_context(|| format!("reading {f}"))?;
            Ok(c2_formats::mobile_arcade::Survive::parse(&t))
        })
        .transpose()
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
    if cmd == "balance" {
        let mods: Vec<PathBuf> = o
            .values("mod")
            .into_iter()
            .flat_map(|m| c2_mods::expand_addons(std::path::Path::new(m)))
            .collect();
        return mods_cli::print_balance(
            &mods_cli::bases(
                o.positional.first().map(String::as_str),
                &o.values("overlay"),
            )?,
            &mods,
        );
    }
    if cmd == "mobile" {
        let apk = o
            .positional
            .first()
            .context("c2-pc mobile <APK> --base GAME --out DIR")?;
        if o.has("values") {
            for (k, v) in c2_mods::mobile::values(std::path::Path::new(apk))? {
                println!("{k} {v:?}");
            }
            return Ok(());
        }
        let bases = mods_cli::bases(o.value("base"), &o.values("overlay"))?;
        let out = PathBuf::from(o.value("out").context("--out DIR")?);
        let base = c2_mods::vanilla(&bases)?;
        let import = c2_mods::mobile::import(std::path::Path::new(apk), &base, &out)?;
        for s in &import.skipped {
            println!("not imported: {s}");
        }
        for a in import.added {
            println!("{} (kind of the {}): {}", a.name, a.donor, a.dir.display());
            for n in &a.notes {
                println!("  {n}");
            }
        }
        return Ok(());
    }
    if cmd == "iceage" {
        let ia = o
            .positional
            .first()
            .context("c2-pc iceage <ICE_AGE_DIR> --base GAME --out DIR")?;
        let bases = mods_cli::bases(o.value("base"), &o.values("overlay"))?;
        let out = PathBuf::from(o.value("out").context("--out DIR")?);
        let base = c2_mods::vanilla(&bases)?;
        let import = c2_mods::ice_age::import(std::path::Path::new(ia), &base, &out)?;
        for s in &import.skipped {
            println!("not imported: {s}");
        }
        for d in import.added {
            println!("{}", d.display());
        }
        return Ok(());
    }
    if cmd == "mod" {
        return mods_cli::run(
            &o.positional,
            mods_cli::bases(o.value("base"), &o.values("overlay"))?,
            o.value("out"),
            o.has("write"),
        );
    }
    if cmd == "recipe" {
        let a = mods_cli::RecipeArgs {
            bases: mods_cli::bases(o.value("base"), &o.values("overlay"))?,
            mods: o.value("mods"),
            out: o.value("out"),
            id: o.value("id"),
            name: o.value("name"),
        };
        return mods_cli::run_recipe(&o.positional, &a);
    }
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
            // --weapon N (1-based): draw that weapon; --bino: binoculars up.
            let weapon = o
                .value("weapon")
                .map(|v| v.parse::<usize>().map(|n| n.saturating_sub(1)))
                .transpose()
                .context("--weapon N")?;
            let bino = o.has("bino");
            let map = o.has("map");
            let fire = o
                .value("fire")
                .map(str::parse::<u32>)
                .transpose()
                .context("--fire MS")?;
            // --target AI[,AI...] or `all`: hunt targets (no menu yet). Calls,
            // placement of T-Rex/Cerato and score depend on them.
            let targets = match o.value("target") {
                None => 0,
                Some("all") => (10..=18).fold(0u128, |m, a| m | 1 << a),
                Some(v) => v
                    .split(',')
                    .map(|a| {
                        a.trim()
                            .parse::<u32>()
                            .map(|a| 1u128.checked_shl(a).unwrap_or(0))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .context("--target AI[,AI...]")?
                    .into_iter()
                    .fold(0, |m, b| m | b),
            };
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
            let game_dir = path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let setup = hunt_view::HuntSetup {
                game_dir,
                map: path.clone(),
                time,
                seed: sim.as_ref().map(|_| 1),
                targets,
                // Without the menu, carry every weapon.
                weapons: u128::MAX,
                double_ammo: false,
                observer: o.has("observer"),
                // --survival normal|hardcore|custom|mobile|modded (README M13).
                survival: match o.value("survival") {
                    None => None,
                    Some("hardcore") => Some(SurvivalKind::Hardcore),
                    Some("custom") => Some(SurvivalKind::Custom),
                    Some("mobile") => Some(SurvivalKind::Mobile),
                    Some("modded") => Some(SurvivalKind::Mee),
                    Some(_) => Some(SurvivalKind::Normal),
                },
                // --mobile-survival FILE (`mobile-survival.txt` from an APK import).
                mobile_survival: mobile_survival(&o)?,
                reg: o.num("player", 0i32)?,
                from_menu: false,
                pc: settings::PcSettings::load(),
            };
            Box::new(move |g| {
                let mut h = hunt_view::HuntView::load(g, setup)?;
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
                if let Some(w) = weapon {
                    h.draw_weapon(w);
                }
                if bino {
                    h.raise_binoculars();
                }
                if map {
                    h.open_map();
                }
                if let Some(ms) = fire {
                    h.fire_and_wait(ms);
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
        "play" => {
            let screen = o.value("screen").map(str::to_owned);
            // --menu-art DIR: the menu style (another game's menu frames).
            if let Some(d) = o.value("menu-art") {
                let _ = menu::MENU_ART.set(PathBuf::from(d));
            }
            // --mobile-menu: the experimental mobile layout of the hunt setup.
            if o.has("mobile-menu") {
                let _ = menu::MOBILE_MENU.set(true);
            }
            // --survival-modes classic,modded[,ours] (which are offered;
            // default the first two; ours is benched), --mobile-survival FILE.
            let modes: Vec<String> = o
                .value("survival-modes")
                .unwrap_or("classic,modded")
                .split(',')
                .map(|m| m.trim().to_ascii_lowercase())
                .collect();
            let has = |names: &[&str]| modes.iter().any(|m| names.contains(&m.as_str()));
            let mobile =
                mobile_survival(&o)?.filter(|_| has(&["classic", "mobile", "android", "both"]));
            let _ = menu::SURVIVALS.set(menu::Survivals {
                ours: has(&["ours"]),
                mobile,
                modded: has(&["modded", "mee", "both"]),
            });
            // --script: canvas clicks / typing / keys, applied before the shot.
            let script = o.value("script").map(str::to_owned);
            // --mod DIR (repeatable): play a combined game from the data dir.
            let mods: Vec<PathBuf> = o
                .values("mod")
                .into_iter()
                .flat_map(|m| c2_mods::expand_addons(std::path::Path::new(m)))
                .collect();
            let recipe = o.value("recipe").map(PathBuf::from);
            // --overlay DIR (repeatable): files copied over the game first
            // (the Modder's Edition release over the original game).
            let overlays = o.values("overlay");
            let bases = mods_cli::bases(o.positional.first().map(String::as_str), &overlays)?;
            // --balance FILE: the player's prices and scores over the game.
            let balance = o.value("balance").map(std::path::Path::new);
            // Ice Age's hunters are its own.
            if mods_cli::is_ice_age(&path) {
                let _ = save::PROFILE.set("ice-age");
            }
            let path = if let Some(r) = recipe {
                let mods_dir = PathBuf::from(o.value("mods").context("--recipe needs --mods DIR")?);
                let out = mods_cli::recipe_pack_dir(&r);
                mods_cli::build_recipe(&r, &bases, &mods_dir, balance, &out)?;
                out
            } else if mods.is_empty()
                && overlays.is_empty()
                && balance.is_none()
                && !mods_cli::is_ice_age(&path)
            {
                path
            } else {
                // Ice Age alone: its rules are rewritten (named after it).
                let named = if mods_cli::is_ice_age(&path) {
                    &bases[..]
                } else {
                    &bases[1..]
                };
                let out = mods_cli::pack_dir(named, &mods);
                mods_cli::build_pack(&bases, &mods, balance, &out)?;
                out
            };
            Box::new(move |g| {
                let mut m = menu::Menu::new(g, &path, None)?;
                if let Some(s) = &screen {
                    m.show(s)?;
                }
                if let Some(s) = &script {
                    m.script(g, s)?;
                }
                Ok(Box::new(m) as Box<dyn scene::Scene>)
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
            // --models: far objects as models too (no sprites).
            let far_sprites = !o.has("models");
            Box::new(move |g| {
                let mut a = area_view::AreaView::load(g, &path, time)?;
                a.far_sprites = far_sprites;
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
