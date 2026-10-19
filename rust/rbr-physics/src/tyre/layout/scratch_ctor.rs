//! `CTyre::CScratch`'s constructor, `Init`, creation and tear-downs.

use crate::entities::def::BASE_VTABLE;
use crate::mem::Mem;
use crate::physics_core::construct::{FLAGS_NAME, REAL_NAME, REAL_VTABLE, UNSIGNED_NAME, UNSIGNED_VTABLE};
use crate::physics_core::material::{FLAGS_VTABLE as MATERIAL_FLAGS_VTABLE, VTABLE as MATERIAL_VTABLE};
use crate::rigid_body::layout::SCRATCH_VTABLE as E47_SCRATCH_VTABLE;

use super::construct::{SCRATCH_SUB_VTABLE, SCRATCH_VTABLE};
use super::{DEF, STATE};

/// `CTyre::CScratch::Init` (0x5327d0) writes, as (offset, bits), relative to the scratch's sub-block (`this+0x20`, the
/// `m_sub` the constructor initialises). The material (`+0x160`) and the rigid body's scratch words are in it.
const SCRATCH_INIT_DEFAULTS: [(u32, u32); 162] = [
    (0x000, SCRATCH_SUB_VTABLE),
    (0x010, 0x3F800000),
    (0x020, 0x00000000),
    (0x024, 0x00000000),
    (0x028, 0x00000000),
    (0x02C, 0x00000000),
    (0x060, 0x00000000),
    (0x064, 0x00000000),
    (0x068, 0x00000000),
    (0x06C, 0x00000000),
    (0x070, 0x00000000),
    (0x074, 0x00000000),
    (0x078, 0x00000000),
    (0x07C, 0x00000000),
    (0x080, 0x00000000),
    (0x084, 0x00000000),
    (0x088, 0x00000000),
    (0x08C, 0x00000000),
    (0x090, 0x00000000),
    (0x094, 0x00000000),
    (0x098, 0x00000000),
    (0x09C, 0x00000000),
    (0x0A0, 0x00000000),
    (0x0A4, 0x00000000),
    (0x0A8, 0x00000000),
    (0x0AC, 0x00000000),
    (0x0B0, 0x00000000),
    (0x0B4, 0x00000000),
    (0x0B8, 0x00000000),
    (0x0BC, 0x00000000),
    (0x0C0, 0x00000000),
    (0x0C4, 0x00000000),
    (0x0C8, 0x00000000),
    (0x0CC, 0x00000000),
    (0x0D0, 0x00000000),
    (0x0D4, 0x00000000),
    (0x0D8, 0x00000000),
    (0x0DC, 0x00000000),
    (0x0E0, 0x00000000),
    (0x0E4, 0x00000000),
    (0x0E8, 0x00000000),
    (0x0EC, 0x00000000),
    (0x0F0, 0x00000000),
    (0x0F4, 0x00000000),
    (0x0F8, 0x00000000),
    (0x0FC, 0x00000000),
    (0x100, 0x00000000),
    (0x104, 0x00000000),
    (0x108, 0x00000000),
    (0x10C, 0x00000000),
    (0x110, 0x00000000),
    (0x120, 0x00000000),
    (0x124, 0x00000000),
    (0x128, 0x00000000),
    (0x12C, 0x00000000),
    (0x130, 0x00000000),
    (0x134, 0x00000000),
    (0x180, MATERIAL_VTABLE),
    (0x184, 0x00000000),
    (0x188, 0x00000000),
    (0x18C, REAL_VTABLE),
    (0x190, REAL_NAME),
    (0x194, 0x00000000),
    (0x198, 0x00000000),
    (0x19C, REAL_VTABLE),
    (0x1A0, REAL_NAME),
    (0x1A4, 0x00000000),
    (0x1A8, 0x00000000),
    (0x1AC, REAL_VTABLE),
    (0x1B0, REAL_NAME),
    (0x1B4, 0x00000000),
    (0x1B8, 0x00000000),
    (0x1BC, REAL_VTABLE),
    (0x1C0, REAL_NAME),
    (0x1C4, 0x00000000),
    (0x1C8, 0x00000000),
    (0x1CC, REAL_VTABLE),
    (0x1D0, REAL_NAME),
    (0x1D4, 0x00000000),
    (0x1D8, 0x00000000),
    (0x1DC, REAL_VTABLE),
    (0x1E0, REAL_NAME),
    (0x1E4, 0x00000000),
    (0x1E8, 0x00000000),
    (0x1EC, REAL_VTABLE),
    (0x1F0, REAL_NAME),
    (0x1F4, 0x00000000),
    (0x1F8, 0x00000000),
    (0x1FC, REAL_VTABLE),
    (0x200, REAL_NAME),
    (0x204, 0x00000000),
    (0x208, 0x00000000),
    (0x20C, REAL_VTABLE),
    (0x210, REAL_NAME),
    (0x214, 0x00000000),
    (0x218, 0x00000000),
    (0x21C, REAL_VTABLE),
    (0x220, REAL_NAME),
    (0x224, 0x00000000),
    (0x228, 0x00000000),
    (0x22C, REAL_VTABLE),
    (0x230, REAL_NAME),
    (0x234, 0x00000000),
    (0x238, 0x00000000),
    (0x23C, REAL_VTABLE),
    (0x240, REAL_NAME),
    (0x244, 0x00000000),
    (0x248, 0x00000000),
    (0x24C, REAL_VTABLE),
    (0x250, REAL_NAME),
    (0x254, 0x00000000),
    (0x258, 0x00000000),
    (0x25C, REAL_VTABLE),
    (0x260, REAL_NAME),
    (0x264, 0x00000000),
    (0x268, 0x00000000),
    (0x26C, REAL_VTABLE),
    (0x270, REAL_NAME),
    (0x274, 0x00000000),
    (0x278, 0x00000000),
    (0x27C, REAL_VTABLE),
    (0x280, REAL_NAME),
    (0x284, 0x00000000),
    (0x288, 0x00000000),
    (0x28C, REAL_VTABLE),
    (0x290, REAL_NAME),
    (0x294, 0x00000000),
    (0x298, 0x00000000),
    (0x29C, REAL_VTABLE),
    (0x2A0, REAL_NAME),
    (0x2A4, 0x00000000),
    (0x2A8, 0x00000000),
    (0x2AC, REAL_VTABLE),
    (0x2B0, REAL_NAME),
    (0x2B4, 0x00000000),
    (0x2B8, 0x00000000),
    (0x2BC, UNSIGNED_VTABLE),
    (0x2C0, UNSIGNED_NAME),
    (0x2C4, 0x00000000),
    (0x2C8, 0x00000000),
    (0x2CC, MATERIAL_FLAGS_VTABLE),
    (0x2D0, FLAGS_NAME),
    (0x2D4, 0x00000000),
    (0x2D8, 0x00000000),
    (0x2DC, 0x00000000),
    (0x2E0, 0x00000000),
    (0x2E4, 0x00000000),
    (0x2F8, 0x00000000),
    (0x30C, 0x00000000),
    (0x310, 0x00000000),
    (0x314, 0x00000000),
    (0x318, 0x00000000),
    (0x31C, 0x00000000),
    (0x320, 0x00000000),
    (0x324, 0x00000000),
    (0x340, 0x00000000),
    (0x344, 0x00000000),
    (0x348, 0x00000000),
    (0x34C, 0x00000000),
    (0x350, 0x00000000),
    (0x38C, 0x00000000),
    (0x390, 0x00000000),
];

