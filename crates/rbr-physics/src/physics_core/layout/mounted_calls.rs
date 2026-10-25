//! `CMountedRigidBody::Vf64` (0x521740) and `Vf6c` (0x521d90): `physics_core::mounted` over the store. The calls they make
//! into the original are the host's.

pub use crate::physics_core::mounted::{mounted_vf64, mounted_vf6c};

/// The calls `CMountedRigidBody::Vf64` makes, each entered at the stack pointer `sp` the original's call has.
pub trait MountedVf64Host {
    /// `CSolid::E43Vf64` (0x52cb20) on the body: the path of a body without an owner. Returns its eax.
    fn solid_vf64(&mut self, sp: u32, body: u32) -> u32;
    /// `CCar::WorldCompose` (0x4e6950) on the owner: the point at `point` into the owner's frame, into `out`.
    fn world_compose(&mut self, sp: u32, owner: u32, point: u32, out: u32);
    /// `E43PairWalk::MB560` on the walker: `src` through the owner, into `out`.
    fn walk(&mut self, sp: u32, src: u32, owner: u32, out: u32);
}

/// The calls `CMountedRigidBody::Vf6c` makes, each entered at the stack pointer `sp` the original's call has. Each
/// returns its eax.
pub trait MountedVf6cHost {
    /// `CSolid::E43Vf6C(dt)` (0x52cc30) on the body: the path of a body without an owner.
    fn solid_vf6c(&mut self, sp: u32, body: u32, dt: f32) -> u32;
    /// `CMultiSignal1<const CSigMrRBodyEvent&>::Emit` (0x51f7c0) on the signal at `signal` with the payload at `sig`.
    fn emit_event(&mut self, sp: u32, signal: u32, sig: u32) -> u32;
    /// `CMultiSignal1<const CSigMrRBodyState&>::Emit` (0x51f700) on the signal at `signal` with the payload at `sig`.
    fn emit_state(&mut self, sp: u32, signal: u32, sig: u32) -> u32;
}
