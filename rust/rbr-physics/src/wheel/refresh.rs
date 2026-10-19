//! `E51Wheel::M3B900` (0x53b900): the wheel's pose refresh for the strut travel `x`. It builds the strut's offset and its
//! direction from the CDef's point and axis, the frame of the tyre's contact (the matrix product of the axis frame and
//! the CDef's basis), and the quaternion and the scalars the wheel keeps. The sequence (its reads, host calls and stores,
//! in the asm's order) is `layout::refresh`; this module holds the pure stages. Operation order follows the asm (the
//! x87 products are f32 products at 24-bit precision, so each one is an f32 operation here). Under NGP the x87 runs at
//! 53 bits: its three chains (the sum `S` and two `sqrt(1 - c*c)`) round once, at their store.
//!
//! The stages split at `GetB0C` (0x4e65e0), the only call whose result the later arithmetic reads. Lane 3 of a vector the
//! asm never writes is 0 here (the lead's decision: the original reads stack leftovers there, `lead/reports`).

use crate::fpu::{precision, Precision};
use crate::math::vec::{Vec3, Vec4};
use crate::math::Mat3;
use crate::Variant;

/// `0.015f` (`0x73569c`): the scale of the block's `+8` word in the strut's first offset.
const BLOCK_8_SCALE: f32 = 0.015;
/// `1.0f` (`0x795e14`).
const ONE: f32 = 1.0;
/// `2.0f` (`0x72ad94`).
const TWO: f32 = 2.0;

/// The CDef words the refresh reads (`this+0x34`). Working names, except the strut's point and axes, which the C++
/// comments call the strut's; see the module README's unknown-meaning rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefPose {
    /// `+0x23c`: the strut height the first offset starts from (`RBR_E51_WheelC.cpp:19`).
    pub height: f32,
    /// `+0x244`, `+0x248`, `+0x24c`: three words the refresh copies into the wheel's `+0x5e4..+0x5ec` (the bit pattern
    /// of `g` is `+0x24c`).
    pub bits_244: u32,
    pub bits_248: u32,
    pub bits_24c: u32,
    /// `+0x880`: the strut point (`x`, `y`, `z`).
    pub point: Vec3,
    /// `+0x890`: the strut axis (lanes 0..2 are the `+0x890`, `+0x894`, `+0x898` words).
    pub axis: Vec4,
    /// `+0x36c`: the gain of the sine term (`E+0x18` in the asm).
    pub gain_36c: f32,
    /// `+0x3fc` and `+0x3ec`: the two terms of `A = x + +0x3fc` and `B = +0x3ec`.
    pub offset_3fc: f32,
    pub term_3ec: f32,
    /// `+0x35c`: the scale of the last sine term.
    pub scale_35c: f32,
    /// `+0x380`, `+0x384`, `+0x388`: the frame weights (`s0`, `s1`, `s2`) of the basis product and the dot.
    pub weights_380: Vec3,
    /// `+0x3a0`, `+0x3a4`, `+0x3a8`: the weights of the basis product into the wheel's `+0x140`.
    pub weights_3a0: Vec3,
    /// `+0x21c`: nonzero when the car's `GetB0C` is read (`RBR_E51_WheelC.cpp:86`).
    pub has_b0c: bool,
    /// `+0x42c`: the scale of the contact offset `+0x9e0` into `+0x140` (`Madd4`).
    pub scale_42c: f32,
    /// `+0x8b0` and `+0x8c0` (vectors): the point `B0C` moves (`b + a * f11`).
    pub v8b0: Vec4,
    pub v8c0: Vec4,
    /// `+0x8c4`, `+0x8c8` and `+0x8c0` (scalars): the two rates of the `dB` term.
    pub rate_8c4: f32,
    pub rate_8c8: f32,
    /// `+0x8d0`, `+0x8d4`, `+0x8d8`: the offset subtracted from `+0x140 + +0x42c * +0x9e0` before the normalise.
    pub offset_8d0: Vec3,
}

