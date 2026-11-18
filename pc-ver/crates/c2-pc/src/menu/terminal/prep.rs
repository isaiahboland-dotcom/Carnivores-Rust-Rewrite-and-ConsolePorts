//! The Advanced DHC OS UI on PC and Android (spec `ADVANCEDDHCOS.md`): an
//! optional presentation of the terminal's hunt setup, on only while the
//! terminal is ("Advanced DHC OS UI", or the launcher's Always On). Its
//! state is shared with the terminal (`c2_core::dhc::prep`): the picks,
//! rules and refusals are the terminal's, so it starts the same hunts.
//!
//! Built so far: the map stage (phase B), a carousel of the areas with a
//! low-resolution relief of the highlighted one turning in the centre
//! (`previews.rs`), the loadout record and the area's survey beside it
//! (`prep_draw.rs`). Until their phases are built, the dinosaur, gun and
//! gear stages are the terminal's own setup tabs; Back from them returns
//! to the stage before.

use c2_core::dhc::prep::{PrepAction, Stage};
use c2_core::dhc::terminal::Screen as TScreen;

use super::*;

/// What a spot of the map stage does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrepHit {
    /// A neighbour in the carousel: highlight it.
    Item(usize),
    /// The highlighted map, or the USE THIS MAP button: commit it.
    Use,
    Prev,
    Next,
    Back,
}

/// A wheel notch (the pad's right stick scrolls a little every frame and
/// would spin the carousel).
const NOTCH: f32 = 0.5;

impl Menu {
    /// Whether the Advanced DHC OS UI presents the hunt setup.
    pub(super) fn advanced_ui(&self) -> bool {
        advanced_ui_look(&self.pc)
    }

    /// Whether the map stage's carousel shows now.
    pub(super) fn prep_shown(&self) -> bool {
        self.screen == Screen::Terminal
            && self.advanced_ui()
            && self
                .term
                .as_ref()
                .is_some_and(|v| v.term.screen == TScreen::Setup && v.prep.stage == Stage::Map)
    }

    /// The map stage: browse with the arrows, the wheel or the chevrons;
    /// use the map with Enter, Space, the pad's A or R, or a click.
    pub(super) fn update_prep(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        let hits = self.prep_hits();
        let Some(v) = &mut self.term else { return };
        v.last_cursor = self.cursor;
        let t = &mut v.term;
        t.step(dt);
        let key = |k| input.pressed(k);
        let shift = input.held(KeyCode::ShiftLeft) || input.held(KeyCode::ShiftRight);
        let mut acts = Vec::new();
        for (k, a) in [
            (KeyCode::ArrowLeft, PrepAction::Prev),
            (KeyCode::ArrowUp, PrepAction::Prev),
            (KeyCode::ArrowRight, PrepAction::Next),
            (KeyCode::ArrowDown, PrepAction::Next),
            (KeyCode::Enter, PrepAction::Commit),
            (KeyCode::NumpadEnter, PrepAction::Commit),
            (KeyCode::Space, PrepAction::Add),
            (KeyCode::Escape, PrepAction::Back),
        ] {
            if key(k) {
                acts.push(a);
            }
        }
        // The pad's R is Tab, its L Shift+Tab (`pad_cursor.rs`).
        if key(KeyCode::Tab) {
            acts.push(if shift {
                PrepAction::Back
            } else {
                PrepAction::Commit
            });
        }
        if input.scroll >= NOTCH {
            acts.push(PrepAction::Prev);
        } else if input.scroll <= -NOTCH {
            acts.push(PrepAction::Next);
        }
        if input.clicked {
            if let Some((_, hit)) = hits.iter().find(|(r, _)| inside(self.cursor, *r)) {
                acts.push(match *hit {
                    PrepHit::Item(i) => PrepAction::Highlight(i),
                    PrepHit::Use => PrepAction::Commit,
                    PrepHit::Prev => PrepAction::Prev,
                    PrepHit::Next => PrepAction::Next,
                    PrepHit::Back => PrepAction::Back,
                });
            }
        }
        let mut beep = None;
        for a in acts {
            beep = v.prep.act(t, a).or(beep);
        }
        v.prep.step(t, dt);
        let outcome = t.take_outcome();
        // The highlighted area's relief once the carousel rests on it; the
        // neighbours are built ahead.
        let n = t.cat.areas.len();
        let path = |i: usize| {
            let item = *v.map[0].get(i)?;
            Some((item, self.areas.get(item)?.clone()))
        };
        let on_map = t.screen == TScreen::Setup && v.prep.stage == Stage::Map;
        let h = v.prep.highlight(t, Stage::Map);
        let want = (on_map && v.prep.settled(t, Stage::Map))
            .then(|| path(h))
            .flatten();
        let next: Vec<_> = if want.is_some() && n > 1 {
            [(h + 1) % n, (h + n - 1) % n]
                .into_iter()
                .filter_map(path)
                .collect()
        } else {
            Vec::new()
        };
        v.previews.update(gpu, dt, want, &next);
        if let Some(o) = outcome {
            self.terminal_outcome(o);
        }
        self.beep(beep);
    }
}
