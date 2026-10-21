//! The 4x4 matrix of the drive's clutch step: its transpose and its inverse by cofactors. Matrices are 16 floats,
//! row-major. Each operation is f32, as the x87 at 24 bits rounds it, in the original's order.

/// The transpose of `m`. Output word `4 k + j` is input word `4 j + k`.
pub fn transpose(m: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| m[4 * (i % 4) + i / 4])
}

/// The inverse of `m` by cofactors, scaled by `1 / det`.
pub fn inverse(m: [f32; 16]) -> [f32; 16] {
    let [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15] = m;
    let mut s = [0.0f32; 16];
    let f8 = a6 * a15;
    let f3 = a15 * a10;
    let f7 = a11 * a14;
    let f6 = a7 * a14;
    let f4 = a6 * a11;
    let f5 = a7 * a10;
    let f16 = a2 * a15;
    let f9 = a3 * a14;
    let f10 = a2 * a11;
    let f11 = a3 * a10;
    let f12 = a2 * a7;
    let f13 = a3 * a6;
    s[0] = (a13 * f4 + a9 * f6 + a5 * f3) - (a13 * f5 + a9 * f8 + a5 * f7);
    s[4] = (a13 * f11 + a9 * f16 + a1 * f7) - (a13 * f10 + a9 * f9 + a1 * f3);
    s[8] = (a13 * f12 + a5 * f9 + a1 * f8) - (a13 * f13 + a5 * f16 + a1 * f6);
    s[12] = (a9 * f13 + a5 * f10 + a1 * f5) - (a9 * f12 + a5 * f11 + a1 * f4);
    s[1] = (a12 * f5 + a8 * f8 + a4 * f7) - (a12 * f4 + a8 * f6 + a4 * f3);
    s[5] = (a12 * f10 + a8 * f9 + a0 * f3) - (a12 * f11 + a8 * f16 + a0 * f7);
    s[9] = (a12 * f13 + a4 * f16 + a0 * f6) - (a12 * f12 + a4 * f9 + a0 * f8);
    let f3 = a4 * a13;
    let f6 = a12 * a5;
    let f7 = a4 * a9;
    let f8 = a8 * a5;
    let f9 = a0 * a13;
    let f16 = a12 * a1;
    let f14 = a0 * a9;
    let f15 = a8 * a1;
    let a5 = a0 * a5;
    let a1 = a4 * a1;
    let a13 = a8 * a13;
    s[13] = (a8 * f12 + a4 * f11 + a0 * f4) - (a8 * f13 + a4 * f10 + a0 * f5);
    let a9 = a12 * a9;
    s[2] = (a7 * a13 + a11 * f6 + a15 * f7) - (a7 * a9 + a15 * f8 + a11 * f3);
    s[6] = (a3 * a9 + a15 * f15 + a11 * f9) - (a3 * a13 + a11 * f16 + a15 * f14);
    s[10] = (a7 * f16 + a3 * f3 + a15 * a5) - (a7 * f9 + a3 * f6 + a15 * a1);
    s[14] = (a7 * f14 + a3 * f8 + a11 * a1) - (a7 * f15 + a3 * f7 + a11 * a5);
    s[3] = (a14 * f8 + a6 * a9 + a10 * f3) - (a14 * f7 + a6 * a13 + a10 * f6);
    s[7] = (a2 * a13 + a14 * f14 + a10 * f16) - (a2 * a9 + a14 * f15 + a10 * f9);
    s[11] = (a2 * f6 + a14 * a1 + a6 * f9) - (a2 * f3 + a14 * a5 + a6 * f16);
    s[15] = (a2 * f7 + a6 * f15 + a10 * a5) - (a2 * f8 + a6 * f14 + a10 * a1);
    let det = a8 * s[8] + a0 * s[0] + a12 * s[12] + s[4] * a4;
    let inv_det = 1.0 / det;
    s.map(|x| inv_det * x)
}

/// The inverse of the transposed matrix, as the clutch step takes it.
pub fn inverse_transposed(m: [f32; 16]) -> [f32; 16] {
    inverse(transpose(m))
}
