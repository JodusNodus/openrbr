//! The entity constructors: `CEntity`, its child list, `E43EntityDef`, `CGround`'s def, `CHingedRigidBody`'s def
//! and body.

use super::entity_slots::{alloc_exclusion, EXCL_SET, PARENT};
use super::frames::construct_entity_frame;
use crate::collision::layout::CtorHost;
use crate::entities::def::{self, Unlink};
use crate::entities::exclusion::Heap;
use crate::entities::frame::Owner;
use crate::entities::ground::{self, Replace};
use crate::entities::transform::Transform;
use crate::entities::tree::{self, ChildList};
use crate::mem::Mem;
use crate::physics_core::construct;
use crate::{ntweak, rigid_body};

/// The vtables a CEntity's constructor writes: the entity's (0x73ccbc), its state's (0x73cc6c, a `CBase`) and its
/// scratch's (0x73cbb4).
const ENTITY_VTABLE: u32 = 0x73CCBC;
const STATE_VTABLE: u32 = 0x73CC6C;
const SCRATCH_VTABLE: u32 = 0x73CBB4;
pub(super) const OLD_FRAME: u32 = 0x1c;
const SORT_KEY: u32 = 0x30;
const DEFINITION: u32 = 0x34;
const STATE: u32 = 0x38;
const NAME: u32 = 0x3c;
const SELF_LINK: u32 = 0x58;
const SCRATCH: u32 = 0x5c;
const OPTIONS: u32 = 0x60;

/// The arguments of `CEntity::CEntity(definition, sort_key, parent, options)` (0x51a640). The definition names the
/// state and is stored at `+0x34`; `sort_key` is stored at `+0x30` (the order `E43_Cmp9A0` compares); `parent` is
/// the entity the frames hang under and is stored at `+0x44`; `options` is stored at `+0x60` with its low bit set.
/// The shared header names the four by their stack slots (`flags, x, y, def`): rbr-recomp `notes/physics/entities.md` 5b.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityArgs {
    pub definition: u32,
    pub sort_key: u32,
    pub parent: u32,
    pub options: u32,
}

/// `CEntity::CEntity(args)` (0x51a640) at `at`: the entity's frame and its old frame, both under `args.parent`; the
/// state (`CBase`, named by the definition unless it is 0); then the stores. Returns `at` (eax). The frames' owner
/// calls go to `calls`; a root entity (no parent) makes none.
pub fn construct_entity(mem: &mut Mem, at: u32, args: &EntityArgs, calls: &mut impl Owner) -> u32 {
    construct_entity_frame(mem, at, at, args.parent, false, calls);
    for off in [0x10, 0x14, 0x18] {
        mem.set_u32(at + off, 0);
    }
    mem.set_u32(at, ENTITY_VTABLE);
    construct_entity_frame(mem, at + OLD_FRAME, at, args.parent, true, calls);
    // GetNameOrNull (0x52f0b0) on a non-null definition reads the name at its +4.
    let name = if args.definition == 0 { 0 } else { mem.u32(args.definition + 4) };
    construct_base(mem, at + STATE, name);
    mem.set_u32(at + STATE, STATE_VTABLE);
    for off in [CHILD_HEAD, CHILD_TAIL, CHILD_COUNT, 0x54] {
        mem.set_u32(at + off, 0);
    }
    mem.set_u32(at + PARENT, args.parent);
    mem.set_u32(at + SELF_LINK, at);
    mem.set_u32(at + NAME, name);
    mem.set_u32(at + SCRATCH, SCRATCH_VTABLE);
    mem.set_u32(at + SORT_KEY, args.sort_key);
    mem.set_u32(at + OPTIONS, args.options | 1);
    mem.set_u32(at + EXCL_SET, 0);
    mem.set_u32(at + DEFINITION, args.definition);
    at
}

/// The child list of a CEntity: its head (`+0x48`), tail (`+0x4c`) and count (`+0x50`); the sibling links of a
/// child (`+0x10` next, `+0x14` previous) and the list it is in (`+0x18`).
const CHILD_HEAD: u32 = 0x48;
const CHILD_TAIL: u32 = 0x4C;
const CHILD_COUNT: u32 = 0x50;
const SIBLING: u32 = 0x10;
const SIBLING_PREV: u32 = 0x14;
const CHILD_LIST: u32 = 0x18;

/// The first child of the node at `at` (0 for none).
pub fn child_head(mem: &Mem, at: u32) -> u32 {
    mem.u32(at + CHILD_HEAD)
}

/// The sibling after the child at `at` (0 at the end of the list).
pub fn sibling(mem: &Mem, at: u32) -> u32 {
    mem.u32(at + SIBLING)
}

