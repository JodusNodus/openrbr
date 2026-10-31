//! The car placement's parts: the part factories, the state resets, the frame walk and grid sync, and
//! `E46_Notify67d0`'s calls on the car.

use super::instance::{body_event_listened, new_body, new_car, new_wheel, BodyKind};
use crate::collision::layout::body_hosts::Vf40Host;
use crate::collision::layout::grid_hosts::AllocHost;
use crate::collision::layout::state::StateHost;
use crate::collision::layout::{Body, GridSyncHost};
use crate::entities::store::EntityStore;
use crate::mem::Mem;
use crate::physics_core::detect::Xy;
use crate::physics_core::layout::construct::Heap;
use crate::physics_core::layout::detect_calls::GridBuildHost;
use crate::physics_core::layout::Filing;
use crate::physics_core::layout::{FilingHost, NotifyHost};
use crate::stage::layout::FRAME_HOOKS;
use crate::{car_body, collision, entities, physics_core, rigid_body};

/// The car's part factories, by the definitions' slot 0x50 (`E39_EntfCreate` calls it with the sort key and the parent):
/// `CCar::CDef::Vf50` (0x4eb3e0), `E51Wheel::CDef::Vf50` (0x53d1c0), `CHingedRigidBody::CDef::Vf50` (0x51f8e0),
/// `CMountedRigidBody::CDef::Vf50` (0x521d20) and `CCarBody::CDef::Vf50x` (0x4ebb00).
const CAR_FACTORY: u32 = 0x004E_B3E0;
const WHEEL_FACTORY: u32 = 0x0053_D1C0;
const HINGED_FACTORY: u32 = 0x0051_F8E0;
const MOUNTED_FACTORY: u32 = 0x0052_1D20;
const CAR_BODY_FACTORY: u32 = 0x004E_BB00;
const DEF_SLOT_NEW: u32 = 0x50;

/// What a car placement's definition makes (its slot 0x50 with the sort key `key` and the parent `parent`).
pub trait PartFactory {
    fn make(
        &mut self,
        mem: &mut Mem,
        def: u32,
        key: u32,
        parent: u32,
    ) -> Result<u32, rigid_body::host::HostUnavailable>;
}

/// The ported factories, their blocks from `heap`.
pub struct Factories<'a, H>(pub &'a mut H);

impl<H: Heap + entities::exclusion::Heap> PartFactory for Factories<'_, H> {
    fn make(
        &mut self,
        mem: &mut Mem,
        def: u32,
        key: u32,
        parent: u32,
    ) -> Result<u32, rigid_body::host::HostUnavailable> {
        let kind = match mem.u32(mem.u32(def) + DEF_SLOT_NEW) {
            CAR_FACTORY => return new_car(mem, self.0, def, key, parent),
            WHEEL_FACTORY => return new_wheel(mem, self.0, def, key, parent),
            HINGED_FACTORY => BodyKind::Hinged,
            MOUNTED_FACTORY => BodyKind::Mounted,
            CAR_BODY_FACTORY => BodyKind::CarBody,
            _ => return Err(rigid_body::host::HostUnavailable),
        };
        Ok(new_body(mem, self.0, kind, def, key, parent))
    }
}

/// `CBody::Vf40`'s state reset, run by the placement after `Vf16Helper` (as `stage::layout` does): the helper's two
/// calls are deferred; its matrix multiply is `E46Mat::Mul`'s, as the stage's.
#[derive(Default)]
struct DeferredVf40 {
    state: Option<u32>,
}

impl rigid_body::rotation::MulHost for DeferredVf40 {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], rigid_body::host::HostUnavailable> {
        Ok(rigid_body::rotation::mul_rows(m, b))
    }
}

impl Vf40Host for DeferredVf40 {
    fn state_unka44(&mut self, state: u32) -> u32 {
        self.state = Some(state);
        0
    }

