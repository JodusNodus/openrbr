//! Arithmetic of `CHingedRigidBody::SimFD00` (0x51fd00), the detach from the car, and `CEntity::E43_InitBD0` (0x519bd0).

/// The body's world point: the car's lane plus the car-transformed local lane, in that order (`movss car; addss local`).
pub fn world_lane(car: f32, local: f32) -> f32 {
    car + local
}

/// The serial `E43_InitBD0` stores: the walker's count plus one.
pub fn next_serial(count: u32) -> u32 {
    count.wrapping_add(1)
}