/// `CEntity::E43Vf150` (AppendChild, 0x51a370) on `parent`, with `child`: the stores in the original's order. Returns 1.
pub fn append_child(mem: &mut Mem, parent: u32, child: u32) -> u32 {
    let list = ChildList {
        head: mem.u32(parent + CHILD_HEAD),
        tail: mem.u32(parent + CHILD_TAIL),
        count: mem.u32(parent + CHILD_COUNT),
    };
    let link = tree::append_child(list, child);
    mem.set_u32(child + SIBLING_PREV, link.prev);
    mem.set_u32(child + SIBLING, 0);
    match link.joined_to {
        Some(tail) => mem.set_u32(tail + SIBLING, child),
        None => mem.set_u32(parent + CHILD_HEAD, child),
    }
    mem.set_u32(parent + CHILD_TAIL, child);
    mem.set_u32(child + CHILD_LIST, parent + CHILD_HEAD);
    mem.set_u32(parent + CHILD_COUNT, link.list.count);
    1
}

/// The def (`E43EntityDef`, `def.rs`): its link (`+0x0c`), its list (`+0x14`, whose count is at +8).
const DEF_LINK: u32 = 0x0C;
const DEF_LIST: u32 = 0x14;
const LIST_COUNT: u32 = 0x08;

/// `NTweak::CBase::CBase(name)` (0x52f090) at `at`: the base's vtable (0x72e444), its name, and a zero word. The
/// entity's state and the def are bases.
pub fn construct_base(mem: &mut Mem, at: u32, name: u32) {
    mem.set_u32(at, def::BASE_VTABLE);
    mem.set_u32(at + 4, name);
    mem.set_u32(at + 8, 0);
}

/// `E43EntityDef::E43EntityDef(name)` (0x51a530) at `at`: the base's constructor, then zeroed links and the vtable.
/// Returns `at` (eax).
pub fn construct_entity_def(mem: &mut Mem, at: u32, name: u32) -> u32 {
    construct_base(mem, at, name);
    for off in [0x10, DEF_LINK, DEF_LIST, 0x18] {
        mem.set_u32(at + off, 0);
    }
    mem.set_u32(at, def::DEF_VTABLE);
    at
}

/// `E43EntityDef::~E43EntityDef` (0x51a560) at `at`: the def's vtable, the release, then the base's vtable and a
/// zero name.
pub fn release_entity_def(mem: &mut Mem, at: u32, unlink: &mut impl Unlink) {
    mem.set_u32(at, def::DEF_VTABLE);
    if let Some(list) = def::release(mem.u32(at + DEF_LIST), at + DEF_LINK, unlink) {
        mem.set_u32(list + LIST_COUNT, mem.u32(list + LIST_COUNT).wrapping_sub(1));
    }
    mem.set_u32(at, def::BASE_VTABLE);
    mem.set_u32(at + 4, 0);
}

/// The transform at `at` (`E33Mat34`): its matrix pointer (+0) and its worker (+4).
pub fn load_transform(mem: &Mem, at: u32) -> Transform {
    Transform { matrix: mem.u32(at), worker: mem.u32(at + 4) }
}

/// `CGround::CDef::CDef(name)` (0x51b990) at `at`: the base (`E43EntityDef`), the def's vtable, then the four
/// tweak members (`TWEAK_MEMBERS`), each two vtable words and two zero words.
pub fn construct_ground_def(mem: &mut Mem, at: u32, name: u32) {
    construct_entity_def(mem, at, name);
    mem.set_u32(at, ground::DEF_VTABLE);
    for (off, kind) in ground::TWEAK_MEMBERS {
        let [first, second] = kind.vtables();
        mem.set_u32(at + off, first);
        mem.set_u32(at + off + 4, second);
        mem.set_u32(at + off + 8, 0);
        mem.set_u32(at + off + 0xC, 0);
    }
}

/// `CGround::CDef::New9` (0x51baa0, `GroundDefNew9`): a def from `heap`, constructed on `name`. Returns it, or 0 when
/// the allocation fails.
pub fn ground_def_new(mem: &mut Mem, name: u32, heap: &mut impl Heap) -> u32 {
    let at = heap.allocate(ground::DEF_SIZE);
    if at != 0 {
        construct_ground_def(mem, at, name);
    }
    at
}

