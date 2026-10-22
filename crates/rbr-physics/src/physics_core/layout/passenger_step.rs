//! `CPassenger::Step` (0x523160) and `E45PSmalls::M2BC0` (0x522bc0).

use super::globals::physics_global;
use super::lists::{f32, word};
use super::passenger_ctor;
use super::vectors::set_vec4s;
use crate::mem::Mem;
use crate::physics_core::math::Vec4;
use crate::physics_core::passenger;

/// The passenger's host pointer (+0x14, read by its spring and pull) and the step's (+0x40).
const PASSENGER_HOST_AT: u32 = 0x14;
const STEP_HOST_AT: u32 = 0x40;
/// The passenger's velocity (+0x30).
const VELOCITY_AT: u32 = 0x30;
/// The host's vectors the pull changes (+0x40 and +0x90) and the offset the step's walk adds (+0x230, three floats).
const HOST_H40_AT: u32 = 0x40;
const HOST_H90_AT: u32 = 0x90;
const HOST_OFFSET_AT: u32 = 0x230;
/// The physics object's CDef (+0x24) and the gravity in it (+0x160b0). The sentinel -0x160a0 in the CDef's place makes
/// the original read address 0.
pub(super) const PHYSICS_DEF_AT: u32 = 0x24;
pub(super) const GRAVITY_AT: u32 = 0x160b0;
const NO_GRAVITY: u32 = 0x160a0u32.wrapping_neg();

/// The gravity the step walks from (three floats in the physics object's CDef), or None for the sentinel.
pub(super) fn gravity_point(mem: &Mem) -> Option<[f32; 3]> {
    let def = word(mem, physics_global(mem) + PHYSICS_DEF_AT);
    if def == NO_GRAVITY {
        return None;
    }
    let at = def + GRAVITY_AT;
    Some([0, 4, 8].map(|k| f32(mem, at + k)))
}

/// `CPassenger::Step` (0x523160) on the passenger at `this`: the walk of the gravity point (`host.walk`), then the spring
/// and the pull (`host.spring`, `host.pull`). Returns the eax the original leaves, or None before any call when the
/// gravity pointer is the sentinel.
pub fn passenger_step(mem: &mut Mem, host: &mut impl passenger_ctor::StepHost, this: u32, dt: f32) -> Option<u32> {
    let gravity = gravity_point(mem)?;
    let body = word(mem, this + STEP_HOST_AT);
    let offset = [0, 4, 8].map(|k| f32(mem, body + HOST_OFFSET_AT + k));
    host.walk(passenger::walk_point(gravity, offset), body, this + VELOCITY_AT);
    host.spring(this, dt);
    Some(host.pull(this, dt))
}

/// `E45PSmalls::M2BC0` (0x522bc0) on the passenger at `this`: the pull on its host's vectors (`passenger::pull_to_rest`).
/// Returns the eax the original leaves, the host's +0x90 vector.
pub fn passenger_pull(mem: &mut Mem, this: u32, dt: f32) -> u32 {
    let host = word(mem, this + PASSENGER_HOST_AT);
    let (h40, h90) = (host + HOST_H40_AT, host + HOST_H90_AT);
    let (h40_next, h90_next) =
        passenger::pull_to_rest(Vec4::load(mem, h40), Vec4::load(mem, h90), Vec4::load(mem, this + VELOCITY_AT), dt);
    set_vec4s(mem, h40, &[h40_next]);
    set_vec4s(mem, h90, &[h90_next]);
    h90
}
