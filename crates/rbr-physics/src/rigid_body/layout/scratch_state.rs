//! The body's scratch (`E47SolidScratch`) and state (`E47SolidState`) words: the resets and the
//! quaternion words.

use super::solid_ctor::SOLID_SCRATCH;
use crate::math::quat::Quat;
use crate::mem::Mem;
use crate::rigid_body::host::{HostUnavailable, Vf20Host, Vf54Host};

/// The scratch block (`CSolid+0x280`, `E47SolidScratch`, 0x150 bytes). `E47SolidScratch::Vf04` clears these vectors
/// (block-relative offsets, 16 bytes each; `0x20` is the four words `m_zero20`) and these words, and leaves the rest.
const SCRATCH_CLEARED_VECTORS: [u32; 11] = [0x20, 0x60, 0x70, 0x80, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0x100, 0x120];
const SCRATCH_CLEARED_WORDS: [u32; 3] = [0x110, 0x130, 0x134];

/// `E47SolidScratch::Vf04` (0x529af0, slot 1): `block` is the scratch at the body's +0x280. Clears the vectors and
/// words listed above; the pointers and the 1.0 at +0x10 are left as they are.
pub fn store_scratch_reset(mem: &mut Mem, block: u32) {
    for v in SCRATCH_CLEARED_VECTORS {
        clear_vec(mem, block + v);
    }
    for w in SCRATCH_CLEARED_WORDS {
        mem.set_u32(block + w, 0);
    }
}

/// The scratch block's vtable word (`E47SolidScratch`, set by its base init).
pub const SCRATCH_VTABLE: u32 = 0x73fb6c;

/// `E47_ScratchBaseInit` (0x52b930): `block` is the scratch at the body's +0x280. Sets the vtable word and `m_f10 =
/// 1.0`, clears `m_v90` and `m_vF0`, then runs `Vf04` (`store_scratch_reset`).
pub fn store_scratch_base_init(mem: &mut Mem, block: u32) {
    mem.set_u32(block, SCRATCH_VTABLE);
    mem.set_f32(block + 0x10, 1.0);
    clear_vec(mem, block + 0x90);
    clear_vec(mem, block + 0xf0);
    store_scratch_reset(mem, block);
}

/// `E47SolidState`'s vtable (0x73fc04), stored at +0.
pub(super) const STATE_VTABLE: u32 = 0x0073_fc04;

/// `E09Vec4::Clear` (0x465d80) on the row at `at`: four zero words.
pub fn clear_vec(mem: &mut Mem, at: u32) {
    for k in 0..4 {
        mem.set_u32(at + 4 * k, 0);
    }
}

/// The link node the cell code writes for a free body (`CSolid+0x160`, the argument of `CCar::AppendA980`).
pub const CELL_LINK: u32 = 0x160;

/// `E47_Vf54Helper` (0x52b6b0) (name ours): the entity call first, then the scratch block's reset (the helper
/// tail-jumps to the scratch's slot 1, `Vf04`). `this` is the body; its scratch block is at +0x280.
pub fn scratch_reset_after_entity(mem: &mut Mem, this: u32, host: &mut impl Vf54Host) -> Result<(), HostUnavailable> {
    host.entity_scratch_tail()?;
    store_scratch_reset(mem, this + SOLID_SCRATCH);
    Ok(())
}

pub(super) fn load_quat(mem: &Mem, at: u32) -> Quat {
    Quat { x: mem.f32(at), y: mem.f32(at + 0x4), z: mem.f32(at + 0x8), w: mem.f32(at + 0xc) }
}

pub(super) fn store_quat(mem: &mut Mem, at: u32, q: Quat) {
    mem.set_f32(at, q.x);
    mem.set_f32(at + 0x4, q.y);
    mem.set_f32(at + 0x8, q.z);
    mem.set_f32(at + 0xc, q.w);
}

/// `CSolid::E43Vf20` (0x529200), the slot-8 entry: a bare jump to the entity walk `CEntity::E43Vf20` (0x51a3a0) with
/// the same stack argument. Returns the walk's result.
pub fn entity_vf20_jump(body: u32, arg: u32, host: &mut impl Vf20Host) -> u32 {
    host.entity_walk_s20(body, arg)
}