    fn entity_vf40(&mut self, _: u32) -> u32 {
        0
    }
}

/// The state's `UnkA44` calls: the owner's node, and the wall sync it asks for.
#[derive(Default)]
struct StateCalls {
    sync: Option<(u32, u32, u32, u32)>,
}

impl StateHost for StateCalls {
    fn owner_vf30(&mut self, owner: u32) -> u32 {
        entities::layout::node_of(owner)
    }

    fn wall_sync(&mut self, grid: u32, a: u32, lower: u32, upper: u32) -> u32 {
        self.sync = Some((grid, a, lower, upper));
        0
    }
}

/// `CColGrid::SyncGrid`'s frame walk on a car part: the entity frames' hooks and the solids' `E43Vf9C` (0x52ac80).
/// Anything else sets `failed`.
struct CarFrameWalk<'a> {
    mem: &'a mut Mem,
    failed: bool,
}

/// The frame hooks after `FRAME_HOOKS` (`CEntityFrame::Vf04`) follow 0x30 bytes apart.
const FRAME_HOOK_STRIDE: u32 = 0x30;
/// A solid's slot 0x9c, `CSolid::E43Vf9C` (0x52ac80, `rigid_body::layout::scale_add_rows`).
const SOLID_HOOK_ONE: (u32, u32) = (0x9C, 0x0052_AC80);

impl entities::walk::Walk for CarFrameWalk<'_> {
    fn node(&self, at: u32) -> entities::walk::Node {
        entities::walk::Node { child: self.mem.u32(at + 4), depth: self.mem.u32(at + 8) }
    }

    fn hook(&mut self, at: u32, hook: entities::frame::Hook, a: u32, b: u32) {
        let slot = 4 * (hook as u32 + 1);
        if self.mem.u32(self.mem.u32(at) + slot) != FRAME_HOOKS + FRAME_HOOK_STRIDE * hook as u32 {
            self.failed = true;
            return;
        }
        let frame = entities::frame::EntityFrame {
            parent: self.mem.u32(at + 4),
            depth: self.mem.u32(at + 8),
            owner: self.mem.u32(at + 0xc),
        };
        entities::frame::entity_hook(&frame, hook, a, b, self);
    }

    fn walk(&mut self, which: entities::walk::Walker, receiver: u32, other: u32, a: u32, b: u32) -> u32 {
        entities::walk::run(self, which, receiver, other, a, b)
    }

    fn copy16(&mut self, to: u32, from: u32) {
        self.mem.copy(to, from, 0x10);
    }

    fn grid_count(&self, grid: u32) -> u32 {
        EntityStore::grid_count(self.mem, grid)
    }
}

impl entities::frame::Owner for CarFrameWalk<'_> {
    fn hook(&mut self, owner: u32, hook: entities::frame::Hook, form: entities::frame::Form, a: u32, b: u32) -> u32 {
        let (slot, function) = SOLID_HOOK_ONE;
        if hook == entities::frame::Hook::One
            && form == entities::frame::Form::X
            && self.mem.u32(self.mem.u32(owner) + slot) == function
        {
            rigid_body::layout::scale_add_rows(self.mem, owner, a, b)
        } else {
            self.failed = true;
            a
        }
    }

    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("a car part's frame asks no gate")
    }

    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a car part's frame asks no frame")
    }

    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a car part's frame asks no old frame")
    }
}

/// `CColGrid::SyncGrid`'s copies: the frame walk (`copy_blocks_and_walk_one_two`) and the vector copy.
#[derive(Default)]
struct CarGridSync {
    failed: bool,
}

impl GridSyncHost for CarGridSync {
    fn copy_transform(&mut self, mem: &mut Mem, a: u32, edges: u32, worker: u32, out: u32, count: u32) {
        let mut walk = CarFrameWalk { mem, failed: false };
        entities::transform::copy_blocks_and_walk_one_two(&mut walk, a, worker, edges, out, count);
        self.failed |= walk.failed;
    }