/// `CHingedRigidBody::CDef`'s size (the block its factory allocates) and vtable (0x73d1ac).
pub const HINGED_DEF_SIZE: u32 = 0x510;
const HINGED_DEF_VTABLE: u32 = 0x0073_D1AC;
/// The two springs (`E44_BlockCtor`, 0x51ca40: an unsigned, then five reals), 0x60 apart.
const HINGED_DEF_SPRINGS: [u32; 2] = [0x1F0, 0x250];
/// The hinged definition's own tweaks after the springs, in the constructor's order.
const HINGED_DEF_VECTORS_RF: [u32; 3] = [0x2B0, 0x2D0, 0x2F0];
const HINGED_DEF_FACE_AREA: u32 = 0x340;
const HINGED_DEF_VECTORS_BF: [u32; 4] = [0x350, 0x370, 0x390, 0x3B0];
const HINGED_DEF_REALS: [u32; 7] = [0x470, 0x480, 0x490, 0x4A0, 0x4B0, 0x4C0, 0x4D0];
const HINGED_DEF_UNSIGNEDS: [u32; 3] = [0x4E0, 0x4F0, 0x500];
/// The vector lanes the constructor stores without marking them (offset, value): the solid's inverse inertia (1, 1, 1),
/// the hinge point (0.7, -1, 1), axis (0, 0, 1) and zero axis (0, 1, 0) in the reference frame, the hinge point (0, -0.5,
/// 0), axis (0, 0, 1) and zero axis (0, 1, 0) in the body frame (the floats at 0x795e14, 0x72f674, 0x795e20, 0x72d160).
const HINGED_DEF_LANES: [(u32, f32); 21] = [
    (0x130, 1.0),
    (0x134, 1.0),
    (0x138, 1.0),
    (0x300, 0.7),
    (0x304, -1.0),
    (0x308, 1.0),
    (0x2C0, 0.0),
    (0x2C4, 0.0),
    (0x2C8, 1.0),
    (0x2E0, 0.0),
    (0x2E4, 1.0),
    (0x2E8, 0.0),
    (0x3C0, 0.0),
    (0x3C4, -0.5),
    (0x3C8, 0.0),
    (0x380, 0.0),
    (0x384, 0.0),
    (0x388, 1.0),
    (0x3A0, 0.0),
    (0x3A4, 1.0),
    (0x3A8, 0.0),
];
/// The reals the constructor sets last, each marked dirty first: the hinge's maximum and minimum angles and +0x4a0
/// (-0.05, -1.8849558 and 0.05, from 0x73996c, 0x73d3b8 and 0x72dd78).
const HINGED_DEF_DEFAULTS: [(u32, u32); 3] = [(0x470, 0xBD4C_CCCD), (0x480, 0xBFF1_463B), (0x4A0, 0x3D4C_CCCD)];

/// `CHingedRigidBody::CDef::CDef(name)` (0x51e680) on the definition at `at`: the solid definition's base, the vtable,
/// the tweaks, then the default lanes and reals.
pub fn construct_hinged_def(mem: &mut Mem, at: u32, name: u32) {
    rigid_body::layout::solid_def_ctor(mem, at, name);
    mem.set_u32(at, HINGED_DEF_VTABLE);
    for spring in HINGED_DEF_SPRINGS {
        ntweak::construct_unsigned(mem, at + spring);
        for k in 1..6 {
            ntweak::construct_real(mem, at + spring + 0x10 * k);
        }
    }
    for off in HINGED_DEF_VECTORS_RF {
        ntweak::construct_vector(mem, at + off);
    }
    ntweak::construct_real(mem, at + HINGED_DEF_FACE_AREA);
    for off in HINGED_DEF_VECTORS_BF {
        ntweak::construct_vector(mem, at + off);
    }
    for off in HINGED_DEF_REALS {
        ntweak::construct_real(mem, at + off);
    }
    for off in HINGED_DEF_UNSIGNEDS {
        ntweak::construct_unsigned(mem, at + off);
    }
    for (off, value) in HINGED_DEF_LANES {
        mem.set_f32(at + off, value);
    }
    for (off, value) in HINGED_DEF_DEFAULTS {
        ntweak::set_dirty(mem, at + off);
        mem.set_u32(at + off + 0xC, value);
    }
}

/// `CHingedRigidBody::NewDef(name)` (0x51f880): a definition block from `host`, built by [`construct_hinged_def`].
/// Returns the block, or 0 when the allocation fails.
pub fn hinged_def_new(mem: &mut Mem, name: u32, host: &mut impl construct::Heap) -> u32 {
    let at = host.alloc(mem, HINGED_DEF_SIZE);
    if at != 0 {
        construct_hinged_def(mem, at, name);
    }
    at
}

