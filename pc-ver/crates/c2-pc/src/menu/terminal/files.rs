//! Drawing the field manual (the specimen database): the index of animals
//! beside the focused one's picture, a file's retrieval sequence, and the
//! file itself (picture or 3D specimen, description and facts). What a
//! file says is `c2_core::dhc::specimen`; its model is `viewer.rs`.

use c2_core::dhc::specimen::{native_areas, Line, Specimen};
use c2_core::dhc::terminal::Terminal;

use super::draw::*;
use super::*;

/// Index rows.
const ROW: f32 = 20.0;
/// File lines, and where a fact's value starts after its label.
const LINE_H: f32 = 17.0;
const VALUE_X: f32 = 124.0;

/// The specimen index's rows that fit.
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

/// File lines that fit under the name in the facts panel.
fn file_rows() -> usize {
    ((484.0 - 52.0) / LINE_H) as usize
}

impl Menu {
    /// The specimen index's pointer targets: its rows.
    pub(super) fn index_hits(&self, v: &TermView) -> Vec<(Rect, Hit)> {
        let list = self.geom().index_list();
        (0..v.term.cat.animals.len())
            .filter_map(|i| Some((index_rect(list, v.index_first, i)?, Hit::Row(i))))
            .collect()
    }

    /// Where the 3D specimen draws this frame, in window pixels (none: no
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

    /// The animal column's item behind catalog animal `i`.
    fn item_of(v: &TermView, i: usize) -> Option<usize> {
        v.map[1].get(i).copied()
    }

    /// Kills of the animal on the hunter's record.
    fn kills_of(&self, name: &str) -> usize {
        self.me().map_or(0, |_| self.record.count_of(name))
    }

