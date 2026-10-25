//! The mounted body's pose update (`CMountedRigidBody::E45_M1980`, 0x521980, `src/rbr/RBR_E45_MountedA.cpp`) over the
//! physics core's store: the base's dirty call and flag from the enable state, the owner's pose composition on the car's
//! block, the two vector copies from the car's pose, and 16 bytes of the body's own vector copied to +0x2a0.

use super::layout::mounted_pose::PoseHost;
use super::store::CoreStore;

/// The body's base (`NTweak::CBase`, +0x1d0).
const BASE_AT: u32 = 0x1d0;
/// The car's block (+0x200, its +0x10 is passed on), and its pose (+0x220).
const CAR_BLOCK_AT: u32 = 0x200;
const CAR_POSE_AT: u32 = 0x220;
/// The body's pose (+0x140) and a copy of it (+0x330).
const POSE_AT: u32 = 0x140;
const COPY_AT: u32 = 0x330;

/// `CMountedRigidBody::E45_M1980` (0x521980) on the body at `body`. The enable state is read through the body's
/// state pointer (+0x580) first. The original's null case for the last copy's source (the body at -0xf0) is not a body,
/// so the source is always the body's +0x100.
pub fn pose_update<S: CoreStore>(store: &mut S, host: &mut impl PoseHost<S>, body: u32) {
    let state = store.mounted_enable(body);
    host.set_dirty(store, body + BASE_AT);
    store.set_mounted_dirty(body, u32::from(state != 0));
    let car = store.mounted_car(body);
    let block = match car.wrapping_add(CAR_BLOCK_AT) {
        0 => 0,
        block => block + 0x10,
    };
    let car_pose = car.wrapping_add(CAR_POSE_AT);
    let owner = store.mounted_owner(body);
    host.body_compose(store, owner, block, car_pose);
    host.copy_from(store, body + POSE_AT, car_pose);
    host.copy_from(store, body + COPY_AT, body + POSE_AT);
    store.copy_pose_source(body);
}
