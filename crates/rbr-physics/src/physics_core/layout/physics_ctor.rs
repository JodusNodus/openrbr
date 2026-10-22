//! The stores of `CPhysics`'s constructors: the state image, the scratch, the physics object, its CDef, the
//! materials and the solver definition, and the create/def/physics globals.

use super::physics_object::PhysicsFront;
use crate::mem::Mem;
use crate::physics_core::layout::construct::{
    self, CDefHost, CDefImage, CState, SolverDefImage, Storage, TweakHeader, TweakReal, TweakUnsigned,
};
use crate::physics_core::layout::material::MaterialImage;
pub(super) use crate::physics_core::layout::PHYSICS_GLOBAL as PHYSICS_GLOBAL_AT;

/// Writes the words of a state image at `at` (the `CState` object). Only the fields the constructor sets are written.
pub fn store_state(mem: &mut Mem, at: u32, s: &CState) {
    store_header(mem, at, &s.base);
    store_unsigned(mem, at + 0x0c, &s.substep_count);
    let bits = s.sim_time.to_bits();
    mem.set_u32(at + 0x20, bits as u32);
    mem.set_u32(at + 0x24, (bits >> 32) as u32);
    mem.set_u32(at + 0x28, s.queue.vtable);
    mem.set_u32(at + 0x2c, s.queue.word_04);
    mem.set_u32(at + 0x30, s.queue.word_08);
    store_unsigned(mem, at + 0x440, &s.unk_440);
    for (i, list) in s.lists.iter().enumerate() {
        let list_at = at + 0x450 + 0x10 * i as u32;
        mem.set_u32(list_at, list.head);
        mem.set_u32(list_at + 4, list.unk4);
        mem.set_u32(list_at + 8, list.count);
        mem.set_u32(list_at + 12, list.flag);
    }
    store_header(mem, at + 0x490, &s.unk_490);
    store_unsigned(mem, at + 0x4a0, &s.unk_4a0);
    mem.set_u32(at + 0x4b0, s.extra);
}

fn store_header(mem: &mut Mem, at: u32, h: &TweakHeader) {
    mem.set_u32(at, h.vtable);
    mem.set_u32(at + 4, h.name);
    mem.set_u32(at + 8, h.dirty);
}

fn store_unsigned(mem: &mut Mem, at: u32, u: &TweakUnsigned) {
    store_header(mem, at, &u.header);
    mem.set_u32(at + 0x0c, u.value);
}

/// Writes a `CPhysics::CScratch`'s vtable word (+0x00).
pub fn set_scratch_vtable(mem: &mut Mem, this: u32, vtable: u32) {
    mem.set_u32(this, vtable);
}

/// The `CScratch` block `Vf04` clears: the float at +0xe840 and the words +0xe844..+0xe884 are zero, and +0xe888,
/// the index of the top connection handler, is -1 (empty).
pub fn clear_scratch_block(mem: &mut Mem, this: u32) {
    mem.set_f32(this + 0xe840, 0.0);
    for at in (0xe844..=0xe884).step_by(4) {
        mem.set_u32(this + at, 0);
    }
    mem.set_u32(this + 0xe888, u32::MAX);
}

/// The offsets of the six signal connections of `CPhysics` (`E46Conn`, 0x18 bytes each).
pub const CONN_AT: [u32; 6] = [0x64, 0x7c, 0x94, 0xac, 0xc4, 0xdc];

/// Writes the words `CPhysics` gets before its first call (see `physics::front`): the vtable, the log storage, the
/// function pointers, each connection's vtable, owner and callback (its links cleared), the def and two zeros.
pub fn store_physics_front(mem: &mut Mem, at: u32, f: &PhysicsFront) {
    mem.set_u32(at, f.vtable);
    for (i, word) in f.log.iter().enumerate() {
        mem.set_u32(at + 0x2c + 4 * i as u32, *word);
    }
    for (i, word) in f.funcs.iter().enumerate() {
        mem.set_u32(at + 0x44 + 4 * i as u32, *word);
    }
    for (&c, conn) in CONN_AT.iter().zip(&f.conns) {
        mem.set_u32(at + c, conn.vtable);
        mem.set_u32(at + c + 4, 0);
        mem.set_u32(at + c + 8, 0);
        mem.set_u32(at + c + 0xc, 0);
        mem.set_u32(at + c + 0x10, conn.owner);
        mem.set_u32(at + c + 0x14, conn.callback);
    }
    mem.set_u32(at + 0x24, f.def);
    mem.set_u32(at + 0x5c, f.unk5c);
    mem.set_u32(at + 0x60, f.unk60);
}

/// The state (+0x20) and the scratch (+0x28) of a `CPhysics`; both zero when there is no def.
pub fn set_physics_pointers(mem: &mut Mem, at: u32, state: u32, scratch: u32) {
    mem.set_u32(at + 0x20, state);
    mem.set_u32(at + 0x28, scratch);
}

/// The words the constructor sets after the factories: +0x04, +0x08, +0x0c, +0x14, +0x18 and +0x1c are zero, and
/// +0xf4 (`m_unkF4`) is `unk_f4`.
pub fn set_physics_tail(mem: &mut Mem, at: u32, unk_f4: u32) {
    for off in [0x04, 0x08, 0x0c, 0x14, 0x18, 0x1c] {
        mem.set_u32(at + off, 0);
    }
    mem.set_u32(at + 0xf4, unk_f4);
}