/// The inputs the refresh takes from the wheel and the tyre block, read at the entry (the asm reads them where it needs
/// them; none of them is written by the refresh before that point, except `state` and `axis`, which the edge stores
/// first and passes here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefreshIn {
    /// The stack argument `x` (the travel).
    pub travel: f32,
    pub def: DefPose,
    /// The tyre block's `+8` word (`RBR_E51_WheelC.cpp:33`, the factor of the first offset).
    pub block_8: f32,
    /// `+0x570` after `M3AB90`: the state lerp the refresh copies first (`P`).
    pub state: Vec4,
    /// `+0x5f0` after `M3A9E0`: the turn axis `a` (lanes 0..2).
    pub axis: Vec4,
}

/// Stage 1: the strut's geometry, up to the first `GetB0C` input. The values are the asm's frame words by use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrutFrame {
    /// The strut height written to the wheel's `+0x5dc` (`E+0x18`, first formula).
    pub dirty_height: f32,
    /// `E+0x14`: the projection of the state onto the axis (`f8`).
    pub f8: f32,
    /// `E+0x140`: the offset from the strut point to the state, three lanes (lane 3 is 0).
    pub off: Vec4,
    /// `E+0x24` (before its reuse): `|off|`.
    pub len: f32,
    /// `E+0x230`: `off / |off|`.
    pub dir: Vec4,
    /// `E+0x240`: `axis x dir`.
    pub perp: Vec3,
    /// `E+0x120`: `(0,0,1) x a`, where `a` is the turn axis.
    pub w_axis: Vec3,
    /// `E+0xa0`: `a x w_axis`.
    pub c_axis: Vec3,
    /// `E+0x20` (after its reuse, the sum `S`): `sqrt(B*B + A*A)` with `A = x + +0x3fc`, `B = +0x3ec`.
    pub s: f32,
    /// `E+0x14`: the clamped cosine of the first angle (`cv`).
    pub cos_1: f32,
    /// `E+0x24`: the sine of the first angle, scaled by `+0x36c` (`E+0x24` after its scale).
    pub sin_1: f32,
}

/// Stage 1, pure: the geometry of the strut from the state, the axis and the CDef's point and axis.
pub fn strut_frame(input: &RefreshIn, variant: Variant) -> StrutFrame {
    let def = &input.def;
    let p = input.state;
    let dirty_height = def.height - input.block_8 * BLOCK_8_SCALE;
    // `d` is the offset of the state from the point, projected on the axis (x87-free SSE order: z, y, x).
    let d = Vec3 { x: def.point.x - p.x, y: def.point.y - p.y, z: def.point.z - p.z };
    let f8 = (def.axis.z * d.z + def.axis.y * d.y) + def.axis.x * d.x;
    // `Madd4`: the axis times f8, plus the state (all four lanes).
    let v =
        Vec4 { x: def.axis.x * f8 + p.x, y: def.axis.y * f8 + p.y, z: def.axis.z * f8 + p.z, w: def.axis.w * f8 + p.w };
    let off = Vec3 { x: v.x - def.point.x, y: v.y - def.point.y, z: v.z - def.point.z };
    let len = off.length3();
    // The normalise is a scale by the f32 reciprocal (x87 `fdiv` at 24-bit, then the scale's `mulps`).
    let inv = ONE / len;
    let dir = Vec4 { x: off.x * inv, y: off.y * inv, z: off.z * inv, w: 0.0 };
    let axis = input.axis;
    let a = Vec3 { x: axis.x, y: axis.y, z: axis.z };
    // `W = (0,0,1) x a`, `c = a x W` (the asm's two cross calls, in that order).
    let w_axis = Vec3 { x: 0.0, y: 0.0, z: ONE }.cross3(a);
    let c_axis = a.cross3(w_axis);
    let perp = def_axis3(def).cross3(Vec3 { x: dir.x, y: dir.y, z: dir.z });
    // The sum `S = sqrt(B*B + A*A)` (the x87 chain: the squares, their sum, the root).
    let b_term = def.term_3ec;
    let s = match precision(variant) {
        Precision::Pc24 => {
            let a_term = input.travel + def.offset_3fc;
            (b_term * b_term + a_term * a_term).sqrt()
        }
        Precision::Pc53 => {
            let a_term = f64::from(input.travel) + f64::from(def.offset_3fc);
            (f64::from(b_term) * f64::from(b_term) + a_term * a_term).sqrt() as f32
        }
    };
    // The cosine of the first angle, from the three lengths (`num / den`, both f32 products and sums).
    let g = f32::from_bits(def.bits_24c);
    let num = (g * g + len * len) - (s * s - f8 * f8);
    let den = (g * len) * TWO;
    let (cos_1, sin_1) = clamp_cos(num / den, variant);
    StrutFrame {
        dirty_height,
        f8,
        off: Vec4 { x: off.x, y: off.y, z: off.z, w: 0.0 },
        len,
        dir,
        perp,
        w_axis,
        c_axis,
        s,
        cos_1,
        sin_1: sin_1 * def.gain_36c,
    }
}

