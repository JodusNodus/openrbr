//! The boot's hosts: its allocator, and the calls the load ports make while [`super::build`] builds the image.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rbr_formats::lsp::Node;
use rbr_gameplay::timing::TimeKeeper;
use rbr_hybrid::timekeeper::layout as keeper_layout;
use rbr_physics::car_body;
use rbr_physics::collision::layout as collision;
use rbr_physics::entities;
use rbr_physics::mem::Mem;
use rbr_physics::ntweak::{ApplyHost, Unsupported};
use rbr_physics::physics_core::construct::{Heap, ScratchHost};
use rbr_physics::physics_core::physics::PhysicsHost;
use rbr_physics::physics_core::prof::{EnterHost, ProfHost};
use rbr_physics::stage::layout::{self as stage, StageHost};
use rbr_physics::stage::Stuff;

use super::{CARD_SIZE, SCRATCH, SCRATCH_SIZE, SIGNALS};

/// The boot's allocator (decision O2): 16-byte aligned blocks, each zero-filled (O4), from a range no exe section and
/// no pure-host block uses. Clones share the cursor and the block list.
#[derive(Clone)]
pub struct BootHeap {
    pub mem: Mem,
    pub next: Rc<Cell<u32>>,
    pub blocks: Rc<RefCell<Vec<(u32, u32)>>>,
}

pub const HEAP_BASE: u32 = 0x1000_0000;
pub const HEAP_END: u32 = 0x4000_0000;

impl BootHeap {
    pub fn take(&self, size: u32) -> u32 {
        let at = self.next.get();
        let span = size.max(1).next_multiple_of(0x10);
        assert!(at + span <= HEAP_END, "the boot heap is full");
        self.next.set(at + span);
        self.mem.clone().map(at, &vec![0; span as usize]);
        self.blocks.borrow_mut().push((at, size));
        at
    }
}

impl Heap for BootHeap {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.take(size)
    }
}

impl entities::exclusion::Heap for BootHeap {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.take(bytes)
    }
}

impl rbr_physics::collision::layout::grid_hosts::AllocHost for BootHeap {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.take(bytes)
    }
}

/// The files' walk: names and definitions from the boot heap.
pub struct Files(pub BootHeap);

impl ApplyHost for Files {
    fn alloc_name(&mut self, _: &mut Mem, len: u32) -> u32 {
        self.0.take(len)
    }

    /// Every object the files reach has 0x52f480 (a `ret`) as its slot 17.
    fn after_parse(&mut self, _: &mut Mem, _: u32) {}

    /// `load::physics_file` answers the registry's `Find`; the setups link nothing.
    fn def_index(&mut self, _: &Mem, _: &str) -> Option<u32> {
        None
    }

    fn define(&mut self, _: &mut Mem, _: &str, _: u32, _: &str, _: &[Node]) -> Result<(), Unsupported> {
        unreachable!("the setup owners have no definition manager")
    }
}

impl Heap for Files {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.0.take(size)
    }
}

/// `CPhysics::CPhysics`'s calls: the blocks, the solver's constructor and `Vf08`, the time keeper (`CTimeKeeper::Init`
/// with the stage's timing, `TimeKeeper::load`), the connections' links.
pub struct PhysicsCalls {
    pub heap: BootHeap,
    pub keeper: TimeKeeper,
    pub prefilled: bool,
}

impl Heap for PhysicsCalls {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.heap.take(size)
    }
}

impl ScratchHost for PhysicsCalls {
    fn ctor_solver(&mut self, mem: &mut Mem, solver: u32, def: u32) {
        collision::ctor_solver(mem, solver, def);
    }

    fn solver_s08(&mut self, mem: &mut Mem, solver: u32) {
        collision::drain_events(mem, solver).expect("a new solver's event list is empty");
    }
}

impl PhysicsHost for PhysicsCalls {
    fn prefill_pool(&mut self) {
        self.prefilled = true;
    }

    fn name_or_null(&mut self, _: u32) {}

    fn hook(&mut self, mem: &mut Mem, object: u32) -> u32 {
        let card = self.heap.take(CARD_SIZE);
        keeper_layout::store_new(mem, object, card, &self.keeper);
        object
    }

    fn link(&mut self, mem: &mut Mem, list: u32, conn: u32) {
        let head = mem.u32(SIGNALS) + list;
        car_body::layout::push_back_connection(mem, head, conn);
    }
}

/// `CApplication::Tic`'s sample: its counter value is not read by the step.
pub struct Profiler;

impl ProfHost for Profiler {
    fn counter(&mut self, _: u32) -> u32 {
        1
    }
}

impl EnterHost for Profiler {
    fn unlink(&mut self, _: u32) {
        unreachable!("the Tic sample is in no queue yet")
    }
}

/// The ground's constructor: a root entity makes no owner calls, and there is no ground before it.
pub struct NoCalls;

impl entities::frame::Owner for NoCalls {
    fn hook(&mut self, _: u32, _: entities::frame::Hook, _: entities::frame::Form, _: u32, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
}

impl entities::ground::Replace for NoCalls {
    fn delete(&mut self, _: u32) {
        unreachable!("there is no ground before the first")
    }
}

/// The stage load's calls: the boot heap, the placements through `stage::place`, the scratch ring, the mesh list.
pub struct StageCalls {
    pub heap: BootHeap,
    pub handles: HashMap<u32, u32>,
    pub keyed: usize,
    pub stuffs: usize,
    pub scratch: u32,
    /// The mesh list's buffer (the `.trk`'s mesh count of words: the original's `std::vector` grows, nothing here
    /// links it).
    pub mesh_list: u32,
    pub mesh_capacity: u32,
    pub meshes: u32,
    pub failed: Option<usize>,
    pub wet: bool,
}

impl Heap for StageCalls {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.heap.take(size)
    }
}

impl rbr_physics::collision::layout::grid_hosts::AllocHost for StageCalls {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.heap.take(bytes)
    }
}

impl StageHost for StageCalls {
    fn wet_meshes(&mut self) -> bool {
        self.wet
    }
    fn delete(&mut self, _: &mut Mem, object: u32) {
        panic!("nothing to delete on the first stage load ({object:#x})")
    }
    fn send_shape_collision_meshes(&mut self) -> bool {
        true
    }
    fn keep_mesh_name(&mut self, _: &[u8]) {}
    fn interactive_handle(&mut self, id: u32) -> Option<u32> {
        let handle = self.handles.get(&id).copied();
        self.keyed += usize::from(handle.is_some());
        handle
    }
    fn stuff(&mut self, mem: &mut Mem, stuff: &Stuff) {
        if stage::place(mem, self, stuff).is_err() {
            self.failed.get_or_insert(self.stuffs);
        }
        self.stuffs += 1;
    }
    fn scratch(&mut self, mem: &mut Mem, size: u32) -> u32 {
        if self.scratch + size > SCRATCH_SIZE {
            self.scratch = 0;
        }
        let at = SCRATCH + self.scratch;
        self.scratch = (self.scratch + size + 0xf) & !0xf;
        mem.map(at, &vec![0; size as usize]);
        at
    }
    fn push_mesh(&mut self, mem: &mut Mem, mesh: u32) {
        assert!(self.meshes < self.mesh_capacity, "the mesh list is full");
        mem.set_u32(self.mesh_list + 4 * self.meshes, mesh);
        self.meshes += 1;
    }
}
