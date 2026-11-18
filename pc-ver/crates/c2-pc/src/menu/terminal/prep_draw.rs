//! Drawing the Advanced DHC OS UI's map stage (spec §2, §3, §8; ref1):
//! the loadout record at the left, the carousel of areas in the middle and
//! the highlighted area's survey at the right, under a title with the
//! account and over the points remaining. Pointer targets come from the
//! same layout ([`Menu::prep_hits`]).

use c2_core::dhc::prep::{Prep, Stage};
use c2_core::dhc::specimen::issued_on;
use c2_core::dhc::terminal::Terminal;
use c2_core::dhc::{survey, Tab};

use super::draw::{PX, SMALL};
use super::geom::Geom;
use super::palette::*;
use super::prep::PrepHit;
use super::*;

/// The colour language (spec §8); outlines stay the terminal's green.
const BLUE: [f32; 4] = [0.45, 0.72, 1.0, 1.0];
const WHITE: [f32; 4] = [0.93, 0.96, 0.94, 1.0];
const YELLOW: [f32; 4] = [1.0, 0.86, 0.3, 1.0];
const ORANGE: [f32; 4] = [1.0, 0.62, 0.2, 1.0];
const BUTTON: [f32; 4] = [0.62, 0.13, 0.13, 1.0];
const BUTTON_HI: [f32; 4] = [0.78, 0.2, 0.2, 1.0];

const TOP: f32 = 72.0;
const PANEL_H: f32 = 404.0;
const RECORD_W: f32 = 176.0;
const INFO_W: f32 = 252.0;
const GAP: f32 = 10.0;
/// The centred item's share of the carousel's width; a neighbour is this
/// much smaller, and shows only its inner edge.
const ITEM_W: f32 = 0.62;
const SIDE_SCALE: f32 = 0.6;
const PEEK: f32 = 0.3;

/// The points remaining's colour: orange with the whole balance left,
/// reaching red as the share left falls (spec §8, linear between them).
pub(super) fn points_colour(score: i32, remaining: i32) -> [f32; 4] {
    let left = c2_core::dhc::prep::points_left(score, remaining);
    let mut c = RED;
    for k in 0..3 {
        c[k] = RED[k] + (ORANGE[k] - RED[k]) * left;
    }
    c
}

/// Where the map stage's panels go.
#[derive(Clone, Copy)]
pub(super) struct PrepGeom {
    pub record: Rect,
    pub carousel: Rect,
    pub info: Rect,
    pub bottom: Rect,
}

impl PrepGeom {
    pub(super) fn new(g: Geom) -> Self {
        let mid_w = g.w - RECORD_W - INFO_W - 2.0 * GAP;
        PrepGeom {
            record: (g.l, TOP, RECORD_W, PANEL_H),
            carousel: (g.l + RECORD_W + GAP, TOP, mid_w, PANEL_H),
            info: (g.right() - INFO_W, TOP, INFO_W, PANEL_H),
            bottom: (g.l, TOP + PANEL_H + 8.0, g.w, 76.0),
        }
    }

    /// The centred item's box (and the 3D preview's).
    pub(super) fn centre(self) -> Rect {
        let c = self.carousel;
        let w = c.2 * ITEM_W;
        let h = (w * 0.75).min(c.3 - 110.0);
        (c.0 + (c.2 - w) / 2.0, c.1 + 44.0, w, h)
    }

    /// Item at `offset` items from the centre: its box, and how lit it is
    /// (none: off the carousel).
    pub(super) fn item(self, offset: f32) -> Option<(Rect, f32)> {
        let (c, mid) = (self.carousel, self.centre());
        let a = offset.abs();
        if a >= 2.0 {
            return None;
        }
        let near = a.min(1.0);
        let scale = 1.0 - (1.0 - SIDE_SCALE) * near;
        let (w, h) = (mid.2 * scale, mid.3 * scale);
        let side_w = mid.2 * SIDE_SCALE;
        // A neighbour's centre sits past the carousel's edge, so only
        // PEEK of it shows.
        let reach = c.2 / 2.0 + side_w * (0.5 - PEEK);
        let x = c.0 + c.2 / 2.0 + offset * reach - w / 2.0;
        let y = mid.1 + (mid.3 - h) / 2.0;
        let lit = if a <= 1.0 {
            1.0 - 0.65 * a
        } else {
            0.35 * (2.0 - a)
        };
        Some(((x, y, w, h), lit))
    }