/// The CDef's strut axis as three lanes (`+0x890`, `+0x894`, `+0x898`).
fn def_axis3(def: &DefPose) -> Vec3 {
    Vec3 { x: def.axis.x, y: def.axis.y, z: def.axis.z }
}

/// `Madd4`: `a * s + b` on the four lanes (the asm's `mulps` then `addps`).
fn madd(a: Vec4, s: f32, b: Vec4) -> Vec4 {
    a.scale(s) + b
}

/// Clamps a cosine to [-1, 1] and gives the sine of its angle, as the asm's `comiss` pairs do: a value at or above 1
/// (or at or below -1) is the clamp with sine 0; a NaN takes the square root path (`sqrt(1 - c*c)`, NaN).
fn clamp_cos(cv: f32, variant: Variant) -> (f32, f32) {
    if cv >= ONE {
        (ONE, 0.0)
    } else if -ONE >= cv {
        (-ONE, 0.0)
    } else {
        let sin = match precision(variant) {
            Precision::Pc24 => (ONE - cv * cv).sqrt(),
            Precision::Pc53 => (1.0 - f64::from(cv) * f64::from(cv)).sqrt() as f32,
        };
        (cv, sin)
    }
}

/// Stage 2, before `GetB0C`: the lift vector, the contact point and the offset from it, and the basis that the
/// second half of the refresh turns into the wheel's frame (asm `0x53bcb3`..`0x53c0ab`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lift {
    /// `E+0xe0`: `dir * cos_1 + perp * sin_1`, times the `g` of `+0x5ec` (`RBR_E51_WheelC.cpp:105-119`).
    pub lift: Vec4,
    /// `E+0x40`: the contact point, `point + lift` (three lanes).
    pub contact: Vec3,
    /// `E+0x60`: the state's offset from the contact point, `(state - contact) / S` (lane 3 is 0).
    pub offset: Vec4,
    /// `E+0x80`: `(B, 0, A) / S`, the unit offset the dot products read.
    pub unit: Vec4,
    /// `E+0x10`: `unit` dotted with the CDef weights `+0x380` (`dot1`).
    pub dot1: f32,
    /// `E+0x110`: `E+0x90 / |E+0x90|`, the normalised weight vector.
    pub wn: Vec4,
    /// `E+0x20` (after `|E+0x90|`): the length of the weight vector before its normalise.
    pub len_wp: f32,
    /// `E+0x1b0`: `E+0x170 / |E+0x170|`.
    pub bv: Vec3,
    /// `E+0x18` (after `|E+0x170|`): the length of `E+0x170`.
    pub len_bv: f32,
    /// `E+0x1c0`: `unit x wn`.
    pub c2: Vec3,
    /// `E+0x2c`: `bv . wn`, the dot the `cos` of the second angle reads.
    pub dot_a: f32,
    /// `E+0x24`: `c2 . bv`.
    pub dot_b: f32,
}

