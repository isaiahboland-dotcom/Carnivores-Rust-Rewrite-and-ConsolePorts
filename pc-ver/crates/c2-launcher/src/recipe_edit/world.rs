//! The recipe editor's Advanced page: the map editor for world zones
//! (`[settings.world]`, `c2_mods::world`). Zones (circles and polygons),
//! player starts and extraction points are drawn on an area's overview
//! picture, the one the game's map screen shows. It writes only the recipe;
//! the picture is read from the instance's game off the UI thread. The edits
//! themselves are in [`super::world_edit`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::overview::SIZE;
use c2_formats::world::GAP_RANGE;
use c2_formats::zone::RADIUS_RANGE;
use c2_mods::recipe::Recipe;
use c2_mods::world::{Marker, World, Zone};
use eframe::egui::{self, RichText};

use super::instance::{instance, Extras};
use super::world_edit::{self as edit, Carry, Grip, Target, SIDE};
use super::world_map::map_ui;
use crate::app::help;
use crate::jobs::balance::Instance;
use crate::theme;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Select,
    Circle,
    Polygon,
    Start,
    Extraction,
}

impl Tool {
    const ALL: [(Tool, &'static str, &'static str); 5] = [
        (
            Tool::Select,
            "Select / move",
            "Click an entry to select it; drag its squares to move or reshape it. Drag inside a \
             polygon to move it, drag from its edge to add a corner, right-click a corner to \
             remove it.",
        ),
        (
            Tool::Circle,
            "Circle",
            "Press at the centre and drag out to the rim.",
        ),
        (
            Tool::Polygon,
            "Polygon",
            "Click each corner in turn; click the first corner again (or Finish) to close it.",
        ),
        (Tool::Start, "Player start", "Click where a hunt may begin."),
        (
            Tool::Extraction,
            "Extraction point",
            "Click where the hunter may be picked up.",
        ),
    ];
}

/// The picture shown, and the instance and area it was read for.
pub(super) struct Picture {
    pub(super) of: (Instance, String),
    pub(super) texture: egui::TextureHandle,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) area: String,
    pub(super) tool: Tool,
    pub(super) selected: Option<Target>,
    /// The selected zone's tags as typed (so a half-typed word is kept).
    tags: (Option<Target>, String),
    pub(super) drag: Option<Grip>,
    /// The polygon being moved whole.
    pub(super) carry: Option<Carry>,
    /// The circle being drawn: its centre.
    pub(super) circle_from: Option<[f64; 2]>,
    /// The polygon being drawn: its corners so far.
    pub(super) outline: Vec<[f64; 2]>,
    pub(super) picture: Option<Picture>,
    /// The picture being read, or read and not yet shown.
    wanted: Option<(Instance, String)>,
    arrived: Option<Vec<u8>>,
    /// What went wrong reading the picture, and what was asked for.
    pub(super) error: Option<((Instance, String), String)>,
    pub(super) note: Option<String>,
}

impl State {
    pub(super) fn failed(&mut self, error: &str) {
        if let (Some(w), None) = (self.wanted.take(), &self.arrived) {
            self.error = Some((w, error.into()));
        }
    }

    fn read(&mut self, area: String, rgba: Vec<u8>) {
        if self.wanted.as_ref().is_some_and(|(_, a)| *a == area) && rgba.len() == SIZE * SIZE * 4 {
            self.arrived = Some(rgba);
        } else {
            self.wanted = None;
        }
    }

    /// Asks for the picture of `of` unless it is shown, on its way or failed.
    fn want(&mut self, of: &(Instance, String), handle: &mut crate::jobs::Handle) {
        let have = |x: Option<&(Instance, String)>| x == Some(of);
        if have(self.picture.as_ref().map(|p| &p.of))
            || have(self.error.as_ref().map(|e| &e.0))
            || self.wanted.is_some()
            || handle.busy()
        {
            return;
        }
        let (i, area) = of.clone();
        match handle.start_task(format!("Reading the map of {area}"), move |_| {
            crate::jobs::balance::read_overview(i, area)
        }) {
            Ok(()) => self.wanted = Some(of.clone()),
            Err(e) => self.error = Some((of.clone(), e)),
        }
    }