    pub(super) fn prev(self) -> Rect {
        let c = self.carousel;
        (c.0 + 2.0, c.1 + c.3 / 2.0 - 40.0, 26.0, 52.0)
    }

    pub(super) fn next(self) -> Rect {
        let c = self.carousel;
        (c.0 + c.2 - 28.0, c.1 + c.3 / 2.0 - 40.0, 26.0, 52.0)
    }

    pub(super) fn button(self) -> Rect {
        let p = self.info;
        (p.0 + 10.0, p.1 + p.3 - 44.0, p.2 - 20.0, 34.0)
    }
}

impl Menu {
    fn prep_geom(&self) -> PrepGeom {
        PrepGeom::new(self.geom())
    }

    /// The map stage's pointer targets: the centred map and the button
    /// commit, a neighbour is highlighted, the chevrons browse.
    pub(super) fn prep_hits(&self) -> Vec<(Rect, PrepHit)> {
        let Some(v) = &self.term else {
            return Vec::new();
        };
        let (pg, t) = (self.prep_geom(), &v.term);
        let mut out = vec![
            (self.geom().back(), PrepHit::Back),
            (pg.button(), PrepHit::Use),
            (pg.prev(), PrepHit::Prev),
            (pg.next(), PrepHit::Next),
        ];
        let n = t.cat.areas.len();
        let h = v.prep.highlight(t, Stage::Map);
        let c = pg.carousel;
        for i in 0..n {
            let Some((r, _)) = pg.item(v.prep.offset(Stage::Map, n, i)) else {
                continue;
            };
            let (x0, x1) = (r.0.max(c.0), (r.0 + r.2).min(c.0 + c.2));
            if x1 > x0 {
                let hit = if i == h {
                    PrepHit::Use
                } else {
                    PrepHit::Item(i)
                };
                out.push(((x0, r.1, x1 - x0, r.3), hit));
            }
        }
        out
    }