/// Stage 2: the geometry after the first sine term, up to the `GetB0C` call (the asm's blocks from `0x53bcb3`).
pub fn lift_frame(input: &RefreshIn, f: &StrutFrame) -> Lift {
    let def = &input.def;
    let p = input.state;
    let dir4 = f.dir;
    // `E+0xe0 = dir * cos_1`, then `E+0xe0 += perp * sin_1` (Madd4; the perp's lane 3 is the cross's 0).
    let perp4 = Vec4 { x: f.perp.x, y: f.perp.y, z: f.perp.z, w: 0.0 };
    let lift = madd(perp4, f.sin_1, dir4.scale(f.cos_1));
    // `E+0xe0 *= g` after the cross with the axis (`E+0x260`, never read), and the contact point is `point + lift`.
    let g = f32::from_bits(def.bits_24c);
    let lift = lift.scale(g);
    let contact = Vec3 { x: def.point.x + lift.x, y: def.point.y + lift.y, z: def.point.z + lift.z };
    let offset3 = Vec3 { x: p.x - contact.x, y: p.y - contact.y, z: p.z - contact.z };
    // `1/S` scales the offset and the unit vector (x87 `fdiv`, then the scale's `mulps`).
    let inv_s = ONE / f.s;
    let offset = Vec4 { x: offset3.x * inv_s, y: offset3.y * inv_s, z: offset3.z * inv_s, w: 0.0 };
    let unit = Vec4 { x: def.term_3ec * inv_s, y: 0.0 * inv_s, z: (def.offset_3fc + input.travel) * inv_s, w: 0.0 };
    // `dot1 = (w_z*u_z + w_y*u_y) + w_x*u_x` with the CDef weights `+0x380`.
    let w380 = def.weights_380;
    let dot1 = (w380.z * unit.z + w380.y * unit.y) + w380.x * unit.x;
    // `E+0x90 = unit * (0 - dot1)`, then `+= weights` on three lanes (the asm adds `+0x380`..`+0x388`).
    let scaled = unit.scale(0.0 - dot1);
    let wp = Vec4 { x: scaled.x + w380.x, y: scaled.y + w380.y, z: scaled.z + w380.z, w: 0.0 };
    let len_wp = Vec3 { x: wp.x, y: wp.y, z: wp.z }.length3();
    let inv_wp = ONE / len_wp;
    let wn = wp.scale(inv_wp);
    let unit3 = Vec3 { x: unit.x, y: unit.y, z: unit.z };
    let c2 = unit3.cross3(Vec3 { x: wn.x, y: wn.y, z: wn.z });
    // `E+0x170 = (1 - u_x*u_x, 0, 0 - u_z*u_x)`, normalised to `E+0x1b0`.
    let q = unit.scale(unit.x);
    let e170 = Vec3 { x: ONE - q.x, y: 0.0, z: 0.0 - q.z };
    let len_bv = e170.length3();
    let bv = e170.scale(ONE / len_bv);
    let dot_a = (bv.z * wn.z + bv.y * wn.y) + bv.x * wn.x;
    let dot_b = (c2.z * bv.z + c2.y * bv.y) + c2.x * bv.x;
    Lift { lift, contact, offset, unit, dot1, wn, len_wp, bv, len_bv, c2, dot_a, dot_b }
}

/// The `|A|` and `|B|` floor below which the refresh writes no inverse (`0.001f`, `0x795e94`).
const EPS: f32 = 0.001;

/// `Σ_k w[k] · rows[k]` per lane, summed as `((a·w0 + b·w1) + c·w2)` (the asm's `mulps` and `addps` on the rows
/// as the copies at `E+0x284` lay them out). The fourth lane is the asm's 16-byte load of the next words: the second
/// lane of `b` times `w0`, the first of `c` times `w1`, and `axis_x` (the axis' first word) times `w2`.
fn mix_rows(rows: [Vec4; 3], w: Vec3, axis_x: f32) -> Vec4 {
    let [a, b, c] = rows;
    Vec4 {
        x: (a.x * w.x + b.x * w.y) + c.x * w.z,
        y: (a.y * w.x + b.y * w.y) + c.y * w.z,
        z: (a.z * w.x + b.z * w.y) + c.z * w.z,
        w: (b.x * w.x + c.x * w.y) + axis_x * w.z,
    }
}

