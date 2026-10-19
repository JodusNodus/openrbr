//! The body's scratch (`E47SolidScratch`) and state (`E47SolidState`) words: the resets, the state rotation and
//! the scratch calls (`Vf39x`, `E43Vf20`, `Vf16Helper`, `E43Vf74`).

use super::solid_ctor::{SOLID_SCRATCH, SOLID_STATE};
use super::{POSITION, SCRATCH_ACCUM, SCRATCH_SCALE};
use crate::collision;
use crate::collision::body::Vf40Host;
use crate::math::quat::Quat;
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::physics_core::{layout as core_layout, math::Vec4Rows};
use crate::rigid_body::host::{HostUnavailable, Vf20Host, Vf54Host};
use crate::rigid_body::rotation::{self, MulHost};
use crate::rigid_body::scratch::{FlushHost, FLUSH_BIT};

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

/// `E47SolidState::Vf17x` (0x529a50): `state` is the body's state at +0xe0. Refreshes its matrix (`+0x30`, which is the
/// body's `+0x110`) from the orientation (`+0x20`), scaled by the diagonal (`+0x70..0x78`) when the gate (`+0x7c`) is
/// positive.
pub fn refresh_state_rotation(mem: &mut Mem, state: u32, host: &mut impl MulHost) -> Result<(), HostUnavailable> {
    let q = load_quat(mem, state + 0x20);
    let gate = mem.f32(state + 0x7c);
    let diag = [mem.f32(state + 0x70), mem.f32(state + 0x74), mem.f32(state + 0x78)];
    let m = rotation::refresh_rotation(q, gate, diag, host)?;
    for (k, v) in m.into_iter().enumerate() {
        mem.set_f32(state + 0x30 + 4 * k as u32, v);
    }
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

/// `E47Solid::Vf39x` (0x52ac80), `E43Vf9C` in the C++: scales the `n` rows at `dst` by the matrix (`+0x110`), then adds
/// the origin (`+0x140`) to them. Returns `dst`. The original's two row calls (`E46Mat::ScaleRows`, `E46Vec::Add`) are
/// `Mat::scale_rows` and `Vec4Rows::add_each`. The matrix and origin are read before the rows are written, so `dst`
/// must not overlap them (the recorded calls never do).
pub fn scale_add_rows(mem: &mut Mem, this: u32, dst: u32, n: u32) -> u32 {
    let matrix = core_layout::mat(mem, this + 0x110);
    let origin = Vec4::load(mem, this + POSITION);
    let mut rows = core_layout::vec4s(mem, dst, n as usize);
    matrix.scale_rows(&mut rows);
    origin.add_each(&mut rows);
    core_layout::set_vec4s(mem, dst, &rows);
    dst
}

/// `CSolid::E43Vf20` (0x529200), the slot-8 entry: a bare jump to the entity walk `CEntity::E43Vf20` (0x51a3a0) with
/// the same stack argument. Returns the walk's result.
pub fn entity_vf20_jump(body: u32, arg: u32, host: &mut impl Vf20Host) -> u32 {
    host.entity_walk_s20(body, arg)
}

/// `E47_Vf16Helper` (0x52b6d0), `CSolid::Vf40` in the C++: `Vf17x` on the state at `+0xe0` twice, then the tail jump to
/// `CBody::Vf40` (0x4e1120) on the body. Returns that call's result.
pub fn vf16_helper(mem: &mut Mem, this: u32, host: &mut (impl MulHost + Vf40Host)) -> Result<u32, HostUnavailable> {
    let state = this + SOLID_STATE;
    refresh_state_rotation(mem, state, host)?;
    refresh_state_rotation(mem, state, host)?;
    Ok(collision::layout::reset_state_and_base(host, this))
}

/// `CSolid::E43Vf74` (0x529ea0): the scratch at `+0x310` gets `pp * scale` (`+0xd0`) added; then the flush runs when
/// `gate` has its bit (the caller reads the gate from the app). The same sum as `scratch::accumulate`, on memory.
pub fn scratch_accumulate(
    mem: &mut Mem,
    this: u32,
    pp: u32,
    gate: u32,
    host: &mut impl FlushHost,
) -> Result<(), HostUnavailable> {
    let scale = mem.f32(this + SCRATCH_SCALE);
    let scratch = Vec4::load(mem, this + SCRATCH_ACCUM);
    let sum = scratch.add_scaled(Vec4::load(mem, pp), scale);
    core_layout::set_vec4s(mem, this + SCRATCH_ACCUM, &[sum]);
    if gate & FLUSH_BIT != 0 {
        host.flush()?;
    }
    Ok(())
}
