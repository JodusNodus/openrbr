//! `CPhysics::E46_Notify67d0` (0x5267d0), the placement receiver, and the entity calls it files objects with.

use super::globals::{entity_list, physics, state_of};
use super::lists::{push_item, unlink_item, vtable_target};
use crate::mem::Mem;
use crate::physics_core::detect::GridBuildHost;
use crate::physics_core::setup::WallHost;
use crate::physics_core::{detect, setup, signal};
use crate::rigid_body::host::HostUnavailable;

/// How `CState::RegWall` files an object: its slots 0x8c, 0x80 and 0x88 (virtual: the host answers for the classes it
/// knows, `Err` for the rest).
pub trait FilingHost {
    fn filing(&mut self, mem: &Mem, object: u32) -> Result<signal::Filing, HostUnavailable>;
}

/// The calls `CPhysics::E46_Notify67d0` (0x5267d0) makes on the definition and the object a placement makes (virtual
/// slots: the host answers for the classes it knows, `Err` for the rest), and `E45_MD60`'s.
pub trait NotifyHost: GridBuildHost + FilingHost {
    /// Slot 0x50 of the definition `def` with `a` and `b` (under `E39_EntfCreate`): returns the new object.
    fn def_new(&mut self, mem: &mut Mem, def: u32, a: u32, b: u32) -> Result<u32, HostUnavailable>;
    /// Slot 0x44 of `object` with the placement at `stuff`.
    fn place(&mut self, mem: &mut Mem, object: u32, stuff: u32) -> Result<(), HostUnavailable>;
    /// Slot 0x150 of `parent` with `object`.
    fn adopt(&mut self, mem: &mut Mem, parent: u32, object: u32) -> Result<(), HostUnavailable>;
    /// Slot 0x90 of `object` (the placement's flag bit 0), else slot 0x94.
    fn wake(&mut self, mem: &mut Mem, object: u32) -> Result<(), HostUnavailable>;
    fn sleep(&mut self, mem: &mut Mem, object: u32) -> Result<(), HostUnavailable>;
}

/// The definition registry (`CEntityFactory`, 0x7c8040): entry `i`'s definition at `+0x1c + 0x14 * i`.
pub const REGISTRY: u32 = 0x007C_8040;
const REGISTRY_DEFS: u32 = 0x1C;
const REGISTRY_ENTRY: u32 = 0x14;
/// `CEntity::Vf00` (0x519950), slot 0 of every object made so far: it only reads (the name).
const ENTITY_VF00: u32 = 0x0051_9950;

/// `E39_EntfCreate` (0x502070) on the registry at `registry`: entry `kind`'s definition makes the object (its slot 0x50
/// with `a` and `b`), then the object's slot 0 runs. Returns the object. No object (the original calls through its
/// null vtable) or a slot 0 other than `CEntity::Vf00` is `Err`.
pub fn entf_create(
    mem: &mut Mem,
    host: &mut impl NotifyHost,
    registry: u32,
    kind: u32,
    a: u32,
    b: u32,
) -> Result<u32, HostUnavailable> {
    let def = mem.u32(registry + REGISTRY_DEFS + REGISTRY_ENTRY * kind);
    let object = host.def_new(mem, def, a, b)?;
    if object == 0 || mem.u32(mem.u32(object)) != ENTITY_VF00 {
        return Err(HostUnavailable);
    }
    Ok(object)
}

/// An entity's parent.
const ENTITY_PARENT: u32 = 0x44;

/// `CEntity::Vf00` (0x519950): the pointer at `+0x3c`, or the class-name fallback when it is null.
pub fn entity_vf00(mem: &Mem, this: u32) -> u32 {
    if this.wrapping_add(0x38) == 0 {
        return 0x0073_CBA0;
    }
    match mem.u32(this + 0x3C) {
        0 => 0x0073_CBA0,
        name => name,
    }
}

/// `CEntity::E43Vf90` (0x519a60) and `CEntity::E43Vf94` (0x519a80) on the entity at `this` (slots 0x90 and 0x94, which
/// `CSolid`'s overrides call directly): with no parent, `E43HostRel::WallRel` (0x5276a0) files it again with its own filing.
pub fn entity_rel(mem: &mut Mem, host: &mut impl FilingHost, this: u32) -> Result<(), HostUnavailable> {
    if mem.u32(this + ENTITY_PARENT) != 0 {
        return Ok(());
    }
    let filing = host.filing(mem, this)?;
    let physics = physics(mem);
    setup::wall_rel(mem, &mut Filer(filing), physics, this);
    Ok(())
}

/// `CEntity::E43Vf158` (0x519aa0): when the entity has no parent, file it again with its own slots; otherwise tail-call
/// slot 86 on the parent at the same entry stack pointer.
pub trait EntityVf158Host: FilingHost {
    fn slot158(&mut self, sp: u32, target: u32, parent: u32) -> Result<u32, HostUnavailable>;
}

pub fn entity_vf158(
    mem: &mut Mem,
    host: &mut impl EntityVf158Host,
    this: u32,
    sp: u32,
) -> Result<u32, HostUnavailable> {
    let parent = mem.u32(this + ENTITY_PARENT);
    if parent == 0 {
        entity_rel(mem, host, this)?;
        return Ok(0);
    }
    let target = vtable_target(mem, parent, 0x158);
    host.slot158(sp, target, parent)
}

/// `E43HostRel::WallRel`'s calls (0x5276a0): the unlink, and the filing again with the entity's own slots.
pub struct Filer(pub signal::Filing);

