//! Running a [`Scene`]: the PC's winit loop (`app_desktop`), Android's own
//! (`app_android`), or one frame offscreen (screenshots).

use anyhow::Result;

use crate::gpu::{Frame, Gpu, Target};
use crate::input::Input;
use crate::scene::render;

pub use crate::scene::MakeScene;

#[cfg(not(target_os = "android"))]
pub use crate::app_desktop::run;

/// The Android activity, handed over by `android_main` before [`run`].
#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<android_activity::AndroidApp> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_app(app: android_activity::AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

/// Android: its own loop on the activity (`app_android`), not winit's.
#[cfg(target_os = "android")]
pub fn run(make: MakeScene) -> Result<()> {
    let app = ANDROID_APP
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no Android activity (set_android_app)"))?;
    crate::app_android::run(app, make)
}

/// `--touch`: screenshots as if the last input was a touch (the phone's
/// on-screen buttons show).
pub static SHOT_TOUCH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Renders a single frame offscreen and writes it as PNG.
pub fn screenshot(size: (u32, u32), out: &std::path::Path, make: MakeScene) -> Result<()> {
    let (gpu, target) = Gpu::offscreen(size)?;
    let Target::Offscreen { texture } = &target else {
        anyhow::bail!("offscreen target expected")
    };
    let frame = Frame::new(&gpu, size);
    let mut scene = make(&gpu)?;
    let input = || {
        let mut i = Input::default();
        i.touch_mode = SHOT_TOUCH.get().copied().unwrap_or(false);
        i
    };
    let mut first = input();
    scene.before_update(&mut first);
    scene.update(&gpu, 0.0, &first);
    let view = texture.create_view(&Default::default());
    render(&gpu, scene.as_mut(), &view, &frame);
    // Follow scene changes (menu -> loading -> hunt).
    for _ in 0..4 {
        let Some(next) = scene.next() else { break };
        scene = next(&gpu)?;
        let mut i = input();
        scene.before_update(&mut i);
        scene.update(&gpu, 0.0, &i);
        render(&gpu, scene.as_mut(), &view, &frame);
    }
    let rgba = gpu.read_rgba(texture)?;
    std::fs::write(out, crate::png::encode_rgba(size.0, size.1, &rgba))?;
    println!("{}: {}", out.display(), scene.title());
    Ok(())
}
