//! The map editor's edits to a recipe's `[settings.world]`, without any
//! drawing: the picture-to-cell transform, what the pointer is over, and
//! adding, moving and removing zones and points. Coordinates are map cells
//! (`[x, z]`), never picture pixels.

use c2_formats::map::MAP_SIZE;
use c2_formats::world::valid_id;
use c2_formats::zone::{Shape, MAX_VERTICES, RADIUS_RANGE};
use c2_mods::world::{Marker, World, Zone};

/// A map's side, cells.
pub(super) const SIDE: f64 = MAP_SIZE as f64;

/// Where the map's picture is on screen: its top left corner and its side,
/// in points. The picture shows the whole map, x to the right and z down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct View {
    pub min: [f32; 2],
    pub side: f32,
}

impl View {
    /// The cell under a screen position (may be outside the map).
    pub fn to_cell(self, p: [f32; 2]) -> [f64; 2] {
        let k = SIDE / f64::from(self.side.max(1.0));
        [
            f64::from(p[0] - self.min[0]) * k,
            f64::from(p[1] - self.min[1]) * k,
        ]
    }

    /// The screen position of a cell.
    pub fn to_screen(self, c: [f64; 2]) -> [f32; 2] {
        let k = f64::from(self.side) / SIDE;
        [
            self.min[0] + (c[0] * k) as f32,
            self.min[1] + (c[1] * k) as f32,
        ]
    }

    /// A length in points as cells.
    pub fn cells(self, points: f32) -> f64 {
        f64::from(points) * SIDE / f64::from(self.side.max(1.0))
    }

    /// A length in cells as points.
    pub fn points(self, cells: f64) -> f32 {
        (cells * f64::from(self.side) / SIDE) as f32
    }
}

/// A cell as the recipe keeps it: on the map, to a tenth of a cell.
pub(super) fn tidy(c: [f64; 2]) -> [f64; 2] {
    let t = |v: f64| (v.clamp(0.0, SIDE) * 10.0).round() / 10.0;
    [t(c[0]), t(c[1])]
}

fn tidy_radius(r: f64) -> f64 {
    let (lo, hi) = (f64::from(RADIUS_RANGE.0), f64::from(RADIUS_RANGE.1));
    (r.clamp(lo, hi) * 10.0).round() / 10.0
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

pub(super) fn same_area(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// An entry of the section, by its place in its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Zone(usize),
    Spawn(usize),
    Extraction(usize),
}

/// A point of an entry that can be dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grip {
    /// A circle's centre: moves the circle.
    Center(usize),
    /// A point on a circle's rim: sets its radius.
    Rim(usize),
    /// A polygon's vertex.
    Vertex(usize, usize),
    Spawn(usize),
    Extraction(usize),
}

impl Grip {
    pub fn target(self) -> Target {
        match self {
            Grip::Center(z) | Grip::Rim(z) | Grip::Vertex(z, _) => Target::Zone(z),
            Grip::Spawn(i) => Target::Spawn(i),
            Grip::Extraction(i) => Target::Extraction(i),
        }
    }
}

fn is_circle(z: &Zone) -> bool {
    z.shape.trim().eq_ignore_ascii_case("circle")
}

/// Every grip of the entries on `area`, with its cell.
pub(super) fn grips(w: &World, area: &str) -> Vec<(Grip, [f64; 2])> {
    let mut out = Vec::new();
    for (i, z) in w.zones.iter().enumerate() {
        if !same_area(&z.area, area) {
            continue;
        }
        if is_circle(z) {
            if let (Some(c), Some(r)) = (z.center, z.radius) {
                out.push((Grip::Center(i), c));
                out.push((Grip::Rim(i), [c[0] + r, c[1]]));
            }
        } else {
            out.extend(
                z.points
                    .iter()
                    .enumerate()
                    .map(|(j, p)| (Grip::Vertex(i, j), *p)),
            );
        }
    }
    for (list, spawn) in [(&w.spawns, true), (&w.extractions, false)] {
        for (i, m) in list.iter().enumerate() {
            if let (true, Some(at)) = (same_area(&m.area, area), m.at) {
                out.push((
                    if spawn {
                        Grip::Spawn(i)
                    } else {
                        Grip::Extraction(i)
                    },
                    at,
                ));
            }
        }
    }
    out
}

