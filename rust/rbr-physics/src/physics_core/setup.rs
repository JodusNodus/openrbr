//! Load-time setup of the physics core (`src/rbr/RBR_E46_Small.cpp`, group D of the README). These run once, not per step.

use super::construct::{self, CDefHost, Heap};
use super::{layout, physics};
use crate::mem::Mem;

/// `CPhysics::CDef`'s size, the block `E46_CreateDef` allocates.
pub const CDEF_SIZE: u32 = 0x16160;

/// The calls `E46_CreateDef` makes into code outside this port, after the def's own constructor.
pub trait CreateHost: Heap + CDefHost {
    /// The def's slot 2 (0x52f0c0), the `StepForward` the original casts the def to. It initialises the def's
    /// sub-objects, so the caller runs it.
    fn step_forward(&mut self, mem: &mut Mem, def: u32, arg: u32);
    /// `E45_PhysUnk34` (0x523a20) on the def; returns its result.
    fn phys_unk34(&mut self, mem: &mut Mem, def: u32) -> u32;
}

/// `E46_CreateDef(arg)` (0x526660): a def built on a new block, stepped with `arg`, and the `E45_PhysUnk34` result
/// stored in the global `g_165b200`. Returns the def (0 when the allocation fails; the original then steps null).
pub fn create_def(mem: &mut Mem, host: &mut impl CreateHost, arg: u32) -> u32 {
    let def = host.alloc(mem, CDEF_SIZE);
    if def != 0 {
        construct::construct_cdef(mem, host, def);
    }
    host.step_forward(mem, def, arg);
    let result = host.phys_unk34(mem, def);
    layout::set_create_result(mem, result);
    def
}

/// The calls `E46_InitPhysics` makes into the script reader (`NTweak`), which the caller runs.
pub trait TokenHost {
    /// `NTweak::StripComments` (0x52eba0): blanks the comments of `script`.
    fn strip_comments(&mut self, mem: &mut Mem, script: u32);
    /// `NTweak::CTokenCursor::Tokenize` (0x52ec80) on the cursor at `cursor`, over `script`.
    fn tokenize(&mut self, mem: &mut Mem, cursor: u32, script: u32);
    /// `NTweak::CTokenCursor::FreeTokens` (0x52ee10) on the cursor at `cursor`.
    fn free_tokens(&mut self, mem: &mut Mem, cursor: u32);
}

/// `E46_InitPhysics(script)` (0x5266d0). `cursor` is the caller's 8-byte token cursor (the original's `tmp`): the
/// def is created on the first call, and stepped with the cursor on the later ones.
pub fn init_physics(
    mem: &mut Mem,
    host: &mut (impl CreateHost + physics::PhysicsHost + TokenHost),
    script: u32,
    cursor: u32,
) {
    host.strip_comments(mem, script);
    host.tokenize(mem, cursor, script);
    let def = layout::cdef_global(mem);
    if def == 0 {
        let def = create_def(mem, host, cursor);
        layout::set_cdef_global(mem, def);
        let object = physics::cdef_new_physics(mem, host, def);
        layout::set_physics_global(mem, object);
    } else {
        host.step_forward(mem, def, cursor);
    }
    host.free_tokens(mem, cursor);
}

/// `E46_InitPhysics` and `E46_Shutdown` (0x5266d0, 0x524c60): the callbacks `E46_AppInit` publishes.
const INIT_PHYSICS: u32 = 0x0052_66d0;
const SHUTDOWN: u32 = 0x0052_4c60;

/// The calls `E46_AppInit` makes into other subsystems, which the caller runs.
pub trait AppHost {
    /// `CBrakeWallManager::ClearWall` (0x4e40d0) on the wall manager at `wall_manager`.
    fn clear_walls(&mut self, mem: &mut Mem, wall_manager: u32);
    /// `E38_004FAD10` (0x4fad10) with the value of the collision global.
    fn collision_clear(&mut self, mem: &mut Mem, value: u32);
    /// `E39WaterTable::Reset` (0x4ffba0) on the water table at `table`.
    fn water_reset(&mut self, mem: &mut Mem, table: u32);
    /// `E09Vec4::Clear` (0x465d80) on the vector at `at`.
    fn vec4_clear(&mut self, mem: &mut Mem, at: u32);
    /// `NTweak::InitTokenScratch` (0x52eb20).
    fn init_token_scratch(&mut self, mem: &mut Mem);
    /// `E37SigInit800` (0x4fa800), the last call (a tail call in the original).
    fn sig_init(&mut self, mem: &mut Mem);
}

/// `E46_AppInit()` (0x526fd0): clears the subsystems, publishes the init and shutdown callbacks, and links the two
/// registrar nodes into the signal manager. The original's walk over `g_7cbab8` only reads, and its result is unused.
pub fn app_init(mem: &mut Mem, host: &mut impl AppHost) {
    host.clear_walls(mem, layout::wall_manager(mem));
    host.collision_clear(mem, layout::collision_global(mem));
    host.water_reset(mem, layout::water_table(mem));
    host.vec4_clear(mem, layout::ZERO_VEC4);
    host.init_token_scratch(mem);
    layout::set_app_callbacks(mem, INIT_PHYSICS, SHUTDOWN);
    let manager = layout::signal_manager(mem);
    for registrar in &layout::REGISTRARS {
        layout::link_registrar(mem, manager, registrar);
    }
    host.sig_init(mem);
}

/// The calls `E43HostRel::WallRel` makes into other code, which the caller runs.
pub trait WallHost {
    /// `E34Link10::UnlinkA` (0x4e41a0) on the relay's link at `link`.
    fn unlink(&mut self, mem: &mut Mem, link: u32);
    /// `CPhysics::CState::RegWall` (0x5263d0) on the state at `state`, with the relay as the wall.
    fn reg_wall(&mut self, mem: &mut Mem, state: u32, wall: u32);
}

/// `E43HostRel::WallRel` (0x5276a0): with a target list, lowers its count, unlinks the relay and registers the relay
/// as a wall with the physics' state. `physics` is the host (`this`), `relay` the wall (the argument).
pub fn wall_rel(mem: &mut Mem, host: &mut impl WallHost, physics: u32, relay: u32) {
    let target = layout::relay_target(mem, relay);
    if target == 0 {
        return;
    }
    layout::release_target(mem, target);
    host.unlink(mem, layout::relay_link(relay));
    let state = layout::state_of(mem, physics);
    host.reg_wall(mem, state, relay);
}
