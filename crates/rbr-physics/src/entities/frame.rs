//! The frames of the entity tree: the link to the parent frame and the depth below the root. The construction is layout.

use super::store::EntityStore;

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

/// A frame of the entity tree (a frame and an old frame share the layout): the base frame's parent and depth, and the owner
/// entity its hooks forward to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityFrame {
    pub parent: u32,
    pub depth: u32,
    pub owner: u32,
}

/// The owner's hooks, numbered in the order of the frame's forwarding slots (the first is `One`, the last `Six`). What they do
/// is not known (the entity's own return their first argument; subclasses may override them).
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

/// The virtual calls a frame makes on game objects. The model decides which call comes next; the caller performs it.
pub trait Owner {
    /// The owner's hook in form `form`, with `(a, b)`. The frame ignores the result.
    fn hook(&mut self, owner: u32, hook: Hook, form: Form, a: u32, b: u32) -> u32;
    /// The owner's gate: 0 picks an old frame's `X` form.
    fn gate(&mut self, owner: u32) -> u32;
    /// The frame of an entity: what a new entity frame hangs under.
    fn frame_of(&mut self, ent: u32) -> u32;
    /// The old frame of an entity: what a new old frame hangs under.
    fn old_frame_of(&mut self, ent: u32) -> u32;
}

/// An entity frame's hook: if the depth is set, the owner's `hook` in form `X`; returns `a`.
pub fn entity_hook(frame: &EntityFrame, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    if frame.depth != 0 {
        owner.hook(frame.owner, hook, Form::X, a, b);
    }
    a
}

/// Three entity slots that return their first argument.
pub fn entity_result(_this: u32, a: u32, _b: u32) -> u32 {
    a
}

/// An old frame's hook: with depth 0 nothing; else the owner's gate picks the hook's form (`X` for gate 0, `Y` otherwise).
/// Returns `a`.
pub fn old_hook(frame: &EntityFrame, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    if frame.depth != 0 {
        let form = if owner.gate(frame.owner) == 0 { Form::X } else { Form::Y };
        owner.hook(frame.owner, hook, form, a, b);
    }
    a
}

/// What a new frame hangs under: the frame of an entity (the frame or old frame of the entity), or the root
/// (no entity: depth 1, under the root identity object `identity`).
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

/// `CEntityFrame::Vf04..Vf18` at `this` (0x519530..0x519620).
pub fn entity_hook_at<S: EntityStore>(store: &S, this: u32, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    entity_hook(&store.load_entity_frame(this), hook, a, b, owner)
}

/// `CEntityOldFrame::Vf04..Vf18` at `this` (0x5196d0..0x519860).
pub fn old_hook_at<S: EntityStore>(store: &S, this: u32, hook: Hook, a: u32, b: u32, owner: &mut impl Owner) -> u32 {
    old_hook(&store.load_entity_frame(this), hook, a, b, owner)
}
