//! The contact step's walks over the owner's contact list (`RBR_E46_Contact.cpp`, the `CPhysics` step): `Work5500`
//! (0x525500). The nodes are the contact classes, which are not in this port, so their slot calls go to `NodeHost`.
//! The damper workers `Work5640` (0x525640) and `Work5760` (0x525760) walk the collision events instead; their
//! sequences are `layout::advance_contact_bodies` and `layout::advance_contact_bodies_remainder`, and their solid calls go to `SolidHost`.

use crate::mem::Mem;

/// The slot calls `Work5500` makes on each node: the vtable slots 0x5c and 0x64 (no argument), and 0x6c and 0x58 (the
/// stack word `p`). The host dispatches them (the nodes' methods are the caller's).
pub trait NodeHost {
    /// Slot 0x5c (name ours: force input).
    fn force_input(&mut self, mem: &mut Mem, node: u32);
    /// Slot 0x64 (name ours: force to acceleration).
    fn force_to_accel(&mut self, mem: &mut Mem, node: u32);
    /// Slot 0x6c (name ours: integration).
    fn integrate(&mut self, mem: &mut Mem, node: u32, p: f32);
    /// Slot 0x58 (name ours: child walk).
    fn child_walk(&mut self, mem: &mut Mem, node: u32, p: f32);
}

/// Calls `each` on every node of the list the owner (`this + 0x20`) holds at `+0x450`, linked at `+0x10`. The owner and
/// the head are read at the start of the walk, and each link after its node's call (the calls may change the list).
fn for_each_node(mem: &mut Mem, this: u32, mut each: impl FnMut(&mut Mem, u32)) {
    let owner = mem.u32(this + 0x20);
    let mut node = mem.u32(owner + 0x450);
    while node != 0 {
        each(mem, node);
        node = mem.u32(node + 0x10);
    }
}

/// `Work5500` (0x525500, thiscall, one stack word `p`): three passes over the owner's list, the slot 0x5c of each node,
/// then the slot 0x64 of each, then the slots 0x6c and 0x58 of each with `p`. The trailing walk of the original reads the
/// list and writes nothing, so it is not here.
pub fn step_bodies(mem: &mut Mem, host: &mut impl NodeHost, this: u32, p: f32) {
    for_each_node(mem, this, |mem, node| host.force_input(mem, node));
    for_each_node(mem, this, |mem, node| host.force_to_accel(mem, node));
    for_each_node(mem, this, |mem, node| {
        host.integrate(mem, node, p);
        host.child_walk(mem, node, p);
    });
}

/// The calls the damper workers make on the two solids of a collision event (its `+0xa0` and `+0x98`), in the original's
/// order. The slot 0x80 answers whether the solid takes part; `VfDx` (0x52cfd0) and `VfEx` (0x52d110) are `CSolid`'s.
pub trait SolidHost {
    /// The solid's slot 0x80, at `target` (its vtable entry), thiscall on the solid: nonzero when it answers (name ours).
    fn takes_part(&mut self, target: u32, solid: u32) -> u32;
    /// `CSolid::VfDx(f)` on the solid (thiscall, one float).
    fn back_step(&mut self, solid: u32, f: f32);
    /// `CSolid::VfEx(f)` on the solid (thiscall, one float).
    fn advance_rest(&mut self, solid: u32, f: f32);
}

/// The solid's flag byte (+0x60) bit the damper workers test: 4.
pub fn damper_flagged(flags: u8) -> bool {
    flags & 4 != 0
}

/// Whether `Work5760` acts on an event: its penetration scalar is below 1.0 (the constant at 0x795e14). The original's
/// `jbe` skips an equal or unordered sample, as `<` does.
pub fn damper_sample_below_one(sample: f32) -> bool {
    sample < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the calls in order: (slot, node, the stack word of the two slots that take one).
    #[derive(Default)]
    struct Calls(Vec<(u32, u32, Option<u32>)>);

    impl NodeHost for Calls {
        fn force_input(&mut self, _: &mut Mem, node: u32) {
            self.0.push((0x5C, node, None));
        }

        fn force_to_accel(&mut self, _: &mut Mem, node: u32) {
            self.0.push((0x64, node, None));
        }

        fn integrate(&mut self, _: &mut Mem, node: u32, p: f32) {
            self.0.push((0x6C, node, Some(p.to_bits())));
        }

        fn child_walk(&mut self, _: &mut Mem, node: u32, p: f32) {
            self.0.push((0x58, node, Some(p.to_bits())));
        }
    }

    /// The object at 0x0f00 has the owner at +0x20 (0x1000), which holds three nodes at 0x2000, 0x3000 and 0x4000 (the
    /// links at +0x10).
    #[test]
    fn three_nodes_in_three_passes() {
        let mut mem = Mem::new();
        mem.set_u32(0x0F20, 0x1000);
        mem.set_u32(0x1450, 0x2000);
        mem.set_u32(0x2010, 0x3000);
        mem.set_u32(0x3010, 0x4000);
        mem.set_u32(0x4010, 0);
        let mut host = Calls::default();
        step_bodies(&mut mem, &mut host, 0x0F00, 1.5);
        assert_eq!(host.0.len(), 12);
        assert_eq!(host.0[0], (0x5C, 0x2000, None));
        assert_eq!(host.0[2], (0x5C, 0x4000, None));
        assert_eq!(host.0[3], (0x64, 0x2000, None));
        assert_eq!(host.0[6], (0x6C, 0x2000, Some(1.5f32.to_bits())));
        assert_eq!(host.0[7], (0x58, 0x2000, Some(1.5f32.to_bits())));
        assert_eq!(host.0[11], (0x58, 0x4000, Some(1.5f32.to_bits())));
    }
}
