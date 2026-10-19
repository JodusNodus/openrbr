//! The placements (`CSigStuff`): `E46_Notify67d0`'s calls answered by the ports, the deferred state resets, the grid
//! sync and the frame walk.

use super::files::{NoParent, StageHost};
use super::meshes::{store_stuff, STUFF_SIZE};
use super::{WALL, WALL_ROOT};
use crate::collision::body::Vf40Host;
use crate::collision::grid::AllocHost;
use crate::collision::layout::{cell_grid_ctor, state_unka44, sync_grid, CtorHost, GridSyncHost};
use crate::collision::state::StateHost;
use crate::entities::frame::{entity_hook, EntityFrame, Form, Hook, Owner};
use crate::entities::layout::{construct_entity, node_of, EntityArgs};
use crate::entities::transform::copy_blocks_then_walk;
use crate::entities::walk::{self, Node, Walk, Walker};
use crate::load::layout::body_event_listened;
use crate::mem::Mem;
use crate::physics_core::construct::Heap;
use crate::physics_core::detect::{GridBuildHost, Xy};
use crate::physics_core::layout::{self as physics, FilingHost, NotifyHost};
use crate::physics_core::math::{Mat, Vec3};
use crate::physics_core::signal::Filing;
use crate::rigid_body::emit::{window_start, BodyEvent};
use crate::rigid_body::host::{HostUnavailable, SolidCtorHost};
use crate::rigid_body::layout::{
    cdef_new_solid, construct_solid, place_solid, scale_add_rows, solid_filing, solid_sleep, solid_wake, vf16_helper,
    Body, PlaceHost, RestHost,
};
use crate::rigid_body::rotation::MulHost;
use crate::stage::Stuff;

/// The cell grid object (`CCellGrid`, 0x14 bytes).
const GRID_SIZE: u32 = 0x14;

/// The calls the stage's objects make, answered by the ports. `E45_MD60`'s: `CBrakeWallManager::QueryWall` (0x4e4140)
/// on the manager, `operator new(0x14)` and `CCellGrid::CCellGrid` (0x4e0550).
pub(super) struct Calls<'a, H>(pub(super) &'a mut H);

impl<H: StageHost> GridBuildHost for Calls<'_, H> {
    fn query_wall(&mut self, mem: &mut Mem, manager: u32) -> Option<[Xy; 2]> {
        let wall = mem.u32(manager + WALL);
        (wall != 0).then(|| crate::stage::wall_corners(mem.f32s(wall + WALL_ROOT)))
    }

    fn alloc_grid(&mut self, mem: &mut Mem) -> u32 {
        self.0.alloc(mem, GRID_SIZE)
    }

    fn construct_grid(&mut self, mem: &mut Mem, object: u32, min: Xy, width: f32, height: f32) -> u32 {
        cell_grid_ctor(self.0, mem, object, min.x, min.y, width, height)
    }
}

/// `CPhysics::E46_Notify67d0` (0x5267d0, `physics_core::layout::notify_stuff`) on a placement the stage load emits,
/// stored in the host's scratch: its object is a `CSolid` (`CSolid::CDef::Vf50`), placed (`CSolid::Vf44`), filed with
/// the state and put to sleep (`CSolid::E43Vf94`; a placement with flag 1 is woken, `CSolid::E43Vf90`). The objects'
/// virtual calls are answered for `CSolid` only; another class, a parent, a dynamic placement or a listener of the body
/// event that is not ported is `Err`.
pub fn place(mem: &mut Mem, host: &mut impl StageHost, stuff: &Stuff) -> Result<(), HostUnavailable> {
    let at = host.scratch(mem, STUFF_SIZE);
    store_stuff(mem, at, stuff);
    physics::notify_stuff(mem, &mut Calls(host), at)
}

/// The virtual slots the placements' objects are checked for, with the function each must hold: `CSolid::CDef::Vf50`
/// (0x52d640) on the definition; on the object `CSolid::Vf44`, `CSolid::Vf40`, the frame slot 0x30 (`node_of`), the
/// filing slots (`solid_filing`), `CSolid::E43Vf90` with `CSolid::S18` (0x529dc0), and `CSolid::E43Vf94`;
/// `CBase::Unk44` (`ret`) on its state (`+0x38`); `CSolid::E43Vf9C` (0x52ac80, `scale_add_rows`), the owner's hook One.
const DEF_VF50: (u32, u32) = (0x50, 0x0052_D640);
const SOLID_VF44: (u32, u32) = (0x44, 0x0052_C700);
const SOLID_VF40: (u32, u32) = (0x40, 0x0052_B6D0);
const NODE_OF: (u32, u32) = (0x30, 0x0051_9C00);
const SOLID_VF90: (u32, u32) = (0x90, 0x0052_BF30);
const SOLID_S18: (u32, u32) = (0x48, 0x0052_9DC0);
const SOLID_VF94: (u32, u32) = (0x94, 0x0052_C130);
const BASE_UNK44: (u32, u32) = (0x44, 0x0052_F480);
const SOLID_HOOK_ONE: (u32, u32) = (0x9C, 0x0052_AC80);
/// `CEntityFrame::Vf04` (0x519530), the first of the six frame hooks (slots 1 to 6, 0x30 bytes apart).
pub(crate) const FRAME_HOOKS: u32 = 0x0051_9530;
/// A solid's state and its first child.
const ENTITY_STATE: u32 = 0x38;
const FIRST_CHILD: u32 = 0x48;

