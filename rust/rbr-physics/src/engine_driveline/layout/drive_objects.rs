//! `CDrive`'s lanes, definition and state as the accessors see them.

use crate::engine_driveline::store::EngineStore;
use crate::store::Raw;

/// An NTweak value with a dirty flag: the flag at +8 (set by every write), the value at +0xc.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lane {
    pub dirty: u32,
    pub value: f32,
}

impl Lane {
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        Lane { dirty: mem.u32(at + 0x08), value: mem.f32(at + 0x0c) }
    }

    pub fn store<R: Raw + ?Sized>(&self, mem: &mut R, at: u32) {
        mem.set_u32(at + 0x08, self.dirty);
        mem.set_f32(at + 0x0c, self.value);
    }

    /// A write: the dirty flag is set and the value replaced.
    pub fn set(&mut self, value: f32) {
        self.dirty = 1;
        self.value = value;
    }

    /// A write of the value's bits (the original passes an int that holds a float's bits).
    pub fn set_bits(&mut self, bits: u32) {
        self.set(f32::from_bits(bits));
    }
}

/// The same header as `Lane`, with an integer value (the commanded gear).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntLane {
    pub dirty: u32,
    pub value: i32,
}

impl IntLane {
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        IntLane { dirty: mem.u32(at + 0x08), value: mem.u32(at + 0x0c) as i32 }
    }

    pub fn store<R: Raw + ?Sized>(&self, mem: &mut R, at: u32) {
        mem.set_u32(at + 0x08, self.dirty);
        mem.set_u32(at + 0x0c, self.value as u32);
    }

    /// A write: the dirty flag is set and the value replaced.
    pub fn set(&mut self, value: i32) {
        self.dirty = 1;
        self.value = value;
    }
}

/// The drive's input lanes (`CDrive`). Each field is a `Lane` at the original's offset (given per field).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    /// Throttle input (`Set44A0`), base +0x20.
    pub throttle_in: Lane,
    /// Brake input (`Set30`), base +0x30.
    pub brake_in: Lane,
    /// Brake torque (`Set30`'s scaled copy of the brake), base +0x64.
    pub brake_torque: Lane,
    /// Clutch slip (`Set4820`), base +0x54.
    pub clutch_slip: Lane,
}

impl Drive {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_drive(at)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_drive(at, self)
    }
}

/// `CDrive`'s object pointers (`notes/physics/engine-driveline.md` 2) and its definition's members
/// (`notes/physics/renames/engine-driveline.csv`).
pub(super) const DRIVE_DEF_PTR: u32 = 0x04;
pub(super) const DRIVE_STATE_PTR: u32 = 0x08;
pub(super) const DRIVE_DEF_MAX_FOOTBRAKE_PRESSURE: u32 = 0x620;
pub(super) const DRIVE_DEF_GEAR_COUNT: u32 = 0x654;

/// The drive's definition (`CDrive::CDef`, `this+4`): the fields the accessors read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DriveDef {
    /// Scale of the brake torque (`Set30`), `+0x620` (`MaxFootbrakePressure`).
    pub max_footbrake_pressure: f32,
    /// Number of gears (`NumberOfGears`), `+0x654`.
    pub gear_count: u32,
}

impl DriveDef {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_drive_def(at)
    }
}

/// `CDrive::CState` members (`notes/physics/renames/engine-driveline.csv`).
pub(super) const DRIVE_STATE_ENGINE_SPEED: u32 = 0x45c;
pub(super) const DRIVE_STATE_GEAR: u32 = 0x54c;
/// The commanded gear's `IntLane` header (its value is at +0x570).
pub(super) const DRIVE_STATE_COMMANDED_GEAR_LANE: u32 = 0x564;

/// The drive's state (`CDrive`'s `this+8`, the car's drive state): the fields the accessors read or write.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DriveState {
    /// Drive speed key, `+0x384` (its meaning is not known).
    pub drive_speed_key: f32,
    /// Engine speed, rad/s (`+0x45c`).
    pub engine_speed: f32,
    /// The gear (`+0x54c`, see the module README).
    pub gear: i32,
    /// The commanded gear (`+0x564` header, value at `+0x570`), written by `Set47B0`.
    pub commanded_gear: IntLane,
}

impl DriveState {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_drive_state(at)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_drive_state(at, self)
    }

    pub fn store_commanded_gear<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_commanded_gear(at, self)
    }
}

/// Where the drive's two objects are: the definition (`this+4`) and the state (`this+8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveObjects {
    pub def: u32,
    pub state: u32,
}

impl DriveObjects {
    pub fn at<S: EngineStore>(mem: &S, drive: u32) -> Self {
        mem.drive_objects(drive)
    }
}
