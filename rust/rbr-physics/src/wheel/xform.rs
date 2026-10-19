//! 0x53a8a0 `E51Wheel::M3A8A0` (thiscall): the car's transforms into the CDef's axes. Four pairs go through
//! `CCar::BodyToWorldPair` (0x4e69c0), four through `CCar::BodyCompose` (0x4e6a30). The sequence (the reads and stores
//! around each call) is `layout::car_frames`; this module holds the car calls and their order.

use crate::math::vec::Vec4;

/// The two car transforms the caller makes (thiscall, `ret 8`): each takes a source vector and returns the vector the car
/// writes to its output. A source is `None` when the CDef's guard is zero (the original passes null). The car answers them
/// (the car_body port, or the original's record in the tests).
pub trait CarXform {
    /// `CCar::BodyToWorldPair(a, b)` (0x4e69c0) on `car`: the result is the vector written to `b`.
    fn body_to_world_pair(&mut self, car: u32, a: Option<Vec4>) -> Vec4;
    /// `CCar::BodyCompose(a, b)` (0x4e6a30) on `car`: the result is the vector written to `b`.
    fn body_compose(&mut self, car: u32, a: Option<Vec4>) -> Vec4;
}

/// Which car transform a pair goes through.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Transform {
    BodyToWorldPair,
    BodyCompose,
}

/// The transform of each of the eight pairs, in the order the asm makes the calls (`RBR_E51_WheelA.cpp:50-67`). The
/// offsets of the pairs are in `layout::XFORM_PAIRS`.
const TRANSFORMS: [Transform; 8] = [
    Transform::BodyToWorldPair,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
    Transform::BodyCompose,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
];

/// The car transform of pair `pair` (an index into `TRANSFORMS`) on `car`, with source `a`.
pub fn transform_call(host: &mut impl CarXform, pair: usize, car: u32, a: Option<Vec4>) -> Vec4 {
    match TRANSFORMS[pair] {
        Transform::BodyToWorldPair => host.body_to_world_pair(car, a),
        Transform::BodyCompose => host.body_compose(car, a),
    }
}