/// Whether slot `slot` of `object`'s vtable holds `function`.
fn holds(mem: &Mem, object: u32, (slot, function): (u32, u32)) -> bool {
    mem.u32(mem.u32(object) + slot) == function
}

/// `Err` unless `ok`: the object is not one whose calls are ported here.
fn check(ok: bool) -> Result<(), HostUnavailable> {
    ok.then_some(()).ok_or(HostUnavailable)
}

impl<H: StageHost> Heap for Calls<'_, H> {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        self.0.alloc(mem, size)
    }
}

impl<H: StageHost> AllocHost for Calls<'_, H> {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.0.allocate(bytes)
    }
}

impl<H: StageHost> NotifyHost for Calls<'_, H> {
    /// A stage object has no parent; its definition must be a `CSolid::CDef`.
    fn def_new(&mut self, mem: &mut Mem, def: u32, a: u32, b: u32) -> Result<u32, HostUnavailable> {
        check(b == 0 && holds(mem, def, DEF_VF50))?;
        cdef_new_solid(mem, def, a, b, self)
    }

    fn place(&mut self, mem: &mut Mem, object: u32, stuff: u32) -> Result<(), HostUnavailable> {
        check(holds(mem, object, SOLID_VF44))?;
        place_solid(mem, self, object, stuff)
    }

    fn adopt(&mut self, _: &mut Mem, _: u32, _: u32) -> Result<(), HostUnavailable> {
        Err(HostUnavailable)
    }

    fn wake(&mut self, mem: &mut Mem, object: u32) -> Result<(), HostUnavailable> {
        check(holds(mem, object, SOLID_VF90))?;
        solid_wake(mem, self, object)
    }

    fn sleep(&mut self, mem: &mut Mem, object: u32) -> Result<(), HostUnavailable> {
        check(holds(mem, object, SOLID_VF94))?;
        solid_sleep(mem, self, object)
    }
}

impl<H> FilingHost for Calls<'_, H> {
    fn filing(&mut self, mem: &Mem, object: u32) -> Result<Filing, HostUnavailable> {
        solid_filing(mem, object)
    }
}

/// `CSolid::CSolid` (0x52d480) under `CSolid::CDef::Vf50`.
impl<H> SolidCtorHost for Calls<'_, H> {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), HostUnavailable> {
        construct_solid(mem, self, solid, def, a, b, flags);
        Ok(())
    }
}

/// `CEntity::CEntity` (0x51a640) under `CBody::CBody`, for an entity with no parent.
impl<H> CtorHost for Calls<'_, H> {
    fn entity_ctor(&mut self, mem: &mut Mem, body: u32, definition: u32, frame: u32, parent: u32, flags: u32) -> u32 {
        let args = EntityArgs { definition, sort_key: frame, parent, options: flags };
        construct_entity(mem, body, &args, &mut NoParent)
    }
}

impl<H: StageHost> PlaceHost for Calls<'_, H> {
    /// `CSolid::Vf40` (0x52b6d0, `vf16_helper`): `CBody::Vf40`'s two calls end it, and nothing reads what they write
    /// before them, so `Deferred` keeps them and they are made here after it, in its order: `CBody::CState::Unk44`
    /// (`state_unka44`, with its last call, `CColGrid::SyncGrid` on the body's grid, made after it the same way), then
    /// `CEntity::Vf40` (0x51a4b0): the state's slot 0x44 (`CBase::Unk44`, nothing) and the walk of the children (a placed
    /// solid has none).
    fn vf40(&mut self, mem: &mut Mem, this: u32) -> Result<(), HostUnavailable> {
        check(holds(mem, this, SOLID_VF40))?;
        let mut deferred = Deferred::default();
        vf16_helper(mem, this, &mut deferred)?;
        let (Some(state), Some(body)) = (deferred.state, deferred.body) else {
            return Err(HostUnavailable);
        };
        check(holds(mem, this, NODE_OF))?;
        let mut sync = Deferred::default();
        state_unka44(&mut sync, mem, state);
        if let Some((grid, a, low, high)) = sync.sync {
            let scratch = self.0.scratch(mem, 16 * u32::from(mem.u8(grid + GRID_EDGES)));
            let mut walk = GridSync { failed: false };
            sync_grid(&mut walk, mem, grid, a, low, high, scratch);
            check(!walk.failed)?;
        }
        check(holds(mem, body + ENTITY_STATE, BASE_UNK44))?;
        check(mem.u32(body + FIRST_CHILD) == 0)
    }
}

/// A collision grid's edge-vector count (`+0x68d`, the vectors from `+0x40`).
const GRID_EDGES: u32 = 0x68D;

