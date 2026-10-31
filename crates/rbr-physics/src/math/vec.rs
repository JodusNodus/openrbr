//! The shared vector types of the physics ports: `Vec3` (three lanes) and `Vec4` (four lanes, `w` last).
//! Four modules grew their own copies (`physics_core::math`, `rigid_body::layout`, `car_body::vec3`,
//! `tyre::hold`); this is the one copy. Raw memory stays in each module's `layout.rs`: the load/store
//! helpers there build these structs, they are not here.

use std::ops::{Add, Sub};

use super::sse;

/// The bound under which the DirectX normalize answers zero (2^-46).
const D3DX_NORMALIZE_MIN: f32 = 1.421_085_5e-14;

/// Three floats: a row vector of the matrix product, or the xyz of a 4-float row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A 4-float row (and the rows of the batch transforms). The `w` lane is only written by `scale_rows`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Add for Vec3 {
    type Output = Vec3;

    fn add(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z }
    }
}

impl Sub for Vec3 {
    type Output = Vec3;

    fn sub(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z }
    }
}

impl Add for Vec4 {
    type Output = Vec4;

    fn add(self, o: Vec4) -> Vec4 {
        Vec4 { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z, w: self.w + o.w }
    }
}

impl Sub for Vec4 {
    type Output = Vec4;

    fn sub(self, o: Vec4) -> Vec4 {
        Vec4 { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z, w: self.w - o.w }
    }
}

impl Vec3 {
    /// The length of the three lanes, `sqrt((x*x + y*y) + z*z)`. Every copy summed in this order.
    pub fn length3(self) -> f32 {
        ((self.x * self.x + self.y * self.y) + self.z * self.z).sqrt()
    }

    /// Each lane times `s`. The copies multiplied in both orders (`v * s`, `s * v`); an f32 multiply commutes,
    /// so the shared copy uses `v * s`.
    pub fn scale(self, s: f32) -> Vec3 {
        Vec3 { x: self.x * s, y: self.y * s, z: self.z * s }
    }

    /// `a x b`. Each lane as the copies compute it (`a.y*b.z - a.z*b.y`, ...).
    pub fn cross3(self, b: Vec3) -> Vec3 {
        Vec3 { x: self.y * b.z - self.z * b.y, y: self.z * b.x - self.x * b.z, z: self.x * b.y - self.y * b.x }
    }

    /// The four-lane vector with `w` as its fourth lane.
    pub fn extend(self, w: f32) -> Vec4 {
        Vec4 { x: self.x, y: self.y, z: self.z, w }
    }

    /// The dot product, summed left to right as `((x*x + y*y) + z*z)`.
    pub fn dot(self, b: Vec3) -> f32 {
        (self.x * b.x + self.y * b.y) + self.z * b.z
    }

    /// The DirectX normalize: `s = (x*x + y*y) + z*z`. Below 2^-46 (or NaN) the result is zero; otherwise each
    /// lane times `(3 - (r*s)*r) * (r*0.5)`, with `r` the `rsqrtss` of `s` (one Newton step).
    pub fn normalize_d3dx(self) -> Vec3 {
        let s = (self.x * self.x + self.y * self.y) + self.z * self.z;
        if !(s >= D3DX_NORMALIZE_MIN) {
            return Vec3::default();
        }
        let r = super::rsqrt::rsqrt(s);
        self.scale((3.0 - (r * s) * r) * (r * 0.5))
    }

    /// The SSE normalise: each lane times the `rsqrtss` of the squared length. Every product
    /// and sum is the SSE operation (`math::sse`), so a NaN operand is selected as the instruction selects it.
    pub fn normalize_sse(self) -> Vec3 {
        let sum = sse::add(sse::add(sse::mul(self.x, self.x), sse::mul(self.y, self.y)), sse::mul(self.z, self.z));
        let reciprocal = super::rsqrt::rsqrt(sum);
        Vec3 { x: sse::mul(self.x, reciprocal), y: sse::mul(self.y, reciprocal), z: sse::mul(self.z, reciprocal) }
    }

