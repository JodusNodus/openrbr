//! The typed slot-1 dispatch at the end of `CCar::Vf54x` (exe 0x4e7f90).

use crate::rigid_body::host::Vf54Host;

/// The call through the `m_scratch1690` vtable after `E47_Vf54Helper` has reset the scratch block.
pub trait Vf54xCalls: Vf54Host {
    fn scratch_slot1(&mut self, scratch: u32) -> u32;
}

/// Tail-dispatch to slot 1 of the car's typed `m_scratch1690` member (name ours).
pub fn dispatch_scratch_slot1(calls: &mut impl Vf54xCalls, scratch: u32) -> u32 {
    calls.scratch_slot1(scratch)
}