/// `CTyre::CScratch::Init` (0x5327d0, thiscall) on the sub-block at `at`: writes the defaults.
pub fn init_scratch(mem: &mut Mem, at: u32) {
    for (off, bits) in SCRATCH_INIT_DEFAULTS {
        mem.set_u32(at + off, bits);
    }
}

/// The scratch constructor's own words (its vtable, the `m_wear` zero and the sub-block initialisers' zeros), without
/// the arguments: `+0x10` (def), `+0x14` (state) and `+0x410` (the record) are written by `construct_scratch`.
const SCRATCH_DEFAULTS: [(u32, u32); 18] = [
    (0x000, SCRATCH_VTABLE),
    (0x414, 0x00000000),
    (0x420, 0x00000000),
    (0x424, 0x00000000),
    (0x470, 0x00000000),
    (0x474, 0x00000000),
    (0x478, 0x00000000),
    (0x47C, 0x00000000),
    (0x480, 0x00000000),
    (0x4C8, 0x00000000),
    (0x4CC, 0x00000000),
    (0x4D0, 0x00000000),
    (0x504, 0x00000000),
    (0x508, 0x00000000),
    (0x50C, 0x00000000),
    (0x510, 0x00000000),
    (0x514, 0x00000000),
    (0x518, 0x00000000),
];

/// `CTyre::CScratch::CScratch(def, state, record)` (0x5328d0, thiscall, three stack words, `ret 0xC`) on the scratch at
/// `this`. It runs `Init` on the sub-block at `this+0x20`, writes its own defaults, then stores the three pointers.
pub fn construct_scratch(mem: &mut Mem, this: u32, def: u32, state: u32, record: u32) {
    init_scratch(mem, this + 0x20);
    for (at, bits) in SCRATCH_DEFAULTS {
        mem.set_u32(this + at, bits);
    }
    mem.set_u32(this + DEF, def);
    mem.set_u32(this + STATE, state);
    mem.set_u32(this + 0x410, record);
}

/// The scratch's allocation size: `operator new(0x3f0)` in `CreateScratch`.
pub const SCRATCH_SIZE: u32 = 0x3F0;

/// `CTyre::CDef::CreateScratch` (0x535220, thiscall) after its allocation: `block` is the block `operator new` returned
/// (the host allocates it, `SCRATCH_SIZE` bytes). `Init` runs on it, and the call returns the block. A null block is
/// returned unchanged, as the original does.
pub fn create_scratch(mem: &mut Mem, block: u32) -> u32 {
    if block != 0 {
        init_scratch(mem, block);
    }
    block
}

