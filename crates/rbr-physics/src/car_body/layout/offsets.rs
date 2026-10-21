//! Addresses inside the car's neighbours that the model modules name by role: the signal emitters' words, the signal
//! manager's emitters, the car's members and the damage handler's words.

use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::store::Raw;

/// The emitter's busy word (+0), its source pointer (+4: to the word holding the signal's index), the list of the
/// receivers of every signal (+0x208) and the lists per index (+8, sixteen bytes each).
pub fn emitter_busy_at(emitter: u32) -> u32 {
    emitter
}

pub fn emitter_source_at(emitter: u32) -> u32 {
    emitter + 4
}

pub fn all_receivers_at(emitter: u32) -> u32 {
    emitter + 0x208
}

pub fn index_receivers_at(emitter: u32, index: u32) -> u32 {
    emitter + 8 + 16 * index
}

/// A receiver node's next pointer (+4).
pub fn next_receiver_at(node: u32) -> u32 {
    node + 4
}

/// The signal manager's emitter at `offset`, found through the global pointer.
pub fn signal_at<R: Raw + ?Sized>(mem: &R, offset: u32) -> u32 {
    mem.u32(SIGNAL_MANAGER) + offset
}

/// The car's light system member (+0x838).
pub fn light_system_at(car: u32) -> u32 {
    car + 0x838
}

/// The car's source damage object pointer (+0x3cc), which the car copies to its own damage handler member.
pub fn damage_source_at(car: u32) -> u32 {
    car + 0x3CC
}

/// The car's position lanes (+0x140, four words).
pub fn position_at(car: u32) -> u32 {
    car + 0x140
}

/// The car's position along the drive line (+0x17c): a float, the index plus a fraction.
pub fn drive_line_position_at(car: u32) -> u32 {
    car + 0x17C
}

/// The vector the publish step walks into the world frame (+0x220).
pub fn walk_source_at(car: u32) -> u32 {
    car + 0x220
}

/// The control payload's words, as offsets (`notes/gameplay.md` "Control payload"): throttle, brake, handbrake, clutch,
/// steering, flags, gear and aux.
pub mod control {
    pub const THROTTLE: u32 = 0x00;
    pub const BRAKE: u32 = 0x04;
    pub const HANDBRAKE: u32 = 0x08;
    pub const CLUTCH: u32 = 0x0C;
    pub const STEERING: u32 = 0x10;
    pub const FLAGS: u32 = 0x14;
    pub const AUX: u32 = 0x1C;
    /// The payload's size, bytes.
    pub const SIZE: u32 = 0x20;
    /// The flags: call for help, and reset.
    pub const FLAG_CALL_FOR_HELP: u32 = 0x10;
    pub const FLAG_RESET: u32 = 0x20;
}

/// The car's kept throttle (+0x1694) and brake (+0x1698) words, from the control payload.
pub const THROTTLE_AT: u32 = 0x1694;
pub const BRAKE_AT: u32 = 0x1698;

/// The partial-contact flag word (+0x9f0): 1 while the car has been in partial contact for over a second.
pub const PARTIAL_CONTACT_FLAG: u32 = 0x9F0;

/// The timing marks the car records at its first thousand-step counts: the timer word (+0x954, four words) and the
/// steering of the payload (+0x974, stride 0x20).
pub fn step_mark_timer_at(car: u32, mark: u32) -> u32 {
    car + 0x954 + 4 * mark
}

pub fn step_mark_steering_at(car: u32, mark: u32) -> u32 {
    car + 0x974 + 0x20 * mark
}

/// The signal manager's problem emitter (the call-for-help path's emit).
pub const PROBLEM_EMITTER_AT: u32 = 0xD590;

/// The members of the car's damage handler (`handler`), as offsets: the per-part damage states the car wires to its
/// parts.
pub mod handler {
    /// The drive's damage state.
    pub const DRIVE: u32 = 0x3A18;
    /// The engine's damage state.
    pub const ENGINE: u32 = 0x3AAC;
    /// The light system's damage block.
    pub const LIGHTS: u32 = 0x3B24;

    /// Wheel `i`'s damage (0x38 bytes each), its value pair (8 bytes each) and its eight-byte value (0x20 each).
    pub fn wheel(handler: u32, i: u32) -> u32 {
        handler + 0x3C04 + 0x38 * i
    }

    pub fn wheel_pair(handler: u32, i: u32) -> u32 {
        handler + 0x3CE4 + 8 * i
    }

    pub fn wheel_value8(handler: u32, i: u32) -> u32 {
        handler + 0x3D04 + 0x20 * i
    }

    /// Hinged body `i`'s damage triple (12 bytes each: the attach state words) and mounted body `i`'s state (4 bytes).
    pub fn hinged(handler: u32, i: u32) -> u32 {
        handler + 0x3D84 + 0xC * i
    }

    pub fn mounted(handler: u32, i: u32) -> u32 {
        handler + 0x3DB4 + 4 * i
    }
}

/// The signal manager's car-init emitter (the replay manager records the car).
pub const CAR_INIT_EMITTER_AT: u32 = 0xE868;

/// The signal manager's retire emitter and stand-still emitter.
pub const RETIRE_EMITTER_AT: u32 = 0xABB0;
pub const STAND_STILL_EMITTER_AT: u32 = 0xCB18;