/// `N · M` as the asm sums it: `out[r][k] = (m[2][k]·n[r][2] + m[1][k]·n[r][1]) + m[0][k]·n[r][0]`.
fn product(n: Mat3, m: Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, cell) in row.iter_mut().enumerate() {
            *cell = (m[2][k] * n[r][2] + m[1][k] * n[r][1]) + m[0][k] * n[r][0];
        }
    }
    out
}

/// The refresh after `GetB0C`, the frame and the matrix the stores read (asm `0x53c0ae`..`0x53c6c0`, `0x53c8ab`..`0x53ca92`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// `E+0x50`, `E+0x30`, `E+0x100`: the rows `a`, `b` and `c` of the frame; lane 3 is 0.
    pub rows: [Vec4; 3],
    /// `E+0x2cc`: `N · M`, the rows `N` being the axis, `w_axis` and `c_axis`.
    pub rot: Mat3,
    /// `+0x140` before `TyreTail_Get10`: the rows weighted by the CDef's `+0x3a0`, plus the contact.
    pub pos: Vec4,
    /// `+0x9a0` and `+0x9b0`: the offset and the contact point, lane 3 is 0.
    pub offset: Vec4,
    pub contact: Vec4,
    /// `+0x990`: `-c`, lane 3 is 0.
    pub c_neg: Vec4,
    /// `E+0x1f0` (`+0xe50`): the rows weighted by `+0x380`, plus the contact; lane 3 is 0.
    pub v1f0: Vec4,
    /// `+0xe60`: `(B0 - contact) - the rows weighted by +0x380`, lane 3 is 0.
    pub dv: Vec4,
    /// `+0x9c0` and `+0x9c4`: `1/A` and `1/B`, both 0 when either is within `EPS` of zero.
    pub inv_a: f32,
    pub inv_b: f32,
}

