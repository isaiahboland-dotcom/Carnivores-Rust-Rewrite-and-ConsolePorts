//! The DHC OS terminal recommended, once per instance, for a recipe with
//! advanced settings (predation, flee limits, the hardcore hunt, DHC rules,
//! world zones, area caps, secrets, ...): in the review shown when such a
//! recipe is imported, else before the instance's first Play. The terminal
//! is never switched on without the player's click. The game's settings
//! file is shared by every instance, so the launcher, which knows the
//! instances, keeps which ones have had the recommendation.

use super::*;
use crate::game_cfg::{flag, set_flag, DHC_TERMINAL};
use c2_mods::classic::advanced_summary;
use c2_mods::recipe::Recipe;

/// Whether the instance at `path` gets the recommendation: its recipe
/// uses an advanced setting, the terminal is off, and the instance hasn't
/// had it before.
pub(super) fn recommends(
    done: &std::collections::BTreeSet<PathBuf>,
    path: &Path,
    recipe: &Recipe,
    terminal_on: bool,
) -> bool {
    !terminal_on && !done.contains(path) && !advanced_summary(recipe).is_empty()
}

const TEXT: &str = "This instance uses advanced settings. The DHC OS terminal, an in-universe \
     DinoHunt Corp front end for the main menu and hunt setup, shows what they offer and \
     restrict on each area. Turn it on? It can be changed any time in the instance's Menu \
     panel or the game's Options. Asked once per instance.";

impl Launcher {
    /// Whether the instance at `path` gets the recommendation now.
    pub(super) fn dhc_hint_for(&self, path: &Path) -> bool {
        Recipe::load(path)
            .is_ok_and(|r| recommends(&self.cfg.dhc_recommended, path, &r, flag(DHC_TERMINAL)))
    }

    /// The instance about to be played, when it gets the recommendation
    /// before it starts.
    pub(super) fn dhc_hint_due(&self) -> Option<PathBuf> {
        let p = self.active_recipe()?.path.clone();
        self.dhc_hint_for(&p).then_some(p)
    }

    /// The recommendation made for `path`; `on`: the terminal turned on.
    pub(super) fn dhc_hint_answer(&mut self, path: &Path, on: bool) {
        self.cfg.dhc_recommended.insert(path.to_path_buf());
        self.save();
        if on {
            if let Err(e) = set_flag(DHC_TERMINAL, true) {
                self.ui.note = Some((format!("Saving the game's settings: {e}"), theme::red()));
            }
        }
    }

    /// The recommendation's text and its two answers (`yes`, `no` labels):
    /// `Some(true)` turn it on, `Some(false)` leave it off.
    pub(super) fn dhc_hint_ui(ui: &mut egui::Ui, yes: &str, no: &str) -> Option<bool> {
        ui.label(
            RichText::new("DHC OS terminal recommended")
                .color(theme::sand())
                .strong(),
        );
        help(ui, TEXT);
        let mut answer = None;
        ui.horizontal(|ui| {
            if ui.button(RichText::new(yes).color(theme::gold())).clicked() {
                answer = Some(true);
            }
            if !no.is_empty() && ui.button(no).clicked() {
                answer = Some(false);
            }
        });
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::recommends;
    use c2_mods::recipe::Recipe;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    const ADVANCED: &str = "[recipe]\nid = \"a\"\nname = \"A\"\nversion = \"1\"\n\n\
        [settings.predation]\nenabled = true\n";

    #[test]
    fn recommended_once_for_advanced_recipes_with_the_terminal_off() {
        let adv = Recipe::parse(ADVANCED).expect("recipe");
        let plain = Recipe::default();
        let p = Path::new("a.toml");
        let mut done = BTreeSet::new();
        assert!(recommends(&done, p, &adv, false));
        assert!(!recommends(&done, p, &adv, true), "already on");
        assert!(!recommends(&done, p, &plain, false), "no advanced settings");
        done.insert(PathBuf::from("a.toml"));
        assert!(!recommends(&done, p, &adv, false), "only once");
        assert!(
            recommends(&done, Path::new("b.toml"), &adv, false),
            "per instance"
        );
    }
}