    /// Turns pixels that arrived into the picture shown.
    fn show_arrived(&mut self, ctx: &egui::Context) {
        let (Some(rgba), Some(of)) = (self.arrived.take(), self.wanted.take()) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &rgba);
        let texture = ctx.load_texture("world-map", image, egui::TextureOptions::LINEAR);
        self.picture = Some(Picture { of, texture });
    }

    fn drop_drawing(&mut self) {
        (self.drag, self.carry, self.circle_from) = (None, None, None);
        self.outline.clear();
    }
}

impl Extras {
    pub(super) fn overview_read(&mut self, area: String, rgba: Vec<u8>) {
        self.world.read(area, rgba);
    }

    /// The Advanced page's map editor.
    pub(super) fn world_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let mut w = r.settings.world.clone().unwrap_or_default();
        let before = w.clone();
        ui.label(
            RichText::new("World zones: map editor (experimental)")
                .color(theme::sand())
                .strong(),
        );
        help(
            ui,
            "Draw zones, player starts and extraction points on an area's map. They are saved \
             in the recipe, in map cells, and built into the game. Hunts do not use them yet: \
             the planned spawn rules, flee limits and missions will refer to them by id.",
        );
        ui.checkbox(&mut w.enabled, "Build these into the game");
        gap_ui(ui, &mut w);
        egui::CollapsingHeader::new("Map editor")
            .id_salt("world-map-editor")
            .show(ui, |ui| {
                self.editor_ui(ui, r, &mut w, game, folders, handle)
            });
        if w != before {
            r.settings.world = (w != World::default()).then_some(w);
        }
    }

    fn editor_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        w: &mut World,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let areas: Vec<String> = values.areas.iter().map(|a| a.name.clone()).collect();
        let s = &mut self.world;
        if !areas.contains(&s.area) {
            s.area = areas.first().cloned().unwrap_or_default();
        }
        if areas.is_empty() {
            help(ui, "This game has no areas.");
            return;
        }
        let was = s.area.clone();
        ui.horizontal(|ui| {
            ui.label("Area");
            egui::ComboBox::from_id_salt("world-area")
                .selected_text(s.area.clone())
                .show_ui(ui, |ui| {
                    for a in &areas {
                        let n = count_on(w, a);
                        let text = if n == 0 {
                            a.clone()
                        } else {
                            format!("{a}  ({n})")
                        };
                        ui.selectable_value(&mut s.area, a.clone(), text);
                    }
                });
        });
        if s.area != was {
            s.selected = None;
            s.drop_drawing();
        }
        tools_ui(ui, s, w);
        if let (Some(i), _) = instance(r, game, folders) {
            s.want(&(i, s.area.clone()), handle);
        }
        s.show_arrived(ui.ctx());
        map_ui(ui, s, w);
        if let Some(n) = &s.note {
            ui.label(RichText::new(n).color(theme::gold()));
        }
        ui.add_space(4.0);
        list_ui(ui, s, w);
        selected_ui(ui, s, w);
        let elsewhere = w
            .zones
            .iter()
            .map(|z| &z.area)
            .chain(w.spawns.iter().chain(&w.extractions).map(|m| &m.area))
            .filter(|a| !areas.iter().any(|n| edit::same_area(n, a)))
            .count();
        if elsewhere > 0 {
            ui.label(
                RichText::new(format!(
                    "{elsewhere} more in the recipe are on areas this game does not have; they \
                     are kept, and left out of the build."
                ))
                .color(theme::gold()),
            );
        }
    }
}

fn count_on(w: &World, area: &str) -> usize {
    let on = |a: &str| edit::same_area(a, area);
    w.zones.iter().filter(|z| on(&z.area)).count()
        + w.spawns
            .iter()
            .chain(&w.extractions)
            .filter(|m| on(&m.area))
            .count()
}

