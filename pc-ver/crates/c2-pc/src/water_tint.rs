//! The underwater depth tint (PC option `modern_water_tint`, not in the
//! original): the deeper the camera, the darker the water, in the water's
//! own hue. From the modernized Modder's Edition (`Core/WaterColor.h`,
//! `Controls.cpp`, `GLRenderer.cpp: DrawUnderwaterOverlay`), read as
//! reference only.

/// Absorption of the water's dominant channel at the reference depth
/// (`kFloor`), and how much more the others lose (`kScale`).
const FLOOR: f32 = 0.40;
const SCALE: f32 = 2.20;

/// The least depth of water, in world units, at which the tint is at its
/// full strength on the bed (`Controls.cpp`).
const REFERENCE_DEPTH: f32 = 1024.0;

/// `CameraWaterDepthFactor`: 0 at the `surface`, 1 with the eye on the bed
/// (`floor`) of water at least [`REFERENCE_DEPTH`] deep; shallower water
/// never reaches 1.
pub fn depth_factor(surface: f32, eye_y: f32, floor: f32) -> f32 {
    let max = (surface - floor).max(REFERENCE_DEPTH);
    ((surface - eye_y) / max).clamp(0.0, 1.0)
}

/// What water of colour `rgb` lets through at `depth` (0..1), per channel
/// (`ModulateWaterColorByDepth`): `exp(-depth * k)` with `k` 0.40 for the
/// colour's strongest channel, up to 2.60 for one it lacks, so brown water
/// deepens to dark brown and blue to dark blue.
pub fn transmitted(rgb: [f32; 3], depth: f32) -> [f32; 3] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    if depth <= 0.0 || max <= 0.0 {
        return [1.0; 3];
    }
    let d = depth.min(1.0);
    rgb.map(|c| (-d * (FLOOR + SCALE * (1.0 - c / max))).exp())
}

/// The water's colour `rgb` at `depth`: the fog and the screen's wash.
/// No channel goes fully black (2 of 255, as there).
pub fn deepened(rgb: [f32; 3], depth: f32) -> [f32; 3] {
    if depth <= 0.0 {
        return rgb;
    }
    let t = transmitted(rgb, depth);
    core::array::from_fn(|k| (rgb[k] * t[k]).clamp(2.0 / 255.0, 1.0))
}

/// The wash's strength over the screen: 44% at the surface, 74% at depth 1
/// (the original's half strength when the option is off).
pub fn wash_alpha(depth: f32) -> f32 {
    0.44 + 0.30 * depth.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_is_untinted() {
        let brown = [0.5, 0.35, 0.2];
        assert_eq!(deepened(brown, 0.0), brown);
        assert_eq!(transmitted(brown, 0.0), [1.0; 3]);
        assert_eq!(depth_factor(4000.0, 4000.0, 3000.0), 0.0);
        assert_eq!(depth_factor(4000.0, 4100.0, 3000.0), 0.0);
    }

    #[test]
    fn water_deepens_in_its_own_hue() {
        // Brown swamp water stays brown (the fixed blue curve it replaced
        // there turned it navy): red leads, and blue falls behind further.
        let brown = [0.5, 0.35, 0.2];
        let deep = deepened(brown, 1.0);
        assert!(deep[0] > deep[1] && deep[1] > deep[2], "{deep:?}");
        assert!(deep[2] / deep[0] < brown[2] / brown[0]);
        // The strongest channel keeps exp(-0.4) of itself.
        assert!((deep[0] / brown[0] - (-0.4f32).exp()).abs() < 1e-5);
        let blue = [0.1, 0.3, 0.6];
        let deep = deepened(blue, 1.0);
        assert!(deep[2] > deep[1] && deep[1] > deep[0], "{deep:?}");
        // Darker the deeper.
        assert!(deepened(blue, 0.5)[2] > deep[2]);
    }

    #[test]
    fn no_channel_goes_black_and_black_water_stays() {
        let d = deepened([1.0, 0.0, 0.0], 1.0);
        assert!(d[1] > 0.0 && d[2] > 0.0);
        assert_eq!(transmitted([0.0; 3], 1.0), [1.0; 3]);
    }

    #[test]
    fn shallow_water_never_reaches_full_depth() {
        // Two cells of height (128 units) of water: the bed is at 1/8.
        assert_eq!(depth_factor(4000.0, 3872.0, 3872.0), 0.125);
        // Deeper than the reference: 1 on the bed, half way at half.
        assert_eq!(depth_factor(4000.0, 2000.0, 2000.0), 1.0);
        assert_eq!(depth_factor(4000.0, 3000.0, 2000.0), 0.5);
        assert!((wash_alpha(0.0) - 0.44).abs() < 1e-6);
        assert!((wash_alpha(2.0) - 0.74).abs() < 1e-6);
    }
}
