//! NGP's drive line: a cubic Hermite path through points with tangents, searched in doubles. The point and the
//! points' floats widen to f64 and only the result rounds back to f32. Against 1.02's search
//! (`engine_driveline::layout::sample_big`) the refinement takes up to 40 bisection steps to a dot of 0.002 (1.02: 9
//! steps, 0.01), and a NaN dot raises the lower bound. [`layout::drive_line`](super::layout::drive_line) reads the
//! line from memory.

use crate::fpu::trunc_i32;

pub type V3 = [f64; 3];

/// A path of `count` points, each with a position and a tangent.
pub trait Line {
    fn count(&self) -> u32;
    fn position(&self, k: u32) -> V3;
    fn tangent(&self, k: u32) -> V3;
}

fn scale(v: V3, s: f64) -> V3 {
    v.map(|x| s * x)
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The Hermite point between the points `idx` and `idx + 1` at `t`; zero outside `0..count - 1`.
fn hermite_point(line: &impl Line, idx: u32, t: f64) -> V3 {
    let n = line.count();
    if n <= 1 || idx >= n - 1 {
        return [0.0; 3];
    }
    let (p0, v0) = (line.position(idx), line.tangent(idx));
    let (p1, v1) = (line.position(idx + 1), line.tangent(idx + 1));
    let t2 = t * t;
    let t3 = t2 * t;
    let (a, b) = (t2 * 3.0, t3 + t3);
    let out = scale(p0, (b - a) + 1.0);
    let out = add(scale(v0, (t3 - (t2 + t2)) + t), out);
    let out = add(out, scale(p1, a - b));
    add(out, scale(v1, t3 - t2))
}

/// The Hermite point at the integer part of `x` and its fraction (index 0 and fraction 0 below 0 or NaN).
fn point_at(line: &impl Line, x: f64) -> V3 {
    if x >= 0.0 {
        let i = trunc_i32(x);
        hermite_point(line, i as u32, x - f64::from(i))
    } else {
        hermite_point(line, 0, 0.0)
    }
}

/// [`hermite_point`] in doubles, rounded to f32 lane by lane.
pub fn hermite_point_f32(line: &impl Line, idx: i32, t: f32) -> [f32; 3] {
    hermite_point(line, idx as u32, f64::from(t)).map(|x| x as f32)
}

/// The tangents of `idx` and `idx + 1` lerped in doubles, `a + (b - a) * t`, rounded to f32 lane by lane; zero outside
/// `0..count - 1`, where 1.02 traps.
pub fn tangent_lerp_f32(line: &impl Line, idx: i32, t: f32) -> [f32; 3] {
    let (n, i) = (line.count(), idx as u32);
    if n <= 1 || i >= n - 1 {
        return [0.0; 3];
    }
    let (a, b) = (line.tangent(i), line.tangent(i + 1));
    let d = scale(sub(b, a), f64::from(t));
    add(a, d).map(|x| x as f32)
}

/// The squared distance, summed in the order the original takes.
fn squared_distance(p: V3, q: V3) -> f64 {
    let d = sub(p, q);
    (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]
}

/// The position along the line of the point `p`, as an index plus a fraction, searching from the index `x`.
pub fn project(line: &impl Line, p: [f32; 3], x: f32) -> f32 {
    project_f64(line, p.map(f64::from), f64::from(x)) as f32
}

fn project_f64(line: &impl Line, p: V3, x: f64) -> f64 {
    let n = line.count();
    if n < 2 || 0.0 > x {
        return 0.0;
    }
    let (start, start_point) = if x >= 0.0 {
        match trunc_i32(x) as u32 {
            i if i < n => (i, point_at(line, x)),
            _ => (n - 1, line.position(n - 1)),
        }
    } else {
        (0, [0.0; 3])
    };
    let d = sub(start_point, p);
    let dist = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
    // The search window: half the distance beyond 6 (at least 1) points either side of the start.
    let reach = match trunc_i32((dist - 6.0) * 0.5) {
        k if k <= 0 => 1,
        k => k,
    };
    let lo = (start as i32).wrapping_sub(reach);
    let mut nearest = if lo < 0 { 0 } else { lo as u32 };
    let end = match (start as i32).wrapping_add(reach) as u32 {
        e if e < n => e,
        _ => n - 1,
    };

    // The nearest point in the window (the first wins a tie).
    let mut best = squared_distance(p, line.position(nearest));
    for u in nearest + 1..=end {
        let dd = squared_distance(p, line.position(u));
        if best > dd {
            best = dd;
            nearest = u;
        }
    }
    let mut segment = nearest;
    if segment != 0 {
        if segment == n - 1 {
            segment -= 1;
        } else if segment < n {
            // A tangent that points back (its dot is negative) steps the segment back one.
            let q = line.position(segment);
            let e = sub(p, q);
            let t = line.tangent(segment);
            if 0.0 > (e[1] * t[1] + e[0] * t[0]) + t[2] * e[2] {
                segment -= 1;
            }
        }
    }

    // Bisection on the dot of the offset with the lerped tangent, from [0, 1].
    let (t0, t1) = (line.tangent(segment), line.tangent(segment + 1));
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let mut mid = 0.0;
    for _ in 0..40 {
        mid = (hi + lo) * 0.5;
        let b = hermite_point(line, segment, mid);
        let w = add(t0, scale(sub(t1, t0), mid));
        let dot = ((p[0] - b[0]) * w[0] + (p[1] - b[1]) * w[1]) + (p[2] - b[2]) * w[2];
        if 0.002 >= dot.abs() {
            break;
        }
        // a positive or NaN dot raises the lower bound
        if 0.0 >= dot {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    f64::from(segment) + mid
}