/// The frame after the `GetB0C` term `k` (0 when the CDef has no `+0x21c`) and the dirty height `h` (`+0x5dc`).
pub fn frame(input: &RefreshIn, f: &StrutFrame, l: &Lift, k: f32, h: f32, variant: Variant) -> Frame {
    let def = &input.def;
    let o = l.offset;
    let c = l.contact;
    // `B0 = v8c0 * k + v8b0` (`E+0xb0`).
    let b0 = madd(def.v8c0, k, def.v8b0);
    // `Pc = offset * dot1 + contact` (`E+0xf0`), and `D = Pc - B0`.
    let pc = Vec3 { x: o.x * l.dot1 + c.x, y: o.y * l.dot1 + c.y, z: o.z * l.dot1 + c.z };
    let d = Vec3 { x: pc.x - b0.x, y: pc.y - b0.y, z: pc.z - b0.z };
    let dd = (o.z * d.z + o.y * d.y) + o.x * d.x;
    // `Q = offset * dd + B0` (`E+0x1e0`), and `R = Q - Pc` (`E+0x150`).
    let q = Vec3 { x: o.x * dd + b0.x, y: o.y * dd + b0.y, z: o.z * dd + b0.z };
    let r = Vec3 { x: q.x - pc.x, y: q.y - pc.y, z: q.z - pc.z };
    let lr = r.length3();
    // The cosine of the second angle: `(LR² + Lw²) - (h² - dd²)` over `2·LR·Lw`.
    let lw = l.len_wp;
    let num = (lr * lr + lw * lw) - (h * h - dd * dd);
    let den = (lr * lw) * TWO;
    let (cv, sn0) = clamp_cos(num / den, variant);
    let sn = sn0 * def.scale_35c;
    // `R̂ = R / LR` (`E+0x220`), and `Q' = offset x R̂` (`E+0x250`).
    let rh = Vec4 { x: r.x, y: r.y, z: r.z, w: 0.0 }.scale(ONE / lr);
    let qp = o.cross3(rh);
    let t1 = cv * l.dot_a - sn * l.dot_b;
    let t2 = cv * l.dot_b + sn * l.dot_a;
    // `a = (Q' * t2 + R̂ * t1) * Lb + offset * U.x`; `b = unit(offset x a)`; `c = a x b`.
    let a1 = Vec4 { x: qp.x * t2 + rh.x * t1, y: qp.y * t2 + rh.y * t1, z: qp.z * t2 + rh.z * t1, w: 0.0 };
    let a2 = a1.scale(l.len_bv);
    let a3 = Vec4 { w: 0.0, ..madd(o, l.unit.x, a2) };
    let b = Vec4 { w: 0.0, ..o.cross3(a3).normalize() };
    let cc = Vec4 { w: 0.0, ..a3.cross3(b) };
    let rows = [a3, b, cc];
    let m: Mat3 = [[a3.x, a3.y, a3.z], [b.x, b.y, b.z], [cc.x, cc.y, cc.z]];
    let n: Mat3 = [
        [input.axis.x, input.axis.y, input.axis.z],
        [f.w_axis.x, f.w_axis.y, f.w_axis.z],
        [f.c_axis.x, f.c_axis.y, f.c_axis.z],
    ];
    let rot = product(n, m);
    let axis_x = input.axis.x;
    let contact = Vec4 { x: c.x, y: c.y, z: c.z, w: 0.0 };
    let mix = mix_rows(rows, def.weights_3a0, axis_x);
    let pos = Vec4 { x: mix.x + c.x, y: mix.y + c.y, z: mix.z + c.z, w: mix.w };
    let m380 = mix_rows(rows, def.weights_380, axis_x);
    let v1f0 = Vec4 { x: m380.x + c.x, y: m380.y + c.y, z: m380.z + c.z, w: 0.0 };
    let dv = Vec4 { x: (b0.x - c.x) - m380.x, y: (b0.y - c.y) - m380.y, z: (b0.z - c.z) - m380.z, w: 0.0 };
    // `Dn = dv / h`, and `cr = m380 x Dn`: the terms `A` and `B` of the two inverses.
    let dn = Vec4 { w: 0.0, ..dv.scale(ONE / h) };
    let cr = m380.cross3(dn);
    let a_term = (cr.x * o.x + cr.z * o.z) + cr.y * o.y;
    let b_term = (def.rate_8c4 * dn.y + def.rate_8c8 * dn.z) + dn.x * def.v8c0.x;
    let (inv_a, inv_b) =
        if a_term.abs() > EPS && b_term.abs() > EPS { (ONE / a_term, ONE / b_term) } else { (0.0, 0.0) };
    Frame { rows, rot, pos, offset: o, contact, c_neg: Vec4 { w: 0.0, ..cc.scale(-ONE) }, v1f0, dv, inv_a, inv_b }
}

/// The refresh after `TyreTail_Get10` (asm `0x53c7e1`..`0x53caef`, `0x53cafe`..`0x53cb50`): `+0x140` takes
/// `+0x9e0 * s` (`s` is `0 - tail.+0x6c`), `E+0x1a0` is `+0x9e0 * d42c + +0x140`, and the unit `+0x9d0` is that less
/// `+0x8d0`. Returns `+0x140` and `+0x9d0`.
pub fn after_tyre(o9e0: Vec4, s: f32, pos: Vec4, scale_42c: f32, offset_8d0: Vec3) -> (Vec4, Vec4) {
    let o140 = madd(o9e0, s, pos);
    let v1a0 = madd(o9e0, scale_42c, o140);
    let v190 = Vec4 { x: v1a0.x - offset_8d0.x, y: v1a0.y - offset_8d0.y, z: v1a0.z - offset_8d0.z, w: 0.0 };
    (o140, v190.normalize())
}
