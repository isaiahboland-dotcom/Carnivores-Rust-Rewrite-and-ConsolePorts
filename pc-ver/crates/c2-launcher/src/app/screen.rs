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
pub(super) fn fit_screen(ctx: &egui::Context, scale: Option<f32>) {
    let zoom = match scale {
        Some(z) => z.clamp(0.5, 2.0),
        None => {
            let (rect, native) = ctx.input(|i| {
                let v = i.viewport();
                (v.inner_rect, v.native_pixels_per_point)
            });
            let (Some(rect), Some(native)) = (rect, native) else {
                return;
            };
            let pixels = rect.size() * ctx.pixels_per_point();
            let fit = (pixels.x / (FIT.x * native)).min(pixels.y / (FIT.y * native));
            if !fit.is_finite() {
                return;
            }
            fit.clamp(0.5, 1.0)
        }
    };
    if (ctx.zoom_factor() - zoom).abs() > 0.01 {
        ctx.set_zoom_factor(zoom);
    }
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
