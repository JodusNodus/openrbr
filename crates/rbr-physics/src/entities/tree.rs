//! The walks over an entity's child list. A gated walk calls one pass on each child whose gate is nonzero; the pre-order
//! walk calls a functor on each gated child and then walks that child's own list; the name walk does the state's pass
//! first.
//!
//! The port makes the calls through the traits below; the caller answers them (the game, or the vector test).

/// The child call of a gated walk: the pass, and its argument where the pass takes one. The names give the step pass the walk
/// belongs to (`notes/physics.md`); `Unnamed1` and `Unnamed2` are not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildCall {
    /// The child walk of the step's substep pass.
    Substep(u32),
    /// Unknown meaning.
    Unnamed1,
    /// The first-step init and the post-slice pass. Working name.
    Init,
    /// The car hook pass.
    CarHook(u32),
    /// The body force pass.
    Force,
    /// The body torque pass.
    Torque,
    /// Unknown meaning.
    Unnamed2(u32),
    /// The integrator pass.
    Integrate(u32),
}

/// The children of a node and the calls on them. `first_child` and `next_sibling` read the list links; `gated` is the
/// child's gate.
pub trait Children {
    /// The first child of `node`, 0 for none.
    fn first_child(&self, node: u32) -> u32;
    /// The child after `child` in its parent's list, 0 at the end.
    fn next_sibling(&self, child: u32) -> u32;
    /// The child's gate: nonzero when the walk reaches it.
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

/// The pre-order walk: on each gated child, the functor (a game object) and then the walk on the child's own list.
pub trait PreOrder: Children {
    fn apply_functor(&mut self, functor: u32, child: u32);
    fn descend(&mut self, child: u32, functor: u32);
}

/// The pre-order walk of `node` with `functor`.
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

/// The name walk: the state's pass on the state object, then the pre-order walk of `node` with the name functor (a stack
/// object: its address is the caller's).
pub trait NameWalk {
    fn state_pass(&mut self, node: u32);
    fn walk_children_pre(&mut self, node: u32, functor: u32);
}

/// The name walk of `node`, with the functor object at `functor`.
pub fn name_walk<T: NameWalk>(t: &mut T, node: u32, functor: u32) {
    t.state_pass(node);
    t.walk_children_pre(node, functor);
}

/// The integrate pass: the object's own name walk (a virtual call on `node`) comes first.
pub trait Integrate: Children + GatedCalls {
    fn name_walk_self(&mut self, node: u32);
}

/// The integrate pass of `node`. The name walk is the object's own (virtual), called before the walk.
pub fn integrate<T: Integrate>(t: &mut T, node: u32, dt: u32) {
    t.name_walk_self(node);
    walk_gated(t, node, ChildCall::Integrate(dt));
}

/// An entity's child list: head, tail and count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChildList {
    pub head: u32,
    pub tail: u32,
    pub count: u32,
}

/// What appending a child writes: the child's previous sibling, the old tail whose next link
/// becomes the child (`None`: the list was empty, so the head is the child), and the list after the append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appended {
    pub prev: u32,
    pub joined_to: Option<u32>,
    pub list: ChildList,
}

/// Appends `child` to the end of `list`. The head is written only when the list was empty, the tail always.
pub fn append_child(list: ChildList, child: u32) -> Appended {
    let joined_to = (list.tail != 0).then_some(list.tail);
    let head = if list.tail == 0 { child } else { list.head };
    Appended { prev: list.tail, joined_to, list: ChildList { head, tail: child, count: list.count + 1 } }
}