    fn copy_vec4(&mut self, mem: &mut Mem, dst: u32, src: u32) {
        mem.copy(dst, src, 0x10);
    }
}

/// A collision grid's point count (`+0x68d`): `SyncGrid`'s vertex buffer holds that many vectors.
const GRID_POINT_COUNT: u32 = 0x68D;
/// The parts' `Vf44` slot functions: `CSolid::Vf44` (0x52c700, the solids) and `CBody::Vf44` (0x4e1140, the car body).
const SOLID_VF44: u32 = 0x0052_C700;
const BODY_VF44: u32 = 0x004E_1140;
const SLOT_VF44: u32 = 0x44;
/// The parts' wake and sleep slots and `CSolid`'s (`E43Vf90` 0x52bf30, `E43Vf94` 0x52c130); other classes are entities.
const SLOT_WAKE: (u32, u32) = (0x90, 0x0052_BF30);
const SLOT_SLEEP: (u32, u32) = (0x94, 0x0052_C130);
/// A body's first child.
const FIRST_CHILD: u32 = 0x48;

/// `CPhysics::E46_Notify67d0` (0x5267d0, `physics_core::layout::notify_stuff`) on the car loader's placements: each
/// definition's factory makes the part (`factory`), which is placed (`CSolid::Vf44`, or `CBody::Vf44` for the car body),
/// attached to the car (`CCar::Vf150x` and `CEntity::E43Vf150`) or filed, and woken or put to sleep. The cell grid is
/// the stage's; `heap` gives the cells' link blocks, and `sync_scratch` is free memory for `SyncGrid`'s vertices (its
/// stack).
pub struct CarParts<'a, F, H> {
    pub factory: F,
    pub heap: &'a mut H,
    pub sync_scratch: u32,
}

impl<F, H> GridBuildHost for CarParts<'_, F, H> {
    fn query_wall(&mut self, _: &mut Mem, _: u32) -> Option<[Xy; 2]> {
        None
    }

    fn alloc_grid(&mut self, _: &mut Mem) -> u32 {
        panic!("the stage builds the cell grid before the car")
    }

    fn construct_grid(&mut self, _: &mut Mem, _: u32, _: Xy, _: f32, _: f32) -> u32 {
        panic!("the stage builds the cell grid before the car")
    }
}

impl<F, H: entities::exclusion::Heap> AllocHost for CarParts<'_, F, H> {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.heap.allocate(bytes)
    }
}

impl<F, H: entities::exclusion::Heap> rigid_body::layout::PlaceHost for CarParts<'_, F, H> {
    fn vf40(&mut self, mem: &mut Mem, this: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        let mut deferred = DeferredVf40::default();
        rigid_body::layout::vf16_helper(mem, this, &mut deferred)?;
        let state = deferred.state.expect("CBody::Vf40 calls its state");
        let mut state_calls = StateCalls::default();
        collision::layout::state_unka44(&mut state_calls, mem, state);
        if let Some((grid, a, lower, upper)) = state_calls.sync {
            let bytes = u32::from(mem.u8(grid + GRID_POINT_COUNT)) * 0x10;
            mem.map(self.sync_scratch, &vec![0; bytes.max(0x10) as usize]);
            let mut sync = CarGridSync::default();
            collision::layout::sync_grid(&mut sync, mem, grid, a, lower, upper, self.sync_scratch);
            if sync.failed {
                return Err(rigid_body::host::HostUnavailable);
            }
        }
        Ok(())
    }
}

impl<F, H> FilingHost for CarParts<'_, F, H> {
    fn filing(&mut self, mem: &Mem, object: u32) -> Result<Filing, rigid_body::host::HostUnavailable> {
        rigid_body::layout::solid_filing(mem, object)
    }
}

