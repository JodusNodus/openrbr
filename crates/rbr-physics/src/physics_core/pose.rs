//! The mounted body's pose update (`CMountedRigidBody::E45_M1980`, 0x521980, `src/rbr/RBR_E45_MountedA.cpp`) over the
//! physics core's store: the base's dirty call and flag from the enable state, the owner's pose composition on the car's
//! block, the two vector copies from the car's pose, and 16 bytes of the body's own vector copied to +0x2a0.

use super::layout::mounted_pose::PoseHost;
use super::store::CoreStore;

/// `CMountedRigidBody::E45_M1980` (0x521980) on the body at `body`. The enable state is read through the body's
/// state pointer (+0x580) first. The original's null case for the last copy's source (the body at -0xf0) is not a body,
/// so the source is always the body's +0x100.
pub fn pose_update<S: CoreStore>(store: &mut S, host: &mut impl PoseHost<S>, body: u32) {
    let state = store.mounted_enable(body);
    let base = store.mounted_base(body);
    host.set_dirty(store, base);
    store.set_mounted_dirty(body, u32::from(state != 0));
    let car = store.mounted_car(body);
    let block = store.car_block(car);
    let car_pose = store.car_pose(car);
    let owner = store.mounted_owner(body);
    host.body_compose(store, owner, block, car_pose);
    let (pose, copy) = (store.mounted_pose_at(body), store.mounted_pose_copy_at(body));
    host.copy_from(store, pose, car_pose);
    host.copy_from(store, copy, pose);
    store.copy_pose_source(body);
}
