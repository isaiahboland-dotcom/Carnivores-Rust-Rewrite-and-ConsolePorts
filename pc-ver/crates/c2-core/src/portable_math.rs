//! Floating-point methods used by the portable gameplay subset.
//! The standard build continues to use native standard-library methods.
#[cfg_attr(test, allow(dead_code))]
pub(crate) trait FloatMath {
    fn sin_cos(self) -> (f32, f32);
    fn sin(self) -> f32;
    fn cos(self) -> f32;
    fn sqrt(self) -> f32;
    fn atan(self) -> f32;
    fn atan2(self, other: f32) -> f32;
    fn trunc(self) -> f32;
    fn powi(self, exponent: i32) -> f32;
}
impl FloatMath for f32 {
    fn sin_cos(self) -> (f32, f32) {
        libm::sincosf(self)
    }
    fn sin(self) -> f32 {
        libm::sinf(self)
    }
    fn cos(self) -> f32 {
        libm::cosf(self)
    }
    fn sqrt(self) -> f32 {
        libm::sqrtf(self)
    }
    fn atan(self) -> f32 {
        libm::atanf(self)
    }
    fn atan2(self, other: f32) -> f32 {
        libm::atan2f(self, other)
    }
    fn trunc(self) -> f32 {
        libm::truncf(self)
    }
    fn powi(self, exponent: i32) -> f32 {
        if exponent == 2 {
            self * self
        } else {
            libm::powf(self, exponent as f32)
        }
    }
}
