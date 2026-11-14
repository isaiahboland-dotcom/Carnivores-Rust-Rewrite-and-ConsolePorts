//! Zone shapes on an area's map (not in the original game): circles and
//! polygons in map cells, as floats (world units / 256), the unit of the
//! Modder's Edition `region{}` rectangles and of `_FLEE.TXT`. Used by
//! `_WORLD.TXT` ([`crate::world`]).
//!
//! Pure arithmetic, so every platform decides "is this point in the zone"
//! the same way.

use alloc::vec::Vec;
use core::fmt;

/// Most vertices a polygon may have (bounds the checks on the consoles).
pub const MAX_VERTICES: usize = 64;
/// Accepted circle radius, cells.
pub const RADIUS_RANGE: (f32, f32) = (0.5, 1024.0);
/// A polygon enclosing less than this many square cells is degenerate.
const MIN_AREA: f32 = 1e-3;

/// A point, cells: `[x, z]`.
pub type Point = [f32; 2];

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Circle {
        center: Point,
        radius: f32,
    },
    /// The vertices in order (either winding); the last joins the first.
    Polygon(Vec<Point>),
}

/// Why a shape is not usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeError {
    /// A coordinate or the radius is NaN or infinite.
    NotFinite,
    /// The radius is outside [`RADIUS_RANGE`].
    Radius,
    TooFewVertices,
    TooManyVertices,
    /// The polygon encloses no area, or two neighbouring vertices coincide.
    Degenerate,
    /// Two of the polygon's edges cross or touch.
    SelfIntersecting,
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::NotFinite => f.write_str("a coordinate is not a finite number"),
            ShapeError::Radius => write!(
                f,
                "the radius is outside {}..={} cells",
                RADIUS_RANGE.0, RADIUS_RANGE.1
            ),
            ShapeError::TooFewVertices => f.write_str("a polygon needs at least 3 points"),
            ShapeError::TooManyVertices => {
                write!(f, "a polygon has at most {MAX_VERTICES} points")
            }
            ShapeError::Degenerate => f.write_str("the polygon encloses no area"),
            ShapeError::SelfIntersecting => f.write_str("the polygon's edges cross"),
        }
    }
}

/// Twice the signed area of the triangle a, b, c: positive when c is to
/// the left of a→b.
fn cross(a: Point, b: Point, c: Point) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn sign(v: f32) -> i8 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// Whether `p`, known to be on the line through a and b, is between them.
fn within(a: Point, b: Point, p: Point) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// Whether segments a-b and c-d share a point (crossing, touching or
/// overlapping).
fn segments_meet(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (d1, d2) = (sign(cross(c, d, a)), sign(cross(c, d, b)));
    let (d3, d4) = (sign(cross(a, b, c)), sign(cross(a, b, d)));
    if d1 != d2 && d3 != d4 {
        return true;
    }
    (d1 == 0 && within(c, d, a))
        || (d2 == 0 && within(c, d, b))
        || (d3 == 0 && within(a, b, c))
        || (d4 == 0 && within(a, b, d))
}

/// The polygon's signed area, square cells: positive when the vertices run
/// counter-clockwise with x to the right and z up.
pub fn signed_area(points: &[Point]) -> f32 {
    let n = points.len();
    let mut sum = 0.0;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum * 0.5
}

fn check_polygon(points: &[Point]) -> Result<(), ShapeError> {
    let n = points.len();
    if n < 3 {
        return Err(ShapeError::TooFewVertices);
    }
    if n > MAX_VERTICES {
        return Err(ShapeError::TooManyVertices);
    }
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(ShapeError::NotFinite);
    }
    if (0..n).any(|i| points[i] == points[(i + 1) % n]) {
        return Err(ShapeError::Degenerate);
    }
    // Neighbouring edges share a vertex by construction: they must not fold
    // back over each other. The others must not meet at all.
    for i in 0..n {
        let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
        if cross(a, b, c) == 0.0 && (within(a, b, c) || within(b, c, a)) {
            return Err(ShapeError::SelfIntersecting);
        }
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_meet(a, b, points[j], points[(j + 1) % n]) {
                return Err(ShapeError::SelfIntersecting);
            }
        }
    }
    let area = signed_area(points);
    if area < MIN_AREA && area > -MIN_AREA {
        return Err(ShapeError::Degenerate);
    }
    Ok(())
}

impl Shape {
    /// Whether the shape can be used: finite, a radius in range, a simple
    /// polygon with area.
    pub fn check(&self) -> Result<(), ShapeError> {
        match self {
            Shape::Circle { center, radius } => {
                if !(center[0].is_finite() && center[1].is_finite() && radius.is_finite()) {
                    Err(ShapeError::NotFinite)
                } else if *radius < RADIUS_RANGE.0 || *radius > RADIUS_RANGE.1 {
                    Err(ShapeError::Radius)
                } else {
                    Ok(())
                }
            }
            Shape::Polygon(points) => check_polygon(points),
        }
    }