/// The least distance between a start and an extraction point.
fn gap_ui(ui: &mut egui::Ui, w: &mut World) {
    ui.horizontal(|ui| {
        let mut on = w.min_extraction_gap.is_some();
        ui.checkbox(&mut on, "Keep extraction points away from player starts");
        match (on, &mut w.min_extraction_gap) {
            (true, None) => w.min_extraction_gap = Some(0.25),
            (false, gap) => *gap = None,
            (true, Some(g)) => {
                let (lo, hi) = (f64::from(GAP_RANGE.0), f64::from(GAP_RANGE.1));
                ui.add(
                    egui::DragValue::new(g)
                        .range(lo..=hi)
                        .speed(0.01)
                        .fixed_decimals(2),
                );
                ui.label(format!("of the map's side ({:.0} cells)", *g * SIDE));
            }
        }
    });
}

fn tools_ui(ui: &mut egui::Ui, s: &mut State, w: &mut World) {
    let was = s.tool;
    ui.horizontal_wrapped(|ui| {
        for (tool, name, tip) in Tool::ALL {
            ui.selectable_value(&mut s.tool, tool, name)
                .on_hover_text(tip);
        }
    });
    if s.tool != was {
        s.drop_drawing();
        s.note = None;
    }
    // Shown for the whole of the tool's use, so the map does not move
    // under the pointer when the first corner is placed.
    if s.tool == Tool::Polygon {
        ui.horizontal(|ui| {
            ui.label(format!("{} corners", s.outline.len()));
            let finish = egui::Button::new("Finish");
            if ui.add_enabled(s.outline.len() >= 3, finish).clicked() {
                finish_outline(s, w);
            }
            let cancel = egui::Button::new("Cancel");
            if ui.add_enabled(!s.outline.is_empty(), cancel).clicked() {
                s.outline.clear();
                s.note = None;
            }
        });
    }
}

pub(super) fn finish_outline(s: &mut State, w: &mut World) {
    match edit::add_polygon(w, &s.area, &s.outline) {
        Ok(t) => {
            s.selected = Some(t);
            s.outline.clear();
            s.note = None;
        }
        Err(e) => s.note = Some(format!("Not added: {e}. Move on or cancel.")),
    }
}

/// The area's entries, one row each: click to select, ✕ to remove.
fn list_ui(ui: &mut egui::Ui, s: &mut State, w: &mut World) {
    let mut rows: Vec<(Target, String)> = Vec::new();
    for (i, z) in w.zones.iter().enumerate() {
        if edit::same_area(&z.area, &s.area) {
            let what = match z.shape() {
                Ok(_) if z.center.is_some() => "circle".to_string(),
                Ok(_) => format!("polygon, {} corners", z.points.len()),
                Err(e) => format!("unusable: {e}"),
            };
            let tags = if z.tags.is_empty() {
                String::new()
            } else {
                format!("  [{}]", z.tags.join(" "))
            };
            rows.push((Target::Zone(i), format!("Zone {}: {what}{tags}", z.id)));
        }
    }
    let marker = |m: &Marker, what: &str| match m.at {
        Some(at) => format!("{what} {}: {:.1}, {:.1}", m.id, at[0], at[1]),
        None => format!("{what} {}: no position", m.id),
    };
    for (i, m) in w.spawns.iter().enumerate() {
        if edit::same_area(&m.area, &s.area) {
            rows.push((Target::Spawn(i), marker(m, "Player start")));
        }
    }
    for (i, m) in w.extractions.iter().enumerate() {
        if edit::same_area(&m.area, &s.area) {
            rows.push((Target::Extraction(i), marker(m, "Extraction point")));
        }
    }
    if rows.is_empty() {
        help(
            ui,
            "Nothing on this area yet: pick a tool and draw on the map.",
        );
        return;
    }
    let mut gone = None;
    for (t, text) in rows {
        ui.horizontal(|ui| {
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                gone = Some(t);
            }
            if ui.selectable_label(s.selected == Some(t), text).clicked() {
                s.selected = Some(t);
            }
        });
    }
    if let Some(t) = gone {
        edit::remove(w, t);
        s.selected = None;
        s.drop_drawing();
    }
}

fn point_ui(ui: &mut egui::Ui, p: &mut [f64; 2]) {
    for (v, name) in p.iter_mut().zip(["x ", "z "]) {
        ui.add(
            egui::DragValue::new(v)
                .range(0.0..=SIDE)
                .speed(0.5)
                .fixed_decimals(1)
                .prefix(name),
        );
    }
}

