//! 0x531fc0 `Align`: the frame of the contact plane from two vectors `a` and `b`. It writes `n = a × b` into the
//! contact frame's second row; when `|n| > 0.1` it normalises `n` there (all four lanes), writes `c = n × a` into the
//! first row (x, y, z), copies `a` to the third row, and writes the dot products `0 - a·b` (`+0x3ac`) and `c·b`
//! (`+0x3b0`). Returns whether it aligned (eax). The vectors are read once on entry: Step's `g` and `i` are not
//! scratch words.

use super::layout::Frame;
use crate::math::vec::Vec4;

/// `Align` (thiscall; the vectors `a` and `b` are Step's `g` and `i`).
pub fn align(f: &mut Frame, a: Vec4, b: Vec4) -> bool {
    let rows = &mut f.scratch.contact_frame;
    let n = a.cross3(b);
    rows[1].x = n.x;
    rows[1].y = n.y;
    rows[1].z = n.z;
    let len = n.length3();
    if !(len > 0.1) {
        return false;
    }
    // The four lanes are scaled, so the stale w of the row is scaled too (the asm's `mulps`).
    let inv = 1.0 / len;
    let n = Vec4 { w: rows[1].w, ..n }.scale(inv);
    rows[1] = n;
    let c = n.cross3(a);
    rows[0].x = c.x;
    rows[0].y = c.y;
    rows[0].z = c.z;
    rows[2] = a;
    f.scratch.f3ac = 0.0 - (a.y * b.y + a.z * b.z + a.x * b.x);
    f.scratch.scale_3b0 = c.y * b.y + c.z * b.z + c.x * b.x;
    true
}
