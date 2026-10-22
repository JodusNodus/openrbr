//! Small pieces of a hinged body's detach from its car, and of an entity's registration in the root frame.

/// The body's world point lane: the car's lane plus the car-transformed local lane, in that order.
pub fn world_lane(car: f32, local: f32) -> f32 {
    car + local
}

/// The serial a registration takes: the frame's registration count plus one.
pub fn next_serial(count: u32) -> u32 {
    count.wrapping_add(1)
}
