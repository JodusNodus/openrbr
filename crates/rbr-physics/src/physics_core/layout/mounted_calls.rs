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

/// The temporaries of `Vf64` (0x521740) in its frame, and the esp each call is entered at.
#[derive(Clone, Copy, Debug)]
pub struct PullFrame(u32);

impl PullFrame {
    pub fn new(frame: u32) -> Self {
        Self(frame)
    }

    pub fn solid_sp(self) -> u32 {
        self.0 - 4
    }

    pub fn compose_sp(self) -> u32 {
        self.0 - 0xc
    }

    pub fn walk_sp(self) -> u32 {
        self.0 - 0x10
    }

    /// The mount point composed into the owner's frame.
    pub fn composed(self) -> u32 {
        self.0 + 0x50
    }

    /// The velocity term less the origin (three lanes; the fourth is the stack's).
    pub fn term(self) -> u32 {
        self.0 + 0x20
    }

    /// The copy of the CDef's origin.
    pub fn origin(self) -> u32 {
        self.0 + 0x40
    }

    /// The walk's output.
    pub fn walked(self) -> u32 {
        self.0 + 0x60
    }

    /// The gap between the body and its mount point (three lanes), and its fourth lane (the stack's).
    pub fn gap(self) -> u32 {
        self.0 + 0x30
    }

    pub fn gap_w(self) -> u32 {
        self.0 + 0x3c
    }
}

/// The payloads `Vf6c` (0x521d90) builds in its frame, and the esp its calls are entered at.
#[derive(Clone, Copy, Debug)]
pub struct StepFrame(u32);

impl StepFrame {
    pub fn new(frame: u32) -> Self {
        Self(frame)
    }

    pub fn solid_sp(self) -> u32 {
        self.0 - 8
    }

    pub fn emit_sp(self) -> u32 {
        self.0 - 8
    }

    /// The body event's payload (first step only).
    pub fn event(self) -> Payload {
        Payload(self.0 + 0x50)
    }

    /// The state payload.
    pub fn state(self) -> Payload {
        Payload(self.0 + 0x20)
    }
}

/// A payload of `Vf6c`: two words (the event has the flag then the key, the state the key), the position and the rotation,
/// and the copies of both the event carries.
#[derive(Clone, Copy, Debug)]
pub struct Payload(u32);

impl Payload {
    pub fn word0(self) -> u32 {
        self.0
    }

    pub fn word1(self) -> u32 {
        self.0 + 4
    }

    pub fn position(self) -> u32 {
        self.0 + 0x10
    }

    pub fn rotation(self) -> u32 {
        self.0 + 0x20
    }

    pub fn position_copy(self) -> u32 {
        self.0 + 0x30
    }

    pub fn rotation_copy(self) -> u32 {
        self.0 + 0x40
    }
}