/// The nearest grip within `reach` cells of `at`.
pub(super) fn grip_at(w: &World, area: &str, at: [f64; 2], reach: f64) -> Option<Grip> {
    grips(w, area)
        .into_iter()
        .map(|(g, p)| (g, distance(p, at)))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(g, _)| g)
}

/// Moves a grip to `to` (kept on the map; a rim sets the radius).
pub(super) fn move_grip(w: &mut World, g: Grip, to: [f64; 2]) {
    let to = tidy(to);
    match g {
        Grip::Center(i) => {
            if let Some(z) = w.zones.get_mut(i) {
                z.center = Some(to);
            }
        }
        Grip::Rim(i) => {
            if let Some(z) = w.zones.get_mut(i) {
                if let Some(c) = z.center {
                    z.radius = Some(tidy_radius(distance(c, to)));
                }
            }
        }
        Grip::Vertex(i, j) => {
            if let Some(p) = w.zones.get_mut(i).and_then(|z| z.points.get_mut(j)) {
                *p = to;
            }
        }
        Grip::Spawn(i) => {
            if let Some(m) = w.spawns.get_mut(i) {
                m.at = Some(to);
            }
        }
        Grip::Extraction(i) => {
            if let Some(m) = w.extractions.get_mut(i) {
                m.at = Some(to);
            }
        }
    }
}

/// The entry on `area` under `at`: a point within `reach` cells, else the
/// zone holding it (the last drawn of several).
pub(super) fn entry_at(w: &World, area: &str, at: [f64; 2], reach: f64) -> Option<Target> {
    let near = |list: &[Marker]| {
        list.iter().position(|m| {
            same_area(&m.area, area) && m.at.is_some_and(|p| distance(p, at) <= reach)
        })
    };
    if let Some(i) = near(&w.spawns) {
        return Some(Target::Spawn(i));
    }
    if let Some(i) = near(&w.extractions) {
        return Some(Target::Extraction(i));
    }
    w.zones
        .iter()
        .enumerate()
        .rev()
        .find(|(_, z)| {
            same_area(&z.area, area)
                && z.shape()
                    .is_ok_and(|s| s.contains(at[0] as f32, at[1] as f32))
        })
        .map(|(i, _)| Target::Zone(i))
}

/// What a press in the Select tool takes hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Hit {
    Grip(Grip),
    /// An edge of the selected polygon: a drag from it adds a corner at
    /// `at`, as the polygon's corner `index`.
    Edge {
        zone: usize,
        index: usize,
        at: [f64; 2],
    },
    /// The inside of a polygon: a drag moves the whole of it.
    Inside(usize),
}

/// The zone's corners if it is a polygon on `area`.
fn polygon_on<'a>(w: &'a World, area: &str, zone: usize) -> Option<&'a [[f64; 2]]> {
    let z = w.zones.get(zone)?;
    (same_area(&z.area, area) && !is_circle(z) && z.points.len() >= 2).then_some(&z.points[..])
}

