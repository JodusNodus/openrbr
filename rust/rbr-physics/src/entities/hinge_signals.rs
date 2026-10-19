//! `CHingedRigidBody::SimFB40` (0x51fb40): the hinge's signals. It runs `SimDB40`, then emits the solid's general state
//! (`CSigSolidGeneralState`, 0x4e8330) and the hinged body's three words (`CSigHingedBody`, 0x51f640), then calls
//! `E43Vf28`. The pose and the rotation come from one of two sources, chosen by the key (+0x30) and the owner car
//! (+0x3d0): `ComposeA` and the hinge's own rotation, or the definition's pose and `ToQuat` of the hinge's quaternion.
//! The calls are the caller's (`SignalCalls`); the payloads are built here, in the original's field order.
//!
//! The asm and the C++ agree; the payload's words 1..=3 are not written by the original (padding in its frame), so
//! they are not part of the state.

/// The solid's general state: the payload `CMultiSignal1<const CSigSolidGeneralState&>::Emit` takes (0x58 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidGeneralState {
    /// The hinge's word at +0x30 (its key; the meaning is unknown).
    pub key: u32,
    /// The pose: `ComposeA`'s output, or the definition's +0x300 vector.
    pub pose: [f32; 4],
    /// The rotation: the hinge's +0x100 vector, or `ToQuat` of its quaternion (+0x550).
    pub rotation: [f32; 4],
    /// The hinge's +0x1c0 vector.
    pub first: [f32; 4],
    /// The hinge's +0x190 vector.
    pub second: [f32; 4],
    /// The constant 1.0 that the payload ends with (its meaning is unknown).
    pub unit: f32,
}

/// The hinged body's signal: three words (`CSigHingedBody`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingedBodySignal {
    /// The definition's raw word at +0x4fc (kept as bits).
    pub definition_value: u32,
    /// Whether the hinge's link flag is zero (the asm's `sete`), as 0 or 1.
    pub unmoved: u32,
    /// The hinge's angle (+0x3fc) minus the definition's reference (+0x49c).
    pub angle_delta: f32,
}

/// The inputs read from the hinged body and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalInput {
    /// The hinged body (the receiver of `SimDB40`, `ComposeA`, and `E43Vf28`).
    pub hinge: u32,
    /// The hinge's quaternion matrix (+0x550, the E34Quat object `ToQuat` reads); `SimD160` writes its product there.
    pub quat_matrix: u32,
    /// The identity global's value (0x7cbb00): `ComposeA`'s `t`.
    pub identity: u32,
    /// The definition's compose context (+0x3c0): `ComposeA`'s `a`.
    pub compose_context: u32,
    /// The hinge's key (+0x30).
    pub key: u32,
    /// The owner car (+0x3d0).
    pub owner: u32,
    /// The hinge's rotation vector (+0x100).
    pub body_rotation: [f32; 4],
    /// The hinge's +0x1c0 vector.
    pub first: [f32; 4],
    /// The hinge's +0x190 vector.
    pub second: [f32; 4],
    /// The definition's pose (+0x300).
    pub definition_pose: [f32; 4],
    /// The definition's raw word (+0x4fc).
    pub definition_value: u32,
    /// The hinge's angle (+0x3fc).
    pub angle: f32,
    /// The definition's reference angle (+0x49c).
    pub reference: f32,
    /// The link flag: the word the hinge's link pointer (+0x650) points to.
    pub link_flag: u32,
}

/// What `SimFB40` calls: `SimDB40`, the two pose sources, the two emits and `E43Vf28`.
pub trait SignalCalls {
    /// `CHingedRigidBody::SimDB40` (0x51db40) on the hinge.
    fn sim_db40(&mut self, hinge: u32);
    /// `E33Mat34::ComposeA` (0x51b4c0) on the hinge with (`a`, `t`); returns the 16 bytes it writes, as four floats.
    fn compose_a(&mut self, hinge: u32, a: u32, t: u32) -> [f32; 4];
    /// `E34Quat::ToQuat` (0x4e4d10) on the quaternion object; returns the four floats it writes.
    fn to_quat(&mut self, quat_matrix: u32) -> [f32; 4];
    /// `CMultiSignal1<const CSigSolidGeneralState&>::Emit` (0x4e8330) with the state.
    fn emit_solid(&mut self, state: &SolidGeneralState);
    /// `CMultiSignal1<const CSigHingedBody&>::Emit` (0x51f640) with the body's signal.
    fn emit_hinged(&mut self, body: &HingedBodySignal);
    /// `CEntity::E43Vf28` (0x51a3e0) on the hinge.
    fn vf28(&mut self, hinge: u32);
}

/// `CHingedRigidBody::SimFB40`: the signals of the hinged body.
pub fn emit_state<C: SignalCalls>(calls: &mut C, input: &SignalInput) {
    calls.sim_db40(input.hinge);
    let (pose, rotation) = if input.key == 0 || input.owner == 0 {
        let pose = calls.compose_a(input.hinge, input.compose_context, input.identity);
        (pose, input.body_rotation)
    } else {
        (input.definition_pose, calls.to_quat(input.quat_matrix))
    };
    calls.emit_solid(&SolidGeneralState {
        key: input.key,
        pose,
        rotation,
        first: input.first,
        second: input.second,
        unit: 1.0,
    });
    calls.emit_hinged(&HingedBodySignal {
        definition_value: input.definition_value,
        unmoved: u32::from(input.link_flag == 0),
        angle_delta: input.angle - input.reference,
    });
    calls.vf28(input.hinge);
}
