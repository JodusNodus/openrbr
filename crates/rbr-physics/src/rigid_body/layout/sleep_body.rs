//! The body and definition fields the sleep step reads and writes, and the parent frame.

use super::contacts::{HOOK, RING_TAIL};
use super::rest::{ACTIVE, VERY_SOFT};
use super::scratch_state::{load_quat, store_quat};
use super::{
    ACCEL_ANGULAR_EST, ACCEL_ANGULAR_FILT, ACCEL_LINEAR_EST, ACCEL_LINEAR_FILT, ANGULAR_ACCEL, ANGULAR_VELOCITY,
    DEF_ANGULAR_SLEEP_VELOCITY, DEF_INVERSE_MASS, DEF_LINEAR_SLEEP_VELOCITY, DEF_LOCAL_INVERSE_INERTIA, DEF_SLEEPABLE,
    DEF_SLEEP_DELAY, ENTITY_FLAGS, FORCE_ACCUM, LINEAR_ACCEL, LINEAR_VELOCITY, ORIENTATION, PARENT, POSITION,
    PREV_ANGULAR_VELOCITY, PREV_LINEAR_VELOCITY, SCRATCH_ACCUM, SCRATCH_SCALE, SLEEP_TIMER, START_ANGULAR_VELOCITY,
    START_LINEAR_VELOCITY, START_ORIENTATION, START_POSITION, STEP_FLAGS, WINDOW_PROGRESS,
};
use crate::math::quat::Quat;
use crate::math::vec::Vec4;
use crate::mem::Mem;

/// The parent frame a body is attached to (`CSolid+0x44` non-null). Only the flag the pose step reads is here: bit
/// 0x4 of the parent's flags byte at +0x60 makes the parent transform the child's position. The asm tests `0x60(%ecx)`
/// (`testb $0x4`); the C++ transcription reads `pi + 6` (+0x18), which the asm does not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParentFrame {
    pub updates_child: bool,
}

impl ParentFrame {
    /// Reads the parent at `parent` (the non-null pointer at `CSolid+0x44`).
    pub fn load(mem: &Mem, parent: u32) -> Self {
        ParentFrame { updates_child: mem.u8(parent + ENTITY_FLAGS) & 0x4 != 0 }
    }
}

/// The definition's sleep settings (the keys `Sleepable`, `SleepDelay`, `LinearSleepVelocity`,
/// `AngularSleepVelocity`), and the mass terms the force pass reads (`InverseMass`, the inverse inertia diagonal).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Def {
    /// Whether the body may sleep (`Sleepable`, a flag word). 0 = never.
    pub sleepable: u32,
    /// Seconds below both thresholds before the body sleeps (`SleepDelay`).
    pub sleep_delay: f32,
    /// Linear speed threshold, m/s (`LinearSleepVelocity`). Compared squared.
    pub linear_sleep_velocity: f32,
    /// Angular speed threshold, rad/s (`AngularSleepVelocity`). Compared squared.
    pub angular_sleep_velocity: f32,
    /// Inverse mass (`InverseMass`, `def+0x16c`): the force pass scales the force by it.
    pub inverse_mass: f32,
    /// Local inverse inertia diagonal (`vecLocalInverseInertiaDiagonal`, `def+0x130`): the torque is scaled by it.
    pub local_inverse_inertia: Vec4,
}

impl Def {
    /// Reads the sleep settings of the definition at `at`.
    pub fn load(mem: &Mem, at: u32) -> Self {
        Def {
            sleepable: mem.u32(at + DEF_SLEEPABLE),
            sleep_delay: mem.f32(at + DEF_SLEEP_DELAY),
            linear_sleep_velocity: mem.f32(at + DEF_LINEAR_SLEEP_VELOCITY),
            angular_sleep_velocity: mem.f32(at + DEF_ANGULAR_SLEEP_VELOCITY),
            inverse_mass: mem.f32(at + DEF_INVERSE_MASS),
            local_inverse_inertia: Vec4::load(mem, at + DEF_LOCAL_INVERSE_INERTIA),
        }
    }
}

