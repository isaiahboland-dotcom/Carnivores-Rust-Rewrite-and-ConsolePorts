//! Drawing the field manual's index (the areas, animals or weapons beside
//! the focused one's picture) and a file's retrieval sequence. The open
//! file is `file_view.rs`; its model is `viewer.rs`.

use c2_core::dhc::specimen::archive;
use c2_core::dhc::terminal::{Terminal, INDEX_TABS};
use c2_core::dhc::Tab;

use super::draw::*;
use super::*;

/// Index rows.
const ROW: f32 = 20.0;
/// Where a fact's value starts after its label.
pub(super) const VALUE_X: f32 = 124.0;
pub(super) const LINE_H: f32 = 17.0;

/// The field manual index's rows that fit.
pub(super) fn index_rows() -> usize {
    ((484.0 - 16.0) / ROW) as usize
}

fn index_rect(list: Rect, first: usize, i: usize) -> Option<Rect> {
    (i >= first && i < first + index_rows()).then(|| {
        (
            list.0 + 4.0,
            list.1 + 8.0 + (i - first) as f32 * ROW,
            list.2 - 8.0,
            ROW,
        )
    })
}

/// An index list's tab label.
fn index_label(tab: Tab) -> &'static str {
    match tab {
        Tab::Area => "AREAS",
        Tab::Arms => "ARMOURY",
        _ => "ANIMALS",
    }
}

impl Menu {
    /// The index's pointer targets: its lists' tabs and its rows.
    pub(super) fn index_hits(&self, v: &TermView) -> Vec<(Rect, Hit)> {
        let g = self.geom();
        let tab = v.term.index_tab();
        let first = v.index_first[tab.index()];
        let tabs = INDEX_TABS
            .iter()
            .enumerate()
            .map(|(i, &t)| (g.tab(i), Hit::Tab(t)));
        let rows = (0..v.term.cat.list(tab).len())
            .filter_map(|i| Some((index_rect(g.index_list(), first, i)?, Hit::Row(i))));
        tabs.chain(rows).collect()
    }

    /// A picture fitted into `w` x `max_h` at (`x`, `y`), framed, or the
    /// placeholder frame when there is none; the height it took.
    pub(super) fn term_picture(
        &self,
        out: &mut Vec<Sprite>,
        pic: Option<usize>,
        (x, y, w, max_h): Rect,
    ) -> f32 {
        let pic = pic.and_then(|id| self.hud.image(id).map(|im| (id, im.w as f32, im.h as f32)));
        match pic {
            Some((id, pw, ph)) if pw > 0.0 && ph > 0.0 => {
                let k = (w / pw).min(max_h / ph);
                let (dw, dh) = (pw * k, ph * k);
                let px = x + (w - dw) / 2.0;
                self.image(out, Some(id), (px, y, dw, dh));
                self.term_box(out, (px - 1.0, y - 1.0, dw + 2.0, dh + 2.0), [0.0; 4], LINE);
                dh
            }
            _ => {
                let h = max_h.min(120.0);
                self.term_box(out, (x, y, w, h), [0.0, 0.0, 0.0, 0.3], FAINT);
                let s = "NO IMAGE ON FILE";
                let tx = x + (w - self.text_width(SMALL, s)) / 2.0;
                self.text(out, tx, y + h / 2.0 - 6.0, SMALL, FAINT, s);
                h
            }
        }
    }

    /// The column's item behind catalog item `i` of list `kind`.
    pub(super) fn item_of(v: &TermView, kind: Tab, i: usize) -> Option<usize> {
        v.map[kind.index()].get(i).copied()
    }

    /// The picture of catalog item `i` of list `kind`.
    pub(super) fn pic_of(&self, v: &TermView, kind: Tab, i: usize) -> Option<usize> {
        let k = Self::item_of(v, kind, i)?;
        self.columns
            .get(kind.index())?
            .pics
            .get(k)
            .copied()
            .flatten()
    }

    /// The hunter's record, when one is signed in.
    pub(super) fn rec(&self) -> Option<&c2_core::record::Record> {
        self.me().map(|_| &self.record)
    }

    /// What the hunter's record says of catalog item `i` of list `kind`,
    /// in the index: kills of the animal or with the weapon, hunts on the
    /// area.
    fn tally(&self, t: &Terminal, kind: Tab, i: usize) -> (&'static str, usize) {
        let Some(rec) = self.rec() else {
            return ("KILLS", 0);
        };
        let Some(it) = t.cat.list(kind).get(i) else {
            return ("KILLS", 0);
        };
        match kind {
            Tab::Area => ("HUNTS", rec.hunts_on(&it.stem)),
            Tab::Arms => ("KILLS", rec.kills_with(&it.name).count()),
            _ => ("KILLS", rec.count_of(&it.name)),
        }
    }

