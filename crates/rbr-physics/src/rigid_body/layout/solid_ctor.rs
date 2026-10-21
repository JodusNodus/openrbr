//! The solid's constructors (`E47SolidState`, `CSolid::CDef`, `CSolid`) and its placement (`CSolid::Vf44`).

use super::contacts::{HOOK, RING_TAIL};
use super::scratch_state::{clear_vec, store_scratch_base_init, CELL_LINK, STATE_VTABLE};
use super::{BODY_FLAG_8, ENTITY_FLAGS, FORCE_ACCUM, ORIENTATION, POSITION, SCRATCH_ACCUM, SCRATCH_SCALE};
use crate::collision::layout::grid_hosts::AllocHost;
use crate::collision::layout::state::body_def_ctor;
use crate::collision::layout::{Body, CtorHost};
use crate::mem::Mem;
use crate::physics_core::layout as core_layout;
use crate::physics_core::layout::construct::Heap;
use crate::rigid_body::host::{HostUnavailable, SolidCtorHost};
use crate::{collision, ntweak};

/// `E47SolidState::ctor` (0x52d280, the state's `E47SolidState(owner, name, unused)`): the NTweak base and the tweak
/// members (`ntweak`'s constructors), with the vtable and the fields around them in the asm's order. The owner is kept
/// only when its flags (+0x60) have bit 2, and the unused third argument is not here.
pub fn construct_state(mem: &mut Mem, state: u32, owner: u32, name: u32) {
    ntweak::construct_base(mem, state, name);
    mem.set_u32(state, STATE_VTABLE);
    ntweak::construct_vector4(mem, state + 0x10);
    mem.set_u32(state + 0x80, 0);
    mem.set_u32(state + 0x84, 0);
    mem.set_u32(state + 0x88, 0);
    mem.set_u32(state + 0x8c, owner);
    ntweak::construct_real(mem, state + 0x90);
    ntweak::construct_vector(mem, state + 0xa0);
    ntweak::construct_vector(mem, state + 0xd0);
    ntweak::construct_unsigned(mem, state + 0xf0);
    ntweak::construct_unsigned(mem, state + 0x100);
    ntweak::construct_vector(mem, state + 0x110);
    ntweak::construct_vector(mem, state + 0x130);
    let kept = if owner != 0 && mem.u8(owner + 0x60) & 4 != 0 { owner } else { 0 };
    mem.set_u32(state + 0xc0, kept);
    for at in [0x170, 0x180, 0x150, 0x160] {
        clear_vec(mem, state + at);
    }
    mem.set_f32(state + 0x70, 1.0);
    mem.set_f32(state + 0x74, 1.0);
    mem.set_f32(state + 0x78, 1.0);
    mem.set_f32(state + 0x7c, 0.0);
    mem.set_u32(state + 0x194, 0);
    mem.set_f32(state + 0x190, 0.0);
}

/// The size `CSolid::NewDef` asks the heap for: the definition block (`CSolid::CDef`).
pub const DEF_SIZE: u32 = 0x1F0;

/// `CSolid::CDef`'s vtable (0x73fafc), at +0 after the body definition's base.
const SOLID_DEF_VTABLE: u32 = 0x0073_FAFC;

/// `CSolid::CDef::CDef(outer)` (0x529840, thiscall): the body definition's base (`body_def_ctor`), the vtable, then the
/// tweaks at +0x120 to +0x1e0 in the asm's order.
pub fn solid_def_ctor(mem: &mut Mem, def: u32, outer: u32) {
    body_def_ctor(mem, def, outer);
    mem.set_u32(def, SOLID_DEF_VTABLE);
    ntweak::construct_vector(mem, def + 0x120);
    ntweak::construct_unsigned(mem, def + 0x140);
    ntweak::construct_real(mem, def + 0x150);
    ntweak::construct_real(mem, def + 0x160);
    ntweak::construct_real(mem, def + 0x170);
    ntweak::construct_vector(mem, def + 0x180);
    ntweak::construct_unsigned(mem, def + 0x1a0);
    ntweak::construct_real(mem, def + 0x1b0);
    ntweak::construct_real(mem, def + 0x1c0);
    ntweak::construct_real(mem, def + 0x1d0);
    ntweak::construct_unsigned(mem, def + 0x1e0);
}

/// `CSolid`'s vtable (0x73fc54), stored after the body's base.
const SOLID_VTABLE: u32 = 0x0073_fc54;
/// The solid's state (`E47SolidState`) and its scratch.
pub(super) const SOLID_STATE: u32 = 0xe0;
pub(super) const SOLID_SCRATCH: u32 = 0x280;

/// `CSolid::CSolid(def, a, b, flags)` (0x52d480, thiscall) on the body at `this`: the body's base (`CBody::CBody`,
/// `collision::layout::ctor`, with the flags' 0x4), the vtable, the state (named by the definition, or 0), the scratch
/// (`E47_ScratchBaseInit`, inline), then the words after it in the asm's order (it clears the force and the scratch
/// vectors twice). Returns `this`.
pub fn construct_solid(
    mem: &mut Mem,
    host: &mut impl CtorHost,
    this: u32,
    def: u32,
    a: u32,
    b: u32,
    flags: u32,
) -> u32 {
    collision::layout::ctor(host, mem, this, def, a, b, flags | 4);
    mem.set_u32(this, SOLID_VTABLE);
    let name = if def != 0 { ntweak::name_or_null(mem, def) } else { 0 };
    construct_state(mem, this + SOLID_STATE, this, name);
    store_scratch_base_init(mem, this + SOLID_SCRATCH);
    mem.set_u32(this + HOOK, 0);
    mem.set_u32(this + HOOK_SET, 0);
    mem.set_f32(this + SCRATCH_SCALE, 1.0);
    clear_vec(mem, this + FORCE_ACCUM);
    clear_vec(mem, this + SCRATCH_ACCUM);
    mem.set_u32(this + RING_TAIL, 0);
    this
}