/// The body's fields the sleep step reads and writes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    /// Body linear velocity, m/s (`CSolid+0x1c0`).
    pub linear_velocity: Vec4,
    /// Body angular velocity, rad/s (`CSolid+0x190`).
    pub angular_velocity: Vec4,
    /// Time below the sleep thresholds, s (`CSolid+0x270`).
    pub sleep_timer: f32,
    /// Non-zero when the body may sleep at its delay (`CSolid+0x274`).
    pub sleep_armed: u32,
    /// The body is in the active set (`IsActive`, `CSolid+0x1dc`). Cleared when it sleeps.
    pub is_active: u32,
    /// Orientation quaternion, unit length after each pose step (`CSolid+0x100`).
    pub orientation: Quat,
    /// World position (`CSolid+0x140`).
    pub position: Vec4,
    /// Fraction of the current window already taken (`CSolid+0x290`). The window steps move the body to it.
    pub progress: f32,
    /// Orientation at the start of the window (`CSolid+0x2a0`, the back step blends with it; name is ours).
    pub start_orientation: Quat,
    /// Position at the start of the window (`CSolid+0x330`, the back step blends with it; name is ours).
    pub start_position: Vec4,
    /// Linear and angular velocity at the start of the window (`CSolid+0x340` and `+0x2e0`; `CSolid::S18` copies them
    /// with the pose; names are ours).
    pub start_linear_velocity: Vec4,
    pub start_angular_velocity: Vec4,
    /// Body flags byte (`CSolid+0x60`): bit 0x8 takes the contact sample in the pose step.
    pub body_flags: u8,
    /// Parent frame pointer (`CSolid+0x44`), 0 when the body is free.
    pub parent: u32,
    /// Step flags (`CSolid+0x3b4`): bit 0 is set when the pose step gets a negative time.
    pub step_flags: u32,
    /// The contact sample the pose step stores (`CSolid+0x17c`). Its meaning is unknown: it is the value
    /// `CDriveLine::SampleBig` returns at the body's position.
    pub sampled: f32,
    /// Dirty flag of the tweak at `CSolid+0x170` (`CBase::m_dirty`, `+0x178`). The pose step sets it with the sample.
    pub sampled_dirty: u32,
    /// Scratch accumulator `E43Vf74` adds to (`CSolid+0x310`). The C++ comment calls it `m_v90`; the offset is the
    /// evidence.
    pub scratch: Vec4,
    /// The scale `E43Vf74` multiplies its vector by (`CSolid+0xd0`, `m_fD0` in the C++).
    pub scratch_scale: f32,
    /// Angular acceleration (`CSolid+0x320`). `Vf25x` writes it from the torque; the integrator adds it to the angular
    /// velocity times `dt`.
    pub angular_accel: Vec4,
    /// Linear force input (`CSolid+0x380`). `Vf25x` scales it by the inverse mass; the integrator adds it to the linear
    /// velocity times `dt`.
    pub linear_accel: Vec4,
    /// Angular velocity at the last integrator pass (`CSolid+0x200`, the data of a `CVector` object at `+0x1f0`).
    pub prev_angular_velocity: Vec4,
    /// Linear velocity at the last integrator pass (`CSolid+0x220`, the data of a `CVector` object at `+0x210`).
    pub prev_linear_velocity: Vec4,
    /// Linear acceleration estimate, `(v - prev_linear_velocity) / dt` (`CSolid+0x230`).
    pub accel_linear_est: Vec4,
    /// Angular acceleration estimate, `(w - prev_angular_velocity) / dt` (`CSolid+0x240`).
    pub accel_angular_est: Vec4,
    /// Filtered linear acceleration, `0.2 * old + 0.8 * estimate` (`CSolid+0x250`).
    pub accel_linear_filt: Vec4,
    /// Filtered angular acceleration, `0.2 * old + 0.8 * estimate` (`CSolid+0x260`).
    pub accel_angular_filt: Vec4,
    /// The body's pointer at `CSolid+0x30`, the first word of the state signal the integrator emits. Its meaning is
    /// open.
    pub state_owner: u32,
    /// Force accumulator (`CSolid+0x370`): `E46_Accum340` (slot 28) adds `v * scratch_scale` to it. The force pass
    /// scales it by the inverse mass into `linear_accel` and clears it.
    pub force: Vec4,
    /// Dirty word of the tweak at `CSolid+0x1d0` (`+0x1d8`, `CBase::m_dirty`). `E43Vf90` sets it with `SetDirty`.
    pub tweak_a_dirty: u32,
    /// Dirty word of the tweak at `CSolid+0x1e0` (`+0x1e8`). `E43Vf90` sets it with `SetDirty`.
    pub tweak_b_dirty: u32,
    /// The value of the tweak at `CSolid+0x1e0` (`+0x1ec`), the key `VerySoft`. No reader in the E47 code; `E43Vf90`
    /// clears it.
    pub very_soft: u32,
    /// The hook object at `CSolid+0x3cc` (`E47S01_8` for `E43Vf128`, the sample sink for `BigA`). Zero when absent.
    pub hook: u32,
    /// The ring tail at `CSolid+0x3c8`, which `E43Vf128` appends to when the global's bit is set. Zero when absent.
    pub ring_tail: u32,
}

