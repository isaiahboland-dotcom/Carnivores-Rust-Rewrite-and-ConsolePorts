//! The map editor's picture ([`super::world`]): the area's overview with
//! its zones and points drawn over it, and what the pointer does there with
//! each tool.

use c2_formats::zone::MAX_VERTICES;
use c2_mods::world::World;
use eframe::egui::{self, Color32, Stroke};

use super::world::{finish_outline, State, Tool};
use super::world_edit::{self as edit, Carry, Grip, Hit, Target, View};
use crate::theme::*;

/// How close, in points, the pointer must be to grab or pick something.
const REACH: f32 = 8.0;
/// The picture's largest side on the page, points.
const MAX_SIDE: f32 = 640.0;
const START: Color32 = Color32::from_rgb(90, 220, 110);
const EXIT: Color32 = Color32::from_rgb(110, 190, 255);

/// The picture with the area's entries over it, and the pointer's edits.
pub(super) fn map_ui(ui: &mut egui::Ui, s: &mut State, w: &mut World) {
    let side = ui.available_width().clamp(128.0, MAX_SIDE);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
    let view = View {
        min: [rect.min.x, rect.min.y],
        side,
    };
    let painter = ui.painter_at(rect);
    let shown = s
        .picture
        .as_ref()
        .filter(|p| p.of.1 == s.area)
        .map(|p| p.texture.id());
    match shown {
        Some(id) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(id, rect, uv, Color32::WHITE);
        }
        None => {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(16, 22, 15));
            let text = match &s.error {
                Some((of, e)) if of.1 == s.area => format!("Can't read the map: {e}"),
                _ => "Reading the map... (entries can be edited meanwhile)".into(),
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(13.0),
                DIM,
            );
        }
    }
    pointer(s, w, view, &resp);
    if s.tool == Tool::Select {
        show_hold(s, w, view, &resp);
    }
    draw_entries(&painter, s, w, view);
    let at = resp
        .hover_pos()
        .map(|p| edit::tidy(view.to_cell([p.x, p.y])));
    ui.horizontal(|ui| {
        match at {
            Some(c) => ui.monospace(format!("cell x {:6.1}  z {:6.1}", c[0], c[1])),
            None => ui.monospace("cell x    -    z    -  "),
        };
        if s.error.as_ref().is_some_and(|(of, _)| of.1 == s.area)
            && ui.small_button("Try again").clicked()
        {
            s.error = None;
        }
    });
}

fn pointer(s: &mut State, w: &mut World, view: View, resp: &egui::Response) {
    let Some(at) = resp
        .interact_pointer_pos()
        .map(|p| view.to_cell([p.x, p.y]))
    else {
        return;
    };
    let reach = view.cells(REACH);
    // A drag is recognised a few points after the press: it began there.
    let from = resp
        .ctx
        .input(|i| i.pointer.press_origin())
        .map_or(at, |p| view.to_cell([p.x, p.y]));
    match s.tool {
        Tool::Select => {
            let primary = egui::PointerButton::Primary;
            if resp.drag_started_by(primary) {
                match edit::hit(w, &s.area, s.selected, from, reach) {
                    Some(Hit::Grip(g)) => s.drag = Some(g),
                    Some(Hit::Edge { zone, index, at }) => {
                        s.drag = edit::add_corner(w, zone, index, at);
                    }
                    Some(Hit::Inside(zone)) => s.carry = Carry::start(w, zone, from),
                    None => {}
                }
                let held = s.drag.map(Grip::target);
                if let Some(t) = held.or(s.carry.as_ref().map(Carry::target)) {
                    s.selected = Some(t);
                }
            }
            if resp.dragged_by(primary) {
                if let Some(g) = s.drag {
                    edit::move_grip(w, g, at);
                } else if let Some(c) = &s.carry {
                    c.to(w, at);
                }
            }
            if resp.clicked() {
                s.selected = edit::entry_at(w, &s.area, at, reach);
            }
            if resp.secondary_clicked() {
                if let Some(Grip::Vertex(zone, corner)) = edit::grip_at(w, &s.area, at, reach) {
                    s.selected = Some(Target::Zone(zone));
                    s.note = (!edit::remove_corner(w, zone, corner))
                        .then(|| "A polygon keeps at least 3 corners.".to_string());
                }
            }
        }
        Tool::Circle => {
            if resp.drag_started() {
                s.circle_from = Some(edit::tidy(from));
            }
            if let (true, Some(c)) = (resp.drag_stopped(), s.circle_from) {
                s.selected = Some(edit::add_circle(w, &s.area, c, at));
            }
        }
        Tool::Polygon => {
            if resp.clicked() {
                let closes = s.outline.len() >= 3
                    && s.outline
                        .first()
                        .is_some_and(|f| (f[0] - at[0]).hypot(f[1] - at[1]) <= reach);
                if closes {
                    finish_outline(s, w);
                } else {
                    s.outline.push(edit::tidy(at));
                    s.note = None;
                }
            }
        }
        Tool::Start | Tool::Extraction => {
            if resp.clicked() {
                s.selected = Some(edit::add_marker(w, &s.area, at, s.tool == Tool::Start));
            }
        }
    }
    if resp.drag_stopped() {
        (s.drag, s.carry, s.circle_from) = (None, None, None);
    }
}