impl<F, H> rigid_body::layout::RestHost for CarParts<'_, F, H> {
    fn slot18(&mut self, mem: &mut Mem, this: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        rigid_body::layout::solid_window_start(mem, this);
        Ok(())
    }

    /// The body event while the car loads: only the idle replay manager listens, or nothing (decision O1).
    fn emit(&mut self, mem: &Mem, _: &rigid_body::emit::BodyEvent) -> Result<(), rigid_body::host::HostUnavailable> {
        if body_event_listened(mem) {
            return Err(rigid_body::host::HostUnavailable);
        }
        Ok(())
    }
}

/// `CCar::Vf150x`'s calls, in order: the car's and the part's slot 59 (`CEntity::E43VfEC`, 0x51a040: each adds the other
/// to its exclusion set; the part's through its vtable, `true`), which `adopt` makes after; `E43Vf150` is
/// `entities::layout::append_child`, which `adopt` runs after them.
#[derive(Default)]
struct AttachCalls(Vec<(u32, u32, bool)>);

impl car_body::wheel::WheelCalls for AttachCalls {
    fn car_excludes(&mut self, car: u32, wheel: u32) {
        self.0.push((car, wheel, false));
    }
    fn wheel_excludes(&mut self, wheel: u32, car: u32) {
        self.0.push((wheel, car, true));
    }

    fn add_child(&mut self, _: u32, _: u32) -> u32 {
        1
    }
}

/// A part's slot 59 and `CEntity`'s, the exclusion set's append.
const SLOT_EXCLUSION_ADD: (u32, u32) = (0xEC, 0x0051_A040);

impl<F: PartFactory, H: entities::exclusion::Heap> NotifyHost for CarParts<'_, F, H> {
    fn def_new(
        &mut self,
        mem: &mut Mem,
        def: u32,
        key: u32,
        parent: u32,
    ) -> Result<u32, rigid_body::host::HostUnavailable> {
        self.factory.make(mem, def, key, parent)
    }

    fn place(&mut self, mem: &mut Mem, object: u32, stuff: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        match mem.u32(mem.u32(object) + SLOT_VF44) {
            SOLID_VF44 => rigid_body::layout::place_solid(mem, self, object, stuff),
            BODY_VF44 => {
                let grid = collision::layout::select_table(mem, stuff);
                Body::new(object).set_grid(mem, grid);
                // `CBody::Vf44` calls `CEntity::Vf44`: its `CBase::Unk44` is empty, and a new body has no children.
                if mem.u32(object + FIRST_CHILD) != 0 {
                    return Err(rigid_body::host::HostUnavailable);
                }
                Ok(())
            }
            _ => Err(rigid_body::host::HostUnavailable),
        }
    }

    fn adopt(&mut self, mem: &mut Mem, parent: u32, object: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        let mut calls = AttachCalls::default();
        car_body::wheel::attach_wheel(&mut calls, mem, parent, object);
        for (entity, member, slot) in calls.0 {
            let (at, function) = SLOT_EXCLUSION_ADD;
            if slot && mem.u32(mem.u32(entity) + at) != function {
                return Err(rigid_body::host::HostUnavailable);
            }
            entities::layout::add_exclusion(mem, entity, member);
        }
        entities::layout::append_child(mem, parent, object);
        Ok(())
    }

    fn wake(&mut self, mem: &mut Mem, object: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        let (slot, function) = SLOT_WAKE;
        if mem.u32(mem.u32(object) + slot) == function {
            rigid_body::layout::solid_wake(mem, self, object)
        } else {
            physics_core::layout::entity_rel(mem, self, object)
        }
    }

    fn sleep(&mut self, mem: &mut Mem, object: u32) -> Result<(), rigid_body::host::HostUnavailable> {
        let (slot, function) = SLOT_SLEEP;
        if mem.u32(mem.u32(object) + slot) == function {
            rigid_body::layout::solid_sleep(mem, self, object)
        } else {
            physics_core::layout::entity_rel(mem, self, object)
        }
    }
}
