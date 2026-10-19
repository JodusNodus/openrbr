//! `CHingedRigidBody::SimEAD0` (0x51ead0): the hinge solve, which writes this+0x624. The calls it makes (`SimE1D0`, the
//! two sums `SimCFF0` and `SimEA80`, the contact vote `E47Solid::Vf81x`, and for an unowned body the force pass
//! `CSolid::E43Vf64`) are the caller's (`SolveCalls`).
//!
//! Where src/rbr/RBR_E44_SimB.cpp is the reference for the term order (the asm is checked where the records disagree).

use crate::math::vec::Vec3;

/// What `SimEAD0` did with the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// `*(this+0x650) != 0`: the early return, nothing written.
    Linked,
    /// `this+0x3d0 == 0`: the force pass ran in place of the solve (writes nothing here). `eax` is the pass's.
    Unowned { eax: u32 },
    /// The solve: the value stored at this+0x624.
    Solved(f32),
}

/// What the solve reads before `SimE1D0`: the definition's `+0x4bc` (`b0`, 0.1 when zero) and `+0x16c` (`mass`, whose
/// reciprocal scales the dot product; 1 when zero).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntryTerms {
    pub b0: f32,
    pub mass: f32,
}

/// What the solve reads after `SimE1D0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveTerms {
    /// The definition's 3x3 block at +0x3e0..+0x400, row-major.
    pub def_matrix: [[f32; 3]; 3],
    /// The hinge's rotation block (+0x598..+0x5b8, `SimC960`'s), row-major.
    pub rotation: [[f32; 3]; 3],
    /// The hinge's pivot vector (+0x600), the second operand of the cross product.
    pub pivot: [f32; 3],
    /// The definition's vector at +0x3d0, the weights of the matrix's columns.
    pub column_weights: [f32; 3],
    /// The definition's vector at +0x2c0 (the `pf` of the dot product).
    pub pf: [f32; 3],
    /// The definition's vector at +0x380 (the vote's weights).
    pub vote_weights: [f32; 3],
    /// The definition's vector at +0x3c0, scaled by -1 for the vote's cross product.
    pub vote_lever: [f32; 3],
    /// Working names (unknown meaning): the definition's `+0x49c` (`ref_angle`), `+0x4cc` (`spring_gain`) and `+0x4dc`
    /// (`damping`); the hinge's angle (+0x3fc), rate (+0x40c) and the settle term (+0x620).
    pub ref_angle: f32,
    pub spring_gain: f32,
    pub damping: f32,
    pub angle: f32,
    pub rate: f32,
    pub settle: f32,
}

/// Scratch the vote writes: its two vectors (16 bytes each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolveFrame {
    pub v90: u32,
    pub v90b: u32,
}

/// The calls and reads of the solve: the link flag, the owner, the reads before and after `SimE1D0`, and the sub-calls.
pub trait SolveCalls {
    /// `*(this+0x650) != 0`: the early return.
    fn linked(&self, this: u32) -> bool;
    /// `this+0x3d0 != 0`: the owner car. An unowned body gets `force_pass` instead.
    fn owned(&self, this: u32) -> bool;
    /// `CSolid::E43Vf64` (0x52cb20) on the body: the force pass of an unowned body. Its eax.
    fn force_pass(&mut self, this: u32) -> u32;
    fn entry_terms(&self, this: u32) -> EntryTerms;
    /// `CHingedRigidBody::SimE1D0` (0x51e1d0).
    fn e1d0(&mut self, this: u32);
    fn solve_terms(&self, this: u32) -> SolveTerms;
    /// `CHingedRigidBody::SimCFF0` (0x51cff0), its st0.
    fn slip_term(&mut self, this: u32) -> f32;
    /// `CHingedRigidBody::SimEA80` (0x51ea80), its st0.
    fn limit_sum(&mut self, this: u32) -> f32;
    /// `E47Solid::Vf81x` (0x52c510) with (`v90`, `v90b`, 3.0): `Some` with the two vectors when it reports a hit.
    fn vote(&mut self, this: u32, frame: &SolveFrame) -> Option<([f32; 3], [f32; 3])>;
}

