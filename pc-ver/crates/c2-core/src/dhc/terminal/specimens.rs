//! The field manual as the specimen database (PC and Android): the index
//! of the catalog's animals, and one animal's file, shown after a short
//! retrieval sequence. The file opens from the index or from the hunt
//! setup's targets tab ([`Action::Inspect`]) and goes back where it came
//! from. What a file says is `dhc::specimen`.

use alloc::string::String;
use alloc::vec::Vec;

use super::{Action, Screen, Terminal};
use crate::dhc::specimen::{retrieve_log, retrieve_shown, RETRIEVE_S};

impl Terminal {
    /// The focused row of the specimen index.
    pub fn specimen_row(&self) -> usize {
        self.specimen_row
    }

    /// The open file's animal (catalog index).
    pub fn file(&self) -> usize {
        self.file
    }

    /// The screen the open file goes back to.
    pub fn file_from(&self) -> Screen {
        self.file_from
    }

    /// The file is still being retrieved (its log shows, not the file).
    pub fn retrieving(&self) -> bool {
        self.screen == Screen::Specimen && self.clock() < RETRIEVE_S
    }

    /// The open file's retrieval log.
    pub fn retrieve_log(&self) -> Vec<String> {
        let name = self
            .cat
            .animals
            .get(self.file)
            .map_or("", |a| a.name.as_str());
        retrieve_log(name, self.specimen_models)
    }

    /// Lines of the retrieval log shown so far.
    pub fn retrieve_shown(&self) -> usize {
        retrieve_shown(self.retrieve_log().len(), self.clock())
    }

    /// Opens animal `i`'s file, to go back to `from`.
    pub(super) fn open_file(&mut self, i: usize, from: Screen) {
        if i >= self.cat.animals.len() {
            return;
        }
        self.file = i;
        self.file_from = from;
        if from == Screen::Specimens {
            self.specimen_row = i;
        }
        self.go(Screen::Specimen);
    }

    pub(super) fn act_specimens(&mut self, a: Action) {
        let n = self.cat.animals.len();
        let last = n.saturating_sub(1);
        match a {
            Action::Up => self.specimen_row = self.specimen_row.saturating_sub(1),
            Action::Down => self.specimen_row = (self.specimen_row + 1).min(last),
            Action::Focus(i) if i < n => self.specimen_row = i,
            Action::Pick(i) => self.open_file(i, Screen::Specimens),
            Action::Confirm | Action::Inspect => {
                self.open_file(self.specimen_row.min(last), Screen::Specimens)
            }
            Action::Back => self.go(Screen::Main),
            _ => {}
        }
    }

    pub(super) fn act_specimen(&mut self, a: Action) {
        match a {
            Action::Back => self.go(self.file_from),
            // Skipping the retrieval shows the file; on the file, a pick
            // closes it (a click anywhere, the pad's A).
            _ if self.retrieving() => {
                if matches!(a, Action::Skip | Action::Confirm | Action::Pick(_)) {
                    self.skip_retrieval();
                }
            }
            Action::Confirm => self.go(self.file_from),
            _ => {}
        }
    }

    /// Shows the open file now, as if its retrieval had run.
    fn skip_retrieval(&mut self) {
        self.step(RETRIEVE_S);
    }
}
