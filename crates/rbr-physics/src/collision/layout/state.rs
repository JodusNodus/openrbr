//! E33 body definition, state, and scratch leaf helpers.

pub use crate::entities::layout::ENTITY_DEF_VTABLE;
use crate::mem::Mem;
pub use crate::ntweak::layout::{BASE_VTABLE as CBASE_VTABLE, REAL_NAME as CREAL_NAME, REAL_VTABLE as CREAL_VTABLE};

pub const BODY_DEF_NAME: u32 = 0x737c24;
pub const BODY_DEF_VTABLE: u32 = 0x737bc4;
pub const BODY_STATE_TABLE: u32 = 0x8eb798;
pub const BODY_STATE_VTABLE: u32 = 0x737c34;
pub const BODY_SCRATCH_VTABLE: u32 = 0x737c8c;
pub const CDEF_SIZE: usize = 0x20;
pub const CSTATE_OWNER_OFFSET: u32 = 0x30;
pub const CSTATE_REAL_OFFSET: u32 = 0x34;

/// CBody::CDef::~CDef (0x4e1000), including E43EntityDef's intrusive-list release.
pub fn body_def_dtor(mem: &mut Mem, this: u32) -> u32 {
    mem.set_u32(this, BODY_DEF_VTABLE);
    mem.set_u32(this, ENTITY_DEF_VTABLE);

    let owner = mem.u32(this + 0x14);
    let eax = if owner == 0 {
        0
    } else {
        mem.set_u32(owner + 8, mem.u32(owner + 8).wrapping_sub(1));

        let next = mem.u32(this + 0x0c);
        let previous = mem.u32(this + 0x10);
        let eax = if next != 0 {
            mem.set_u32(next + 0x10, previous);
            if previous != 0 {
                mem.set_u32(previous + 0x0c, next);
                previous
            } else {
                mem.set_u32(owner, next);
                owner
            }
        } else if previous != 0 {
            mem.set_u32(previous + 0x0c, 0);
            mem.set_u32(owner + 4, previous);
            owner
        } else {
            mem.set_u32(owner, 0);
            mem.set_u32(owner + 4, 0);
            owner
        };

        mem.set_u32(this + 0x0c, 0);
        mem.set_u32(this + 0x10, 0);
        mem.set_u32(this + 0x14, 0);
        eax
    };

    mem.set_u32(this, CBASE_VTABLE);
    mem.set_u32(this + 4, 0);
    eax
}

/// CBody::CDef::CDef (0x4e0fd0).
pub fn body_def_ctor(mem: &mut Mem, this: u32, name: u32) -> u32 {
    mem.set_u32(this + 4, name);
    mem.set_u32(this + 8, 0);
    for offset in (0x0c..=0x18).step_by(4) {
        mem.set_u32(this + offset, 0);
    }
    mem.set_u32(this, BODY_DEF_VTABLE);
    this
}

/// CBody::CScratch::CScratch (0x4e3440).
pub fn scratch_ctor(mem: &mut Mem, scratch: u32) -> u32 {
    mem.set_u32(scratch, BODY_SCRATCH_VTABLE);
    scratch
}

/// CBody::CDef::Vf4c (0x4e0ff0).
pub fn body_def_name() -> u32 {
    BODY_DEF_NAME
}

/// CBody::CState::Vf20 (0x4e1050).
pub fn state_table_vf20() -> u32 {
    BODY_STATE_TABLE
}

/// CBody::CState::Vf1C (0x4e1060).
pub fn state_table_vf1c() -> u32 {
    BODY_STATE_TABLE
}

/// CBody::CState::GetOwner (0x4e1100).
pub fn state_owner(mem: &Mem, state: u32) -> u32 {
    mem.u32(state + CSTATE_OWNER_OFFSET)
}

/// Calls outside CState::Unk44 (0x4e33e0): the owner's slot-12 transform and the wall sync. `vec_a` and `vec_b` are
/// addresses in game memory that the host reads and writes.
pub trait StateHost {
    fn owner_vf30(&mut self, owner: u32) -> u32;
    fn wall_sync(&mut self, wall: u32, body: u32, vec_a: u32, vec_b: u32) -> u32;
}

/// CBody::CState::Uninit (0x4e10e0), demoting both state vptrs to CBase.
pub fn state_uninit(mem: &mut Mem, state: u32) {
    mem.set_u32(state + CSTATE_REAL_OFFSET, CBASE_VTABLE);
    mem.set_u32(state + CSTATE_REAL_OFFSET + 4, 0);
    mem.set_u32(state, CBASE_VTABLE);
    mem.set_u32(state + 4, 0);
}

/// CBody::CScratch::Vf04 (0x4e1110), an empty virtual function.
pub fn scratch_vf04(_mem: &Mem, _scratch: u32) {}
