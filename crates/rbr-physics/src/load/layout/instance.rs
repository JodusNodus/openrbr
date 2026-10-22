//! The car's and wheels' constructor calls outside the ports, and the car, wheel and body instances they make.

use crate::mem::Mem;
use crate::physics_core::layout::construct::Heap;
use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::physics_core::layout::{mounted_body, mounted_ctor};
use crate::{car_body, entities, ntweak, rigid_body, wheel};

/// The calls the car's and wheels' constructors make outside the ports, ported, with `heap` for the blocks (`operator
/// new` and the exclusion sets'). Every parent here is the car, whose frame slots (12 and 13) are `CEntity`'s:
/// `0x519c00` returns the entity and `0x519c10` its old frame (+0x1c).
struct Instance<'a, H> {
    heap: &'a mut H,
    /// Whether the body event's signal (`[0x893600] + 0xec98`) would run a listener that is not ported.
    listened: bool,
}

/// The signal object (`[0x893600]`) and its body event signal (`CMultiSignal1<const CSigMrRBodyEvent&>`, +0xec98): the
/// busy word (+0), the current type's pointer (+4), the lists by type (+8, 0x10 each) and the one for every type
/// (+0x208).
const BODY_EVENT: u32 = 0xEC98;
/// The replay manager's connection (`CConnection<NMrReplay::CMrReplayManager, CSigMrRBodyEvent>`, vtable 0x74fa5c: its
/// slot 1 calls the handler at +0x14 on the manager at +0x10; the next connection at +4) and its handler (0x61fcf0),
/// which returns while the manager's byte at +0x10 (recording; name is ours) is 0.
const REPLAY_CONNECTION: u32 = 0x0074_FA5C;
const REPLAY_HANDLER: u32 = 0x0061_FCF0;
/// `CEntity`'s frame slots and the old frame's offset.
const FRAME_SLOTS: [(u32, u32); 2] = [(0x30, 0x0051_9C00), (0x34, 0x0051_9C10)];
const OLD_FRAME: u32 = 0x1C;

/// Whether the connections from `first` on call anything but the idle replay manager.
fn calls_listener(mem: &Mem, first: u32) -> bool {
    let mut at = first;
    while at != 0 {
        let idle = mem.u32(at) == REPLAY_CONNECTION
            && mem.u32(at + 0x14) == REPLAY_HANDLER
            && mem.u8(mem.u32(at + 0x10) + 0x10) == 0;
        if !idle {
            return true;
        }
        at = mem.u32(at + 4);
    }
    false
}

/// Whether `CMultiSignal1::Emit` (0x51f7c0) on the body event would run code that is not ported, as far as it reads:
/// with no current type nothing; busy, the original's assert; otherwise the lists' connections.
pub(crate) fn body_event_listened(mem: &Mem) -> bool {
    let signal = mem.u32(SIGNAL_MANAGER) + BODY_EVENT;
    let current = mem.u32(signal + 4);
    current != 0
        && (mem.u32(signal) != 0
            || calls_listener(mem, mem.u32(signal + 0x208))
            || calls_listener(mem, mem.u32(signal + 8 + 0x10 * mem.u32(current))))
}

impl<'a, H> Instance<'a, H> {
    fn new(mem: &Mem, heap: &'a mut H) -> Self {
        let listened = body_event_listened(mem);
        Instance { heap, listened }
    }
}

/// The frames' calls on the car: its frame and old frame.
struct ParentFrames;

impl entities::frame::Owner for ParentFrames {
    fn hook(&mut self, _: u32, _: entities::frame::Hook, _: entities::frame::Form, _: u32, _: u32) -> u32 {
        unreachable!("a frame's constructor calls no hook")
    }

    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("a frame's constructor asks no gate")
    }

    fn frame_of(&mut self, ent: u32) -> u32 {
        ent
    }

    fn old_frame_of(&mut self, ent: u32) -> u32 {
        ent + OLD_FRAME
    }
}

impl<H: Heap> Heap for Instance<'_, H> {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        self.heap.alloc(mem, size)
    }
}

impl<H: entities::exclusion::Heap> entities::exclusion::Heap for Instance<'_, H> {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.heap.allocate(bytes)
    }
}

/// `CEntity::CEntity` (0x51a640) under the base constructors.
impl<H> crate::collision::layout::CtorHost for Instance<'_, H> {
    fn entity_ctor(&mut self, mem: &mut Mem, body: u32, definition: u32, frame: u32, parent: u32, flags: u32) -> u32 {
        if parent != 0 {
            let vtable = mem.u32(parent);
            for (slot, function) in FRAME_SLOTS {
                assert_eq!(mem.u32(vtable + slot), function, "the parent's frame slot {slot:#x}");
            }
        }
        let args = entities::layout::EntityArgs { definition, sort_key: frame, parent, options: flags };
        entities::layout::construct_entity(mem, body, &args, &mut ParentFrames)
    }
}

