//! `CPhysics` (0xf8 bytes) as its constructor makes it (`src/rbr/RBR_E46_Physics.cpp`) and `CDef::Vf4c` builds it
//! (`RBR_E46_Small.cpp`). The constructor's calls and its signal links into `g_signalManager` are the host's
//! (`PhysicsHost`): the links write the connection's next, prev and list words and the signal list, so the port leaves
//! those words to the host and writes the rest.

use super::construct::{self, Heap, ScratchHost};
use super::contact_walk::NodeHost;
use crate::mem::Mem;
use crate::physics_core::layout;

/// `CPhysics`'s size, the block `CDef::Vf4c` allocates.
pub const PHYSICS_SIZE: u32 = 0xf8;
/// `CPhysics`'s vtable, at +0x00.
const PHYSICS_VTABLE: u32 = 0x0073_f98c;
/// The function pointers at +0x44, +0x48, +0x4c and +0x50.
const FUNCS: [u32; 4] = [0x0073_cbe8, 0x0052_36e0, 0x0073_cbe8, 0x0052_36e0];
/// The connections' vtables and callbacks, in `layout::CONN_AT` order.
const CONN_VTABLE: [u32; 6] = [0x0073_f944, 0x0073_f950, 0x0073_f95c, 0x0073_f950, 0x0073_f95c, 0x0073_f968];
const CONN_CALLBACK: [u32; 6] = [0x0052_3d40, 0x0052_67d0, 0x0052_69e0, 0x0052_72e0, 0x0052_7330, 0x0052_3e80];
/// The signal list each connection is linked into (the list's head word in `g_signalManager`); the last is not linked.
const CONN_LIST: [Option<u32>; 6] = [Some(0x3448), Some(0x4938), Some(0x4b50), Some(0x4f80), Some(0x5198), None];
/// `m_unkF4`'s value after the factories.
const UNK_F4: u32 = 2000;
/// The size of the object `E21Stub::Hook` is given (`operator new(0x10)`).
const HOOK_OBJECT_SIZE: u32 = 0x10;

/// A connection as the constructor leaves it before the host links it: its vtable, owner and callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Connection {
    pub vtable: u32,
    pub owner: u32,
    pub callback: u32,
}

/// The words `CPhysics` gets before its first call: the vtable, the log storage, the function pointers, the
/// connections, the def and two zeros (`m_unk5c`, `m_unk60`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsFront {
    pub vtable: u32,
    pub log: [u32; 5],
    pub funcs: [u32; 4],
    pub conns: [Connection; 6],
    pub def: u32,
    pub unk5c: u32,
    pub unk60: u32,
}

/// The calls `CPhysics::CPhysics` makes into code outside this port, and its signal links. The caller runs them
/// against the game memory; `link` is the caller's writes into `g_signalManager` for one connection.
pub trait PhysicsHost: Heap + ScratchHost {
    /// `E37Prefill5340` (0x4f5340): the pool prefill.
    fn prefill_pool(&mut self);
    /// `NTweak::CBase::GetNameOrNull(def)` (0x52f0b0). Its name feeds `Vf50`, whose state does not read it.
    fn name_or_null(&mut self, def: u32);
    /// `E21Stub::Hook` (0x4a0150) on the object at `object`; returns its result.
    fn hook(&mut self, mem: &mut Mem, object: u32) -> u32;
    /// Appends the connection `conn` to the signal list whose head word is at `list` (an offset in `g_signalManager`),
    /// writing the connection's next, prev and list words and the list's head, tail and count.
    fn link(&mut self, mem: &mut Mem, list: u32, conn: u32);
}

/// The profiler samples and their names passed by `CPhysics::StepForward`.
pub const STEP_FORWARD_SAMPLE: u32 = 0x0165_9f38;
pub const STEP_FORWARD_NAME: u32 = 0x0073_f8ec;
pub const PENETRATION_SAMPLE: u32 = 0x0165_a088;
pub const PENETRATION_NAME: u32 = 0x0073_f8d0;

