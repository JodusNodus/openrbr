//! The original's functions the car-body steps call through the host, by role, and the stack locals of the publish
//! step. The addresses are the exe's; the roles are named by use where the original's meaning is open.

use crate::store::Raw;

/// The virtual target of `obj`'s `slot`: the vtable's word at `4 * slot`.
pub fn virtual_target<R: Raw + ?Sized>(mem: &R, obj: u32, slot: u32) -> u32 {
    mem.u32(mem.u32(obj) + 4 * slot)
}

/// The vtable slot of an entity's identity getter (it returns `this`).
pub const IDENTITY_SLOT: u32 = 12;

/// The vtable slot of the damage object's method the publish step calls.
pub const DAMAGE_PUBLISH_SLOT: u32 = 5;

/// Copies 16 bytes from the argument into `this`.
pub const VEC4_COPY_FROM: u32 = 0x43C5B0;
/// Zeroes the 16-byte vector at `this`.
pub const VEC4_CLEAR: u32 = 0x465D80;

/// The publish step's callees.
pub mod publish {
    /// On the drive.
    pub const DRIVE_UPDATE: u32 = 0x508480;
    /// On the passenger.
    pub const PASSENGER_UPDATE: u32 = 0x523210;
    /// On the engine.
    pub const ENGINE_UPDATE: u32 = 0x518CF0;
    /// The light system's update (`light::update`).
    pub const LIGHTS_UPDATE: u32 = 0x4F2150;
    /// On the car itself.
    pub const CAR_UPDATE: u32 = 0x51A3E0;
    /// The test on each mounted body: true when the body's attach state is 1.
    pub const MOUNTED_TEST: u32 = 0x521920;
    /// The mounted body's update, when its test holds.
    pub const MOUNTED_UPDATE: u32 = 0x522000;
    /// The ground probe on the car (`vote::probe_ground`): takes the output vector and the distance.
    pub const PROBE_GROUND: u32 = 0x529D30;
    /// The car's body frame composed into the world frame (`world_placement::compose_body_into_world`).
    pub const COMPOSE_BODY_INTO_WORLD: u32 = 0x4E8130;
    /// On each wheel, with two vectors of the frame.
    pub const WHEEL_UPDATE: u32 = 0x5390D0;
    /// The sensor sampler (`sensors::sample_sensors`).
    pub const SAMPLE_SENSORS: u32 = 0x4E9800;
    /// The vector walk through the world (`MB560`).
    pub const WALK_VECTOR: u32 = 0x51B560;
    /// The car's ground direction (`accessors::ground_direction`): writes a vector into the frame.
    pub const GROUND_DIRECTION: u32 = 0x4E5CF0;
    /// Composes a frame with another (`transform::FrameCompose`).
    pub const COMPOSE_FRAMES: u32 = 0x51B4C0;
    /// The engine's last update.
    pub const ENGINE_LAST_UPDATE: u32 = 0x50E9A0;

    /// The signal emits and the offsets of their emitters in the signal manager.
    pub const STATE_EMIT_A: u32 = 0x4E83F0;
    pub const STATE_EMIT_A_AT: u32 = 0x2E10;
    pub const STATE_EMIT_B: u32 = 0x4E8330;
    pub const STATE_EMIT_B_AT: u32 = 0x29E0;
    pub const DRIVE_LINE_EMIT: u32 = 0x4E84B0;
    pub const DRIVE_LINE_EMIT_AT: u32 = 0x51A8;
    pub const SPEED_EMIT: u32 = 0x4E81B0;
    pub const SPEED_EMIT_AT: u32 = 0x860;

    /// The ground probe's distance, m (named by use: the car passes 5.0).
    pub const PROBE_DISTANCE: f32 = 5.0;

    /// The publish step's stack locals, as offsets from the frame base.
    pub mod frame {
        /// The drive line position payload (one word).
        pub const DRIVE_LINE_PAYLOAD: u32 = 0x18;
        /// A zero vector.
        pub const ZERO: u32 = 0x20;
        /// The walked car vector, then its copy.
        pub const WALKED: u32 = 0x30;
        pub const WALKED_COPY: u32 = 0x60;
        /// The ground direction, then its copy.
        pub const DIRECTION: u32 = 0x40;
        pub const DIRECTION_COPY: u32 = 0x70;
        /// The speed payload (one float).
        pub const SPEED_PAYLOAD: u32 = 0x50;
        /// The keeper branch's two composed frames.
        pub const WORLD_FRAME: u32 = 0x80;
        pub const BODY_FRAME: u32 = 0x90;
        /// The car state payload: the sort key, then (16 bytes each) the position copy, the velocity and the angular
        /// velocity.
        pub const STATE_PAYLOAD: u32 = 0xA0;
        pub const POSITION: u32 = 0xB0;
        pub const VELOCITY: u32 = 0xD0;
        pub const ANGULAR_VELOCITY: u32 = 0xE0;
        /// The ground probe's output and distance.
        pub const PROBE_OUT: u32 = 0xF0;
        pub const PROBE_DISTANCE: u32 = 0xF4;
        /// Wheel `i`'s two vectors.
        pub const WHEEL_A: u32 = 0x100;
        pub const WHEEL_B: u32 = 0x140;
    }
}

/// The setup step's callees: each takes the setup's words for one of the car's members.
pub mod setup {
    /// On the drive, with the drive's word.
    pub const DRIVE_APPLY: u32 = 0x508770;
    /// On the engine, with the engine's word (the original's callee does nothing).
    pub const ENGINE_APPLY: u32 = 0x50D300;
    /// On the scratch sim, with the scratch sim's word.
    pub const SCRATCH_APPLY: u32 = 0x537C90;
    /// On each wheel, with its three words.
    pub const WHEEL_APPLY: u32 = 0x53CF30;
}

/// The damage wiring's callees: each attaches the handler's state to a part, or refreshes the part after it.
pub mod wiring {
    /// On the drive, with the drive's damage state.
    pub const DRIVE_ATTACH: u32 = 0x503AB0;
    /// On the engine, with the engine's damage state.
    pub const ENGINE_ATTACH: u32 = 0x50D310;
    /// The light system's attach (on the light system, with its damage block).
    pub const LIGHTS_ATTACH: u32 = 0x4F1E00;
    /// On each wheel, with its damage, value pair and eight-byte value.
    pub const WHEEL_ATTACH: u32 = 0x53A860;
    /// On the same wheel, after the attach.
    pub const WHEEL_REFRESH: u32 = 0x53D030;
    /// On each hinged body, with its damage triple.
    pub const HINGED_ATTACH: u32 = 0x51DA50;
    /// On the same hinged body: its frame setup (`entities/hinge_efe0.rs`).
    pub const HINGED_FRAME_SETUP: u32 = 0x51EFE0;
    /// On each mounted body, with its state word.
    pub const MOUNTED_ATTACH: u32 = 0x521910;
    /// On the same mounted body, after the attach.
    pub const MOUNTED_REFRESH: u32 = 0x521980;
    /// On the drive, last: a hook that returns at once.
    pub const DRIVE_HOOK: u32 = 0x503AA0;
    /// The control update on the car (`control::apply_control`).
    pub const APPLY_CONTROL: u32 = 0x4E91F0;
    /// The damage object's vtable slot the wiring calls first.
    pub const DAMAGE_FIRST_SLOT: u32 = 4;
}
