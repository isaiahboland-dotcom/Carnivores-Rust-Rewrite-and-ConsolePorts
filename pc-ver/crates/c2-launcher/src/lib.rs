//! The launchers (README M10): set up the game from its folders, manage
//! add-ons, play on PC, and build 3DS installs. Two builds of one app:
//! `c2-launcher` (PC, with the 3DS section if the user wants it) and
//! `c2-3ds-builder` (3DS only). The work is done by `c2-pc` and `c2-cook`,
//! run as programs; this only gathers the choices (the balance editor reads
//! the game's prices and scores itself, with `c2-mods`).

mod app;
mod balance;
mod config;
mod jobs;

use std::path::PathBuf;

/// Which build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Play on PC; the 3DS section if turned on.
    Full,
    /// Build 3DS installs only.
    ThreeDs,
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

/// A tool the launcher runs (`c2-pc`, `c2-cook`, `c2-3ds.3dsx`): in the
/// tools folder if set, beside the launcher, or (working from the source
/// tree) in the other workspace's build folder.
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
        }
    }
    dirs.into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

pub fn run(mode: Mode) -> eframe::Result {
    let title = match mode {
        Mode::Full => "Carnivores 2 (Rust) - Launcher",
        Mode::ThreeDs => "Carnivores 2 (Rust) - 3DS Builder",
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
