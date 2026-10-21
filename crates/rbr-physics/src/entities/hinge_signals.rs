//! The hinged body's state signals. It walks the body through its frames ([`super::hinge_frame_walks`]), then emits the
//! solid's general state (key, position, orientation, velocities) and the hinged body's three words (`BodyId`, whether the
//! link flag is zero, and the angle's distance from `HingeDefaultAngle`), then runs the entity pass. The position and the
//! orientation come from one of two sources, chosen by the key and the owner car: the hinge point composed with the body and
//! the body's own orientation, or `vecHingePoint_RF` and the quaternion of the body's orientation matrix. The calls are the
//! caller's ([`SignalCalls`]); the payloads are built here, in the original's field order.
//!
//! The payload's words 1..=3 are not written by the original (padding in its frame), so they are not part of the state.

/// The solid's general state: the payload of the solid-state signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidGeneralState {
    /// The body's key (named by use: the meaning is not known).
    pub key: u32,
    /// The body's position: the hinge point composed with the body, or `vecHingePoint_RF`.
    pub position: [f32; 4],
    /// The body's orientation: the body's own, or the quaternion of its orientation matrix.
    pub orientation: [f32; 4],
    /// The body's linear velocity.
    pub linear_velocity: [f32; 4],
    /// The body's angular velocity.
    pub angular_velocity: [f32; 4],
    /// The constant 1.0 that the payload ends with (its meaning is unknown).
    pub unit: f32,
}

/// The hinged body's signal: three words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingedBodySignal {
    /// `BodyId`, kept as bits.
    pub body_id: u32,
    /// Whether the body's link flag is zero, as 0 or 1.
    pub unmoved: u32,
    /// The hinge angle minus `HingeDefaultAngle`, rad.
    pub angle_delta: f32,
}

/// The inputs read from the hinged body and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalInput {
    /// The hinged body, as an object handle.
    pub body: u32,
    /// The body's orientation matrix (a handle): `SimD160`'s product, which the quaternion is taken from.
    pub orientation_matrix: u32,
    /// The identity frame (the root), as an object handle.
    pub identity: u32,
    /// `vecHingePoint_BF`, as a handle: what the body composes.
    pub hinge_point_bf: u32,
    /// The body's key.
    pub key: u32,
    /// The body's owner car (0 for none).
    pub owner: u32,
    /// The body's orientation.
    pub orientation: [f32; 4],
    /// The body's linear and angular velocities.
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    /// `vecHingePoint_RF`.
    pub hinge_point_rf: [f32; 4],
    /// `BodyId`, as bits.
    pub body_id: u32,
    /// The hinge angle and `HingeDefaultAngle`, rad.
    pub angle: f32,
    pub default_angle: f32,
    /// The link flag: the word the body's link points to.
    pub link_flag: u32,
}

/// What the signals call: the frame walks, the two position sources, the two emits and the entity pass.
pub trait SignalCalls {
    /// Walks the body through its frames ([`super::hinge_frame_walks::walk_hinge_frames`]).
    fn walk_frames(&mut self, body: u32);
    /// Composes `hinge_point_bf` with the body in the `identity` frame; returns the 16 bytes it writes, as four floats.
    fn compose_position(&mut self, body: u32, hinge_point_bf: u32, identity: u32) -> [f32; 4];
    /// The quaternion of the orientation matrix; returns the four floats it writes.
    fn matrix_to_quaternion(&mut self, orientation_matrix: u32) -> [f32; 4];
    /// Emits the solid's general state.
    fn emit_solid_state(&mut self, state: &SolidGeneralState);
    /// Emits the hinged body's signal.
    fn emit_hinged_state(&mut self, body: &HingedBodySignal);
    /// The entity pass on the body (named by use: the meaning of the pass is not known).
    fn run_pass(&mut self, body: u32);
}

/// Emits the hinged body's signals.
pub fn emit_hinge_state<C: SignalCalls>(calls: &mut C, input: &SignalInput) {
    calls.walk_frames(input.body);
    let (position, orientation) = if input.key == 0 || input.owner == 0 {
        let position = calls.compose_position(input.body, input.hinge_point_bf, input.identity);
        (position, input.orientation)
    } else {
        (input.hinge_point_rf, calls.matrix_to_quaternion(input.orientation_matrix))
    };
    calls.emit_solid_state(&SolidGeneralState {
        key: input.key,
        position,
        orientation,
        linear_velocity: input.linear_velocity,
        angular_velocity: input.angular_velocity,
        unit: 1.0,
    });
    calls.emit_hinged_state(&HingedBodySignal {
        body_id: input.body_id,
        unmoved: u32::from(input.link_flag == 0),
        angle_delta: input.angle - input.default_angle,
    });
    calls.run_pass(input.body);
}
