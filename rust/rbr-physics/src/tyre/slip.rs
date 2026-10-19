//! Contact-plane slip: `CScratch::ComputeSlip` (0x532a30), `CScratch::SplitSlip` (0x5346f0) and `CScratch::SolveDirs`
//! (0x533640).

use crate::fpu::round_x87;
use crate::Variant;

use super::layout::{Frame, Slip, SlipMatrix};

/// 0x532a30 `ComputeSlip`: the slip velocity `v = M · dir`, its speed `sqrt(v0² + v1²)` (the root is
/// rounded to 24 bits, as the x87 does), the contact point's longitudinal velocity `R·ω + v1` and the
/// slip-ratio term `-(1/R)·v1 - ω`.
/// Operation order: each dot product is `(m1·d1 + m2·d2) + m0·d0`, as the asm adds the terms.
pub fn compute_slip(matrix: &SlipMatrix, radius: f32, dir: [f32; 3], spin: f32, variant: Variant) -> Slip {
    let round24 = |v: f64| round_x87(v, variant);
    let row = |r: usize| (matrix[r][1] * dir[1] + matrix[r][2] * dir[2]) + matrix[r][0] * dir[0];
    let v = [row(0), row(1), row(2)];
    let speed_sq = v[1] * v[1] + v[0] * v[0];
    Slip {
        v,
        speed_sq,
        speed: round24((speed_sq as f64).sqrt()) as f32,
        long_contact_vel: radius * spin + v[1],
        slip_ratio_term: -1.0 / radius * v[1] - spin,
    }
}

/// `ComputeSlip` on the frame: the direction `dir` (the `j` vector of Step), the contact frame's slip matrix, the
/// state's radius and the spin. Writes the slip words (`+0x360..0x37c`).
pub fn apply_compute_slip(f: &mut Frame, dir: [f32; 3], variant: Variant) {
    let rows = f.scratch.contact_frame.map(|row| [row.x, row.y, row.z]);
    let slip = compute_slip(&rows, f.state.radius, dir, f.scratch.spin, variant);
    let s = &mut f.scratch;
    s.slip_v = slip.v;
    s.speed_sq = slip.speed_sq;
    s.speed = slip.speed;
    s.long_contact_vel = slip.long_contact_vel;
    s.slip_ratio_term = slip.slip_ratio_term;
}

/// 0x5346f0 `SplitSlip` on the frame: splits the slip speed (`+0x360`) into the two weights stored at `+0x3dc` and
/// `+0x3f0`, with the thresholds 2 and 1. Above 2 the first weight is 1 and the second 0; from 1 to 2 they are
/// `m - 1` and `2 - m`; at 1 or below the two `+0x380` and `+0x384` are zeroed, the weights are 0 and 1. The
/// comparisons are the asm's `!(a > b)`, so a NaN `m` takes the last branch.
pub fn split_slip(f: &mut Frame) {
    let m = f.scratch.speed;
    let (first, second) = if !(2.0 > m) {
        (1.0, 0.0)
    } else if !(1.0 > m) {
        let first = m - 1.0;
        (first, 1.0 - first)
    } else {
        f.scratch.f380 = 0.0;
        f.scratch.f384 = 0.0;
        (0.0, 1.0)
    };
    f.scratch.f3dc = first;
    f.scratch.gate = second;
}

/// 0x533640 `SolveDirs`: the absolute direction cosines of the slip and their sum, from `a`, `b`, `c`
/// and the effective radius `R`. `c1 = a·R + c` (x87, each op rounded to the precision control); the length
/// `e = sqrt(b² + c1²)`, where the square is the x87's `c1` times its f32 store (they differ at 53 bits, under NGP).
/// Below `0.001` the result is `(1, 1, 2)`.
pub fn solve_dirs(a: f32, b: f32, c: f32, radius: f32, variant: Variant) -> [f32; 3] {
    let round24 = |v: f64| round_x87(v, variant);
    const LIMIT: f32 = 0.001;
    let st = round24(round24(a as f64 * radius as f64) + c as f64);
    let c1 = st as f32;
    let e = round24(round24(round24(st * c1 as f64) + round24(b as f64 * b as f64)).sqrt()) as f32;
    if e > LIMIT {
        let inv = 1.0f32 / e;
        let bp = inv * b;
        let first = if bp > 0.0 { bp } else { 0.0 - bp };
        let cp = inv * c1;
        let second = if cp > 0.0 { cp } else { 0.0 - cp };
        [first, second, first + second]
    } else {
        [1.0, 1.0, 2.0]
    }
}