    /// Whether the point (cells) is in the shape. A circle includes its rim;
    /// a point exactly on a polygon's edge may fall either way.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        match self {
            Shape::Circle { center, radius } => {
                let (dx, dz) = (x - center[0], z - center[1]);
                dx * dx + dz * dz <= radius * radius
            }
            Shape::Polygon(points) => {
                // Even-odd: edges crossed by the ray from the point to +x.
                let n = points.len();
                let mut inside = false;
                for i in 0..n {
                    let (a, b) = (points[i], points[(i + 1) % n]);
                    if (a[1] > z) != (b[1] > z)
                        && x < a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                    {
                        inside = !inside;
                    }
                }
                inside
            }
        }
    }

    /// The enclosing rectangle, cells: `[x min, z min, x max, z max]`.
    pub fn bounds(&self) -> [f32; 4] {
        match self {
            Shape::Circle { center, radius } => [
                center[0] - radius,
                center[1] - radius,
                center[0] + radius,
                center[1] + radius,
            ],
            Shape::Polygon(points) => {
                let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for p in points {
                    b = [
                        b[0].min(p[0]),
                        b[1].min(p[1]),
                        b[2].max(p[0]),
                        b[3].max(p[1]),
                    ];
                }
                b
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn square() -> Vec<Point> {
        vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]]
    }

    #[test]
    fn a_circle_holds_its_rim_and_nothing_past_it() {
        let c = Shape::Circle {
            center: [100.0, 50.0],
            radius: 10.0,
        };
        assert_eq!(c.check(), Ok(()));
        assert!(c.contains(100.0, 50.0) && c.contains(110.0, 50.0) && c.contains(93.0, 57.0));
        assert!(!c.contains(110.1, 50.0) && !c.contains(108.0, 58.0));
        assert_eq!(c.bounds(), [90.0, 40.0, 110.0, 60.0]);
    }

    #[test]
    fn circles_out_of_range_or_not_finite_are_rejected() {
        let c = |radius: f32, x: f32| Shape::Circle {
            center: [x, 0.0],
            radius,
        };
        assert_eq!(c(0.1, 0.0).check(), Err(ShapeError::Radius));
        assert_eq!(c(-5.0, 0.0).check(), Err(ShapeError::Radius));
        assert_eq!(c(2000.0, 0.0).check(), Err(ShapeError::Radius));
        assert_eq!(c(f32::NAN, 0.0).check(), Err(ShapeError::NotFinite));
        assert_eq!(c(5.0, f32::INFINITY).check(), Err(ShapeError::NotFinite));
    }

    #[test]
    fn a_polygon_holds_its_inside_in_either_winding() {
        let mut p = square();
        for _ in 0..2 {
            let s = Shape::Polygon(p.clone());
            assert_eq!(s.check(), Ok(()));
            assert!(s.contains(15.0, 15.0) && s.contains(10.5, 19.5));
            assert!(!s.contains(9.9, 15.0) && !s.contains(15.0, 20.1) && !s.contains(25.0, 15.0));
            assert_eq!(s.bounds(), [10.0, 10.0, 20.0, 20.0]);
            p.reverse();
        }
    }

    #[test]
    fn winding_is_the_sign_of_the_area() {
        let mut p = square();
        assert_eq!(signed_area(&p), 100.0);
        p.reverse();
        assert_eq!(signed_area(&p), -100.0);
    }

    #[test]
    fn a_concave_polygon_leaves_its_notch_out() {
        // An L: the square with its upper right quarter cut away.
        let l = Shape::Polygon(vec![
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [10.0, 10.0],
            [10.0, 20.0],
            [0.0, 20.0],
        ]);
        assert_eq!(l.check(), Ok(()));
        assert!(l.contains(5.0, 15.0) && l.contains(15.0, 5.0) && l.contains(5.0, 5.0));
        assert!(!l.contains(15.0, 15.0));
    }

    #[test]
    fn degenerate_polygons_are_rejected() {
        let check = |p: Vec<Point>| Shape::Polygon(p).check();
        assert_eq!(
            check(vec![[0.0, 0.0], [1.0, 1.0]]),
            Err(ShapeError::TooFewVertices)
        );
        assert_eq!(
            check(vec![[0.0, 0.0]; MAX_VERTICES + 1]),
            Err(ShapeError::TooManyVertices)
        );
        assert_eq!(
            check(vec![[0.0, 0.0], [5.0, 0.0], [5.0, 0.0], [0.0, 5.0]]),
            Err(ShapeError::Degenerate),
            "a repeated vertex"
        );
        assert_eq!(
            check(vec![[0.0, 0.0], [f32::NAN, 0.0], [0.0, 5.0]]),
            Err(ShapeError::NotFinite)
        );
        // Three points on a line fold back over themselves.
        assert!(check(vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]).is_err());
        assert_eq!(
            check(vec![[0.0, 0.0], [0.01, 0.0], [0.0, 0.01]]),
            Err(ShapeError::Degenerate),
            "no area to speak of"
        );
    }

    #[test]
    fn self_intersecting_polygons_are_rejected() {
        let check = |p: Vec<Point>| Shape::Polygon(p).check();
        // A bow tie.
        assert_eq!(
            check(vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]),
            Err(ShapeError::SelfIntersecting)
        );
        // An edge that only touches another's middle.
        assert_eq!(
            check(vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 10.0],
                [5.0, 0.0],
                [0.0, 10.0]
            ]),
            Err(ShapeError::SelfIntersecting)
        );
        // A spike going back along its own edge.
        assert_eq!(
            check(vec![[0.0, 0.0], [10.0, 0.0], [4.0, 0.0], [0.0, 10.0]]),
            Err(ShapeError::SelfIntersecting)
        );
    }
}