    /// The 3D preview's window rectangle, while it shows.
    pub(in crate::menu) fn prep_view_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let v = self.term.as_ref()?;
        if !self.prep_shown() || !v.prep.settled(&v.term, Stage::Map) {
            return None;
        }
        let (s, ox, oy) = self.canvas();
        let r = self.prep_geom().centre();
        Some((ox + r.0 * s, oy + r.1 * s, r.2 * s, r.3 * s))
    }

    /// Picture `id` fitted into `r` (aspect kept, centred), cut to the
    /// columns `clip` spans, tinted `k` (1: as drawn).
    fn prep_picture(
        &self,
        out: &mut Vec<Sprite>,
        id: Option<usize>,
        r: Rect,
        clip: (f32, f32),
        k: f32,
    ) {
        let Some((id, pw, ph)) =
            id.and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
        else {
            self.prep_clipped_fill(out, r, clip, [0.0, 0.0, 0.0, 0.5 * k]);
            return;
        };
        if pw <= 0.0 || ph <= 0.0 {
            return;
        }
        let f = (r.2 / pw).min(r.3 / ph);
        let (w, h) = (pw * f, ph * f);
        let (x, y) = (r.0 + (r.2 - w) / 2.0, r.1 + (r.3 - h) / 2.0);
        let (x0, x1) = (x.max(clip.0), (x + w).min(clip.1));
        if x1 <= x0 {
            return;
        }
        let (s, ox, oy) = self.canvas();
        out.push(Sprite {
            size: Some([(x1 - x0) * s, h * s]),
            uv: [(x0 - x) / w, 0.0, (x1 - x) / w, 1.0],
            color: [k, k, k, 1.0],
            ..Sprite::new(id, ox + x0 * s, oy + y * s, 1.0)
        });
    }

    fn prep_clipped_fill(&self, out: &mut Vec<Sprite>, r: Rect, clip: (f32, f32), c: [f32; 4]) {
        let (x0, x1) = (r.0.max(clip.0), (r.0 + r.2).min(clip.1));
        if x1 > x0 {
            self.fill(out, (x0, r.1, x1 - x0, r.3), c);
        }
    }

    /// The map stage.
    pub(super) fn draw_prep(&self, out: &mut Vec<Sprite>) {
        let Some(v) = &self.term else { return };
        let (t, prep) = (&v.term, &v.prep);
        let (g, pg) = (self.geom(), self.prep_geom());
        self.term_frame(out);
        self.prep_title(out, t, prep, g);
        self.prep_carousel(out, v, pg);
        self.prep_record(out, t, pg.record);
        self.prep_info(out, v, pg);
        self.prep_bottom(out, t, pg);
        let hints = if self.pad_used {
            "[B] BACK   D-PAD BROWSE   [A]/[R] USE THIS MAP"
        } else {
            "[ESC] BACK   LEFT/RIGHT or wheel BROWSE   ENTER USE THIS MAP"
        };
        self.term_status(out, hints, &format!("CREDITS {}", t.score));
    }

    /// The stages along the top, the one shown lit; the account at the
    /// right in yellow.
    fn prep_title(&self, out: &mut Vec<Sprite>, t: &Terminal, prep: &Prep, g: Geom) {
        let mut x = g.l;
        self.text(out, x, 46.0, PX, HI, "HUNT PREPARATION //");
        x += self.text_width(PX, "HUNT PREPARATION //") + 10.0;
        for s in Stage::ALL {
            let label = match s {
                Stage::Map => "MAP",
                Stage::Dinos => "DINOSAURS",
                Stage::Guns => "GUNS",
                Stage::Gear => "EQUIPMENT",
            };
            let c = if s == prep.stage { HI } else { FAINT };
            self.text(out, x, 48.0, SMALL, c, label);
            x += self.text_width(SMALL, label) + 8.0;
            if s.next().is_some() {
                self.text(out, x, 48.0, SMALL, FAINT, ">");
                x += self.text_width(SMALL, ">") + 8.0;
            }
        }
        let total = format!("TOTAL POINTS: {}", t.score);
        let w = self.text_width(PX, &total);
        self.text(out, g.right() - w, 46.0, PX, YELLOW, &total);
    }

    /// The carousel: neighbours first (farthest first), cut to the panel
    /// so only their inner edges show; then the centred map, as its relief
    /// when it is ready (drawn after the sprites, `scene.rs`) or its
    /// picture.
    fn prep_carousel(&self, out: &mut Vec<Sprite>, v: &TermView, pg: PrepGeom) {
        let (t, prep) = (&v.term, &v.prep);
        let c = pg.carousel;
        self.term_box(out, c, PANEL, LINE);
        let n = t.cat.areas.len();
        let x = c.0 + c.2 / 2.0;
        let head = format!("AREA SURVEYS  {}/{}", prep.highlight(t, Stage::Map) + 1, n);
        self.text(
            out,
            x - self.text_width(SMALL, &head) / 2.0,
            c.1 + 12.0,
            SMALL,
            LO,
            &head,
        );
        if n == 0 {
            let s = "NOTHING ON FILE";
            self.text(
                out,
                x - self.text_width(SMALL, s) / 2.0,
                c.1 + c.3 / 2.0,
                SMALL,
                LO,
                s,
            );
            return;
        }
        let clip = (c.0 + 1.0, c.0 + c.2 - 1.0);
        let mut order: Vec<(usize, f32)> =
            (0..n).map(|i| (i, prep.offset(Stage::Map, n, i))).collect();
        order.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        let h = prep.highlight(t, Stage::Map);
        let settled = prep.settled(t, Stage::Map);
        let pics = &self.columns[0].pics;
        for (i, off) in order {
            let Some((r, lit)) = pg.item(off) else {
                continue;
            };
            let pic = v.map[0]
                .get(i)
                .and_then(|&k| pics.get(k).copied().flatten());
            let centred = i == h && settled;
            if centred && v.previews_ready() {
                // The relief turns here; a dark stage behind it.
                self.fill(out, r, [0.0, 0.0, 0.0, 0.55]);
            } else {
                self.prep_picture(out, pic, r, clip, lit);
            }
            if i == h && off.abs() < 0.5 {
                self.term_box(
                    out,
                    (r.0 - 1.0, r.1 - 1.0, r.2 + 2.0, r.3 + 2.0),
                    [0.0; 4],
                    if settled { HI } else { LINE },
                );
            }
        }
        // Under the centred map: its name, and whether it is the one in
        // the record.
        let mid = pg.centre();
        let name = t.cat.areas[h.min(n - 1)].name.to_uppercase();
        let y = mid.1 + mid.3 + 12.0;
        self.term_text(
            out,
            x - self.text_width(PX, &name).min(c.2 - 20.0) / 2.0,
            y,
            PX,
            WHITE,
            &name,
            c.2 - 20.0,
        );
        let tag = if h == t.area {
            ("IN YOUR LOADOUT", HI)
        } else if v.previews_building(h) {
            ("RENDERING SURVEY...", LO)
        } else {
            ("", LO)
        };
        self.text(
            out,
            x - self.text_width(SMALL, tag.0) / 2.0,
            y + 22.0,
            SMALL,
            tag.1,
            tag.0,
        );
        for (r, label) in [(pg.prev(), "<"), (pg.next(), ">")] {
            let c = if inside(self.cursor, r) { HI } else { MD };
            self.text(
                out,
                r.0 + (r.2 - self.text_width(PX, label)) / 2.0,
                r.1 + 18.0,
                PX,
                c,
                label,
            );
        }
    }

    /// The loadout record: map, dinosaurs (blue), guns (white), equipment.
    fn prep_record(&self, out: &mut Vec<Sprite>, t: &Terminal, r: Rect) {
        self.fill(out, r, BG);
        self.term_box(out, r, PANEL, LINE);
        let (x, w) = (r.0 + 10.0, r.2 - 20.0);
        let mut y = r.1 + 8.0;
        self.text(out, x, y, SMALL, LO, "LOADOUT RECORD");
        y += 22.0;
        let map = t.cat.areas.get(t.area).map(|a| a.name.clone());
        let sections: [(&str, Tab, [f32; 4]); 4] = [
            ("MAP", Tab::Area, WHITE),
            ("DINOSAURS", Tab::Targets, BLUE),
            ("GUNS", Tab::Arms, WHITE),
            ("EQUIPMENT", Tab::Gear, MD),
        ];
        let bottom = r.1 + r.3 - 8.0;
        for (k, (head, tab, colour)) in sections.into_iter().enumerate() {
            if k > 0 {
                self.fill(out, (x, y, w, 1.0), LINE);
                y += 8.0;
            }
            self.text(out, x, y, SMALL, LO, head);
            y += 16.0;
            let names: Vec<String> = match tab {
                Tab::Area => map.iter().cloned().collect(),
                _ => (0..t.cat.list(tab).len())
                    .filter(|&i| t.picked(tab, i))
                    .map(|i| t.cat.list(tab)[i].name.clone())
                    .collect(),
            };
            // Each list keeps room for the ones after it.
            let room = ((bottom - y - (3 - k) as f32 * 40.0) / 16.0)
                .floor()
                .max(1.0) as usize;
            if names.is_empty() {
                self.text(out, x, y, SMALL, FAINT, "Pending");
                y += 16.0;
            }
            let shown = if names.len() > room {
                room.saturating_sub(1)
            } else {
                names.len()
            };
            for n in &names[..shown] {
                self.term_text(out, x, y, SMALL, colour, n, w);
                y += 16.0;
            }
            if shown < names.len() {
                self.text(
                    out,
                    x,
                    y,
                    SMALL,
                    LO,
                    &format!("+{} MORE", names.len() - shown),
                );
                y += 16.0;
            }
            y += 4.0;
        }
    }

    /// The highlighted area's survey: the menu's picture, its name and
    /// permit, description, the dinosaurs (and how many are undiscovered),
    /// the weapons, and the USE THIS MAP button.
    fn prep_info(&self, out: &mut Vec<Sprite>, v: &TermView, pg: PrepGeom) {
        let t = &v.term;
        let p = pg.info;
        self.fill(out, p, BG);
        self.term_box(out, p, PANEL, LINE);
        let (x, w) = (p.0 + 10.0, p.2 - 20.0);
        self.text(out, x, p.1 + 8.0, SMALL, LO, "AREA SURVEY");
        let h = v.prep.highlight(t, Stage::Map);
        let (Some(it), Some(&k)) = (t.cat.areas.get(h), v.map[0].get(h)) else {
            return;
        };
        let col = &self.columns[0];
        let mut y = p.1 + 26.0;
        y += self.term_picture(out, col.pics.get(k).copied().flatten(), (x, y, w, 104.0)) + 8.0;
        self.term_text(out, x, y, PX, WHITE, &it.name.to_uppercase(), w);
        y += 18.0;
        self.text(out, x, y, SMALL, MD, &format!("PERMIT {} PTS", it.price));
        y += 16.0;
        let (allowed, afford) = (t.allowed(Tab::Area, h), t.affordable(Tab::Area, h));
        let status = if !allowed {
            ("RESTRICTED", AMBER)
        } else if h == t.area {
            ("IN YOUR LOADOUT", HI)
        } else if !afford {
            ("INSUFFICIENT FUNDS", RED)
        } else {
            ("AVAILABLE", MD)
        };
        self.text(out, x, y, SMALL, status.1, status.0);
        y += 18.0;
        let us = self.me().is_some_and(|s| s.options.sys != 0);
        let texts = if us { &col.texts_us } else { &col.texts };
        let desc = texts.get(k).map_or(&[][..], |d| &d[..]);
        let limit = pg.button().1 - 8.0;
        for l in wrap(&self.hud, SMALL, w, desc).iter().take(4) {
            self.text(out, x, y, SMALL, LO, l);
            y += 14.0;
        }
        y += 6.0;
        // The dinosaurs issued here, and the species with no permit that
        // may turn up: counted, never named (spec §3).
        let names = |tab: Tab| -> Option<Vec<String>> {
            issued_on(&t.cat, &t.rules, h, tab).map(|l| {
                l.iter()
                    .map(|&i| t.cat.list(tab)[i].name.trim().to_string())
                    .collect()
            })
        };
        let appear = self.specimens.appear.get(k).map_or(&[][..], |a| &a[..]);
        let unknown = survey::unknown(&t.cat, appear);
        for (head, tab, colour, all) in [
            ("DINOSAURS", Tab::Targets, BLUE, "ALL REGULAR SPECIES"),
            ("WEAPONS", Tab::Arms, WHITE, "ALL WEAPONS"),
        ] {
            if y + 30.0 > limit {
                break;
            }
            self.text(out, x, y, SMALL, LO, head);
            y += 15.0;
            let line = match names(tab) {
                None => vec![all.to_string()],
                Some(l) if l.is_empty() => vec!["NONE AUTHORISED".to_string()],
                Some(l) => wrap(&self.hud, SMALL, w, &[l.join(", ")]),
            };
            let room = if tab == Tab::Targets { 3 } else { 2 };
            for (i, l) in line.iter().enumerate().take(room) {
                let cut = i + 1 == room && line.len() > room;
                let l = if cut { format!("{l}...") } else { l.clone() };
                self.term_text(out, x, y, SMALL, colour, &l, w);
                y += 14.0;
            }
            if tab == Tab::Targets {
                let note = format!("Undiscovered dinosaurs on this map: {unknown}");
                for l in wrap(&self.hud, SMALL, w, &[note]) {
                    self.text(out, x, y, SMALL, MD, &l);
                    y += 14.0;
                }
            }
            y += 6.0;
        }
        let b = pg.button();
        let ok = allowed && afford;
        let fill = match (ok, inside(self.cursor, b)) {
            (false, _) => PANEL,
            (true, true) => BUTTON_HI,
            (true, false) => BUTTON,
        };
        self.term_box(out, b, fill, if ok { RED } else { LINE });
        let label = "USE THIS MAP";
        let tc = if ok { WHITE } else { FAINT };
        self.text(
            out,
            b.0 + (b.2 - self.text_width(PX, label)) / 2.0,
            b.1 + 11.0,
            PX,
            tc,
            label,
        );
    }

    /// The points remaining (orange towards red as the balance is spent),
    /// the permits and earnings, and the last refusal.
    fn prep_bottom(&self, out: &mut Vec<Sprite>, t: &Terminal, pg: PrepGeom) {
        let b = pg.bottom;
        self.term_box(out, b, PANEL, LINE);
        let left = t.remaining();
        let pts = format!("POINTS REMAINING: {left}");
        self.text(
            out,
            b.0 + 12.0,
            b.1 + 10.0,
            PX,
            points_colour(t.score, left),
            &pts,
        );
        let k = t.earnings();
        let line = format!("PERMITS -{}   EARNINGS x{k:.2}", t.spent());
        self.text(out, b.0 + 12.0, b.1 + 32.0, SMALL, WHITE, &line);
        if let Some(n) = t.notice {
            self.text(
                out,
                b.0 + 12.0,
                b.1 + 52.0,
                SMALL,
                AMBER,
                &format!("! {}", n.text()),
            );
        }
        let hint = if self.pad_used {
            "[A] / [R]  USE THIS MAP"
        } else {
            "ENTER / R  USE THIS MAP"
        };
        let w = self.text_width(SMALL, hint);
        self.text(out, b.0 + b.2 - w - 12.0, b.1 + 32.0, SMALL, MD, hint);
    }
}