/// The point of the segment `a`-`b` nearest to `p`.
fn nearest_on(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> [f64; 2] {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dz * dz;
    if len2 <= 0.0 {
        return a;
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / len2).clamp(0.0, 1.0);
    [a[0] + dx * t, a[1] + dz * t]
}

/// Whether `at` is inside the polygon. A polygon whose edges cross still
/// has an inside here, so one dragged into a tangle can be moved.
fn inside(points: &[[f64; 2]], at: [f64; 2]) -> bool {
    points.len() >= 3
        && Shape::Polygon(points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect())
            .contains(at[0] as f32, at[1] as f32)
}

/// What a press at `at` takes hold of: a grip within `reach` cells, else an
/// edge of the selected polygon, else the polygon `at` is inside (the
/// selected one first, then the last drawn; not one under a circle).
pub(super) fn hit(
    w: &World,
    area: &str,
    selected: Option<Target>,
    at: [f64; 2],
    reach: f64,
) -> Option<Hit> {
    if let Some(g) = grip_at(w, area, at, reach) {
        return Some(Hit::Grip(g));
    }
    let picked = match selected {
        Some(Target::Zone(i)) => polygon_on(w, area, i).map(|p| (i, p)),
        _ => None,
    };
    if let Some((zone, points)) = picked.filter(|(_, p)| p.len() < MAX_VERTICES) {
        let n = points.len();
        let edge = (0..n)
            .map(|j| (j, nearest_on(points[j], points[(j + 1) % n], at)))
            .map(|(j, p)| (j, p, distance(p, at)))
            .filter(|(_, _, d)| *d <= reach)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((j, p, _)) = edge {
            return Some(Hit::Edge {
                zone,
                index: j + 1,
                at: tidy(p),
            });
        }
    }
    if let Some((zone, _)) = picked.filter(|(_, p)| inside(p, at)) {
        return Some(Hit::Inside(zone));
    }
    let top = w.zones.iter().enumerate().rev().find(|(_, z)| {
        same_area(&z.area, area)
            && if is_circle(z) {
                z.shape()
                    .is_ok_and(|s| s.contains(at[0] as f32, at[1] as f32))
            } else {
                inside(&z.points, at)
            }
    })?;
    (!is_circle(top.1)).then_some(Hit::Inside(top.0))
}

/// Adds a corner to a polygon as its corner `index` and gives its grip.
/// Nothing is added to a circle, a full polygon or past the last corner.
pub(super) fn add_corner(w: &mut World, zone: usize, index: usize, at: [f64; 2]) -> Option<Grip> {
    let z = w.zones.get_mut(zone)?;
    if is_circle(z) || z.points.len() >= MAX_VERTICES || index > z.points.len() {
        return None;
    }
    z.points.insert(index, tidy(at));
    Some(Grip::Vertex(zone, index))
}

/// Removes a polygon's corner; a triangle keeps its three.
pub(super) fn remove_corner(w: &mut World, zone: usize, corner: usize) -> bool {
    match w.zones.get_mut(zone) {
        Some(z) if !is_circle(z) && z.points.len() > 3 && corner < z.points.len() => {
            z.points.remove(corner);
            true
        }
        _ => false,
    }
}

/// A polygon being moved whole: where the pointer took hold of it and its
/// corners then. Each move is made from these, so it does not drift.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Carry {
    zone: usize,
    from: [f64; 2],
    points: Vec<[f64; 2]>,
}

impl Carry {
    pub fn start(w: &World, zone: usize, from: [f64; 2]) -> Option<Carry> {
        let z = w.zones.get(zone)?;
        (!is_circle(z)).then(|| Carry {
            zone,
            from,
            points: z.points.clone(),
        })
    }

    pub fn target(&self) -> Target {
        Target::Zone(self.zone)
    }

    /// Moves the polygon by the pointer's travel to `at`, stopped at the
    /// map's edges so that its shape is kept.
    pub fn to(&self, w: &mut World, at: [f64; 2]) {
        let Some(z) = w.zones.get_mut(self.zone) else {
            return;
        };
        if z.points.len() != self.points.len() {
            return;
        }
        let mut by = [0.0; 2];
        for (k, d) in by.iter_mut().enumerate() {
            let (lo, hi) = self
                .points
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                    (lo.min(p[k]), hi.max(p[k]))
                });
            // A polygon already off the map (typed in the file) is not
            // pulled out of shape: it moves only further in.
            let (least, most) = ((-lo).min(0.0), (SIDE - hi).max(0.0));
            *d = (((at[k] - self.from[k]).clamp(least, most)) * 10.0).round() / 10.0;
        }
        for (p, o) in z.points.iter_mut().zip(&self.points) {
            *p = [o[0] + by[0], o[1] + by[1]].map(|v| (v * 10.0).round() / 10.0);
        }
    }
}