    pub(super) fn draw_term_index(&self, out: &mut Vec<Sprite>, v: &TermView) {
        let t = &v.term;
        let g = self.geom();
        let list = g.index_list();
        self.term_box(out, list, PANEL, LINE);
        let focus = t.specimen_row();
        for (i, it) in t.cat.animals.iter().enumerate() {
            let Some(r) = index_rect(list, v.index_first, i) else {
                continue;
            };
            let kills = self.kills_of(&it.name);
            let right = if kills > 0 {
                format!("{kills} KILLS")
            } else {
                String::new()
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
        if t.cat.animals.is_empty() {
            self.text(
                out,
                list.0 + 12.0,
                list.1 + 12.0,
                SMALL,
                LO,
                "NO SPECIMENS ON FILE",
            );
            return;
        }
        // The focused animal: its picture and the file's headline.
        let d = g.index_detail();
        self.term_box(out, d, PANEL, LINE);
        let Some(k) = Self::item_of(v, focus) else {
            return;
        };
        let (x, w) = (d.0 + 10.0, d.2 - 20.0);
        let pic = self
            .columns
            .get(1)
            .and_then(|c| c.pics.get(k).copied().flatten());
        let mut y = d.1 + 10.0;
        y += self.term_picture(out, pic, (x, y, w, 220.0)) + 14.0;
        let it = &t.cat.animals[focus];
        self.term_text(out, x, y, PX, HI, &it.name.to_uppercase(), w);
        y += 26.0;
        let fact = self.specimens.facts.get(k);
        let mut lines = vec![("PERMIT", format!("{} PTS", it.price))];
        if let Some(s) = fact.and_then(|f| f.score) {
            lines.push(("BOUNTY", format!("{s} PTS")));
        }
        lines.push(("KILLS", self.kills_of(&it.name).to_string()));
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

    /// The open file's facts, with its areas as catalog indices, and its
    /// lines.
    fn file_lines(&self, v: &TermView) -> Vec<Line> {
        let t = &v.term;
        let Some(f) = Self::item_of(v, t.file()).and_then(|k| self.specimens.facts.get(k)) else {
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
        let native = native_areas(&t.cat, &t.rules, t.file(), s.homes.as_deref());
        let areas: Vec<String> = t.cat.areas.iter().map(|a| a.name.to_uppercase()).collect();
        let us = self.me().is_some_and(|p| p.options.sys != 0);
        let rec = self.me().map(|_| &self.record);
        s.lines(&areas, native.as_deref(), rec, us)
    }

    /// The first file line shown: the scroll, kept within the file.
    pub(super) fn file_first(&self, v: &TermView) -> usize {
        let n = self.file_lines(v).len();
        v.file_scroll.min(n.saturating_sub(file_rows()))
    }

    pub(super) fn draw_term_file(&self, out: &mut Vec<Sprite>, v: &TermView) {
        let t = &v.term;
        if t.retrieving() {
            self.draw_term_retrieval(out, t);
            return;
        }
        let g = self.geom();
        let k = Self::item_of(v, t.file());
        let name = t.cat.animals.get(t.file()).map_or("", |a| a.name.as_str());
        // The picture panel: the 3D specimen (drawn after the sprites,
        // `viewer.rs`) or the picture.
        let p = g.file_pic();
        self.term_box(out, p, PANEL, LINE);
        if self.viewer.is_some() {
            self.fill(
                out,
                (p.0 + 2.0, p.1 + 2.0, p.2 - 4.0, p.3 - 22.0),
                [0.0, 0.0, 0.0, 0.6],
            );
            let s = "3D SPECIMEN  DRAG: TURN  WHEEL: ZOOM";
            self.term_text(out, p.0 + 8.0, p.1 + p.3 - 16.0, SMALL, LO, s, p.2 - 16.0);
        } else {
            let pic = k.and_then(|k| self.columns.get(1)?.pics.get(k).copied().flatten());
            let pic_r = (p.0 + 10.0, p.1 + 10.0, p.2 - 20.0, p.3 - 20.0);
            self.term_picture(out, pic, pic_r);
        }
        // The description from the game's files.
        let d = g.file_text();
        self.term_box(out, d, PANEL, LINE);
        self.text(out, d.0 + 10.0, d.1 + 8.0, SMALL, HI, "FIELD NOTES");
        let us = self.me().is_some_and(|p| p.options.sys != 0);
        let texts = if us {
            &self.specimens.texts_us
        } else {
            &self.specimens.texts
        };
        let notes = k.and_then(|k| texts.get(k)).map_or(&[][..], |v| &v[..]);
        let mut y = d.1 + 28.0;
        for l in wrap(&self.hud, SMALL, d.2 - 20.0, notes) {
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
        self.term_text(
            out,
            x,
            f.1 + 10.0,
            PX,
            HI,
            &format!("SPECIMEN FILE: {}", name.to_uppercase()),
            w,
        );
        self.fill(out, (x, f.1 + 32.0, w, 1.0), LINE);
        let lines = self.file_lines(v);
        let first = self.file_first(v);
        let mut y = f.1 + 42.0;
        for l in lines.iter().skip(first).take(file_rows()) {
            match l {
                Line::Head(h) => self.text(out, x, y, SMALL, AMBER, h),
                Line::Fact(label, value) => {
                    self.text(out, x + 8.0, y, SMALL, LO, label);
                    self.term_text(out, x + VALUE_X, y, SMALL, MD, value, w - VALUE_X);
                }
                Line::Item(s) => self.term_text(out, x + 8.0, y, SMALL, MD, s, w - 8.0),
                Line::Blank => {}
            }
            y += LINE_H;
        }
        if first + file_rows() < lines.len() {
            self.text(out, f.0 + f.2 - 24.0, f.1 + f.3 - 18.0, SMALL, LO, "v");
        }
        if first > 0 {
            self.text(out, f.0 + f.2 - 24.0, f.1 + 40.0, SMALL, LO, "^");
        }
    }

    /// The retrieval sequence: its log so far over a progress bar.
    fn draw_term_retrieval(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let x = self.geom().l + 44.0;
        self.text(out, x, 84.0, PX, HI, "DHC SPECIMEN DATABASE");
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
