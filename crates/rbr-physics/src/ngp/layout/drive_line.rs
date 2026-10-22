//! The drive line in memory, [`crate::ngp::drive_line`]: NGP's sampler `0x10015fc0`, the target of the calls of
//! `CDriveLine::SampleBig` (`0x50b300`) that NGP retargets (`CSolid::VfC8`, `E35CarOps::Step230`), with its own
//! `ClampMix` `0x100161c0` and `BlendB` `0x10016aa0`; and NGP's `BlendB` and `LerpAt` rewrites at `CCar::Split17C`'s
//! calls (`0x4e555f`, `0x4e5571`).

use crate::engine_driveline::layout::{entry_at, COUNT_AT};
use crate::mem::Mem;
use crate::ngp::drive_line::{hermite_point_f32, project, tangent_lerp_f32, Line, V3};

/// The line of the drive object `this`: its entries' positions, and their tangents 0x10 after.
struct MemLine<'a> {
    mem: &'a Mem,
    this: u32,
}

fn load(mem: &Mem, at: u32) -> V3 {
    [0, 4, 8].map(|k| f64::from(mem.f32(at + k)))
}

impl Line for MemLine<'_> {
    fn count(&self) -> u32 {
        self.mem.u32(self.this + COUNT_AT)
    }

    fn position(&self, k: u32) -> V3 {
        load(self.mem, entry_at(self.this, k))
    }

    fn tangent(&self, k: u32) -> V3 {
        load(self.mem, entry_at(self.this, k) + 0x10)
    }
}

/// `0x10016d40` (thiscall on the drive line, `(i, t, out)`, `ret 0xc`): the Hermite point in doubles, rounded to f32
/// lane by lane. `out`'s fourth lane is not written.
pub fn blend_b_f32(mem: &Mem, this: u32, idx: i32, t: f32) -> [f32; 3] {
    hermite_point_f32(&MemLine { mem, this }, idx, t)
}

/// `0x10016960` (thiscall on the drive line, `(i, t, out)`, `ret 0xc`): the tangents of `i` and `i + 1` lerped. `out`'s
/// fourth lane is not written.
pub fn lerp_at_f32(mem: &Mem, this: u32, idx: i32, t: f32) -> [f32; 3] {
    tangent_lerp_f32(&MemLine { mem, this }, idx, t)
}

/// `0x10015fc0` (thiscall on the drive line, `(pos, x)`, `ret 8`, st0): the position along the line of the point at
/// `pos` (three f32) from the index `x`.
pub fn sample(mem: &Mem, this: u32, pos: u32, x: f32) -> f32 {
    let p = [0, 4, 8].map(|k| mem.f32(pos + k));
    project(&MemLine { mem, this }, p, x)
}
