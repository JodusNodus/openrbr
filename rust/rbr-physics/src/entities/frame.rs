//! `CFrame` (`NPhysics::CFrame`): the link to the parent frame and the depth below the root. The
//! constructor (`CFrame::CFrame`, 0x51af70) also writes the vtable word; `layout` does that.

/// A frame's parent (its address, 0 for none) and its depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub parent: u32,
    pub depth: u32,
}

impl Frame {
    /// The frame under `parent`, whose depth is `parent_depth`; with no parent (`None`) the depth is 0.
    pub fn under(parent: u32, parent_depth: Option<u32>) -> Self {
        let depth = parent_depth.map_or(0, |d| d.wrapping_add(1));
        Frame { parent, depth }
    }
}

/// A frame of the entity tree (`CEntityFrame` and `CEntityOldFrame`, vtables 0x73cb64 and 0x73cb84): the
/// base frame's parent and depth, and the owner entity (`+0xc`) its hooks forward to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityFrame {
    pub parent: u32,
    pub depth: u32,
    pub owner: u32,
}

/// The owner's hooks, numbered by the frame slot that forwards to them (Vf04 is `One`, Vf18 is `Six`). What
/// they do is not known (the `CEntity` ones return their first argument; subclasses may override them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
}

/// Which of a hook's two owner slots a call uses: `X` for a frame's hook, and for an old frame the `X` when
/// the owner's gate is 0, else `Y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    X,
    Y,
}

/// The virtual calls a frame makes on game objects. The port decides which call comes next; the caller
/// performs it and answers with eax.
pub trait Owner {
    /// The owner's hook in form `form`, with `(a, b)`. The frame ignores the result.
    fn hook(&mut self, owner: u32, hook: Hook, form: Form, a: u32, b: u32) -> u32;
    /// The owner's gate (its `S80`): 0 picks an old frame's `X` form.
    fn gate(&mut self, owner: u32) -> u32;
    /// `ent->Vf30()`: the frame a new entity frame hangs under.
    fn frame_of(&mut self, ent: u32) -> u32;
    /// `ent->Vf34()`: the old frame a new old frame hangs under.
    fn old_frame_of(&mut self, ent: u32) -> u32;
}

/// `CEntityFrame::Vf04..Vf18` (0x519530..0x519620): if the depth is set, the owner's `hook` in form `X`;
/// returns `a`.
pub fn entity_hook(frame: &EntityFrame, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    if frame.depth != 0 {
        owner.hook(frame.owner, hook, Form::X, a, b);
    }
    a
}

/// `CEntity::E43Vf9C` (0x51a700), `E43VfA0` (0x51a710) and `E43VfB0` (0x51a750): return the first stack argument.
pub fn entity_result(_this: u32, a: u32, _b: u32) -> u32 {
    a
}

/// `CEntityOldFrame::Vf04..Vf18` (0x5196d0..0x519860): with depth 0 nothing; else the owner's gate picks the
/// hook's form (`X` for gate 0, `Y` otherwise). Returns `a`.
pub fn old_hook(frame: &EntityFrame, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    if frame.depth != 0 {
        let form = if owner.gate(frame.owner) == 0 { Form::X } else { Form::Y };
        owner.hook(frame.owner, hook, form, a, b);
    }
    a
}

/// What a new frame hangs under: the frame of an entity (`ent`, the `Vf30` or `Vf34` frame), or the root
/// (no entity: depth 1, under the root identity object at `identity`).
pub enum Under {
    Root { identity: u32 },
    Frame { frame: u32, frame_depth: u32 },
}

/// The base frame of a new entity frame: its parent and depth, as `Frame::under` gives them.
pub fn under_frame(under: &Under) -> Frame {
    match *under {
        Under::Root { identity } => Frame { parent: identity, depth: 1 },
        Under::Frame { frame, frame_depth } => Frame::under(frame, Some(frame_depth)),
    }
}
