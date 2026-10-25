//! The mounted body's pose update (`CMountedRigidBody::E45_M1980`, 0x521980, `src/rbr/RBR_E45_MountedA.cpp`): the
//! base's dirty call and flag from the enable state, the owner's pose composition on the car's block, the two vector
//! copies from the car's pose, and 16 bytes of the body's own vector copied to +0x2a0. The calls are the caller's
//! (`PoseHost`).

use crate::mem::Mem;

pub use crate::physics_core::pose::pose_update;

/// The calls the pose update makes into code outside this port, in the original's order. Each gets the address of the
/// object it works on.
pub trait PoseHost<S = Mem> {
    /// `NTweak::CBase::SetDirty` (0x52f450) on the base at `base`.
    fn set_dirty(&mut self, mem: &mut S, base: u32);
    /// `CCar::BodyCompose(a, b)` (0x4e6a30) on the owner `owner`.
    fn body_compose(&mut self, mem: &mut S, owner: u32, a: u32, b: u32);
    /// `E01Vec4::CopyFrom(src)` (0x43c5b0) on the vector at `to`.
    fn copy_from(&mut self, mem: &mut S, to: u32, src: u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host whose calls write nothing (the caller's code does that).
    struct Silent;

    impl PoseHost for Silent {
        fn set_dirty(&mut self, _: &mut Mem, _: u32) {}

        fn body_compose(&mut self, _: &mut Mem, _: u32, _: u32, _: u32) {}

        fn copy_from(&mut self, _: &mut Mem, _: u32, _: u32) {}
    }

    const DIRTY_AT: u32 = 0x1dc;
    const SOURCE_AT: u32 = 0x100;
    const TARGET_AT: u32 = 0x2a0;
    const BODY: u32 = 0x1000;
    const STATE: u32 = 0x3000;

    /// A body (0x590 bytes) whose state pointer (+0x580) points at a state word holding `state`.
    fn body(state: u32) -> Mem {
        let mut mem = Mem::new();
        mem.map(BODY, &[0; 0x590]);
        mem.map(STATE, &[0; 4]);
        mem.set_u32(BODY + 0x580, STATE);
        mem.set_u32(STATE, state);
        mem
    }

    #[test]
    fn dirty_flag_is_set_for_any_enabled_state() {
        for (state, dirty) in [(0, 0), (1, 1), (2, 1)] {
            let mut mem = body(state);
            pose_update(&mut mem, &mut Silent, BODY);
            assert_eq!(mem.u32(BODY + DIRTY_AT), dirty, "state {state}");
        }
    }

    #[test]
    fn copies_the_last_vector_bits() {
        let mut mem = body(0);
        mem.set_u32(BODY + SOURCE_AT, 0x3f80_0000);
        mem.set_u32(BODY + SOURCE_AT + 12, 0x7fc0_0001);
        pose_update(&mut mem, &mut Silent, BODY);
        assert_eq!(mem.u32(BODY + TARGET_AT), 0x3f80_0000);
        assert_eq!(mem.u32(BODY + TARGET_AT + 12), 0x7fc0_0001);
    }
}
