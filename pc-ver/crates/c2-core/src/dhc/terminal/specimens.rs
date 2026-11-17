//! The field manual as a database (PC and Android): an index of the
//! catalog's areas, animals and weapons, and one item's file, shown after a
//! short retrieval sequence. A file opens from the index or from the hunt
//! setup ([`Action::Inspect`]) and goes back where it came from; a file's
//! links (an area's animals and weapons, a weapon's areas) open their own
//! files, which go back to it. What a file says is `dhc::specimen`,
//! `dhc::survey` and `dhc::armoury`.

use alloc::string::String;
use alloc::vec::Vec;

use super::{Action, FileRef, Screen, Terminal};
use crate::dhc::specimen::{retrieve_log, retrieve_shown, RETRIEVE_S};
use crate::dhc::{armoury, survey, Tab};

/// The index's lists, in the setup's order.
pub const INDEX_TABS: [Tab; 3] = [Tab::Area, Tab::Targets, Tab::Arms];

/// Files opened from links before the oldest is forgotten.
const MAX_FILES: usize = 8;

impl Terminal {
    /// The index's list.
    pub fn index_tab(&self) -> Tab {
        self.index_tab
    }

    /// The focused row of the index's list.
    pub fn index_row(&self) -> usize {
        self.index_rows[self.index_tab.index()]
    }

    /// The file shown (none: no file open).
    pub fn open_file_ref(&self) -> Option<FileRef> {
        self.files.last().copied()
    }

    /// The shown file's item (index into its list of the catalog).
    pub fn file(&self) -> usize {
        self.files.last().map_or(0, |f| f.i)
    }

    /// The shown file's list: an area, an animal or a weapon.
    pub fn file_kind(&self) -> Tab {
        self.files.last().map_or(Tab::Targets, |f| f.kind)
    }

    /// The shown file's focused link.
    pub fn file_link(&self) -> Option<usize> {
        self.files.last().and_then(|f| f.link)
    }

    /// The open files, oldest first (the shown one last).
    pub fn files(&self) -> &[FileRef] {
        &self.files
    }

    /// The screen the first open file goes back to.
    pub fn file_from(&self) -> Screen {
        self.file_from
    }

    /// The other files the shown one links to, in the order its lines list
    /// them: an area's animals and weapons, a weapon's areas, each only
    /// when the island rules restrict them.
    pub fn links(&self) -> Vec<(Tab, usize)> {
        match self.files.last() {
            Some(f) if f.kind == Tab::Area => survey::links(&self.cat, &self.rules, f.i),
            Some(f) if f.kind == Tab::Arms => armoury::links(&self.cat, &self.rules, f.i),
            _ => Vec::new(),
        }
    }

    /// The file is still being retrieved (its log shows, not the file).
    pub fn retrieving(&self) -> bool {
        self.screen == Screen::Specimen && self.clock() < RETRIEVE_S
    }

    /// The shown file's retrieval log.
    pub fn retrieve_log(&self) -> Vec<String> {
        let kind = self.file_kind();
        let name = self
            .cat
            .list(kind)
            .get(self.file())
            .map_or("", |a| a.name.as_str());
        retrieve_log(kind, name, self.specimen_models)
    }

    /// Lines of the retrieval log shown so far.
    pub fn retrieve_shown(&self) -> usize {
        retrieve_shown(self.retrieve_log().len(), self.clock())
    }

    /// Opens the file of item `i` of `kind`, to go back to `from`.
    pub(super) fn open_file(&mut self, kind: Tab, i: usize, from: Screen) {
        if kind == Tab::Gear || i >= self.cat.list(kind).len() {
            return;
        }
        self.files.clear();
        self.files.push(FileRef {
            kind,
            i,
            link: None,
        });
        self.file_from = from;
        if from == Screen::Specimens {
            self.index_tab = kind;
            self.index_rows[kind.index()] = i;
        }
        self.go(Screen::Specimen);
    }

    /// Opens link `k` of the shown file. A file already open is shown again
    /// (the ones after it close); a new one is retrieved.
    fn open_link(&mut self, k: usize) {
        let Some((kind, i)) = self.links().get(k).copied() else {
            return;
        };
        if let Some(top) = self.files.last_mut() {
            top.link = Some(k);
        }
        if let Some(at) = self.files.iter().position(|f| (f.kind, f.i) == (kind, i)) {
            self.files.truncate(at + 1);
            self.show_retrieved();
            return;
        }
        if self.files.len() >= MAX_FILES {
            self.files.remove(0);
        }
        self.files.push(FileRef {
            kind,
            i,
            link: None,
        });
        self.go(Screen::Specimen);
    }

    /// Closes the shown file: back to the one it was opened from, else
    /// where the first came from.
    fn close_file(&mut self) {
        self.files.pop();
        if self.files.is_empty() {
            self.go(self.file_from);
        } else {
            self.show_retrieved();
        }
    }

    /// Shows the top file at once (it was retrieved before).
    fn show_retrieved(&mut self) {
        self.go(Screen::Specimen);
        self.step(RETRIEVE_S);
    }

    pub(super) fn act_specimens(&mut self, a: Action) {
        let tab = self.index_tab;
        let n = self.cat.list(tab).len();
        let last = n.saturating_sub(1);
        let row = &mut self.index_rows[tab.index()];
        let at = INDEX_TABS.iter().position(|&t| t == tab).unwrap_or(1);
        match a {
            Action::Up => *row = row.saturating_sub(1),
            Action::Down => *row = (*row + 1).min(last),
            Action::Left => self.index_tab = INDEX_TABS[(at + 2) % 3],
            Action::Right => self.index_tab = INDEX_TABS[(at + 1) % 3],
            Action::Tab(t) if INDEX_TABS.contains(&t) => self.index_tab = t,
            Action::Focus(i) if i < n => *row = i,
            Action::Pick(i) => self.open_file(tab, i, Screen::Specimens),
            Action::Confirm | Action::Inspect => {
                let i = (*row).min(last);
                self.open_file(tab, i, Screen::Specimens)
            }
            Action::Back => self.go(Screen::Main),
            _ => {}
        }
    }

    pub(super) fn act_specimen(&mut self, a: Action) {
        let links = self.links().len();
        match a {
            Action::Back => self.close_file(),
            // Skipping the retrieval shows the file.
            _ if self.retrieving() => {
                if matches!(a, Action::Skip | Action::Confirm | Action::Pick(_)) {
                    self.skip_retrieval();
                }
            }
            // The links take the focus; a pick opens one.
            Action::Up | Action::Down if links > 0 => {
                if let Some(top) = self.files.last_mut() {
                    top.link = Some(match (a, top.link) {
                        (Action::Down, None) => 0,
                        (Action::Down, Some(k)) => (k + 1).min(links - 1),
                        (_, k) => k.unwrap_or(0).saturating_sub(1),
                    });
                }
            }
            Action::Focus(k) if k < links => {
                if let Some(top) = self.files.last_mut() {
                    top.link = Some(k);
                }
            }
            Action::Pick(k) if k < links => self.open_link(k),
            Action::Confirm => match self.file_link().filter(|&k| k < links) {
                Some(k) => self.open_link(k),
                // A file without a focused link closes (the pad's A).
                None => self.close_file(),
            },
            _ => {}
        }
    }

    /// Shows the open file now, as if its retrieval had run.
    fn skip_retrieval(&mut self) {
        self.step(RETRIEVE_S);
    }
}