impl TermView {
    /// The centred map's relief is ready to draw.
    pub(super) fn previews_ready(&self) -> bool {
        self.previews.ready()
    }

    pub(super) fn previews_building(&self, h: usize) -> bool {
        self.map[0]
            .get(h)
            .is_some_and(|&k| self.previews.building(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_go_from_orange_to_red_by_the_share_spent() {
        assert_eq!(points_colour(1000, 1000), ORANGE);
        assert_eq!(points_colour(1000, 0), RED);
        assert_eq!(points_colour(1000, -5), RED);
        assert_eq!(points_colour(0, 0), ORANGE);
        // 20,000 down to 2,000 warns more than 100 down to 80 (spec §8).
        let (big, small) = (points_colour(20_000, 2_000), points_colour(100, 80));
        assert!(big[1] < small[1], "{big:?} {small:?}");
    }

    #[test]
    fn the_panels_sit_side_by_side_above_the_status_bar() {
        for win in [800.0, 1600.0] {
            let pg = PrepGeom::new(Geom::for_width(win));
            let (r, c, i, b) = (pg.record, pg.carousel, pg.info, pg.bottom);
            assert!(r.0 + r.2 < c.0 && c.0 + c.2 < i.0, "{r:?} {c:?} {i:?}");
            assert!(c.1 + c.3 < b.1 && b.1 + b.3 <= 568.0);
            let mid = pg.centre();
            assert!(mid.0 > c.0 && mid.0 + mid.2 < c.0 + c.2 && mid.1 + mid.3 + 40.0 < c.1 + c.3);
            let btn = pg.button();
            assert!(btn.1 + btn.3 <= i.1 + i.3);
        }
    }

    #[test]
    fn neighbours_peek_in_from_the_edges_and_fade() {
        let pg = PrepGeom::new(Geom::for_width(800.0));
        let c = pg.carousel;
        let (centre, lit) = pg.item(0.0).unwrap_or_default();
        assert_eq!((centre, lit), (pg.centre(), 1.0));
        let (right, lit_r) = pg.item(1.0).unwrap_or_default();
        let shown = c.0 + c.2 - right.0;
        assert!(
            (shown - right.2 * PEEK).abs() < 0.5,
            "{shown} of {}",
            right.2
        );
        assert!(lit_r < 0.5);
        let (left, _) = pg.item(-1.0).unwrap_or_default();
        assert!((left.0 + left.2 - c.0 - right.2 * PEEK).abs() < 0.5);
        assert!(pg.item(2.0).is_none() && pg.item(-2.5).is_none());
    }
}