    /// The dot product with the y and z terms summed first, `x*x' + (y*y' + z*z')` (x87 order).
    pub fn dot_yz_first(self, b: Vec3) -> f32 {
        self.x * b.x + (self.y * b.y + self.z * b.z)
    }
}

/// The zero row. The caller stores the four lanes.
pub fn clear_vec4() -> Vec4 {
    Vec4::default()
}

impl From<[f32; 4]> for Vec4 {
    fn from(v: [f32; 4]) -> Vec4 {
        Vec4 { x: v[0], y: v[1], z: v[2], w: v[3] }
    }
}

impl Vec4 {
    /// The three first lanes.
    pub fn xyz(self) -> Vec3 {
        Vec3 { x: self.x, y: self.y, z: self.z }
    }

    /// The length of the three first lanes, summed as `((x*x + y*y) + z*z)`. Every copy summed in this order.
    pub fn length3(self) -> f32 {
        ((self.x * self.x + self.y * self.y) + self.z * self.z).sqrt()
    }

    /// Each lane times `s`, all four lanes. See `Vec3::scale` for the operand order.
    pub fn scale(self, s: f32) -> Vec4 {
        Vec4 { x: self.x * s, y: self.y * s, z: self.z * s, w: self.w * s }
    }

    /// Each lane times the `rsqrtss` of the squared length of the first three lanes, `(x*x + y*y) + z*z`.
    /// The reference CPU's approximation (`math::rsqrt`); the fourth lane is scaled too.
    pub fn normalize(self) -> Vec4 {
        let r = crate::math::rsqrt::rsqrt((self.x * self.x + self.y * self.y) + self.z * self.z);
        self.scale(r)
    }

    /// `self * b` lanewise (the `mulps` over all four lanes).
    pub fn mul_lanes(self, b: Vec4) -> Vec4 {
        Vec4 { x: self.x * b.x, y: self.y * b.y, z: self.z * b.z, w: self.w * b.w }
    }

    /// `self + v * s` lanewise (the product rounds, then the sum).
    pub fn add_scaled(self, v: Vec4, s: f32) -> Vec4 {
        Vec4 { x: self.x + v.x * s, y: self.y + v.y * s, z: self.z + v.z * s, w: self.w + v.w * s }
    }

    /// `self + (b - self) * s` lanewise, in order (the difference, the product, then the sum with `self`).
    pub fn lerp(self, b: Vec4, s: f32) -> Vec4 {
        Vec4 {
            x: self.x + (b.x - self.x) * s,
            y: self.y + (b.y - self.y) * s,
            z: self.z + (b.z - self.z) * s,
            w: self.w + (b.w - self.w) * s,
        }
    }

    /// Componentwise SSE `minps`: `self < other ? self : other` per lane, so an unordered or
    /// equal lane takes `other`.
    pub fn min_lanes(self, other: Vec4) -> Vec4 {
        Vec4 {
            x: if self.x < other.x { self.x } else { other.x },
            y: if self.y < other.y { self.y } else { other.y },
            z: if self.z < other.z { self.z } else { other.z },
            w: if self.w < other.w { self.w } else { other.w },
        }
    }

    /// Componentwise SSE `maxps`: `self > other ? self : other` per lane, so an unordered or
    /// equal lane takes `other`.
    pub fn max_lanes(self, other: Vec4) -> Vec4 {
        Vec4 {
            x: if self.x > other.x { self.x } else { other.x },
            y: if self.y > other.y { self.y } else { other.y },
            z: if self.z > other.z { self.z } else { other.z },
            w: if self.w > other.w { self.w } else { other.w },
        }
    }

    /// `self x b` on the three first lanes, with `w` 0.0.
    pub fn cross3(self, b: Vec4) -> Vec4 {
        Vec4 { x: self.y * b.z - self.z * b.y, y: self.z * b.x - self.x * b.z, z: self.x * b.y - self.y * b.x, w: 0.0 }
    }
}
