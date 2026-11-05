//! Fitting the launcher to the screen. Owner report (2026-11-05): added to
//! Steam as a non-Steam game, the Steam Deck showed it zoomed in, with the
//! instances' Play buttons out of reach; `WINIT_X11_SCALE_FACTOR=1.0` and
//! the game resolution setting did not help. So the launcher sizes itself:
//! full screen under Steam's game mode / Big Picture (gamescope), and an
//! interface size that by default shrinks the UI until a whole page fits.
use super::*;

/// The smallest page (in points) the tabs are laid out for.
const FIT: egui::Vec2 = egui::vec2(880.0, 600.0);

/// Steam's game mode or Big Picture (gamescope) is running the launcher,
/// or `C2_LAUNCHER_FULLSCREEN=1` asks for full screen.
pub(crate) fn steam_game_mode() -> bool {
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty() && v != "0");
    [
        "C2_LAUNCHER_FULLSCREEN",
        "SteamGamepadUI",
        "SteamTenfoot",
        "GAMESCOPE_WAYLAND_DISPLAY",
    ]
    .iter()
    .any(|k| set(k))
        || std::env::var("SteamDeck").is_ok_and(|v| v == "1") && set("SteamAppId")
}

/// The zoom for this frame: the chosen size, or (`None`) the largest up
/// to the system's at which a [`FIT`] page fits the window.
///
/// The window is measured by egui's viewport rect, which is in this pass's
/// points (egui rescales it on the pass a new zoom takes effect), times
/// this pass's `pixels_per_point`. The viewport info's `inner_rect` is not
/// used: it was measured at the last pass's zoom, and mixing the two made
/// Auto flip between the right zoom and its square every other frame (the
/// owner's Steam Deck in game mode, 2026-11-05); X11 may not report it at
/// all.
pub(super) fn fit_screen(ctx: &egui::Context, scale: Option<f32>) {
    let zoom = match scale {
        Some(z) => z.clamp(0.5, 2.0),
        None => {
            let (rect, native) =
                ctx.input(|i| (i.viewport_rect(), i.viewport().native_pixels_per_point));
            let Some(native) = native else {
                return;
            };
            match auto_zoom(rect.size() * ctx.pixels_per_point(), native) {
                Some(z) if rect.width() > 0.0 && rect.height() > 0.0 => z,
                _ => return,
            }
        }
    };
    if (ctx.zoom_factor() - zoom).abs() > 0.01 {
        ctx.set_zoom_factor(zoom);
    }
}

/// The Auto zoom for a window of `pixels` at the system's `native` scale:
/// the largest up to 1 at which a [`FIT`] page fits, never below 0.5.
fn auto_zoom(pixels: egui::Vec2, native: f32) -> Option<f32> {
    let fit = (pixels.x / (FIT.x * native)).min(pixels.y / (FIT.y * native));
    fit.is_finite().then(|| fit.clamp(0.5, 1.0))
}

/// Scroll bars that are always there and wide enough to drag on a touch
/// screen.
pub(super) fn scroll_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.spacing.scroll = egui::style::ScrollStyle::solid();
        s.spacing.scroll.bar_width = 12.0;
    });
}

impl Launcher {
    /// Settings: the interface size. Whether it changed.
    pub(super) fn ui_scale_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let shown = |s: Option<f32>| match s {
            None => "Auto (fit the window)".to_string(),
            Some(z) => format!("{:.0}%", z * 100.0),
        };
        ui.horizontal(|ui| {
            ui.label("Interface size:");
            egui::ComboBox::from_id_salt("ui-scale")
                .selected_text(shown(self.cfg.ui_scale))
                .show_ui(ui, |ui| {
                    for s in [
                        None,
                        Some(0.6),
                        Some(0.75),
                        Some(0.9),
                        Some(1.0),
                        Some(1.25),
                        Some(1.5),
                    ] {
                        changed |= ui
                            .selectable_value(&mut self.cfg.ui_scale, s, shown(s))
                            .changed();
                    }
                });
        });
        help(
            ui,
            "Auto makes the launcher smaller when the window can't show a whole page (the \
             Steam Deck, small screens). Under Steam's game mode or Big Picture it starts full \
             screen (C2_LAUNCHER_FULLSCREEN=1 asks for that anywhere).",
        );
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Steam Deck screen at a 1.5 system scale: one zoom, applied once.
    #[test]
    fn auto_zoom_is_steady() {
        let pixels = egui::vec2(1280.0, 800.0);
        let z = auto_zoom(pixels, 1.5).unwrap();
        assert!((z - 800.0 / 900.0).abs() < 1e-4);
        // measured at that zoom, the same window gives the same answer
        let points = pixels / (1.5 * z);
        assert_eq!(auto_zoom(points * z * 1.5, 1.5), Some(z));
    }
}
