//! NGP's drive line sampler `0x10015fc0`, the target of the calls of `CDriveLine::SampleBig` (`0x50b300`) that NGP
//! retargets (`CSolid::VfC8`, `E35CarOps::Step230`): 1.02's search in doubles (`0x10015a80`, with its own `ClampMix`
//! `0x100161c0` and `BlendB` `0x10016aa0`). The point and the table's floats widen to f64 and only the result rounds
//! back to f32. Against 1.02 (`engine_driveline::layout::sample_big`) the refinement takes up to 40 bisection steps to a
//! dot of 0.002 (1.02: 9 steps, 0.01), and a NaN dot raises the lower bound.

use crate::engine_driveline::drive_line::{entry_at, COUNT_AT};
use crate::fpu::trunc_i32;
use crate::mem::Mem;

type V3 = [f64; 3];

fn load(mem: &Mem, at: u32) -> V3 {
    [0, 4, 8].map(|k| f64::from(mem.f32(at + k)))
}

fn position(mem: &Mem, this: u32, k: u32) -> V3 {
    load(mem, entry_at(this, k))
}

fn tangent(mem: &Mem, this: u32, k: u32) -> V3 {
    load(mem, entry_at(this, k) + 0x10)
}

/// `0x10015730`: `v * s`.
fn scale(v: V3, s: f64) -> V3 {
    v.map(|x| s * x)
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `0x10016aa0`: the Hermite blend of the entries `idx` and `idx + 1` at `t`; zero outside `0..n - 1`.
fn blend_b(mem: &Mem, this: u32, n: u32, idx: u32, t: f64) -> V3 {
    if n <= 1 || idx >= n - 1 {
        return [0.0; 3];
    }
    let (p0, v0) = (position(mem, this, idx), tangent(mem, this, idx));
    let (p1, v1) = (position(mem, this, idx + 1), tangent(mem, this, idx + 1));
    let t2 = t * t;
    let t3 = t2 * t;
    let (a, b) = (t2 * 3.0, t3 + t3);
    let out = scale(p0, (b - a) + 1.0);
    let out = add(scale(v0, (t3 - (t2 + t2)) + t), out);
    let out = add(out, scale(p1, a - b));
    add(out, scale(v1, t3 - t2))
}

/// `0x100161c0`: `blend_b` at the integer part of `x` and its fraction (index 0 and fraction 0 below 0 or NaN).
fn clamp_mix(mem: &Mem, this: u32, n: u32, x: f64) -> V3 {
    if x >= 0.0 {
        let i = trunc_i32(x);
        blend_b(mem, this, n, i as u32, x - f64::from(i))
    } else {
        blend_b(mem, this, n, 0, 0.0)
    }
}

/// `(e[1] a[1] + e[0] a[0]) + e[2] a[2]` or the other order the asm takes: the caller sums.
fn sq_dist(p: V3, q: V3) -> f64 {
    let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]
}

/// `0x10015fc0` (thiscall on the drive line, `(pos, x)`, `ret 8`, st0): the arc position of the point at `pos` (three
/// f32) from the index `x`.
pub fn sample(mem: &Mem, this: u32, pos: u32, x: f32) -> f32 {
    sample_at(mem, this, load(mem, pos), f64::from(x)) as f32
}

/// `0x10015a80` on the point `p`.
fn sample_at(mem: &Mem, this: u32, p: V3, x: f64) -> f64 {
    let n = mem.u32(this + COUNT_AT);
    if n < 2 || 0.0 > x {
        return 0.0;
    }
    let (idx, va) = if x >= 0.0 {
        match trunc_i32(x) as u32 {
            i if i < n => (i, clamp_mix(mem, this, n, x)),
            _ => (n - 1, position(mem, this, n - 1)),
        }
    } else {
        (0, [0.0; 3])
    };
    let d = [va[0] - p[0], va[1] - p[1], va[2] - p[2]];
    let dist = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
    let k = match trunc_i32((dist - 6.0) * 0.5) {
        k if k <= 0 => 1,
        k => k,
    };
    let lo = (idx as i32).wrapping_sub(k);
    let mut best_at = if lo < 0 { 0 } else { lo as u32 };
    let end = match (idx as i32).wrapping_add(k) as u32 {
        e if e < n => e,
        _ => n - 1,
    };

    // The nearest entry in best_at..=end (the first wins a tie).
    let mut best = sq_dist(p, position(mem, this, best_at));
    for u in best_at + 1..=end {
        let dd = sq_dist(p, position(mem, this, u));
        if best > dd {
            best = dd;
            best_at = u;
        }
    }
    let mut ib = best_at;
    if ib != 0 {
        if ib == n - 1 {
            ib -= 1;
        } else if ib < n {
            // A tangent that points back (its dot is negative) steps the entry back one.
            let q = position(mem, this, ib);
            let e = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
            let t = tangent(mem, this, ib);
            if 0.0 > (e[1] * t[1] + e[0] * t[0]) + t[2] * e[2] {
                ib -= 1;
            }
        }
    }

    // Bisection on the dot of the offset with the lerped tangent, from [0, 1].
    let (t0, t1) = (tangent(mem, this, ib), tangent(mem, this, ib + 1));
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let mut mid = 0.0;
    for _ in 0..40 {
        mid = (hi + lo) * 0.5;
        let b = blend_b(mem, this, n, ib, mid);
        let w = add(t0, scale([t1[0] - t0[0], t1[1] - t0[1], t1[2] - t0[2]], mid));
        let dot = ((p[0] - b[0]) * w[0] + (p[1] - b[1]) * w[1]) + (p[2] - b[2]) * w[2];
        if 0.002 >= dot.abs() {
            break;
        }
        // `comisd 0, dot; jb`: a positive or NaN dot raises the lower bound.
        if 0.0 >= dot {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    f64::from(ib) + mid
}