/// The selected entry's id, tags and numbers.
fn selected_ui(ui: &mut egui::Ui, s: &mut State, w: &mut World) {
    let Some(t) = s.selected else { return };
    ui.separator();
    let id_ui = |ui: &mut egui::Ui, id: &mut String| {
        ui.label("Id");
        ui.add(egui::TextEdit::singleline(id).desired_width(160.0));
        if !c2_formats::world::valid_id(id.trim()) {
            ui.label(RichText::new("letters, digits, - and _ only").color(theme::red()));
        }
    };
    match t {
        Target::Zone(i) => {
            let Some(z) = w.zones.get_mut(i) else { return };
            if s.tags.0 != Some(t) {
                s.tags = (Some(t), z.tags.join(" "));
            }
            ui.horizontal(|ui| {
                id_ui(ui, &mut z.id);
                ui.label("Tags");
                let tags = egui::TextEdit::singleline(&mut s.tags.1)
                    .desired_width(200.0)
                    .hint_text("words, e.g. no_spawn");
                if ui.add(tags).changed() {
                    z.tags = edit::parse_tags(&s.tags.1);
                }
            });
            zone_numbers_ui(ui, z);
        }
        Target::Spawn(i) | Target::Extraction(i) => {
            let list = if matches!(t, Target::Spawn(_)) {
                &mut w.spawns
            } else {
                &mut w.extractions
            };
            let Some(m) = list.get_mut(i) else { return };
            ui.horizontal(|ui| {
                id_ui(ui, &mut m.id);
                point_ui(ui, m.at.get_or_insert([0.0, 0.0]));
            });
        }
    }
}