/// Calls the reached functions outside this port while `CPhysics::StepForward` runs. Calls whose exact Rust ports are
/// available (`SetDirty`, `Work5500`, and the profiler operations) stay in Rust; entity slots and the remaining
/// listeners, collision work, and solver functions are dispatched by the host at their original stack depths.
pub trait StepHost<S = Mem>: NodeHost<S> {
    fn entity_20(&mut self, entry_sp: u32, target: u32, node: u32, dt: f32);
    fn entity_24(&mut self, entry_sp: u32, target: u32, node: u32, dt: f32);
    fn entity_48(&mut self, entry_sp: u32, target: u32, node: u32);
    fn entity_54(&mut self, entry_sp: u32, target: u32, node: u32);
    fn entity_98(&mut self, entry_sp: u32, target: u32, node: u32);
    fn telemetry_60(&mut self, entry_sp: u32, receiver: u32, count: u32);
    fn collide_window(&mut self, entry_sp: u32, physics: u32, window: f32);
    fn penetration_scope(&mut self, stack: layout::StepStack, physics: u32, telemetry: u32);
    fn solver_clear_rows(&mut self, entry_sp: u32, solver: u32);
    fn solver_hit_count(&mut self, entry_sp: u32, solver: u32) -> u32;
    fn solver_sweep_events(&mut self, entry_sp: u32, solver: u32);
    fn solver_step(&mut self, entry_sp: u32, solver: u32);
    fn solver_pass(&mut self, entry_sp: u32, solver: u32);
    fn solver_post(&mut self, entry_sp: u32, target: u32, solver: u32);
    fn telemetry_90(&mut self, entry_sp: u32, receiver: u32);
    fn drive_set4510(&mut self, entry_sp: u32, drive: u32);
}

/// The front of a `CPhysics` at `this` built on `def` (see `PhysicsFront`).
fn front(this: u32, def: u32) -> PhysicsFront {
    PhysicsFront {
        vtable: PHYSICS_VTABLE,
        log: [construct::LOG_FLAGS_VTABLE, construct::FLAGS_NAME, 0, 0, 0],
        funcs: FUNCS,
        conns: std::array::from_fn(|i| Connection { vtable: CONN_VTABLE[i], owner: this, callback: CONN_CALLBACK[i] }),
        def,
        unk5c: 0,
        unk60: 0,
    }
}

/// `CPhysics::CPhysics(def)` (0x527c60) on the object at `this`, in the original's call order. A null def makes no
/// state, scratch or links.
pub fn construct_physics(mem: &mut Mem, host: &mut impl PhysicsHost, this: u32, def: u32) {
    layout::store_physics_front(mem, this, &front(this, def));
    host.prefill_pool();
    if def == 0 {
        layout::set_physics_pointers(mem, this, 0, 0);
        return;
    }
    host.name_or_null(def);
    let state = construct::cdef_new_state(mem, host, this);
    let scratch = construct::cdef_new_scratch(mem, host, def);
    layout::set_physics_pointers(mem, this, state, scratch);
    layout::set_physics_tail(mem, this, UNK_F4);
    for (&c, list) in layout::CONN_AT.iter().zip(CONN_LIST) {
        if let Some(list) = list {
            host.link(mem, list, this + c);
        }
    }
    let object = host.alloc(mem, HOOK_OBJECT_SIZE);
    let hooked = if object != 0 { host.hook(mem, object) } else { 0 };
    layout::set_physics_hook(mem, this, hooked);
}

/// `CPhysics::CDef::Vf4c()` (0x5284f0): a `CPhysics` of `PHYSICS_SIZE` bytes built on the def at `cdef`. Returns it, or
/// 0 when the allocation fails.
pub fn cdef_new_physics(mem: &mut Mem, host: &mut impl PhysicsHost, cdef: u32) -> u32 {
    let at = host.alloc(mem, PHYSICS_SIZE);
    if at != 0 {
        construct_physics(mem, host, at, cdef);
    }
    at
}