/// `NTweak::CBase` headers `NPhysics::CTyre` tear-downs write: the vtable and a zero, every 0x10 bytes.
/// `CBASE_HEADERS_17` is `FUN_005324a0` (0x5324a0, 17 headers), and `CBASE_HEADERS_3` is `FUN_00532550` (3 headers).
pub fn cbase_headers_17(mem: &mut Mem, at: u32) {
    for k in 0..17 {
        mem.set_u32(at + 0x10 * k, BASE_VTABLE);
        mem.set_u32(at + 0x10 * k + 4, 0);
    }
}

/// See `cbase_headers_17`: the three headers of `FUN_00532550` (0x532550).
pub fn cbase_headers_3(mem: &mut Mem, at: u32) {
    for k in 0..3 {
        mem.set_u32(at + 0x10 * k, BASE_VTABLE);
        mem.set_u32(at + 0x10 * k + 4, 0);
    }
}

/// The scratch's destructor body (`CScratch::CScratch()`, 0x532570), as (offset, bits): the material's bitfield and
/// CBase headers, the rigid body's vtable, and the sub-block's words. Recorded with the flags' pointer null; the
/// CBitField destructor calls through that pointer only when it is not null (see the README).
const SCRATCH_BODY_DEFAULTS: [(u32, u32); 42] = [
    (0x000, E47_SCRATCH_VTABLE),
    (0x180, BASE_VTABLE),
    (0x184, 0x00000000),
    (0x18C, BASE_VTABLE),
    (0x190, 0x00000000),
    (0x19C, BASE_VTABLE),
    (0x1A0, 0x00000000),
    (0x1AC, BASE_VTABLE),
    (0x1B0, 0x00000000),
    (0x1BC, BASE_VTABLE),
    (0x1C0, 0x00000000),
    (0x1CC, BASE_VTABLE),
    (0x1D0, 0x00000000),
    (0x1DC, BASE_VTABLE),
    (0x1E0, 0x00000000),
    (0x1EC, BASE_VTABLE),
    (0x1F0, 0x00000000),
    (0x1FC, BASE_VTABLE),
    (0x200, 0x00000000),
    (0x20C, BASE_VTABLE),
    (0x210, 0x00000000),
    (0x21C, BASE_VTABLE),
    (0x220, 0x00000000),
    (0x22C, BASE_VTABLE),
    (0x230, 0x00000000),
    (0x23C, BASE_VTABLE),
    (0x240, 0x00000000),
    (0x24C, BASE_VTABLE),
    (0x250, 0x00000000),
    (0x25C, BASE_VTABLE),
    (0x260, 0x00000000),
    (0x26C, BASE_VTABLE),
    (0x270, 0x00000000),
    (0x27C, BASE_VTABLE),
    (0x280, 0x00000000),
    (0x28C, BASE_VTABLE),
    (0x290, 0x00000000),
    (0x29C, BASE_VTABLE),
    (0x2A0, 0x00000000),
    (0x2AC, BASE_VTABLE),
    (0x2BC, BASE_VTABLE),
    (0x2CC, BASE_VTABLE),
];

/// `CTyre::CScratch::CScratch()`, the destructor body (0x532570, thiscall): writes the defaults on the scratch at `at`.
pub fn destroy_scratch_body(mem: &mut Mem, at: u32) {
    for (off, bits) in SCRATCH_BODY_DEFAULTS {
        mem.set_u32(at + off, bits);
    }
}

/// `CTyre::CScratch::~CScratch` (0x5329a0, thiscall): the scratch's vtable, its def and state cleared, then the body on
/// the sub-block at `+0x20`. The def's release (`CTyre::CDef::Unk54`, 0x5314e0, the def's vtable slot 0x54) is a no-op
/// and is not called.
pub fn destroy_scratch(mem: &mut Mem, this: u32) {
    mem.set_u32(this, SCRATCH_VTABLE);
    mem.set_u32(this + DEF, 0);
    mem.set_u32(this + STATE, 0);
    destroy_scratch_body(mem, this + 0x20);
}

/// `CTyre::CScratch::DeleteScratch(flags)` (0x5328b0, thiscall): the body on the scratch. Returns whether `flags` asks for
/// the block to be freed (`flags & 1`); the caller frees it.
pub fn delete_scratch(mem: &mut Mem, this: u32, flags: u32) -> bool {
    destroy_scratch_body(mem, this);
    flags & 1 != 0
}

/// `CTyre::CDef::DeleteContainer(flags)` (0x535280, thiscall): `~CScratch` on the scratch. Returns whether the block is
/// to be freed, as `delete_scratch` does.
pub fn delete_container(mem: &mut Mem, this: u32, flags: u32) -> bool {
    destroy_scratch(mem, this);
    flags & 1 != 0
}