    pub(super) fn draw_term_index(&self, out: &mut Vec<Sprite>, v: &TermView) {
        let t = &v.term;
        let g = self.geom();
        let kind = t.index_tab();
        for (i, &tab) in INDEX_TABS.iter().enumerate() {
            let r = g.tab(i);
            let label = format!("{} {}", i + 1, index_label(tab));
            let on = tab == kind;
            let line = if inside(self.cursor, r) { HI } else { LINE };
            self.term_box(out, r, if on { MD } else { PANEL }, line);
            let x = r.0 + (r.2 - self.text_width(PX, &label)) / 2.0;
            self.text(out, x, r.1 + 6.0, PX, if on { INV } else { MD }, &label);
        }
        let list = g.index_list();
        self.term_box(out, list, PANEL, LINE);
        let focus = t.index_row();
        let items = t.cat.list(kind);
        for (i, it) in items.iter().enumerate() {
            let Some(r) = index_rect(list, v.index_first[kind.index()], i) else {
                continue;
            };
            let right = match self.tally(t, kind, i) {
                (_, 0) => String::new(),
                (what, n) => format!("{n} {what}"),
            };
            let (c, rc) = if i == focus {
                self.fill(out, r, LO);
                (INV, INV)
            } else {
                (MD, LO)
            };
            let rw = self.text_width(SMALL, &right);
            let label = format!("{:>2}  {}", i + 1, it.name.to_uppercase());
            self.term_text(out, r.0 + 6.0, r.1 + 4.0, SMALL, c, &label, r.2 - rw - 24.0);
            self.text(out, r.0 + r.2 - rw - 6.0, r.1 + 4.0, SMALL, rc, &right);
        }
        if items.is_empty() {
            let s = "NOTHING ON FILE";
            self.text(out, list.0 + 12.0, list.1 + 12.0, SMALL, LO, s);
            return;
        }
        // The focused item: its picture and the file's headline.
        let d = g.index_detail();
        self.term_box(out, d, PANEL, LINE);
        let Some(it) = items.get(focus) else {
            return;
        };
        let (x, w) = (d.0 + 10.0, d.2 - 20.0);
        let mut y = d.1 + 10.0;
        let pic = self.pic_of(v, kind, focus);
        y += self.term_picture(out, pic, (x, y, w, 220.0)) + 14.0;
        self.term_text(out, x, y, PX, HI, &it.name.to_uppercase(), w);
        y += 26.0;
        let price = if kind == Tab::Arms { "PRICE" } else { "PERMIT" };
        let mut lines = vec![(price, format!("{} PTS", it.price))];
        if kind == Tab::Targets {
            let k = Self::item_of(v, kind, focus);
            if let Some(s) = k.and_then(|k| self.specimens.facts.get(k)?.score) {
                lines.push(("BOUNTY", format!("{s} PTS")));
            }
        }
        let (what, n) = self.tally(t, kind, focus);
        lines.push((what, n.to_string()));
        for (label, value) in lines {
            self.text(out, x, y, SMALL, LO, label);
            self.text(out, x + VALUE_X, y, SMALL, MD, &value);
            y += LINE_H;
        }
        let open = if self.pad_used {
            "[A] OPEN FILE"
        } else {
            "[ENTER] OPEN FILE"
        };
        self.text(out, x, d.1 + d.3 - 26.0, SMALL, HI, open);
    }

    /// The retrieval sequence: its log so far over a progress bar.
    pub(super) fn draw_term_retrieval(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let x = self.geom().l + 44.0;
        self.text(out, x, 84.0, PX, HI, archive(t.file_kind()));
        let log = t.retrieve_log();
        for (i, l) in log.iter().take(t.retrieve_shown()).enumerate() {
            let c = if i + 1 == log.len() { HI } else { MD };
            self.text(out, x, 130.0 + i as f32 * 28.0, PX, c, l);
        }
        let p = (t.clock() / c2_core::dhc::specimen::RETRIEVE_S).clamp(0.0, 1.0);
        let bar = (200.0, 400.0, 400.0, 14.0);
        self.term_box(out, bar, PANEL, LINE);
        self.fill(
            out,
            (bar.0 + 2.0, bar.1 + 2.0, (bar.2 - 4.0) * p, bar.3 - 4.0),
            HI,
        );
    }
}