/// The size `CSolid::CDef::Vf50` asks the heap for: the body (`CSolid`).
pub const SOLID_SIZE: u32 = 0x3D0;

/// `CSolid::NewDef` (0x52b8b0): a definition block from the heap, built by the `CDef` constructor on `outer`. Returns
/// the block, or 0 when the allocation fails (the constructor is not run then).
pub fn new_def(mem: &mut Mem, outer: u32, host: &mut impl Heap) -> u32 {
    let block = host.alloc(mem, DEF_SIZE);
    if block != 0 {
        solid_def_ctor(mem, block, outer);
    }
    block
}

/// `CSolid::CDef::Vf50` (0x52d640): a body from the heap, built by the `CSolid` constructor on this definition with `a`
/// and `b`. Returns the body, or 0 when the allocation fails.
pub fn cdef_new_solid(
    mem: &mut Mem,
    def: u32,
    a: u32,
    b: u32,
    host: &mut (impl Heap + SolidCtorHost),
) -> Result<u32, HostUnavailable> {
    let block = host.alloc(mem, SOLID_SIZE);
    if block != 0 {
        host.solid_ctor(mem, block, def, a, b, 0)?;
    }
    Ok(block)
}

/// The calls `CSolid::Vf44` (0x52c700) makes outside the body: the cell grid's link blocks (`CCellGrid::LookupTwin`,
/// 0x4e03b0) and the body's slot 16, `this->Vf40()`, which `CEntity::Vf44` (0x519e60) makes under `CBody::Vf44`.
pub trait PlaceHost: AllocHost {
    fn vf40(&mut self, mem: &mut Mem, this: u32) -> Result<(), HostUnavailable>;
}

/// A placement (`CSigStuff`): its position, orientation, extent, sample word (`+0x30`) and hook (`+0x48`).
const STUFF_POSITION: u32 = 0x0;
const STUFF_ORIENTATION: u32 = 0x10;
const STUFF_EXTENT: u32 = 0x20;
const STUFF_SAMPLE: u32 = 0x30;
const STUFF_HOOK: u32 = 0x48;
/// The `DbgFlags` bit that takes the placement's orientation (else the identity).
const DBG_ORIENTATION: u32 = 0x400;
/// The body's extent (`+0x150`; `+0x15c` becomes 1.0 unless the extent is (1, 1, 1)), the hook flag word, the sample
/// tweak (`CReal`, value at `+0xc`); the cell link is [`CELL_LINK`].
const EXTENT: u32 = 0x150;
const HOOK_SET: u32 = 0x3C0;
const SAMPLE: u32 = 0x170;

/// `CSolid::Vf44(stuff)` (0x52c700) on the body at `this`: the placement at `stuff` gives the orientation (with
/// `DbgFlags` bit 0x400, else the identity), the extent, the hook and the position; the sample tweak is marked dirty
/// and takes the placement's word; the body moves to the grid cell under its position; then `CBody::Vf44` (0x4e1140):
/// the grid `E36SelectTable` picks for the placement, and the body's slot 16. A body with flag 8 and no hook gets a new
/// `E36DamageOps` (0x3eb0 bytes, 0x4ef410): not ported.
pub fn place_solid(mem: &mut Mem, host: &mut impl PlaceHost, this: u32, stuff: u32) -> Result<(), HostUnavailable> {
    let one = 1.0f32.to_bits();
    let orientation = if core_layout::cdef_flags(mem) & DBG_ORIENTATION != 0 {
        std::array::from_fn(|i| mem.u32(stuff + STUFF_ORIENTATION + 4 * i as u32))
    } else {
        [0, 0, 0, one]
    };
    for (i, word) in (0..).zip(orientation) {
        mem.set_u32(this + ORIENTATION + 4 * i, word);
    }
    let extent: [f32; 3] = mem.f32s(stuff + STUFF_EXTENT);
    for (i, value) in (0..).zip(extent) {
        mem.set_f32(this + EXTENT + 4 * i, value);
    }
    if extent.iter().any(|&v| v != 1.0) {
        mem.set_u32(this + EXTENT + 0xc, one);
    }
    mem.set_u32(this + HOOK_SET, 0);
    let hook = mem.u32(stuff + STUFF_HOOK);
    if mem.u8(this + ENTITY_FLAGS) & BODY_FLAG_8 != 0 && hook == 0 {
        return Err(HostUnavailable);
    }
    mem.set_u32(this + HOOK, hook);
    for i in 0..3 {
        mem.set_u32(this + POSITION + 4 * i, mem.u32(stuff + STUFF_POSITION + 4 * i));
    }
    ntweak::set_dirty(mem, this + SAMPLE);
    mem.set_u32(this + SAMPLE + 0xc, mem.u32(stuff + STUFF_SAMPLE));
    let (x, y) = (mem.f32(this + POSITION), mem.f32(this + POSITION + 4));
    let cell = collision::layout::lookup_twin(host, mem, core_layout::cell_grid(mem), x, y);
    collision::layout::move_to_cell(mem, this + CELL_LINK, cell);
    let grid = collision::layout::select_table(mem, stuff);
    Body::new(this).set_grid(mem, grid);
    host.vf40(mem, this)
}
