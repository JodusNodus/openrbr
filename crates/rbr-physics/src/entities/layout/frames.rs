//! `CFrame`, `CEntityFrame` and `CEntityOldFrame`: the constructors and the frame hooks.

use crate::entities::frame::{self, EntityFrame, Frame, Hook, Owner, Under};
use crate::mem::Mem;

/// `CFrame` vtable (0x73ce4c).
const FRAME_VTABLE: u32 = 0x73ce4c;
const FRAME_PARENT: u32 = 0x04;
const FRAME_DEPTH: u32 = 0x08;

/// `CFrame::CFrame(parent)` (0x51af70): the frame at `at` under `parent` (0 for none). The parent's
/// depth is read only when there is a parent.
pub fn construct_frame(mem: &mut Mem, at: u32, parent: u32) {
    let parent_depth = (parent != 0).then(|| mem.u32(parent + FRAME_DEPTH));
    let frame = Frame::under(parent, parent_depth);
    mem.set_u32(at, FRAME_VTABLE);
    mem.set_u32(at + FRAME_PARENT, frame.parent);
    mem.set_u32(at + FRAME_DEPTH, frame.depth);
}

/// `CEntityFrame` and `CEntityOldFrame` (vtables 0x73cb64 and 0x73cb84): the base frame, then the owner at +0xc.
const ENTITY_FRAME_VTABLE: u32 = 0x73cb64;
const OLD_FRAME_VTABLE: u32 = 0x73cb84;
const OWNER: u32 = 0x0c;
/// The root identity object (`g_7cbb00`): the parent of a frame with no entity.
pub const ROOT_IDENTITY: u32 = 0x007cbb00;

/// The entity frame at `at`.
pub fn load_entity_frame(mem: &Mem, at: u32) -> EntityFrame {
    EntityFrame { parent: mem.u32(at + FRAME_PARENT), depth: mem.u32(at + FRAME_DEPTH), owner: mem.u32(at + OWNER) }
}

/// `CEntityFrame::Vf04..Vf18` at `this` (0x519530..0x519620).
pub fn entity_hook_at(mem: &Mem, this: u32, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    frame::entity_hook(&load_entity_frame(mem, this), hook, a, b, owner)
}

/// `CEntityOldFrame::Vf04..Vf18` at `this` (0x5196d0..0x519860).
pub fn old_hook_at(mem: &Mem, this: u32, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    frame::old_hook(&load_entity_frame(mem, this), hook, a, b, owner)
}

/// `CEntityFrame::CEntityFrame(owner, ent)` (0x5194b0), or the old frame's (0x519650, `old`), at `at`. With
/// an entity the frame hangs under `ent`'s frame, which the original asks for twice (base constructor and
/// body); with none, under the root identity object.
pub fn construct_entity_frame(mem: &mut Mem, at: u32, owner: u32, ent: u32, old: bool, calls: &mut impl Owner) {
    let under = if ent == 0 {
        Under::Root { identity: mem.u32(ROOT_IDENTITY) }
    } else {
        let mut parent = || {
            if old {
                calls.old_frame_of(ent)
            } else {
                calls.frame_of(ent)
            }
        };
        parent();
        let frame = parent();
        Under::Frame { frame, frame_depth: mem.u32(frame + FRAME_DEPTH) }
    };
    let base = frame::under_frame(&under);
    mem.set_u32(at, if old { OLD_FRAME_VTABLE } else { ENTITY_FRAME_VTABLE });
    mem.set_u32(at + FRAME_PARENT, base.parent);
    mem.set_u32(at + FRAME_DEPTH, base.depth);
    mem.set_u32(at + OWNER, owner);
}