/// The Select tool's pointer shape for what a press would take hold of.
fn show_hold(s: &State, w: &World, view: View, resp: &egui::Response) {
    let icon = if s.carry.is_some() {
        egui::CursorIcon::Grabbing
    } else if s.drag.is_some() {
        egui::CursorIcon::Move
    } else {
        let at = resp.hover_pos().map(|p| view.to_cell([p.x, p.y]));
        match at.and_then(|at| edit::hit(w, &s.area, s.selected, at, view.cells(REACH))) {
            Some(Hit::Grip(_)) => egui::CursorIcon::Move,
            Some(Hit::Edge { .. }) => egui::CursorIcon::Crosshair,
            Some(Hit::Inside(_)) => egui::CursorIcon::Grab,
            None => return,
        }
    };
    resp.ctx.set_cursor_icon(icon);
}

fn draw_entries(painter: &egui::Painter, s: &State, w: &World, view: View) {
    let pos = |c: [f64; 2]| {
        let p = view.to_screen(c);
        egui::pos2(p[0], p[1])
    };
    let label = |at: egui::Pos2, text: &str, color: Color32| {
        painter.text(
            at + egui::vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(12.0),
            color,
        );
    };
    for (i, z) in w.zones.iter().enumerate() {
        if !edit::same_area(&z.area, &s.area) {
            continue;
        }
        let picked = s.selected == Some(Target::Zone(i));
        let color = match (z.shape().is_ok(), picked) {
            (false, _) => RED,
            (true, true) => Color32::WHITE,
            (true, false) => GOLD,
        };
        let stroke = Stroke::new(if picked { 2.5 } else { 1.5 }, color);
        if let (Some(c), Some(r)) = (z.center, z.radius) {
            painter.circle(pos(c), view.points(r), GOLD.gamma_multiply(0.12), stroke);
            label(pos(c), &z.id, color);
        } else if let Some(first) = z.points.first() {
            let points: Vec<egui::Pos2> = z.points.iter().map(|p| pos(*p)).collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            label(pos(*first), &z.id, color);
        }
    }
    for (list, spawn) in [(&w.spawns, true), (&w.extractions, false)] {
        for (i, m) in list.iter().enumerate() {
            let (true, Some(at)) = (edit::same_area(&m.area, &s.area), m.at) else {
                continue;
            };
            let target = if spawn {
                Target::Spawn(i)
            } else {
                Target::Extraction(i)
            };
            let color = if spawn { START } else { EXIT };
            let ring = if s.selected == Some(target) {
                Color32::WHITE
            } else {
                Color32::BLACK
            };
            painter.circle(pos(at), 5.0, color, Stroke::new(1.5, ring));
            label(pos(at), &m.id, color);
        }
    }
    if s.tool == Tool::Select {
        // Where a drag from the selected polygon's edge adds a corner.
        if let Some(Target::Zone(i)) = s.selected {
            let z = w.zones.get(i).filter(|z| {
                edit::same_area(&z.area, &s.area)
                    && z.center.is_none()
                    && (2..MAX_VERTICES).contains(&z.points.len())
            });
            for (j, a) in z.iter().flat_map(|z| z.points.iter().enumerate()) {
                let b = w.zones[i].points[(j + 1) % w.zones[i].points.len()];
                let mid = pos([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
                painter.circle(mid, 3.0, Color32::BLACK, Stroke::new(1.0, Color32::WHITE));
            }
        }
        for (g, c) in edit::grips(w, &s.area) {
            if s.selected == Some(g.target()) && !matches!(g, Grip::Spawn(_) | Grip::Extraction(_))
            {
                let r = egui::Rect::from_center_size(pos(c), egui::vec2(7.0, 7.0));
                painter.rect_filled(r, 0.0, Color32::WHITE);
            }
        }
    }
    // What is being drawn.
    let pointer = painter.ctx().pointer_latest_pos();
    let draft = Stroke::new(1.5, SAND);
    if let (Some(c), Some(p)) = (s.circle_from, pointer) {
        painter.circle_stroke(pos(c), p.distance(pos(c)), draft);
    }
    if !s.outline.is_empty() {
        let mut points: Vec<egui::Pos2> = s.outline.iter().map(|p| pos(*p)).collect();
        for p in &points {
            painter.circle_filled(*p, 3.0, SAND);
        }
        points.extend(pointer);
        painter.add(egui::Shape::line(points, draft));
    }
}