/// `CHingedRigidBody::SimEAD0`: the outcome; the solve's value is what it writes to this+0x624.
pub fn hinge_solve<C: SolveCalls>(calls: &mut C, this: u32, frame: &SolveFrame) -> Outcome {
    if calls.linked(this) {
        return Outcome::Linked;
    }
    if !calls.owned(this) {
        return Outcome::Unowned { eax: calls.force_pass(this) };
    }
    let entry = calls.entry_terms(this);
    // asm: `b0 == 0` takes the default (an unordered compare keeps the NaN)
    let b0 = if entry.b0 == 0.0 { 0.1 } else { entry.b0 };
    // asm: an equal compare takes 1.0; a NaN divides (C++ `x != 0`)
    let f11 = if entry.mass != 0.0 { 1.0 / entry.mass } else { 1.0 };

    calls.e1d0(this);
    let t = calls.solve_terms(this);

    // d = rotation * def_matrix, each element in the C++ term order
    let (r, dm) = (t.rotation, t.def_matrix);
    let d = [
        dm[1][0] * r[0][1] + r[0][2] * dm[2][0] + r[0][0] * dm[0][0],
        dm[1][1] * r[0][1] + r[0][2] * dm[2][1] + r[0][0] * dm[0][1],
        dm[1][2] * r[0][1] + r[0][2] * dm[2][2] + r[0][0] * dm[0][2],
        r[1][2] * dm[2][0] + r[1][0] * dm[0][0] + r[1][1] * dm[1][0],
        r[1][0] * dm[0][1] + r[1][1] * dm[1][1] + r[1][2] * dm[2][1],
        r[1][2] * dm[2][2] + dm[0][2] * r[1][0] + r[1][1] * dm[1][2],
        dm[1][0] * r[2][1] + r[2][2] * dm[2][0] + r[2][0] * dm[0][0],
        r[2][0] * dm[0][1] + dm[1][1] * r[2][1] + r[2][2] * dm[2][1],
        r[2][2] * dm[2][2] + r[2][0] * dm[0][2] + dm[1][2] * r[2][1],
    ];
    // SSE lanes: m[i] = (d[i]*c0 + d[3+i]*c1) + d[6+i]*c2
    let c = t.column_weights;
    let m = [0, 1, 2].map(|i| (d[i] * c[0] + d[3 + i] * c[1]) + d[6 + i] * c[2]);
    let cr = cross(m, t.pivot);
    let pf = t.pf;
    let dot = (cr[1] * pf[1] + cr[2] * pf[2]) + cr[0] * pf[0];
    let dot = dot * f11;

    // asm `fchs` negates the dot (the C++ writes 0 - dot; the two differ only in a zero's sign)
    let mut fsum = -dot;
    fsum += (t.ref_angle - t.angle) * t.spring_gain;
    fsum -= t.damping * t.rate;
    fsum += calls.slip_term(this);
    fsum += calls.limit_sum(this);

    // asm-matching sign: 1 above zero, -1 below, 0 at zero or NaN
    let sgn = if t.rate > 0.0 {
        1.0_f32
    } else if 0.0 > t.rate {
        -1.0
    } else {
        0.0
    };
    let mut b4 = sgn * -0.3;
    let mut skip_fsum = false;
    if t.rate == 0.0 {
        let af = if fsum <= 0.0 { 0.0 - fsum } else { fsum };
        let ab = if b4 <= 0.0 { 0.0 - b4 } else { b4 };
        if ab > af {
            b4 = 0.0;
            skip_fsum = true;
        }
    }
    if !skip_fsum {
        b4 += fsum;
    }

    if let Some((v90, v90b)) = calls.vote(this, frame) {
        let lever = t.vote_lever.map(|v| -v);
        let a0 = cross(lever, v90);
        let w = t.vote_weights;
        b4 += w[2] * (v90b[2] + a0[2]) + w[1] * (v90b[1] + a0[1]) + w[0] * (v90b[0] + a0[0]);
    }
    Outcome::Solved(b4 / b0 - t.settle)
}

/// `E01VecSetA::Cross`: `a x b`, via the shared `math::vec` copy (same lane order as the asm).
pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let r = Vec3 { x: a[0], y: a[1], z: a[2] }.cross3(Vec3 { x: b[0], y: b[1], z: b[2] });
    [r.x, r.y, r.z]
}