/// The calls `vf16_helper` and `state_unka44` end with, kept to be made after them: `CBody::Vf40`'s state and body, and
/// `SyncGrid`'s grid and arguments.
#[derive(Default)]
struct Deferred {
    state: Option<u32>,
    body: Option<u32>,
    sync: Option<(u32, u32, u32, u32)>,
}

/// `E46Mat::Mul` (0x528c20, `Mat::mul_rows`): the rows of `b` times `m`. A placed solid's state gates it with the
/// extent's fourth word, which `CSolid::Vf44` sets unless the extent is (1, 1, 1).
impl MulHost for Deferred {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable> {
        let m = Mat { rows: std::array::from_fn(|i| [m[3 * i], m[3 * i + 1], m[3 * i + 2]]), extra: 0.0 };
        let rows = m.mul_rows(std::array::from_fn(|k| Vec3 { x: b[3 * k], y: b[3 * k + 1], z: b[3 * k + 2] }));
        Ok(std::array::from_fn(|i| {
            let row = rows[i / 3];
            [row.x, row.y, row.z][i % 3]
        }))
    }
}

impl Vf40Host for Deferred {
    fn state_unka44(&mut self, state: u32) -> u32 {
        self.state = Some(state);
        0
    }

    fn entity_vf40(&mut self, body: u32) -> u32 {
        self.body = Some(body);
        0
    }
}

/// The state's owner slot 0x30 is `node_of` (checked by the caller).
impl StateHost for Deferred {
    fn owner_vf30(&mut self, owner: u32) -> u32 {
        node_of(owner)
    }

    fn wall_sync(&mut self, wall: u32, body: u32, vec_a: u32, vec_b: u32) -> u32 {
        self.sync = Some((wall, body, vec_a, vec_b));
        0
    }
}

/// `SyncGrid`'s calls: `E37E43::M_B460` (`copy_blocks_then_walk`) with the walk on game memory, and the vector copy.
struct GridSync {
    failed: bool,
}

impl GridSyncHost for GridSync {
    fn copy_transform(&mut self, mem: &mut Mem, a: u32, edges: u32, worker: u32, out: u32, count: u32) {
        let mut walk = FrameWalk { mem, failed: false };
        copy_blocks_then_walk(&mut walk, a, worker, edges, out, count);
        self.failed |= walk.failed;
    }

    fn copy_vec4(&mut self, mem: &mut Mem, dst: u32, src: u32) {
        let bytes = mem.read(src, 0x10);
        mem.map(dst, &bytes);
    }
}

/// The pair walk on game memory, for a solid's frame under the root: the frames' hooks are `CEntityFrame`'s, which call
/// the owner's; a solid's hook One is `scale_add_rows`. Any other hook sets `failed`.
struct FrameWalk<'m> {
    mem: &'m mut Mem,
    failed: bool,
}

impl Walk for FrameWalk<'_> {
    fn node(&self, at: u32) -> Node {
        Node { child: self.mem.u32(at + 4), depth: self.mem.u32(at + 8) }
    }

    fn hook(&mut self, at: u32, hook: Hook, a: u32, b: u32) {
        let index = hook as u32;
        if !holds(self.mem, at, (4 * (index + 1), FRAME_HOOKS + 0x30 * index)) {
            self.failed = true;
            return;
        }
        let frame =
            EntityFrame { parent: self.mem.u32(at + 4), depth: self.mem.u32(at + 8), owner: self.mem.u32(at + 0xC) };
        entity_hook(&frame, hook, a, b, self);
    }

    fn walk(&mut self, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
        walk::run(self, which, receiver, o2, a, b)
    }

    fn grid_count(&self, grid: u32) -> u32 {
        crate::entities::layout::grid_count(self.mem, grid)
    }

    fn copy16(&mut self, to: u32, from: u32) {
        let bytes = self.mem.read(from, 0x10);
        self.mem.map(to, &bytes);
    }
}

/// The owner's side of a frame hook.
impl Owner for FrameWalk<'_> {
    fn hook(&mut self, owner: u32, hook: Hook, form: Form, a: u32, b: u32) -> u32 {
        if hook == Hook::One && form == Form::X && holds(self.mem, owner, SOLID_HOOK_ONE) {
            scale_add_rows(self.mem, owner, a, b);
        } else {
            self.failed = true;
        }
        a
    }
    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("an entity frame's hook asks no gate")
    }
    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a frame hook asks no entity for its frame")
    }
    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a frame hook asks no entity for its old frame")
    }
}

/// `CSolid::E43Vf90`'s and `E43Vf94`'s calls: the body's slot 18 and the body event.
impl<H> RestHost for Calls<'_, H> {
    /// `CSolid::S18` (0x529dc0): the walk of the children (a placed solid has none), then `emit::window_start`.
    fn slot18(&mut self, mem: &mut Mem, this: u32) -> Result<(), HostUnavailable> {
        check(holds(mem, this, SOLID_S18) && mem.u32(this + FIRST_CHILD) == 0)?;
        let mut body = Body::load(mem, this);
        window_start(&mut body);
        body.store(mem, this);
        Ok(())
    }

    fn emit(&mut self, mem: &Mem, _: &BodyEvent) -> Result<(), HostUnavailable> {
        check(!body_event_listened(mem))
    }
}
