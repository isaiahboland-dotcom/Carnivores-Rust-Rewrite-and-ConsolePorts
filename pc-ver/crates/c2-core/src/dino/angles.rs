//! Angle and easing helpers shared by every AI.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

use super::PI;

/// `DeltaFunc`: move `a` toward `b` by at most `d`.
pub fn delta_func(a: &mut f32, b: f32, d: f32) {
    if b > *a {
        *a = (*a + d).min(b);
    } else {
        *a = (*a - d).max(b);
    }
}

/// C `SGN`.
pub fn sgn(v: f32) -> f32 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `FindVectorAlpha`: angle of (vx, vy) in [0, 2pi), found by the same
/// 10-step bisection as the original (so results match to the last bit
/// that matters for AI decisions).
pub fn find_vector_alpha(vx: f32, vy: f32) -> f32 {
    let (adx, ady) = (vx.abs(), vy.abs());
    let mut alpha = PI / 4.0;
    let mut dalpha = PI / 8.0;
    for _ in 0..10 {
        // Mul2dVectors(adx, ady, cos, sin) = adx*sin - cos*ady
        alpha -= dalpha * sgn(adx * alpha.sin() - alpha.cos() * ady);
        dalpha /= 2.0;
    }
    if vx < 0.0 {
        if vy < 0.0 {
            alpha += PI;
        } else {
            alpha = PI - alpha;
        }
    } else if vy < 0.0 {
        alpha = 2.0 * PI - alpha;
    }
    alpha
}

/// `CorrectedAlpha`.
pub fn corrected_alpha(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    if d < PI {
        return (a + b) / 2.0;
    }
    let mut d = a + PI * 2.0 - b;
    if d < 0.0 {
        d += 2.0 * PI;
    }
    if d > 2.0 * PI {
        d -= 2.0 * PI;
    }
    d
}

/// `AngleDifference`.
pub fn angle_difference(a: f32, b: f32) -> f32 {
    let mut a = (a - b).abs();
    if a > PI {
        a = 2.0 * PI - a;
    }
    a
}

pub fn wrap_angle(a: &mut f32) {
    if *a < 0.0 {
        *a += 2.0 * PI;
    }
    if *a > 2.0 * PI {
        *a -= 2.0 * PI;
    }
}