impl Body {
    /// Reads the body at `this`. The memory must cover the object (0x3d0 bytes).
    pub fn load(mem: &Mem, this: u32) -> Self {
        Body {
            linear_velocity: Vec4::load(mem, this + LINEAR_VELOCITY),
            angular_velocity: Vec4::load(mem, this + ANGULAR_VELOCITY),
            sleep_timer: mem.f32(this + SLEEP_TIMER),
            sleep_armed: mem.u32(this + 0x274),
            is_active: mem.u32(this + ACTIVE),
            orientation: load_quat(mem, this + ORIENTATION),
            position: Vec4::load(mem, this + POSITION),
            progress: mem.f32(this + WINDOW_PROGRESS),
            start_orientation: load_quat(mem, this + START_ORIENTATION),
            start_position: Vec4::load(mem, this + START_POSITION),
            start_linear_velocity: Vec4::load(mem, this + START_LINEAR_VELOCITY),
            start_angular_velocity: Vec4::load(mem, this + START_ANGULAR_VELOCITY),
            body_flags: mem.u8(this + ENTITY_FLAGS),
            parent: mem.u32(this + PARENT),
            step_flags: mem.u32(this + STEP_FLAGS),
            sampled: mem.f32(this + 0x17C),
            sampled_dirty: mem.u32(this + 0x178),
            scratch: Vec4::load(mem, this + SCRATCH_ACCUM),
            scratch_scale: mem.f32(this + SCRATCH_SCALE),
            angular_accel: Vec4::load(mem, this + ANGULAR_ACCEL),
            linear_accel: Vec4::load(mem, this + LINEAR_ACCEL),
            prev_angular_velocity: Vec4::load(mem, this + PREV_ANGULAR_VELOCITY),
            prev_linear_velocity: Vec4::load(mem, this + PREV_LINEAR_VELOCITY),
            accel_linear_est: Vec4::load(mem, this + ACCEL_LINEAR_EST),
            accel_angular_est: Vec4::load(mem, this + ACCEL_ANGULAR_EST),
            accel_linear_filt: Vec4::load(mem, this + ACCEL_LINEAR_FILT),
            accel_angular_filt: Vec4::load(mem, this + ACCEL_ANGULAR_FILT),
            state_owner: mem.u32(this + 0x30),
            force: Vec4::load(mem, this + FORCE_ACCUM),
            tweak_a_dirty: mem.u32(this + 0x1D8),
            tweak_b_dirty: mem.u32(this + 0x1E8),
            very_soft: mem.u32(this + VERY_SOFT),
            hook: mem.u32(this + HOOK),
            ring_tail: mem.u32(this + RING_TAIL),
        }
    }

    /// Writes the fields back. Only the fields the ports change need to be stored; this writes all of them.
    pub fn store(&self, mem: &mut Mem, this: u32) {
        self.linear_velocity.store(mem, this + LINEAR_VELOCITY);
        self.angular_velocity.store(mem, this + ANGULAR_VELOCITY);
        mem.set_f32(this + SLEEP_TIMER, self.sleep_timer);
        mem.set_u32(this + 0x274, self.sleep_armed);
        mem.set_u32(this + ACTIVE, self.is_active);
        store_quat(mem, this + ORIENTATION, self.orientation);
        self.position.store(mem, this + POSITION);
        mem.set_f32(this + WINDOW_PROGRESS, self.progress);
        store_quat(mem, this + START_ORIENTATION, self.start_orientation);
        self.start_position.store(mem, this + START_POSITION);
        self.start_linear_velocity.store(mem, this + START_LINEAR_VELOCITY);
        self.start_angular_velocity.store(mem, this + START_ANGULAR_VELOCITY);
        mem.map(this + ENTITY_FLAGS, &[self.body_flags]);
        mem.set_u32(this + STEP_FLAGS, self.step_flags);
        mem.set_f32(this + 0x17C, self.sampled);
        mem.set_u32(this + 0x178, self.sampled_dirty);
        self.scratch.store(mem, this + SCRATCH_ACCUM);
        self.angular_accel.store(mem, this + ANGULAR_ACCEL);
        self.linear_accel.store(mem, this + LINEAR_ACCEL);
        self.prev_angular_velocity.store(mem, this + PREV_ANGULAR_VELOCITY);
        self.prev_linear_velocity.store(mem, this + PREV_LINEAR_VELOCITY);
        self.accel_linear_est.store(mem, this + ACCEL_LINEAR_EST);
        self.accel_angular_est.store(mem, this + ACCEL_ANGULAR_EST);
        self.accel_linear_filt.store(mem, this + ACCEL_LINEAR_FILT);
        self.accel_angular_filt.store(mem, this + ACCEL_ANGULAR_FILT);
        mem.set_u32(this + 0x30, self.state_owner);
        self.force.store(mem, this + FORCE_ACCUM);
        mem.set_u32(this + 0x1D8, self.tweak_a_dirty);
        mem.set_u32(this + 0x1E8, self.tweak_b_dirty);
        mem.set_u32(this + VERY_SOFT, self.very_soft);
        mem.set_u32(this + HOOK, self.hook);
        mem.set_u32(this + RING_TAIL, self.ring_tail);
    }
}