/// `prefix-N`, the first not among `taken` (case ignored).
pub(super) fn fresh_id<'a>(prefix: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    (1..)
        .map(|n| format!("{prefix}-{n}"))
        .find(|id| !taken.clone().any(|t| t.trim().eq_ignore_ascii_case(id)))
        .unwrap_or_default()
}

fn zone_id(w: &World) -> String {
    fresh_id("zone", w.zones.iter().map(|z| z.id.as_str()))
}

/// Adds a circle on `area` from its centre and a point on its rim.
pub(super) fn add_circle(w: &mut World, area: &str, center: [f64; 2], rim: [f64; 2]) -> Target {
    let center = tidy(center);
    w.zones.push(Zone {
        id: zone_id(w),
        area: area.trim().into(),
        shape: "circle".into(),
        center: Some(center),
        radius: Some(tidy_radius(distance(center, rim))),
        ..Zone::default()
    });
    Target::Zone(w.zones.len() - 1)
}

/// Adds a polygon on `area`, or says why its outline can't be used.
pub(super) fn add_polygon(
    w: &mut World,
    area: &str,
    points: &[[f64; 2]],
) -> Result<Target, String> {
    let z = Zone {
        id: zone_id(w),
        area: area.trim().into(),
        shape: "polygon".into(),
        points: points.iter().copied().map(tidy).collect(),
        ..Zone::default()
    };
    z.shape()?;
    w.zones.push(z);
    Ok(Target::Zone(w.zones.len() - 1))
}

/// Adds a player start (`spawn`) or an extraction point on `area`.
pub(super) fn add_marker(w: &mut World, area: &str, at: [f64; 2], spawn: bool) -> Target {
    let (list, prefix) = if spawn {
        (&mut w.spawns, "start")
    } else {
        (&mut w.extractions, "exit")
    };
    list.push(Marker {
        id: fresh_id(prefix, list.iter().map(|m| m.id.as_str())),
        area: area.trim().into(),
        at: Some(tidy(at)),
    });
    let i = list.len() - 1;
    if spawn {
        Target::Spawn(i)
    } else {
        Target::Extraction(i)
    }
}

pub(super) fn remove(w: &mut World, t: Target) {
    match t {
        Target::Zone(i) if i < w.zones.len() => {
            w.zones.remove(i);
        }
        Target::Spawn(i) if i < w.spawns.len() => {
            w.spawns.remove(i);
        }
        Target::Extraction(i) if i < w.extractions.len() => {
            w.extractions.remove(i);
        }
        _ => {}
    }
}