fn zone_numbers_ui(ui: &mut egui::Ui, z: &mut Zone) {
    if let (Some(c), Some(r)) = (&mut z.center, &mut z.radius) {
        ui.horizontal(|ui| {
            ui.label("Centre");
            point_ui(ui, c);
            ui.add(
                egui::DragValue::new(r)
                    .range(f64::from(RADIUS_RANGE.0)..=f64::from(RADIUS_RANGE.1))
                    .speed(0.5)
                    .fixed_decimals(1)
                    .prefix("radius "),
            );
        });
        return;
    }
    egui::CollapsingHeader::new(format!("{} corners", z.points.len()))
        .id_salt("world-zone-corners")
        .show(ui, |ui| {
            let mut gone = None;
            let many = z.points.len() > 3;
            for (j, p) in z.points.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    point_ui(ui, p);
                    if many
                        && ui
                            .small_button("✕")
                            .on_hover_text("Remove corner")
                            .clicked()
                    {
                        gone = Some(j);
                    }
                });
            }
            if let Some(j) = gone {
                z.points.remove(j);
            }
        });
    if let Err(e) = z.shape() {
        ui.label(RichText::new(format!("Unusable: {e}")).color(theme::red()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: &str = "Ravan";

    /// Runs the map and its lists for one frame with `events`.
    fn frame(ctx: &egui::Context, s: &mut State, w: &mut World, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            tools_ui(ui, s, w);
            map_ui(ui, s, w);
            list_ui(ui, s, w);
            selected_ui(ui, s, w);
        });
        out.textures_delta.clear();
    }

    fn button(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    fn click(ctx: &egui::Context, s: &mut State, w: &mut World, at: egui::Pos2) {
        frame(ctx, s, w, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, s, w, vec![button(at, true)]);
        frame(ctx, s, w, vec![button(at, false)]);
    }

    fn drag(ctx: &egui::Context, s: &mut State, w: &mut World, from: egui::Pos2, to: egui::Pos2) {
        frame(ctx, s, w, vec![egui::Event::PointerMoved(from)]);
        frame(ctx, s, w, vec![button(from, true)]);
        for k in 1..=4 {
            let at = from + (to - from) * (k as f32 / 4.0);
            frame(ctx, s, w, vec![egui::Event::PointerMoved(at)]);
        }
        frame(ctx, s, w, vec![button(to, false)]);
    }

    fn state(tool: Tool) -> State {
        State {
            area: AREA.into(),
            tool,
            ..State::default()
        }
    }

    #[test]
    fn the_pointer_draws_moves_and_selects_without_a_picture() {
        let ctx = egui::Context::default();
        let mut w = World::default();
        let (a, b) = (egui::pos2(200.0, 250.0), egui::pos2(260.0, 250.0));

        let mut s = state(Tool::Circle);
        frame(&ctx, &mut s, &mut w, vec![]);
        drag(&ctx, &mut s, &mut w, a, b);
        assert_eq!(w.zones.len(), 1, "{w:?}");
        let (c, r) = (
            w.zones[0].center.expect("centre"),
            w.zones[0].radius.expect("radius"),
        );
        assert!(r > 10.0 && w.zones[0].area == AREA && w.zones[0].shape().is_ok());
        assert_eq!(s.selected, Some(Target::Zone(0)));

        // Its centre square moves it; the radius stays.
        s.tool = Tool::Select;
        frame(&ctx, &mut s, &mut w, vec![]);
        drag(&ctx, &mut s, &mut w, a, egui::pos2(200.0, 330.0));
        let moved = w.zones[0].center.expect("centre");
        assert!(
            moved[1] > c[1] + 10.0 && (moved[0] - c[0]).abs() < 1.0,
            "{moved:?}"
        );
        assert_eq!(w.zones[0].radius, Some(r));

        // Three clicks and the first corner again close a polygon.
        s.tool = Tool::Polygon;
        frame(&ctx, &mut s, &mut w, vec![]);
        let corners = [
            (400.0, 200.0),
            (500.0, 200.0),
            (450.0, 300.0),
            (400.0, 200.0),
        ];
        for (x, y) in corners {
            click(&ctx, &mut s, &mut w, egui::pos2(x, y));
        }
        assert_eq!(w.zones.len(), 2, "{:?} {:?}", s.outline, s.note);
        assert_eq!(w.zones[1].points.len(), 3);
        assert!(s.outline.is_empty());

        s.tool = Tool::Start;
        frame(&ctx, &mut s, &mut w, vec![]);
        click(&ctx, &mut s, &mut w, egui::pos2(300.0, 500.0));
        s.tool = Tool::Extraction;
        frame(&ctx, &mut s, &mut w, vec![]);
        click(&ctx, &mut s, &mut w, egui::pos2(350.0, 520.0));
        assert_eq!((w.spawns.len(), w.extractions.len()), (1, 1));

        // Select picks the zone under a click.
        s.tool = Tool::Select;
        frame(&ctx, &mut s, &mut w, vec![]);
        click(&ctx, &mut s, &mut w, egui::pos2(450.0, 230.0));
        assert_eq!(s.selected, Some(Target::Zone(1)));
    }

    #[test]
    fn the_pointer_moves_a_polygon_and_adds_and_removes_its_corners() {
        let ctx = egui::Context::default();
        let mut w = World::default();
        let mut s = state(Tool::Polygon);
        frame(&ctx, &mut s, &mut w, vec![]);
        let corners = [
            (300.0, 200.0),
            (500.0, 200.0),
            (500.0, 400.0),
            (300.0, 400.0),
            (300.0, 200.0),
        ];
        for (x, y) in corners {
            click(&ctx, &mut s, &mut w, egui::pos2(x, y));
        }
        assert_eq!(w.zones.len(), 1, "{:?} {:?}", s.outline, s.note);
        let square = w.zones[0].points.clone();
        assert_eq!(square.len(), 4);

        // Where a cell is on screen with the Select tool's layout (the
        // polygon tool's row is gone): found from a point placed by a click.
        s.tool = Tool::Start;
        frame(&ctx, &mut s, &mut w, vec![]);
        click(&ctx, &mut s, &mut w, egui::pos2(100.0, 100.0));
        let mark = w.spawns[0].at.expect("placed");
        let k = (SIDE / 640.0) as f32;
        let screen = |c: [f64; 2]| {
            egui::pos2(
                100.0 + (c[0] - mark[0]) as f32 / k,
                100.0 + (c[1] - mark[1]) as f32 / k,
            )
        };
        let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];

        // A drag from inside moves every corner alike.
        s.tool = Tool::Select;
        s.selected = None;
        frame(&ctx, &mut s, &mut w, vec![]);
        let from = screen(mid(square[0], square[2]));
        drag(&ctx, &mut s, &mut w, from, from + egui::vec2(-40.0, 50.0));
        assert_eq!(s.selected, Some(Target::Zone(0)));
        assert!(s.carry.is_none(), "let go");
        let by = [
            w.zones[0].points[0][0] - square[0][0],
            w.zones[0].points[0][1] - square[0][1],
        ];
        assert!(by[0] < -10.0 && by[1] > 10.0, "{by:?}");
        for (p, o) in w.zones[0].points.iter().zip(&square) {
            let d = (p[0] - o[0] - by[0]).abs() + (p[1] - o[1] - by[1]).abs();
            assert!(d < 0.11, "{p:?} from {o:?}");
        }

        // A drag from the middle of its first edge adds a corner there and
        // carries it along.
        let moved = w.zones[0].points.clone();
        let edge = screen(mid(moved[0], moved[1]));
        let out = edge + egui::vec2(0.0, -30.0);
        drag(&ctx, &mut s, &mut w, edge, out);
        assert_eq!(w.zones[0].points.len(), 5, "{:?}", w.zones[0].points);
        let added = w.zones[0].points[1];
        assert!(
            added[1] < moved[0][1] - 10.0 && added[0] > moved[0][0] && added[0] < moved[1][0],
            "{added:?}"
        );
        assert!(w.zones[0].shape().is_ok());
        assert_eq!(w.zones[0].points[2], moved[1], "the others stay");

        // A right-click on a corner removes it; a triangle keeps its own.
        let secondary = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        };
        let right_click = |s: &mut State, w: &mut World, at: egui::Pos2| {
            frame(&ctx, s, w, vec![egui::Event::PointerMoved(at)]);
            frame(&ctx, s, w, vec![secondary(at, true)]);
            frame(&ctx, s, w, vec![secondary(at, false)]);
        };
        right_click(&mut s, &mut w, out);
        assert_eq!(w.zones[0].points, moved);
        right_click(&mut s, &mut w, screen(moved[3]));
        assert_eq!(w.zones[0].points, moved[..3]);
        right_click(&mut s, &mut w, screen(moved[2]));
        assert_eq!(w.zones[0].points.len(), 3);
        assert!(s.note.is_some());
    }

    #[test]
    fn a_picture_is_shown_only_when_it_is_the_one_asked_for() {
        let i = |dir: &str| Instance {
            bases: vec![dir.into()],
            base_skip: Default::default(),
            standalones: vec![],
            standalone_ids: vec![],
            unlisted: vec![],
            mods: vec![],
            mod_ids: vec![],
            random_ambients: false,
            scratch: PathBuf::new(),
        };
        let ctx = egui::Context::default();
        let mut s = state(Tool::Select);
        s.wanted = Some((i("a"), AREA.into()));
        s.read("Elsewhere".into(), vec![0; SIZE * SIZE * 4]);
        assert!(
            s.wanted.is_none() && s.arrived.is_none(),
            "another area's is dropped"
        );
        s.wanted = Some((i("a"), AREA.into()));
        s.read(AREA.into(), vec![0; 16]);
        assert!(
            s.arrived.is_none(),
            "a picture of the wrong size is dropped"
        );
        s.wanted = Some((i("a"), AREA.into()));
        s.read(AREA.into(), vec![0; SIZE * SIZE * 4]);
        s.show_arrived(&ctx);
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        assert!(s
            .picture
            .as_ref()
            .is_some_and(|p| p.of == (i("a"), AREA.to_string())));
        // A failed read is remembered for what was asked, not asked again.
        s.wanted = Some((i("b"), AREA.into()));
        s.failed("no such folder");
        assert!(s.wanted.is_none());
        assert_eq!(
            s.error.as_ref().map(|e| e.1.as_str()),
            Some("no such folder")
        );
    }
}
