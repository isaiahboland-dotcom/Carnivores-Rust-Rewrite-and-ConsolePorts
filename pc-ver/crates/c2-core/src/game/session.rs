//! Pause, exit prompts and hunt outcomes.

use super::Hunt;

/// How a hunt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Left alive: the trophy room should be saved.
    Exited,
    /// Died (or quit while dead): this hunt's trophies are discarded.
    Died,
}

impl Hunt {
    /// How the hunt ended, once it has (death: 12 s after dying).
    pub fn outcome(&self) -> Option<Outcome> {
        if self.death.is_some_and(|d| d.time > 12_000) {
            return Some(Outcome::Died);
        }
        self.outcome
    }

    /// Show the exit prompt (`EXITMODE`, or 6 s after death).
    pub fn show_exit(&self) -> bool {
        self.exit_prompt
            || self
                .death
                .is_some_and(|d| d.time > 6_000 || !self.opts.death_anim)
    }

    /// Escape: toggle the exit prompt; the trophy room is left at once.
    pub fn press_escape(&mut self) {
        if self.opts.trophy {
            self.outcome = Some(Outcome::Exited);
            return;
        }
        if self.paused {
            self.paused = false;
            return;
        }
        if self.exit_time == 0 {
            self.exit_prompt = !self.exit_prompt;
        }
    }

    /// Y / Enter at the prompt: leave (in 4 s if alive).
    pub fn confirm_exit(&mut self) {
        if self.show_exit() && self.exit_time == 0 {
            self.exit_time = if self.health > 0 { 4000 } else { 1 };
            self.exit_prompt = false;
        }
    }

    /// The pause key: toggle `PAUSE`, closing the exit prompt.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.exit_prompt = false;
    }

    /// Counts down a confirmed exit; on leaving alive, folds this hunt into
    /// the totals and updates the rank (`AnimateProcesses`, `SaveTrophy`).
    pub(super) fn exit_step(&mut self, dt: i32) {
        if self.exit_time == 0 || self.outcome.is_some() {
            return;
        }
        self.exit_time -= dt;
        if self.exit_time > 0 {
            return;
        }
        if self.health > 0 {
            let r = &mut self.room;
            r.last = c2_formats::sav::Stats {
                shots_made: self.shots_made as i32,
                success: self.successes as i32,
                path: self.path,
                time: self.time_s,
            };
            r.total.shots_made += r.last.shots_made;
            r.total.success += r.last.success;
            r.total.path += r.last.path;
            r.total.time += r.last.time;
            r.rank = if r.score >= 300 {
                2
            } else if r.score >= 100 {
                1
            } else {
                0
            };
            self.outcome = Some(Outcome::Exited);
        } else {
            self.outcome = Some(Outcome::Died);
        }
    }
}