/// The hook result (+0x40, `m_unk40`) and +0x54 (`m_unk54`, zero) of a `CPhysics`.
pub fn set_physics_hook(mem: &mut Mem, at: u32, hooked: u32) {
    mem.set_u32(at + 0x40, hooked);
    mem.set_u32(at + 0x54, 0);
}

/// Writes the words `CPhysics::CDef` gets from its own constructor (see `CDefImage`).
pub fn store_cdef(mem: &mut Mem, at: u32, c: &CDefImage) {
    store_header(mem, at, &c.base);
    store_unsigned(mem, at + 0x16070, &c.unk16070);
    store_real(mem, at + 0x16080, &c.unk16080);
    store_real(mem, at + 0x16090, &c.unk16090);
    store_header(mem, at + 0x160a0, &c.unk160a0.header);
    for (i, v) in c.unk160a0.value.iter().enumerate() {
        mem.set_f32(at + 0x160b0 + 4 * i as u32, *v);
    }
    store_storage(mem, at + 0x160c0, &c.flags);
    store_storage(mem, at + 0x160d4, &c.dbg);
    store_storage(mem, at + 0x160e8, &c.log);
    store_real(mem, at + 0x16100, &c.unk16100);
    store_real(mem, at + 0x16110, &c.unk16110);
    store_real(mem, at + 0x16120, &c.unk16120);
    store_unsigned(mem, at + 0x16130, &c.unk16130);
    store_unsigned(mem, at + 0x16140, &c.unk16140);
    store_unsigned(mem, at + 0x16150, &c.unk16150);
}

fn store_real(mem: &mut Mem, at: u32, r: &TweakReal) {
    store_header(mem, at, &r.header);
    mem.set_f32(at + 0x0c, r.value);
}

fn store_storage(mem: &mut Mem, at: u32, s: &Storage) {
    mem.set_u32(at, s.vtable);
    mem.set_u32(at + 4, s.name);
    mem.set_u32(at + 8, 0);
    mem.set_u32(at + 0xc, 0);
    mem.set_u32(at + 0x10, 0);
}

/// The global `g_165b200`, the result of `E46_CreateDef`'s `E45_PhysUnk34` call.
const CREATE_RESULT_AT: u32 = 0x0165_b200;

/// Stores `E46_CreateDef`'s result in its global.
pub fn set_create_result(mem: &mut Mem, value: u32) {
    mem.set_u32(CREATE_RESULT_AT, value);
}

/// The global `g_1659b6c`: the def `E46_InitPhysics` created (0 before the first call).
const CDEF_GLOBAL_AT: u32 = 0x0165_9b6c;

pub fn cdef_global(mem: &Mem) -> u32 {
    mem.u32(CDEF_GLOBAL_AT)
}

pub fn set_cdef_global(mem: &mut Mem, def: u32) {
    mem.set_u32(CDEF_GLOBAL_AT, def);
}

pub fn set_physics_global(mem: &mut Mem, object: u32) {
    mem.set_u32(PHYSICS_GLOBAL_AT, object);
}

/// Stores a `CMaterial` image (0x160 bytes at `at`): the base, the 19 reals, the unsigned and the flags.
pub fn store_material(mem: &mut Mem, at: u32, m: &MaterialImage) {
    store_header(mem, at, &m.base);
    for (i, r) in m.reals.iter().enumerate() {
        store_real(mem, at + 0x0c + 0x10 * i as u32, r);
    }
    store_unsigned(mem, at + 0x13c, &m.unsigned);
    store_storage(mem, at + 0x14c, &m.flags);
}

/// `CMaterialManager::CMaterialManager` (0x520b30) on the manager at `at`: its header, then its 256 materials in order.
pub fn construct_material_manager(mem: &mut Mem, at: u32) {
    use crate::physics_core::layout::material;
    store_header(mem, at, &material::manager_image());
    for tag in 0..material::TAGS {
        material::construct(mem, material::entry(at, tag));
    }
}

/// `CPhysics::CDef`'s members built by their ports.
pub struct Ported;

impl CDefHost for Ported {
    fn def_manager(&mut self, mem: &mut Mem, at: u32) {
        store_header(mem, at, &construct::def_manager_image());
    }

    fn materials(&mut self, mem: &mut Mem, at: u32) {
        construct_material_manager(mem, at);
    }

    fn solver_def(&mut self, mem: &mut Mem, at: u32) {
        store_solver_def(mem, at, &construct::solver_def_image());
    }
}

/// Writes `CCollisionSolver::CDef` (0x4c bytes): its header, the three reals from +0x0c and the unsigned at +0x3c.
pub fn store_solver_def(mem: &mut Mem, at: u32, s: &SolverDefImage) {
    store_header(mem, at, &s.base);
    for (i, r) in s.reals.iter().enumerate() {
        store_real(mem, at + 0x0c + 0x10 * i as u32, r);
    }
    store_unsigned(mem, at + 0x3c, &s.steps);
}

/// A byte at `at`.
pub fn byte(mem: &Mem, at: u32) -> u8 {
    mem.u8(at)
}
