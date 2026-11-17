//! Drawing an open file of the field manual: its picture or 3D model
//! (`viewer.rs`), its notes from the game's files, and its facts
//! (`c2_core::dhc::{specimen, survey, armoury}`), scrolled. A fact that
//! opens another file (an area's animals and weapons, a weapon's areas) is
//! a link: hover or the arrows focus it, a click or Enter opens it.

use c2_core::dhc::specimen::{native_areas, Line, Specimen};
use c2_core::dhc::{survey, Tab};

use super::draw::*;
use super::files::{LINE_H, VALUE_X};
use super::*;

/// The first fact line's top, below the file's name.
const FACTS_Y: f32 = 42.0;

/// A source line at least this long was broken to fit the original
/// menu's panel: the next line goes on with its paragraph.
const BROKEN: usize = 30;

/// A `Name   : value` line of a description's header.
fn is_field(l: &str) -> bool {
    l.find(':').is_some_and(|c| c <= 10)
}

/// The game's text files break their lines for the original menu's
/// panel: prose broken that way is one paragraph again, to wrap to the
/// notes panel. Short lines (a title, the `Name : value` fields) and blank
/// lines stay as they are.
fn paragraphs(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for l in lines.iter().map(|l| l.trim()) {
        match out.last_mut() {
            Some(p) if open && !l.is_empty() && !is_field(l) => {
                p.push(' ');
                p.push_str(l);
            }
            _ => out.push(l.to_string()),
        }
        open = l.chars().count() >= BROKEN && !is_field(l);
    }
    out
}

/// File lines that fit under the name in the facts panel.
fn file_rows() -> usize {
    ((484.0 - 52.0) / LINE_H) as usize
}

