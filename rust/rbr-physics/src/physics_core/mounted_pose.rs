//! The mounted body's pose update (`CMountedRigidBody::E45_M1980`, 0x521980, `src/rbr/RBR_E45_MountedA.cpp`): the
//! base's dirty call and flag from the enable state, the owner's pose composition on the car's block, the two vector
//! copies from the car's pose, and 16 bytes of the body's own vector copied to +0x2a0. The calls are the caller's
//! (`PoseHost`).

use super::layout;
use crate::mem::Mem;

/// The body's base (`NTweak::CBase`, +0x1d0) and its dirty flag (+0x1dc, set from the enable state).
const BASE_AT: u32 = 0x1d0;
const DIRTY_AT: u32 = 0x1dc;
/// The car (+0x34) and the owner that composes the pose (+0x3d0).
const CAR_AT: u32 = 0x34;
const OWNER_AT: u32 = 0x3d0;
/// The car's block (+0x200, its +0x10 is passed on), and its pose (+0x220).
const CAR_BLOCK_AT: u32 = 0x200;
const CAR_POSE_AT: u32 = 0x220;
/// The body's vectors: the pose (+0x140), a copy of it (+0x330), and the source and target of the last copy (+0x100
/// and +0x2a0).
const POSE_AT: u32 = 0x140;
const COPY_AT: u32 = 0x330;
const SOURCE_AT: u32 = 0x100;
const TARGET_AT: u32 = 0x2a0;

/// The calls the pose update makes into code outside this port, in the original's order. Each gets the address of the
/// object it works on.
pub trait PoseHost {
    /// `NTweak::CBase::SetDirty` (0x52f450) on the base at `base`.
    fn set_dirty(&mut self, mem: &mut Mem, base: u32);
    /// `CCar::BodyCompose(a, b)` (0x4e6a30) on the owner `owner`.
    fn body_compose(&mut self, mem: &mut Mem, owner: u32, a: u32, b: u32);
    /// `E01Vec4::CopyFrom(src)` (0x43c5b0) on the vector at `to`.
    fn copy_from(&mut self, mem: &mut Mem, to: u32, src: u32);
}

/// `CMountedRigidBody::E45_M1980` (0x521980) on the body at `body`. The enable state is read through the body's
/// state pointer (+0x580) first. The original's null case for the last copy's source (the body at -0xf0) is not a body,
/// so the source is always the body's +0x100.
pub fn pose_update(mem: &mut Mem, host: &mut impl PoseHost, body: u32) {
    let state = layout::mounted_state(mem, body);
    host.set_dirty(mem, body + BASE_AT);
    layout::set_word(mem, body + DIRTY_AT, u32::from(state != 0));
    let car = layout::word(mem, body + CAR_AT);
    let block = match car.wrapping_add(CAR_BLOCK_AT) {
        0 => 0,
        block => block + 0x10,
    };
    let car_pose = car.wrapping_add(CAR_POSE_AT);
    let owner = layout::word(mem, body + OWNER_AT);
    host.body_compose(mem, owner, block, car_pose);
    host.copy_from(mem, body + POSE_AT, car_pose);
    host.copy_from(mem, body + COPY_AT, body + POSE_AT);
    layout::copy_vec4(mem, body + SOURCE_AT, body + TARGET_AT);
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