/// `CSolid::CSolid` (0x52d480) under the wheel's constructor.
impl<H> rigid_body::host::SolidCtorHost for Instance<'_, H> {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), rigid_body::host::HostUnavailable> {
        rigid_body::layout::construct_solid(mem, self, solid, def, a, b, flags);
        Ok(())
    }
}

/// The wheel's body event (its owner is the car).
impl<H> rigid_body::emit::EmitHost for Instance<'_, H> {
    /// `CSolid::S18` (0x529dc0, the wheel's slot 18): a new body has no children (the entity's constructor clears the
    /// list), so its walk calls nothing.
    fn slot18(&mut self, body: &mut rigid_body::layout::Body) {
        rigid_body::emit::window_start(body);
    }

    /// `CEntity::E43Vf90` (0x519a60): the body has a parent, so nothing.
    fn entity_notify(&mut self) {}

    /// The signal's emit: with only the idle replay manager listening, nothing; another listener is not ported.
    fn emit(&mut self, _: &rigid_body::emit::BodyEvent) -> Result<(), rigid_body::host::HostUnavailable> {
        if self.listened {
            return Err(rigid_body::host::HostUnavailable);
        }
        Ok(())
    }
}

/// The car (`CCar::CDef::Vf50(a, b)` on the car's definition `def`, step 4), its blocks from `heap`. `a` and `b` are the
/// arguments `E39_EntfCreate` (0x502070) passes on: the sort key and the parent (none). Returns the car.
pub fn new_car(
    mem: &mut Mem,
    heap: &mut (impl Heap + entities::exclusion::Heap),
    def: u32,
    a: u32,
    b: u32,
) -> Result<u32, rigid_body::host::HostUnavailable> {
    let mut host = Instance::new(mem, heap);
    car_body::layout::new_car(mem, &mut host, def, a, b)
}

/// A wheel (`E51Wheel::CDef::Vf50(a, b)` on the wheel's definition `def`), its block from `heap`: `a` is the sort key
/// and `b` the car. Returns the wheel.
pub fn new_wheel(
    mem: &mut Mem,
    heap: &mut (impl Heap + entities::exclusion::Heap),
    def: u32,
    a: u32,
    b: u32,
) -> Result<u32, rigid_body::host::HostUnavailable> {
    let mut host = Instance::new(mem, heap);
    wheel::layout::new_wheel(mem, &mut host, def, a, b)
}

/// The mounted body's calls (`physics_core::layout::mounted_body`), ported.
impl<H: entities::exclusion::Heap> mounted_body::BodyHost for Instance<'_, H> {
    fn solid_base(&mut self, mem: &mut Mem, at: u32, def: u32, a: u32, b: u32, flags: u32) {
        rigid_body::layout::construct_solid(mem, self, at, def, a, b, flags);
    }

    fn base_name(&mut self, mem: &mut Mem, base: u32) -> u32 {
        ntweak::name_or_null(mem, base)
    }

    fn state_ctor(&mut self, mem: &mut Mem, at: u32, _: u32, _: u32, _: u32) {
        mounted_ctor::construct_state(mem, at);
    }

    fn scratch_base(&mut self, mem: &mut Mem, at: u32) {
        rigid_body::layout::store_scratch_base_init(mem, at);
    }

    fn exclusion(&mut self, mem: &mut Mem, at: u32, n: u32) {
        entities::layout::alloc_exclusion(mem, at, n, self.heap);
    }
}

/// The car's body factories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// `CHingedRigidBody::CDef::Vf50` (0x51f8e0): the doors, the bonnet, the exhaust pipe.
    Hinged,
    /// `CMountedRigidBody::CDef::Vf50` (0x521d20): the bumpers and the wing.
    Mounted,
    /// `CCarBody::CDef::Vf50x` (0x4ebb00).
    CarBody,
}

/// A body from `kind`'s factory (`Vf50(a, b)` on the body's definition `def`), its blocks from `heap`: `a` is the sort
/// key and `b` the car. Returns the body.
pub fn new_body(
    mem: &mut Mem,
    heap: &mut (impl Heap + entities::exclusion::Heap),
    kind: BodyKind,
    def: u32,
    a: u32,
    b: u32,
) -> u32 {
    let mut host = Instance::new(mem, heap);
    match kind {
        BodyKind::Hinged => entities::layout::new_hinged(mem, &mut host, def, a, b),
        BodyKind::Mounted => mounted_body::new_body(mem, &mut host, def, a, b),
        BodyKind::CarBody => car_body::layout::new_body(mem, &mut host, def, a, b),
    }
}
