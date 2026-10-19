//! The subsystem globals `E46_AppInit` reads and publishes, the registrars, the wall relays, the physics object and
//! its state's entity lists, the cell grid and the CDef flags.

use super::physics_ctor::PHYSICS_GLOBAL_AT;
use crate::mem::Mem;
use crate::stage::layout::COLLISION_SYSTEM;

/// The subsystem globals `E46_AppInit` reads: the wall manager, the collision def and the water table.
pub const WALL_MANAGER_GLOBAL: u32 = 0x007c_63c8;
pub(crate) const COLLISION_GLOBAL: u32 = 0x007c_7464;
/// `g_8ef7e0`: the zero vector. An `E09Vec4` that `E46_AppInit` clears and nothing writes again (the original only
/// passes its address). Every collision event gets it as `CCollisionEvent::InitFull`'s fourth argument
/// (`E33MakeEvent`'s seventh): `InitFull` copies it to the event's +0xd0 and runs it through the body transform
/// (`E43PairWalk::MB560`).
pub const ZERO_VEC4: u32 = 0x008e_f7e0;
/// The signal manager (`g_signalManager`).
pub(crate) const SIGNAL_MANAGER: u32 = 0x0089_3600;
/// The replay manager (`g_e19Ptr893634`), the telemetry receiver of the contact loop.
pub const TELEMETRY_GLOBAL: u32 = 0x0089_3634;
/// The callbacks `E46_AppInit` publishes: `E46_InitPhysics` and `E46_Shutdown`.
const INIT_CALLBACK_AT: u32 = 0x0165_a348;
const SHUTDOWN_CALLBACK_AT: u32 = 0x0165_a3ac;

pub fn wall_manager(mem: &Mem) -> u32 {
    mem.u32(WALL_MANAGER_GLOBAL)
}

pub fn collision_global(mem: &Mem) -> u32 {
    mem.u32(COLLISION_GLOBAL)
}

pub fn water_table(mem: &Mem) -> u32 {
    mem.u32(COLLISION_SYSTEM)
}

pub fn signal_manager(mem: &Mem) -> u32 {
    mem.u32(SIGNAL_MANAGER)
}

pub fn set_app_callbacks(mem: &mut Mem, init: u32, shutdown: u32) {
    mem.set_u32(INIT_CALLBACK_AT, init);
    mem.set_u32(SHUTDOWN_CALLBACK_AT, shutdown);
}

/// A registrar node linked into the signal manager by `E46_AppInit`: the node, the manager's list head (its tail is at
/// +4 and its count at +8), and the globals that keep the previous tail (`prev`), a zero (`zero`) and the head's
/// address (`list`).
pub struct Registrar {
    pub node: u32,
    pub head: u32,
    pub prev: u32,
    pub zero: u32,
    pub list: u32,
}

/// The two registrar nodes `E46_AppInit` links, in the original's order.
pub const REGISTRARS: [Registrar; 2] = [
    Registrar { node: 0x0165_a338, head: 0x2e00, prev: 0x0165_a340, zero: 0x0165_a33c, list: 0x0165_a344 },
    Registrar { node: 0x0165_a39c, head: 0x3660, prev: 0x0165_a3a4, zero: 0x0165_a3a0, list: 0x0165_a3a8 },
];

/// Appends `r.node` to the signal manager's list at `mgr + r.head`: the previous tail goes to `r.prev`, `r.zero` is
/// cleared, the old tail (or the head) points at the node, the tail becomes the node, `r.list` is the head's address,
/// and the count goes up by one.
pub fn link_registrar(mem: &mut Mem, mgr: u32, r: &Registrar) {
    let head = mgr + r.head;
    let prev = mem.u32(head + 4);
    mem.set_u32(r.prev, prev);
    mem.set_u32(r.zero, 0);
    if prev == 0 {
        mem.set_u32(head, r.node);
    } else {
        mem.set_u32(prev + 4, r.node);
    }
    mem.set_u32(head + 4, r.node);
    mem.set_u32(r.list, head);
    let count = mem.u32(head + 8);
    mem.set_u32(head + 8, count.wrapping_add(1));
}

/// The target of a wall relay (`E43HostRel` +0x18): the list whose count the release lowers.
pub fn relay_target(mem: &Mem, relay: u32) -> u32 {
    mem.u32(relay + 0x18)
}

/// The link of a wall relay (`E43HostRel` +0x10), the one `E34Link10::UnlinkA` takes.
pub fn relay_link(relay: u32) -> u32 {
    relay + 0x10
}

/// Lowers the count (+0x08) of the list at `target` by one.
pub fn release_target(mem: &mut Mem, target: u32) {
    let count = mem.u32(target + 8);
    mem.set_u32(target + 8, count.wrapping_sub(1));
}

/// The state of a `CPhysics` (+0x20).
pub fn state_of(mem: &Mem, physics: u32) -> u32 {
    mem.u32(physics + 0x20)
}

/// The physics object (`g_1659b70`).
pub const PHYSICS_GLOBAL: u32 = 0x0165_9b70;

pub fn physics(mem: &Mem) -> u32 {
    mem.u32(PHYSICS_GLOBAL)
}

/// The entity lists of a `CPhysics::CState` (head, tail, count), as `CState::RegWall` (0x5263d0) picks them from the
/// entity's slots 0x8c, 0x80 and 0x88: +0x480 for the first, +0x450 for the second, else +0x470 for the third and
/// +0x460 for none.
pub fn entity_list(state: u32, s8c: bool, s80: bool, s88: bool) -> u32 {
    state
        + match (s8c, s80, s88) {
            (true, _, _) => 0x480,
            (false, true, _) => 0x450,
            (false, false, true) => 0x470,
            (false, false, false) => 0x460,
        }
}

/// The cell grid global (`g_8eb794`) the detectors look their cells up on (`CCellGrid`).
pub const CELL_GRID_GLOBAL: u32 = 0x008e_b794;

pub fn cell_grid(mem: &Mem) -> u32 {
    mem.u32(CELL_GRID_GLOBAL)
}

pub fn set_cell_grid(mem: &mut Mem, grid: u32) {
    mem.set_u32(CELL_GRID_GLOBAL, grid);
}

/// The CDef's flags word (`+0x160cc` of the CDef the physics global points to, at its `+0x24`).
pub fn cdef_flags(mem: &Mem) -> u32 {
    let cdef = mem.u32(mem.u32(PHYSICS_GLOBAL_AT) + 0x24);
    mem.u32(cdef + 0x160cc)
}

/// The CDef's debug flags word (`+0x160e0`, `DbgFlags`'s value, of the CDef the physics global points to).
pub fn cdef_debug_flags(mem: &Mem) -> u32 {
    let cdef = mem.u32(mem.u32(PHYSICS_GLOBAL_AT) + 0x24);
    mem.u32(cdef + 0x160e0)
}

/// The physics object global (`g_1659b70`).
pub fn physics_global(mem: &Mem) -> u32 {
    mem.u32(PHYSICS_GLOBAL_AT)
}
