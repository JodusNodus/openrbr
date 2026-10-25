//! The brake walls (`E34WallVf`, `CBrakeWall`, `CBrakeWallManager`): the wall layout's constants and `VfF0`. The probes
//! are `collision::wall`, generic over `CollisionStore`.
//!
//! `brake_wall_vf_f0` stays on `Mem`: it clears the sub-block through `car_body::light::sub_block_init`.

use crate::mem::Mem;

use super::vec_ops::vec4_copy_from;
use super::wall_tree::VfF0Host;
use crate::math::vec::Vec4;

pub use crate::collision::wall::{brake_wall_vf_e8, wall_probe, wall_raycast, wall_vf_cc};

/// The wall's fields that `E34WallVf::Raycast` reads and writes: the latch word (+0x214) and the segment vertex table
/// (+0x218).
pub(super) const WALL_LATCH: u32 = 0x214;
pub(super) const WALL_MESH: u32 = 0x218;
/// The leaf's count (`seg[4]`) and index table (`seg[5]`); the query's list count is the word at 0x190 (`hits[100]`).
pub(super) const SEG_COUNT: u32 = 0x10;
pub(super) const SEG_INDEX: u32 = 0x14;
pub(super) const HITS_COUNT: u32 = 0x190;
/// The signal manager (global 0x893600) and the penetration emitter's offset in it (0xa780).
pub(super) const PENETRATION_EMITTER: u32 = 0xa780;
/// The wall's leaf list (+0x7c) that `CBrakeWall::QueryBox` fills (its count is the list's word at +0x190).
pub(super) const WALL_LEAVES: u32 = 0x7c;

/// `CBrakeWall::VfF0` (0x4e3970) on the wall at `this` with its eight pointer arguments `a`..`h`; `entry` is its entry
/// esp. In the original's `and esp, -0x10` frame `CE36SubBlock::Init` clears the block at +0x2c, then `VfE8` fills the
/// four vectors at +0xc. On a hit: `CopyFrom` of the first vector to `c`, the frame word at +8 points at the second,
/// which goes to `e`; the fourth's `y` goes to `d`, and `f`, `g` and `h` get 0. Returns 1 on a hit, else 0 (eax).
pub fn brake_wall_vf_f0(host: &mut impl VfF0Host, mem: &mut Mem, this: u32, args: [u32; 8], entry: u32) -> u32 {
    let [a, b, c, d, e, f, g, h] = args;
    let frame = ((entry - 4) & !0xf) - 0x4c;
    let out = frame + 0xc;
    crate::car_body::light::sub_block_init(mem, frame + 0x2c);
    if host.vf_e8(frame - 0x1c, this, a, b, out) == 0 {
        return 0;
    }
    vec4_copy_from(mem, c, out);
    mem.set_u32(frame + 8, out + 0x10);
    Vec4::load(mem, out + 0x10).store(mem, e);
    let y = mem.u32(out + 0x34);
    mem.set_u32(d, y);
    for at in [f, g, h] {
        mem.set_u32(at, 0);
    }
    1
}
