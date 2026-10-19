//! The walks over a CEntity's child list (src/rbr/RBR_E43_EntityA.cpp, EntityC.cpp). The list starts at the
//! node's `+0x48` and links its children through `+0x10`. A gated walk calls one slot on each child whose gate
//! (`S80`, vtable slot 32) is nonzero; the pre-order walk (`WalkChildrenPre`) calls a functor on each gated child
//! and then walks that child's own list; the name walk (`NameWalk`) does the state's slot 17 first.
//!
//! The port makes the calls through the traits below; the caller answers them (the game, or the vector test).

/// The child call of a gated walk: the child's slot, and its argument where the slot takes one. The names give
/// the step pass the walk belongs to (`notes/physics/step.md`, 1c); `Unknown10` and `Unknown26` are not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildCall {
    /// `E43Vf20(p)`, slot 8: the child walk of the step's S20 pass (`WalkChildrenS20`).
    Substep(u32),
    /// `E43Vf28()`, slot 10. Unknown meaning.
    Unknown10,
    /// `E43Vf48()`, slot 18: the first-step init and the post-slice pass (step.md, S48). Working name.
    Init,
    /// `E43Vf58(p)`, slot 22: the car hook pass (S58).
    CarHook(u32),
    /// `E43Vf5C()`, slot 23: the body force pass (S5C).
    Force,
    /// `E43Vf64()`, slot 25: the body torque pass (S64).
    Torque,
    /// `E43Vf68(p)`, slot 26. Unknown meaning.
    Unknown26(u32),
    /// `E43Vf6C(dt)`, slot 27: the integrator pass (S6C).
    Integrate(u32),
}

/// The children of a node and the calls on them. `first_child` and `next_sibling` read the list links; `gated`
/// is the child's gate (`S80`); `call` is the child's call of `call`.
pub trait Children {
    /// The first child of `node` (its `+0x48`), 0 for none.
    fn first_child(&self, node: u32) -> u32;
    /// The child after `child` in its parent's list (`+0x10`), 0 at the end.
    fn next_sibling(&self, child: u32) -> u32;
    /// The child's gate (`S80`): nonzero when the walk reaches it.
    fn gated(&mut self, child: u32) -> bool;
}

/// A gated walk: every gated child gets `call`, in list order. Each child's gate is asked, then its call, then the
/// next sibling is read (the original's order).
pub fn walk_gated<T: Children + GatedCalls>(t: &mut T, node: u32, call: ChildCall) {
    let mut child = t.first_child(node);
    while child != 0 {
        if t.gated(child) {
            t.call(child, call);
        }
        child = t.next_sibling(child);
    }
}

/// The calls a gated walk makes on a child.
pub trait GatedCalls {
    fn call(&mut self, child: u32, call: ChildCall);
}

/// The pre-order walk (`E43_Walk1F0`): on each gated child, the functor (`functor->Vf04x(child)`, a game object) and
/// then the walk on the child's own list (`child.E43_Walk1F0(functor)`).
pub trait PreOrder: Children {
    fn apply_functor(&mut self, functor: u32, child: u32);
    fn descend(&mut self, child: u32, functor: u32);
}

/// `E43_Walk1F0(functor)` on `node`.
pub fn walk_pre_order<T: PreOrder>(t: &mut T, node: u32, functor: u32) {
    let mut child = t.first_child(node);
    while child != 0 {
        if t.gated(child) {
            t.apply_functor(functor, child);
            t.descend(child, functor);
        }
        child = t.next_sibling(child);
    }
}

/// The name walk (`CEntity::Vf40`, `NameWalk`): the state's slot 17 (`Unk44`, on the state object), then the
/// pre-order walk of `node` with the name functor (a stack object: its address is the caller's).
pub trait NameWalk {
    fn state_unk44(&mut self, node: u32);
    fn walk_children_pre(&mut self, node: u32, functor: u32);
}

/// `CEntity::Vf40` on `node`, with the functor object at `functor`.
pub fn name_walk<T: NameWalk>(t: &mut T, node: u32, functor: u32) {
    t.state_unk44(node);
    t.walk_children_pre(node, functor);
}

/// `CEntity::Vf6C(dt)`: the object's own name walk (`Vf40`, a virtual call on `node`) comes first.
pub trait Integrate: Children + GatedCalls {
    fn name_walk_self(&mut self, node: u32);
}

/// `CEntity::Vf6C(dt)` on `node`. The name walk is the object's own (virtual) `Vf40`, called before the walk.
pub fn integrate<T: Integrate>(t: &mut T, node: u32, dt: u32) {
    t.name_walk_self(node);
    walk_gated(t, node, ChildCall::Integrate(dt));
}

/// A CEntity's child list (`+0x48` head, `+0x4c` tail, `+0x50` count).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChildList {
    pub head: u32,
    pub tail: u32,
    pub count: u32,
}

/// What appending a child writes: the child's previous sibling (its `+0x14`), the old tail whose `+0x10` link
/// becomes the child (`None`: the list was empty, so the head is the child), and the list after the append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appended {
    pub prev: u32,
    pub joined_to: Option<u32>,
    pub list: ChildList,
}

/// `CEntity::E43Vf150` (AppendChild, 0x51a370): appends `child` to the end of `list`. The head is written only
/// when the list was empty (the original's `je`), the tail always.
pub fn append_child(list: ChildList, child: u32) -> Appended {
    let joined_to = (list.tail != 0).then_some(list.tail);
    let head = if list.tail == 0 { child } else { list.head };
    Appended { prev: list.tail, joined_to, list: ChildList { head, tail: child, count: list.count + 1 } }
}
