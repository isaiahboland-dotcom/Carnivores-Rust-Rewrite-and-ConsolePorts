//! c2rs PC frontend: the game (`play`) plus model, area and hunt viewers.
//! A library so the Android build (`android-ver/`) can host the same game.

mod app;
#[cfg(target_os = "android")]
mod app_android;
#[cfg(not(target_os = "android"))]
mod app_desktop;
mod area_view;
mod audio;
mod camera;
pub mod cli;
mod dinos;
mod effects_view;
mod gpu;
mod hud;
mod hunt_kit;
mod hunt_view;
mod input;
mod keys;
mod menu;
mod model_view;
mod mods_cli;
mod notice;
mod pad_cursor;
mod png;
pub mod portable;
mod render;
mod save;
mod scene;
mod settings;
mod sounds;
mod weapons;

#[cfg(target_os = "android")]
pub use app::set_android_app;
pub use notice::show as notice;

/// Logs to stderr, `RUST_LOG` or the default filter.
pub fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn,naga=warn".into()),
        )
        .init();
}