impl Menu {
    /// Where the 3D model draws this frame, in window pixels (none: no
    /// file showing, or it shows the picture).
    pub(in crate::menu) fn viewer_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let v = self.term.as_ref()?;
        let shows = self.screen == Screen::Terminal
            && v.term.screen == TScreen::Specimen
            && !v.term.retrieving()
            && self.viewer.is_some();
        if !shows {
            return None;
        }
        let (s, ox, oy) = self.canvas();
        let r = self.geom().file_pic();
        let (x, y, w, h) = (r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 22.0);
        Some((ox + x * s, oy + y * s, w * s, h * s))
    }

    /// Whether figures are in US units for the hunter signed in.
    fn us_units(&self) -> bool {
        self.me().is_some_and(|p| p.options.sys != 0)
    }

    /// The shown file's lines.
    fn file_lines(&self, v: &TermView) -> Vec<Line> {
        let t = &v.term;
        let (kind, i) = (t.file_kind(), t.file());
        let Some(k) = Self::item_of(v, kind, i) else {
            return Vec::new();
        };
        let (rec, us) = (self.rec(), self.us_units());
        match kind {
            Tab::Area => {
                let appear = self.specimens.appear.get(k).map_or(&[][..], |a| &a[..]);
                let unknown = survey::unknown(&t.cat, appear);
                survey::lines(&t.cat, &t.rules, i, unknown, rec)
            }
            Tab::Arms => self
                .specimens
                .arms
                .get(k)
                .map(|a| a.lines(&t.cat, &t.rules, i, rec, us))
                .unwrap_or_default(),
            _ => {
                let Some(f) = self.specimens.facts.get(k) else {
                    return Vec::new();
                };
                // The column's area items as the catalog's.
                let cat_area = |col: &usize| v.map[0].iter().position(|x| x == col);
                let s = Specimen {
                    homes: f
                        .homes
                        .as_ref()
                        .map(|h| h.iter().filter_map(cat_area).collect()),
                    appears: f.appears.iter().filter_map(cat_area).collect(),
                    ..f.clone()
                };
                let native = native_areas(&t.cat, &t.rules, i, s.homes.as_deref());
                let areas: Vec<String> =
                    t.cat.areas.iter().map(|a| a.name.to_uppercase()).collect();
                s.lines(&areas, native.as_deref(), rec, us)
            }
        }
    }

    /// The first file line shown: the scroll, kept within the file and,
    /// for `keep_link`, scrolled to show the focused link.
    pub(super) fn file_first(&self, v: &TermView, keep_link: bool) -> usize {
        let lines = self.file_lines(v);
        let rows = file_rows();
        let mut first = v.file_scroll;
        let at = v.term.file_link().and_then(|k| {
            lines
                .iter()
                .position(|l| matches!(l, Line::Link(j, _) if *j == k))
        });
        if let Some(at) = at.filter(|_| keep_link) {
            first = first.clamp((at + 1).saturating_sub(rows), at);
        }
        first.min(lines.len().saturating_sub(rows))
    }

    /// The facts panel's line `row` (from the first shown), as a rect.
    fn fact_rect(&self, row: usize) -> Rect {
        let f = self.geom().file_facts();
        (
            f.0 + 8.0,
            f.1 + FACTS_Y - 2.0 + row as f32 * LINE_H,
            f.2 - 16.0,
            LINE_H,
        )
    }

    /// The open file's pointer targets: its links shown.
    pub(super) fn file_hits(&self, v: &TermView) -> Vec<(Rect, Hit)> {
        if v.term.retrieving() {
            return Vec::new();
        }
        let first = self.file_first(v, false);
        self.file_lines(v)
            .iter()
            .skip(first)
            .take(file_rows())
            .enumerate()
            .filter_map(|(row, l)| match l {
                Line::Link(k, _) => Some((self.fact_rect(row), Hit::Row(*k))),
                _ => None,
            })
            .collect()
    }

    pub(super) fn draw_term_file(&self, out: &mut Vec<Sprite>, v: &TermView) {
        let t = &v.term;
        if t.retrieving() {
            self.draw_term_retrieval(out, t);
            return;
        }
        let g = self.geom();
        let (kind, i) = (t.file_kind(), t.file());
        let k = Self::item_of(v, kind, i);
        let name = t.cat.list(kind).get(i).map_or("", |a| a.name.as_str());
        let (title, model, notes_head) = match kind {
            Tab::Area => ("SURVEY", "TERRAIN MODEL", "SURVEY NOTES"),
            Tab::Arms => ("ARMOURY FILE", "3D SCHEMATIC", "ARMOURY NOTES"),
            _ => ("SPECIMEN FILE", "3D SPECIMEN", "FIELD NOTES"),
        };
        // The picture panel: the 3D model (drawn after the sprites,
        // `viewer.rs`) or the picture.
        let p = g.file_pic();
        self.term_box(out, p, PANEL, LINE);
        if self.viewer.is_some() {
            self.fill(
                out,
                (p.0 + 2.0, p.1 + 2.0, p.2 - 4.0, p.3 - 22.0),
                [0.0, 0.0, 0.0, 0.6],
            );
            let s = format!("{model}  DRAG: TURN  WHEEL: ZOOM");
            self.term_text(out, p.0 + 8.0, p.1 + p.3 - 16.0, SMALL, LO, &s, p.2 - 16.0);
        } else {
            let pic_r = (p.0 + 10.0, p.1 + 10.0, p.2 - 20.0, p.3 - 20.0);
            self.term_picture(out, self.pic_of(v, kind, i), pic_r);
        }
        // The description from the game's files.
        let d = g.file_text();
        self.term_box(out, d, PANEL, LINE);
        self.text(out, d.0 + 10.0, d.1 + 8.0, SMALL, HI, notes_head);
        let us = self.us_units();
        let sp = &self.specimens;
        let texts = match (kind, us) {
            (Tab::Area, _) => &sp.area_texts,
            (Tab::Arms, _) => &sp.arm_texts,
            (_, false) => &sp.texts,
            (_, true) => &sp.texts_us,
        };
        let notes = k.and_then(|k| texts.get(k)).map_or(&[][..], |v| &v[..]);
        let mut y = d.1 + 28.0;
        for l in wrap(&self.hud, SMALL, d.2 - 20.0, &paragraphs(notes)) {
            if y > d.1 + d.3 - 16.0 {
                break;
            }
            self.text(out, d.0 + 10.0, y, SMALL, LO, &l);
            y += 15.0;
        }
        // The facts, scrolled.
        let f = g.file_facts();
        self.term_box(out, f, PANEL, LINE);
        let (x, w) = (f.0 + 12.0, f.2 - 24.0);
        let head = format!("{title}: {}", name.to_uppercase());
        self.term_text(out, x, f.1 + 10.0, PX, HI, &head, w);
        self.fill(out, (x, f.1 + 32.0, w, 1.0), LINE);
        let lines = self.file_lines(v);
        let first = self.file_first(v, false);
        let focus = t.file_link();
        for (row, l) in lines.iter().skip(first).take(file_rows()).enumerate() {
            let y = f.1 + FACTS_Y + row as f32 * LINE_H;
            match l {
                Line::Head(h) => self.text(out, x, y, SMALL, AMBER, h),
                Line::Fact(label, value) => {
                    self.text(out, x + 8.0, y, SMALL, LO, label);
                    self.term_text(out, x + VALUE_X, y, SMALL, MD, value, w - VALUE_X);
                }
                Line::Item(s) => self.term_text(out, x + 8.0, y, SMALL, MD, s, w - 8.0),
                Line::Link(k, s) => {
                    let c = if focus == Some(*k) {
                        self.fill(out, self.fact_rect(row), LO);
                        INV
                    } else {
                        HI
                    };
                    self.term_text(out, x + 8.0, y, SMALL, c, &format!("> {s}"), w - 8.0);
                }
                Line::Blank => {}
            }
        }
        if first + file_rows() < lines.len() {
            self.text(out, f.0 + f.2 - 24.0, f.1 + f.3 - 18.0, SMALL, LO, "v");
        }
        if first > 0 {
            self.text(out, f.0 + f.2 - 24.0, f.1 + 40.0, SMALL, LO, "^");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::paragraphs;

    #[test]
    fn notes_join_into_paragraphs_under_their_title() {
        let owned = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
        let area = owned(&[
            "Fort Ciskin",
            "",
            "A good sized area consisting of dense ",
            "pine forests encircling a murky swamp. ",
            "In the southwest corner lies an abandoned ",
            "",
            "Mild difficulty.",
        ]);
        assert_eq!(
            paragraphs(&area),
            owned(&[
                "Fort Ciskin",
                "",
                "A good sized area consisting of dense pine forests encircling a murky \
                 swamp. In the southwest corner lies an abandoned",
                "",
                "Mild difficulty.",
            ])
        );
        // A header of fields keeps its lines, however long.
        let fields = owned(&[
            "Weapon : Double-barelled shotgun with a long name",
            "Size   : length 5-8 meters",
            "Weight : up to 3 tonns",
            "",
            " Not dangerous.",
        ]);
        let want = owned(&[
            "Weapon : Double-barelled shotgun with a long name",
            "Size   : length 5-8 meters",
            "Weight : up to 3 tonns",
            "",
            "Not dangerous.",
        ]);
        assert_eq!(paragraphs(&fields), want);
    }
}
