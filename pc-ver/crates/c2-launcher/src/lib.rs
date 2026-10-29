//! The launchers (README M10): set up the game from its folders, manage
//! add-ons, play on PC, and build 3DS and Wii U installs. Three builds of
//! one app: `c2-launcher` (PC, with the 3DS and Wii U sections if the user
//! wants them), `c2-3ds-builder` (3DS only) and `c2-wiiu-builder` (Wii U
//! only). The work is done by `c2-pc` and `c2-cook`,
//! run as programs; this only gathers the choices (the balance editor reads
//! the game's prices and scores itself, with `c2-mods`).

mod app;
mod balance;
mod cia;
mod config;
mod jobs;
mod options;
mod wiiu;

use std::path::PathBuf;

/// Which build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Play on PC; the 3DS section if turned on.
    Full,
    /// Build 3DS installs only.
    ThreeDs,
    /// Build Wii U installs only.
    WiiU,
}

/// The game menus' colours (`c2-pc` `menu.rs`, `c2-3ds` `menu.rs`): dark
/// olive panels, pale green text, gold for what is picked, sand for titles.
/// No original artwork is used.
pub mod theme {
    use eframe::egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(16, 22, 16);
    pub const PANEL: Color32 = Color32::from_rgb(32, 44, 30);
    pub const PANEL_HI: Color32 = Color32::from_rgb(48, 66, 44);
    pub const BUTTON: Color32 = Color32::from_rgb(60, 82, 52);
    pub const GREEN: Color32 = Color32::from_rgb(60, 110, 60);
    pub const TEXT: Color32 = Color32::from_rgb(158, 209, 128);
    pub const HOVER: Color32 = Color32::from_rgb(230, 255, 191);
    pub const GOLD: Color32 = Color32::from_rgb(240, 200, 70);
    pub const SAND: Color32 = Color32::from_rgb(230, 199, 153);
    pub const DIM: Color32 = Color32::from_rgb(92, 107, 77);
    pub const RED: Color32 = Color32::from_rgb(240, 80, 60);
    pub const BLUE: Color32 = Color32::from_rgb(140, 199, 242);
}

/// A tool the launcher runs (`c2-pc`, `c2-cook`, `c2-3ds.3dsx`, and for
/// `.cia` builds `c2-3ds.elf`, `makerom`, `bannertool`; the Wii U app
/// `c2-wiiu-perf-probe.rpx`): in the tools
/// folder if set, beside the launcher, or (working from the source tree) in
/// the other workspace's build folder or the tree's top folder.
pub fn find_tool(name: &str, tools: Option<&std::path::Path>) -> Option<PathBuf> {
    let file = if cfg!(windows) && !name.contains('.') {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf));
    let mut dirs: Vec<PathBuf> = tools
        .map(std::path::Path::to_path_buf)
        .into_iter()
        .collect();
    if let Some(d) = &exe_dir {
        dirs.push(d.clone());
        // pc-ver/target/release -> 3ds-ver/target/...
        if let Some(root) = d.ancestors().nth(3) {
            dirs.push(root.join("3ds-ver/target/release"));
            dirs.push(root.join("3ds-ver/target/armv6k-nintendo-3ds/release"));
            dirs.push(root.join("wiiu-ver/perf-probe/build"));
            dirs.push(root.to_path_buf());
        }
    }
    dirs.into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// `c2-3ds-builder --cia NAME --tools DIR --out DIR [--target o3ds|n3ds]
/// [--art-from GAME] [--icon PNG] [--banner PNG] [--sound WAV]`: the
/// `.cia` build without the window (the packs are `c2-cook`'s). Writes
/// `OUT/cia/NAME.cia`; `TOOLS` holds `c2-3ds.elf`, `makerom`, `bannertool`.
pub fn cia_cli(args: &[String]) -> Result<std::path::PathBuf, String> {
    use std::path::PathBuf;
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let name = config::install_name(&get("--cia").ok_or("--cia NAME")?.to_string_lossy());
    let tools = get("--tools").ok_or("--tools DIR")?;
    let out = get("--out").ok_or("--out DIR")?;
    let target = match get("--target").as_deref().and_then(|p| p.to_str()) {
        Some("n3ds") => config::Target::New,
        _ => config::Target::Old,
    };
    let tool = |n: &str| {
        let p = tools.join(n);
        p.exists()
            .then_some(p)
            .ok_or_else(|| format!("no {n} in {}", tools.display()))
    };
    let (icon, banner, sound, art) = (
        get("--icon"),
        get("--banner"),
        get("--sound"),
        get("--art-from"),
    );
    let steps = cia::steps(&cia::Cia {
        name: &name,
        target,
        elf: tool("c2-3ds.elf")?,
        makerom: tool("makerom")?,
        bannertool: tool("bannertool")?,
        icon: icon.as_deref(),
        banner: banner.as_deref(),
        sound: sound.as_deref(),
        art_from: art.as_deref(),
        work: config::data_dir().join("cia").join(&name),
        out: out.join("cia").join(format!("{name}.cia")),
    });
    let job = jobs::Job::start(format!("cia {name}"), steps);
    while job.result().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    for l in job.lines() {
        eprintln!("{l}");
    }
    if job.result() == Some(true) {
        Ok(out.join("cia").join(format!("{name}.cia")))
    } else {
        Err(format!("building {name}.cia failed"))
    }
}

pub fn run(mode: Mode) -> eframe::Result {
    let title = match mode {
        Mode::Full => "Carnivores 2 (Rust) - Launcher",
        Mode::ThreeDs => "Carnivores 2 (Rust) - 3DS Builder",
        Mode::WiiU => "Carnivores 2 (Rust) - Wii U Builder",
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([640.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| Ok(Box::new(app::Launcher::new(cc, mode)))),
    )
}