impl WallHost for Filer {
    fn unlink(&mut self, mem: &mut Mem, link: u32) {
        unlink_item(mem, link);
    }
    fn reg_wall(&mut self, mem: &mut Mem, state: u32, wall: u32) {
        let (s8c, s80, s88) = self.0;
        push_item(mem, entity_list(state, s8c, s80, s88), wall);
    }
}

/// A placement (`CSigStuff`): its collision id (or descriptor), object type, object, handle and flags.
const STUFF_COLLISION: u32 = 0x34;
const STUFF_TYPE: u32 = 0x38;
const STUFF_OBJECT: u32 = 0x3C;
const STUFF_HANDLE: u32 = 0x40;
const STUFF_FLAGS: u32 = 0x44;
/// A collision grid's name.
const GRID_NAME: u32 = 0x684;
/// The physics object's definition and scratch, and its own debug word; the scratch's stack of parents (`+0xe848`) and
/// its top index (`+0xe888`, -1 when empty); the definition's debug word.
const PHYSICS_DEF: u32 = 0x24;
const PHYSICS_SCRATCH: u32 = 0x28;
const PHYSICS_DEBUG: u32 = 0x38;
const PARENTS: u32 = 0xE848;
const PARENT_TOP: u32 = 0xE888;
const DEF_DEBUG: u32 = 0x160F4;

/// The parent stack's top: the object new placements hang under, 0 when the stack is empty.
fn current_parent(mem: &Mem, scratch: u32) -> u32 {
    let top = mem.u32(scratch + PARENT_TOP) as i32;
    if top < 0 {
        0
    } else {
        mem.u32(scratch + PARENTS + 4 * top as u32)
    }
}

/// `CPhysics::E46_Notify67d0` (0x5267d0) on `[0x1659b70]`, the receiver of a placement (`CSigStuff`) at `stuff`:
/// `E45_MD60` (the cell grid, if there is none yet); the object of the registry entry `signal::stuff_kind` picks
/// ([`entf_create`]), made with the placement's handle (the sort key: the emitter sets it) and the current parent,
/// kept at the placement's `+0x3c`; its slot 0x44 on the placement; filed under the parent (its slot 0x150) or, with
/// none, with the state (`CState::RegWall`); its slot 0x90 or 0x94. A placement with flag 2 makes its object the
/// parent of the next ones, one with flag 4 ends the current parent (the car: the car, then its wheels and bodies, the
/// last with flag 4). A dynamic placement first has its descriptor's grid built (`E37SigEmit9B30`, whose signal's
/// receiver is `E37SigFactory720`), under the next free collision id with flags 0x30. The debug branch (bit 0x100 of
/// the definition's or the physics' debug word: the object's slot 0x118 and its state's slot 0x4c) is not ported.
pub fn notify_stuff(mem: &mut Mem, host: &mut impl NotifyHost, stuff: u32) -> Result<(), HostUnavailable> {
    let physics = physics(mem);
    detect::build_cell_grid(mem, host);
    let desc = mem.u32(stuff + STUFF_COLLISION);
    match signal::mesh_emit(mem.u32(stuff + STUFF_FLAGS)) {
        signal::MeshEmit::None => {}
        emit => {
            if emit == signal::MeshEmit::NewId {
                let id = crate::collision::layout::alloc_collision_id(mem);
                mem.set_u32(desc, id);
                mem.set_u32(desc + 4, mem.u32(desc + 4) | 4);
            }
            crate::collision::layout::collision_mesh_factory(mem, desc);
        }
    }
    let collision = mem.u32(stuff + STUFF_COLLISION);
    let grid = if collision == 0 { 0 } else { crate::collision::layout::select_table(mem, stuff) };
    let name = if grid == 0 { 0 } else { mem.u32(grid + GRID_NAME) };
    let name = (name != 0).then(|| c_string(mem, name));
    let kind = signal::stuff_kind(mem.u32(stuff + STUFF_TYPE), name.as_deref());
    let scratch = mem.u32(physics + PHYSICS_SCRATCH);
    let parent = current_parent(mem, scratch);
    let object = entf_create(mem, host, REGISTRY, kind, mem.u32(stuff + STUFF_HANDLE), parent)?;
    mem.set_u32(stuff + STUFF_OBJECT, object);
    host.place(mem, object, stuff)?;
    let parent = current_parent(mem, scratch);
    if parent != 0 {
        host.adopt(mem, parent, object)?;
    } else {
        let (s8c, s80, s88) = host.filing(mem, object)?;
        push_item(mem, entity_list(state_of(mem, physics), s8c, s80, s88), object);
    }
    if mem.u32(stuff + STUFF_FLAGS) & 1 != 0 {
        host.wake(mem, object)?;
    } else {
        host.sleep(mem, object)?;
    }
    let def = mem.u32(physics + PHYSICS_DEF);
    if (mem.u32(def + DEF_DEBUG) | mem.u32(physics + PHYSICS_DEBUG)) & 0x100 != 0 {
        return Err(HostUnavailable);
    }
    let flags = mem.u32(stuff + STUFF_FLAGS);
    let top = mem.u32(scratch + PARENT_TOP) as i32;
    if flags & 2 != 0 {
        mem.set_u32(scratch + PARENT_TOP, (top + 1) as u32);
        mem.set_u32(scratch + PARENTS + 4 * (top + 1) as u32, object);
    } else if flags & 4 != 0 {
        mem.set_u32(scratch + PARENT_TOP, (top - 1).max(-1) as u32);
    }
    Ok(())
}

/// The zero-terminated string at `at`.
fn c_string(mem: &Mem, at: u32) -> Vec<u8> {
    (at..).map(|a| mem.u8(a)).take_while(|&b| b != 0).collect()
}