/// `CHingedRigidBody`'s size (the block its definition's `Vf50` allocates), its vtable (0x73d23c), its state's
/// (0x73d228) and its scratch's (0x73d230).
pub const HINGED_SIZE: u32 = 0x660;
const HINGED_VTABLE: u32 = 0x0073_D23C;
const HINGED_STATE_VTABLE: u32 = 0x0073_D228;
const HINGED_SCRATCH_VTABLE: u32 = 0x0073_D230;
/// The body's owner, state and scratch.
const HINGED_OWNER: u32 = 0x3D0;
const HINGED_STATE: u32 = 0x3D4;
const HINGED_SCRATCH: u32 = 0x410;
/// The owner's flags byte: the body keeps the owner only when bit 3 is set.
const OWNER_FLAGS: u32 = 0x60;
const OWNER_FLAG: u8 = 0x8;
/// The state's reals, and the one set dirty with -0.75 (0x73d220); the two words it clears.
const HINGED_STATE_REALS: [u32; 3] = [0x4, 0x1C, 0x2C];
const HINGED_STATE_DIRTY: u32 = 0x1C;
const HINGED_STATE_ZEROS: [u32; 2] = [0x14, 0x18];

/// `CHingedRigidBody::CState::CState` (0x51cc90) on the state at `at`: the vtable, three reals, the second set dirty
/// with -0.75, and two cleared words. It reads none of its arguments.
fn construct_hinged_state(mem: &mut Mem, at: u32) {
    mem.set_u32(at, HINGED_STATE_VTABLE);
    for off in HINGED_STATE_REALS {
        ntweak::construct_real(mem, at + off);
    }
    ntweak::set_dirty(mem, at + HINGED_STATE_DIRTY);
    mem.set_f32(at + HINGED_STATE_DIRTY + 0xC, -0.75);
    for off in HINGED_STATE_ZEROS {
        mem.set_f32(at + off, 0.0);
    }
}

/// `CHingedRigidBody::CHingedRigidBody(def, a, b, flags)` (0x51e9a0) on the body at `this`: the solid base (with flag
/// 0x400), the vtable, the state, the scratch base and its vtable, the owner (`b` when its flag is set), and a
/// two-entry exclusion set from `host`.
pub fn construct_hinged(
    mem: &mut Mem,
    host: &mut (impl CtorHost + Heap),
    this: u32,
    def: u32,
    a: u32,
    b: u32,
    flags: u32,
) -> u32 {
    rigid_body::layout::construct_solid(mem, host, this, def, a, b, flags | 0x400);
    mem.set_u32(this, HINGED_VTABLE);
    construct_hinged_state(mem, this + HINGED_STATE);
    rigid_body::layout::store_scratch_base_init(mem, this + HINGED_SCRATCH);
    mem.set_u32(this + HINGED_SCRATCH, HINGED_SCRATCH_VTABLE);
    let owner = if b != 0 && mem.u8(b + OWNER_FLAGS) & OWNER_FLAG != 0 { b } else { 0 };
    mem.set_u32(this + HINGED_OWNER, owner);
    alloc_exclusion(mem, this, 2, host);
    this
}

/// `CHingedRigidBody::CDef::Vf50(a, b)` (0x51f8e0) on the definition `def`: a body block from `host`, built by
/// [`construct_hinged`] with no flags. Returns the body, or 0 when the allocation fails.
pub fn new_hinged(mem: &mut Mem, host: &mut (impl construct::Heap + CtorHost + Heap), def: u32, a: u32, b: u32) -> u32 {
    let at = construct::Heap::alloc(host, mem, HINGED_SIZE);
    if at == 0 {
        return 0;
    }
    construct_hinged(mem, host, at, def, a, b, 0)
}

/// `CGround::CDef::Vf50(a, b)` (0x51c810) on the def `def`: if the singleton holds a ground, that ground is deleted
/// (`calls`) and the singleton cleared; then a ground from `heap` is constructed as `CEntity(def, a, b, 0x80)`, its
/// vtable is stored and it becomes the singleton. Returns the ground, or 0 when the allocation fails.
pub fn ground_make(
    mem: &mut Mem,
    def: u32,
    a: u32,
    b: u32,
    heap: &mut impl Heap,
    calls: &mut (impl Owner + Replace),
) -> u32 {
    let old = mem.u32(ground::SINGLETON);
    if old != 0 {
        calls.delete(old);
        mem.set_u32(ground::SINGLETON, 0);
    }
    let at = heap.allocate(ground::GROUND_SIZE);
    if at == 0 {
        return 0;
    }
    let args = EntityArgs { definition: def, sort_key: a, parent: b, options: ground::GROUND_OPTIONS };
    construct_entity(mem, at, &args, calls);
    mem.set_u32(at, ground::GROUND_VTABLE);
    mem.set_u32(ground::SINGLETON, at);
    at
}
