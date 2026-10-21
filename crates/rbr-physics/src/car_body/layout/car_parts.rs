//! `CCar`'s parts as the ports find them: the body links, the passengers, the sensors, the drive, the wheels, the
//! grid, and the seed block `UpdateA720` applies.

use crate::store::Raw;

/// The car's passenger object (+0x8b0), stepped by `Vf6cx`.
pub fn passenger_at(car: u32) -> u32 {
    car + 0x8b0
}

/// The car's scratch sim (+0x850), whose `NopA` `Vf6cx` calls.
pub fn scratch_at(car: u32) -> u32 {
    car + 0x850
}

/// The car's engine (+0x690): the sensor getters' `this`.
pub fn engine_at(car: u32) -> u32 {
    car + 0x690
}

/// The clock values read by `CTimeKeeper::Diff` (0x4a0290): the card pointer is at keeper+4, its finish flag at
/// card+0x58 selects card+0x50, otherwise the argument points to the current double. The origin is card+0x38.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeKeeperDiff {
    pub origin: f64,
    pub end: f64,
}

/// The car's drive (+0x4b0).
pub fn drive_at(car: u32) -> u32 {
    car + 0x4b0
}

/// The slot of wheel `i` (+0x480 + 4 i): `Vf6cx` clears it when the wheel is done.
pub fn hinged_slot(car: u32, i: u32) -> u32 {
    car + 0x480 + 4 * i
}

/// The address of `m_wheels[i]` (+0x470 + 4 i), the car's wheel list (`wheel` reads it).
pub fn wheel_list_at(car: u32, i: u32) -> u32 {
    car + 0x470 + 4 * i
}

/// The slot of rigid body `i` (+0x490 + 4 i), which `Vf6cx` steps and clears.
pub fn mounted_slot(car: u32, i: u32) -> u32 {
    car + 0x490 + 4 * i
}

/// The flow object's global (`0x165fa90`, the game's options object): the object `Update9AF0`'s flow check, the
/// retire announcer and the drive state's mode test (`E56_Mode6690`) read.
pub const FLOW_OBJECT: u32 = 0x0165_FA90;

/// Where each frame word goes: the target's offset, and the frame's offset the word comes from, in the asm's order.
pub const FRAME_COPY: [(u32, u32); 9] = [
    (0x224, 0x10),
    (0x228, 0x14),
    (0x22C, 0x18),
    (0x4458, 0x20),
    (0x445C, 0x24),
    (0x4460, 0x28),
    (0x4468, 0x30),
    (0x446C, 0x34),
    (0x4470, 0x38),
];

/// The seed block `CCar::UpdateA720` applies: the drive's, the damage's and the scratch sim's argument words (+8, +0xc,
/// +0x10), each wheel's three argument words (+0x14, +0x24 and +0x34, plus 4 per wheel), and the frame words that the
/// car takes (`FRAME_COPY`, read through the frame pointer at +4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seed {
    pub drive: u32,
    pub damage: u32,
    pub scratch: u32,
    pub wheels: [[u32; 3]; 4],
    pub frame: [u32; 9],
}

impl Seed {
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        let frame = mem.u32(at + 4);
        Seed {
            drive: mem.u32(at + 8),
            damage: mem.u32(at + 0xC),
            scratch: mem.u32(at + 0x10),
            wheels: std::array::from_fn(|i| {
                let i = i as u32;
                [mem.u32(at + 0x14 + 4 * i), mem.u32(at + 0x24 + 4 * i), mem.u32(at + 0x34 + 4 * i)]
            }),
            frame: std::array::from_fn(|k| mem.u32(frame + FRAME_COPY[k].1)),
        }
    }
}