/// Tags from typed text: words split at spaces and commas, lower case,
/// each once; ones that are not plain words are dropped.
pub(super) fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in text
        .split([' ', ',', ';'])
        .map(|t| t.trim().to_ascii_lowercase())
    {
        if valid_id(&t) && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: &str = "Ravan";

    #[test]
    fn cells_and_screen_positions_round_trip() {
        let v = View {
            min: [40.0, 120.0],
            side: 512.0,
        };
        assert_eq!(v.to_cell([40.0, 120.0]), [0.0, 0.0]);
        assert_eq!(v.to_cell([552.0, 376.0]), [1024.0, 512.0]);
        assert_eq!(v.to_screen([512.0, 1024.0]), [296.0, 632.0]);
        for c in [[0.0, 0.0], [300.5, 17.3], [1024.0, 1024.0], [77.7, 901.2]] {
            let back = v.to_cell(v.to_screen(c));
            assert!(distance(back, c) < 0.01, "{c:?} -> {back:?}");
        }
        // The picture's size never changes what a recipe holds.
        let small = View {
            min: [0.0, 0.0],
            side: 256.0,
        };
        assert_eq!(small.to_cell([64.0, 128.0]), v.to_cell([168.0, 376.0]));
        assert_eq!(v.cells(8.0), 16.0);
        assert_eq!(v.points(16.0), 8.0);
    }

    #[test]
    fn cells_are_kept_on_the_map_to_a_tenth() {
        assert_eq!(tidy([-5.0, 2000.0]), [0.0, 1024.0]);
        assert_eq!(tidy([300.449, 17.251]), [300.4, 17.3]);
    }

    #[test]
    fn shapes_and_points_are_added_with_fresh_ids() {
        let mut w = World::default();
        assert_eq!(
            add_circle(&mut w, AREA, [100.0, 100.0], [130.0, 140.0]),
            Target::Zone(0)
        );
        assert_eq!(w.zones[0].radius, Some(50.0));
        let tri = [[10.0, 10.0], [50.0, 10.0], [30.0, 40.04]];
        assert_eq!(add_polygon(&mut w, AREA, &tri), Ok(Target::Zone(1)));
        assert_eq!(w.zones[1].points[2], [30.0, 40.0]);
        let ids: Vec<&str> = w.zones.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, ["zone-1", "zone-2"]);
        // A bow tie is refused and nothing is added.
        let bow = [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
        assert!(add_polygon(&mut w, AREA, &bow).is_err());
        assert_eq!(w.zones.len(), 2);
        assert_eq!(add_marker(&mut w, AREA, [5.0, 6.0], true), Target::Spawn(0));
        assert_eq!(
            add_marker(&mut w, AREA, [7.0, 8.0], false),
            Target::Extraction(0)
        );
        assert_eq!(
            (w.spawns[0].id.as_str(), w.extractions[0].id.as_str()),
            ("start-1", "exit-1")
        );
        // Every entry the editor adds is one the build accepts.
        w.enabled = true;
        let f = w.to_format(&[(AREA.to_string(), "area2".to_string())]);
        let a = f.area("area2").unwrap_or_else(|| panic!("no area"));
        assert_eq!(
            (a.zones.len(), a.spawns.len(), a.extractions.len()),
            (2, 1, 1)
        );
        // A removed entry's id is free again.
        remove(&mut w, Target::Zone(0));
        assert_eq!(zone_id(&w), "zone-1");
        remove(&mut w, Target::Zone(9));
        assert_eq!(w.zones.len(), 1);
    }

    #[test]
    fn grips_are_found_and_moved() {
        let mut w = World::default();
        add_circle(&mut w, AREA, [100.0, 100.0], [120.0, 100.0]);
        add_polygon(&mut w, AREA, &[[10.0, 10.0], [50.0, 10.0], [30.0, 40.0]]).ok();
        add_marker(&mut w, AREA, [200.0, 200.0], true);
        add_circle(&mut w, "Elsewhere", [100.0, 100.0], [120.0, 100.0]);
        assert_eq!(grips(&w, AREA).len(), 2 + 3 + 1);
        assert_eq!(grip_at(&w, AREA, [101.0, 99.0], 4.0), Some(Grip::Center(0)));
        assert_eq!(grip_at(&w, AREA, [121.0, 100.0], 4.0), Some(Grip::Rim(0)));
        assert_eq!(
            grip_at(&w, AREA, [49.0, 11.0], 4.0),
            Some(Grip::Vertex(1, 1))
        );
        assert_eq!(grip_at(&w, AREA, [600.0, 600.0], 4.0), None);
        assert_eq!(Grip::Vertex(1, 1).target(), Target::Zone(1));

        move_grip(&mut w, Grip::Rim(0), [100.0, 130.0]);
        assert_eq!(w.zones[0].radius, Some(30.0));
        move_grip(&mut w, Grip::Center(0), [300.0, 2000.0]);
        assert_eq!(w.zones[0].center, Some([300.0, 1024.0]));
        assert_eq!(w.zones[0].radius, Some(30.0), "moving keeps the radius");
        move_grip(&mut w, Grip::Vertex(1, 2), [30.0, 60.0]);
        assert_eq!(w.zones[1].points[2], [30.0, 60.0]);
        move_grip(&mut w, Grip::Spawn(0), [1.0, 2.0]);
        assert_eq!(w.spawns[0].at, Some([1.0, 2.0]));
        // A rim dragged onto its centre keeps the least radius.
        move_grip(&mut w, Grip::Rim(0), [300.0, 1024.0]);
        assert_eq!(w.zones[0].radius, Some(0.5));
    }

    #[test]
    fn a_press_takes_a_grip_then_an_edge_then_the_inside() {
        let mut w = World::default();
        let square = [
            [100.0, 100.0],
            [200.0, 100.0],
            [200.0, 200.0],
            [100.0, 200.0],
        ];
        add_polygon(&mut w, AREA, &square).ok();
        let on = Some(Target::Zone(0));
        assert_eq!(
            hit(&w, AREA, on, [199.0, 101.0], 4.0),
            Some(Hit::Grip(Grip::Vertex(0, 1)))
        );
        // The edge from corner 1 to corner 2, at the point nearest the press.
        assert_eq!(
            hit(&w, AREA, on, [202.0, 150.04], 4.0),
            Some(Hit::Edge {
                zone: 0,
                index: 2,
                at: [200.0, 150.0]
            })
        );
        // The closing edge adds after the last corner.
        assert!(matches!(
            hit(&w, AREA, on, [100.0, 150.0], 4.0),
            Some(Hit::Edge { index: 4, .. })
        ));
        assert_eq!(hit(&w, AREA, on, [150.0, 150.0], 4.0), Some(Hit::Inside(0)));
        // Edges are for the selected polygon only; its inside is not.
        assert_eq!(
            hit(&w, AREA, None, [198.0, 150.0], 4.0),
            Some(Hit::Inside(0))
        );
        assert_eq!(hit(&w, AREA, None, [203.0, 150.0], 4.0), None);
        assert_eq!(hit(&w, "Elsewhere", on, [150.0, 150.0], 4.0), None);
        // A circle drawn over it is in the way unless the polygon is selected.
        add_circle(&mut w, AREA, [150.0, 150.0], [150.0, 170.0]);
        assert_eq!(hit(&w, AREA, None, [160.0, 160.0], 4.0), None);
        assert_eq!(hit(&w, AREA, on, [160.0, 160.0], 4.0), Some(Hit::Inside(0)));
        assert_eq!(
            hit(&w, AREA, None, [110.0, 110.0], 4.0),
            Some(Hit::Inside(0))
        );
        // A full polygon takes no more corners.
        w.zones[0].points = (0..MAX_VERTICES)
            .map(|k| {
                let a = k as f64 / MAX_VERTICES as f64 * std::f64::consts::TAU;
                tidy([500.0 + 100.0 * a.cos(), 500.0 + 100.0 * a.sin()])
            })
            .collect();
        assert_eq!(hit(&w, AREA, on, [599.0, 504.0], 3.0), Some(Hit::Inside(0)));
        assert_eq!(add_corner(&mut w, 0, 1, [600.0, 505.0]), None);
    }

    #[test]
    fn corners_are_added_and_removed() {
        let mut w = World::default();
        let tri = [[10.0, 10.0], [50.0, 10.0], [30.0, 40.0]];
        add_polygon(&mut w, AREA, &tri).ok();
        add_circle(&mut w, AREA, [100.0, 100.0], [120.0, 100.0]);
        assert!(!remove_corner(&mut w, 0, 1), "a triangle keeps its corners");
        assert_eq!(
            add_corner(&mut w, 0, 1, [30.04, -3.0]),
            Some(Grip::Vertex(0, 1))
        );
        assert_eq!(
            w.zones[0].points,
            [[10.0, 10.0], [30.0, 0.0], [50.0, 10.0], [30.0, 40.0]]
        );
        assert!(w.zones[0].shape().is_ok());
        assert_eq!(
            add_corner(&mut w, 0, 4, [15.0, 30.0]),
            Some(Grip::Vertex(0, 4))
        );
        assert_eq!(add_corner(&mut w, 0, 9, [0.0, 0.0]), None);
        assert_eq!(add_corner(&mut w, 1, 0, [0.0, 0.0]), None, "not a circle");
        assert!(remove_corner(&mut w, 0, 4) && remove_corner(&mut w, 0, 1));
        assert_eq!(w.zones[0].points, tri);
        assert!(!remove_corner(&mut w, 0, 7) && !remove_corner(&mut w, 1, 0));
    }

    #[test]
    fn a_polygon_is_moved_whole_and_stops_at_the_maps_edge() {
        let mut w = World::default();
        let tri = [[10.0, 10.0], [50.0, 10.0], [30.0, 40.0]];
        add_polygon(&mut w, AREA, &tri).ok();
        add_circle(&mut w, AREA, [100.0, 100.0], [120.0, 100.0]);
        assert_eq!(Carry::start(&w, 1, [100.0, 100.0]), None);
        let c = Carry::start(&w, 0, [30.0, 20.0]).unwrap_or_else(|| panic!("no carry"));
        assert_eq!(c.target(), Target::Zone(0));
        c.to(&mut w, [130.04, 220.0]);
        assert_eq!(
            w.zones[0].points,
            [[110.0, 210.0], [150.0, 210.0], [130.0, 240.0]]
        );
        // Moves are made from where it was taken, not added up.
        c.to(&mut w, [31.0, 20.0]);
        assert_eq!(w.zones[0].points[0], [11.0, 10.0]);
        // Past the corner of the map it stops whole, its shape kept.
        c.to(&mut w, [-500.0, 5000.0]);
        assert_eq!(
            w.zones[0].points,
            [[0.0, 994.0], [40.0, 994.0], [20.0, 1024.0]]
        );
        // One typed off the map is not squeezed: it only comes further in.
        w.zones[0].points = vec![[-20.0, 10.0], [50.0, 10.0], [30.0, 40.0]];
        let c = Carry::start(&w, 0, [30.0, 20.0]).unwrap_or_else(|| panic!("no carry"));
        c.to(&mut w, [0.0, 20.0]);
        assert_eq!(w.zones[0].points[0], [-20.0, 10.0]);
        c.to(&mut w, [45.0, 20.0]);
        assert_eq!(
            w.zones[0].points,
            [[-5.0, 10.0], [65.0, 10.0], [45.0, 40.0]]
        );
    }

    #[test]
    fn the_entry_under_the_pointer_is_a_point_then_the_top_zone() {
        let mut w = World::default();
        add_circle(&mut w, AREA, [100.0, 100.0], [150.0, 100.0]);
        add_circle(&mut w, AREA, [100.0, 100.0], [110.0, 100.0]);
        add_marker(&mut w, AREA, [100.0, 100.0], false);
        assert_eq!(
            entry_at(&w, AREA, [101.0, 100.0], 4.0),
            Some(Target::Extraction(0))
        );
        assert_eq!(
            entry_at(&w, AREA, [106.0, 100.0], 4.0),
            Some(Target::Zone(1))
        );
        assert_eq!(
            entry_at(&w, AREA, [140.0, 100.0], 4.0),
            Some(Target::Zone(0))
        );
        assert_eq!(entry_at(&w, AREA, [400.0, 100.0], 4.0), None);
        assert_eq!(entry_at(&w, "Elsewhere", [106.0, 100.0], 4.0), None);
    }

    #[test]
    fn tags_are_plain_words_once_each() {
        assert_eq!(
            parse_tags("No_Spawn, wet  no_spawn;caf\u{e9} deep-water"),
            ["no_spawn", "wet", "deep-water"]
        );
        assert!(parse_tags(" , ").is_empty());
    }
}
