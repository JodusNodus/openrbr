//! `CCompleteCarSetup` and `CCompleteCarSetupOptions`: their members, constructors, files, the seed block and the
//! selections and copies `CCar::UpdateA720` makes from them.

use rbr_formats::lsp::Node;
use rbr_formats::lsp_keys::Class;

use crate::engine_driveline::layout as drive;
use crate::load::DriverAids;
use crate::mem::Mem;
use crate::ntweak::{self, ApplyHost, Unsupported};
use crate::{car_body, spring_damper, tyre, vcu, wheel};

/// `CCompleteCarSetup`'s vtable (0x72ed24) and its members: the car, drive, engine and control unit setups, then the
/// wheels', spring dampers' and tyres' setups (first one, stride), in `CCompleteCarSetup::CCompleteCarSetup`'s order.
const COMPLETE_SETUP_VTABLE: u32 = 0x0072_ED24;
const CAR_SETUP: u32 = 0xC;
const DRIVE_SETUP: u32 = 0x48;
const ENGINE_SETUP: u32 = 0x684;
const VCU_SETUP: u32 = 0x6A0;
const WHEEL_SETUPS: (u32, u32) = (0x1210, 0x70);
const SPRING_SETUPS: (u32, u32) = (0x13D0, 0x9C);
const TYRE_SETUPS: (u32, u32) = (0x1640, 0x30);
/// `CCompleteCarSetupOptions`'s vtable (0x72ecac) and its members, in its constructor's order: the car, drive, control
/// unit and engine options, then the spring dampers', tyres' and wheels' options and the seven tyre types (first one,
/// stride).
const SETUP_OPTIONS_VTABLE: u32 = 0x0072_ECAC;
const CAR_OPTIONS: u32 = 0x10;
const DRIVE_OPTIONS: u32 = 0x220;
const VCU_OPTIONS: u32 = 0x12DC;
const ENGINE_OPTIONS: u32 = 0x13A8;
const SPRING_OPTIONS: (u32, u32) = (0x13E4, 0x18C);
const TYRE_OPTIONS: (u32, u32) = (0x1A14, 0x3C);
const WHEEL_OPTIONS: (u32, u32) = (0x1B10, 0x320);
const TYRE_TYPES: (u32, u32) = (0x2790, 0x6AC);
/// `tyres.lsp`'s tyre types, `TyreType00` to `TyreType06`.
const TYRE_TYPE_COUNT: u32 = 7;

/// The `n`-th object of an array member (first one, stride) of the owner at `owner`.
fn nth(owner: u32, (first, stride): (u32, u32), n: u32) -> u32 {
    owner + first + n * stride
}

/// `CCompleteCarSetup::CCompleteCarSetup` (0x47f770) on the owner at `at`: an unnamed tweak, its vtable, then its
/// setups.
pub fn construct_complete_setup(mem: &mut Mem, at: u32) {
    ntweak::construct_base(mem, at, 0);
    mem.set_u32(at, COMPLETE_SETUP_VTABLE);
    car_body::layout::construct_setup(mem, at + CAR_SETUP);
    drive::construct_drive_setup(mem, at + DRIVE_SETUP);
    drive::construct_engine_setup(mem, at + ENGINE_SETUP);
    vcu::layout::construct_setup(mem, at + VCU_SETUP);
    for n in 0..4 {
        wheel::layout::setup_ctor(mem, nth(at, WHEEL_SETUPS, n));
    }
    for n in 0..4 {
        spring_damper::layout::construct_setup(mem, nth(at, SPRING_SETUPS, n));
    }
    for n in 0..4 {
        tyre::layout::construct_setup(mem, nth(at, TYRE_SETUPS, n));
    }
}

/// `CCompleteCarSetupOptions::CCompleteCarSetupOptions` (0x47f0f0) on the owner at `at`: an unnamed tweak, its vtable,
/// then its options and tyre types.
pub fn construct_setup_options(mem: &mut Mem, at: u32) {
    ntweak::construct_base(mem, at, 0);
    mem.set_u32(at, SETUP_OPTIONS_VTABLE);
    car_body::layout::construct_options(mem, at + CAR_OPTIONS);
    drive::construct_drive_options(mem, at + DRIVE_OPTIONS);
    vcu::layout::construct_options(mem, at + VCU_OPTIONS);
    drive::construct_engine_options(mem, at + ENGINE_OPTIONS);
    for n in 0..4 {
        spring_damper::layout::construct_options(mem, nth(at, SPRING_OPTIONS, n));
    }
    for n in 0..4 {
        tyre::layout::construct_options(mem, nth(at, TYRE_OPTIONS, n));
    }
    for n in 0..4 {
        wheel::layout::construct_options(mem, nth(at, WHEEL_OPTIONS, n));
    }
    for n in 0..TYRE_TYPE_COUNT {
        tyre::layout::construct_tyre_type(mem, nth(at, TYRE_TYPES, n));
    }
}

/// A setup file's root group (`setups/d_gravel.lsp`, ...) walked on `CCompleteCarSetup` at `complete`, as
/// `E16SaveHost::ParseSetup` (0x48d940) hands it to the owner's `Parse`.
pub fn setup_file(mem: &mut Mem, host: &mut impl ApplyHost, complete: u32, root: &[Node]) -> Result<(), Unsupported> {
    ntweak::apply(mem, host, complete, Class::CompleteCarSetup, root)
}

/// An options file's root group (`r_gravel.lsp`, ..., and `tyres.lsp`, `E16_LoadTyres` 0x48d9f0) walked on
/// `CCompleteCarSetupOptions` at `options`, as the loader hands it to the owner's `Parse`.
pub fn options_file(mem: &mut Mem, host: &mut impl ApplyHost, options: u32, root: &[Node]) -> Result<(), Unsupported> {
    ntweak::apply(mem, host, options, Class::CompleteCarSetupOptions, root)
}

/// One wheel's setup objects in `CCompleteCarSetup`: `Wheel..`, `SpringDamper..` and `Tyre..`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelSetup {
    pub wheel: u32,
    pub spring: u32,
    pub tyre: u32,
}

/// The setup objects of `CCompleteCarSetup` at `complete` the car copies: its `Car`, `Drive` and `VehicleControlUnit`
/// setups and the wheels' (in [`POSITIONS`] order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setups {
    pub car: u32,
    pub drive: u32,
    pub vcu: u32,
    pub wheels: [WheelSetup; 4],
}

/// The setup objects of `CCompleteCarSetup` at `complete`.
pub fn setups(complete: u32) -> Setups {
    Setups {
        car: complete + CAR_SETUP,
        drive: complete + DRIVE_SETUP,
        vcu: complete + VCU_SETUP,
        wheels: std::array::from_fn(|n| WheelSetup {
            wheel: nth(complete, WHEEL_SETUPS, n as u32),
            spring: nth(complete, SPRING_SETUPS, n as u32),
            tyre: nth(complete, TYRE_SETUPS, n as u32),
        }),
    }
}

/// The block `CCar::UpdateA720` (0x4ea720) takes (`car_body::layout::Seed`), as the setup's emit (0x48e620) builds it on
/// its stack from `CCompleteCarSetup` at `complete`: a zero word, the car, drive, engine and control unit setups, then
/// the wheels', spring dampers' and tyres' setups in [`crate::load::POSITIONS`] order.
pub fn store_seed(mem: &mut Mem, at: u32, complete: u32) {
    let s = setups(complete);
    let words = [0, s.car, s.drive, complete + ENGINE_SETUP, s.vcu]
        .into_iter()
        .chain(s.wheels.iter().map(|w| w.wheel))
        .chain(s.wheels.iter().map(|w| w.spring))
        .chain(s.wheels.iter().map(|w| w.tyre));
    for (k, word) in words.enumerate() {
        mem.set_u32(at + 4 * k as u32, word);
    }
}

/// The size of [`store_seed`]'s block.
pub const SEED_SIZE: u32 = 0x44;

/// The size of `CCompleteCarSetup` (up to its last member's end) and of `CCompleteCarSetupOptions`.
pub const COMPLETE_SETUP_SIZE: u32 = 0x1700;
pub const SETUP_OPTIONS_SIZE: u32 = TYRE_TYPES.0 + TYRE_TYPE_COUNT * TYRE_TYPES.1;

/// The drive options of `CCompleteCarSetupOptions` at `options`.
pub fn drive_options(options: u32) -> u32 {
    options + DRIVE_OPTIONS
}

/// The tyre type `n` (`TyreType00` to `06`) of `CCompleteCarSetupOptions` at `options`.
pub fn tyre_type(options: u32, n: u32) -> u32 {
    nth(options, TYRE_TYPES, n)
}

/// The drive setup's gear rows and brakes from the drive options, as the setup pages apply them before the race: the
/// gears page's `CCarSettingsManager::NormalizeSetup` (0x547300). The brakes page's `vf48` (0x5487d0) is not run: NGP
/// patches it to return at once, as the setup's members there are NGP's own keys.
pub fn select_drive_options(mem: &mut Mem, setup: u32, options: u32) {
    drive::select_gear_rows(mem, setup, options);
}

/// The wheels' top mounts from the options of `CCompleteCarSetupOptions` at `options`, as the geometry pages apply them
/// before the race (`wheel::layout::select_top_mount`).
pub fn select_top_mounts(mem: &mut Mem, setups: &Setups, options: u32) {
    for (n, w) in (0..).zip(&setups.wheels) {
        wheel::layout::select_top_mount(mem, w.wheel, nth(options, WHEEL_OPTIONS, n));
    }
}

/// The player's driver aids into the control unit's setup, as the settings code stores them (`vcu::layout::set_driver_aids`).
pub fn set_driver_aids(mem: &mut Mem, setups: &Setups, aids: &DriverAids) {
    let words = [aids.auto_gears, aids.gear_guard, aids.neutral_lock, aids.clutch_help];
    vcu::layout::set_driver_aids(mem, setups.vcu, words);
}

/// The car's setups into its definition at `def`, as `CCar::UpdateA720` (0x4ea720) does them on the car: the drive
/// setup (`CDrive::M8770`), the control unit's knot maps (the definition's part of `CScratchSim::CopyDef`), then the car
/// setup's words.
pub fn copy_car_setups(mem: &mut Mem, def: u32, setups: &Setups) {
    drive::drive_setup_into_def(mem, def + car_body::layout::DEF_DRIVE, setups.drive);
    vcu::layout::copy_maps(mem, def + car_body::layout::DEF_VCU, setups.vcu);
    car_body::layout::copy_setup(mem, def, setups.car);
}

/// The setup's emit (0x48e620): every tyre setup gets the tyre type the race chose (its index from 0x566250).
pub fn select_tyre_type(mem: &mut Mem, setups: &Setups, tyre_type: u32) {
    for setup in &setups.wheels {
        tyre::layout::set_setup_type(mem, setup.tyre, tyre_type);
    }
}

/// The wheel loop of `CCar::UpdateA720` (0x4ea720, all of it `car_body::apply_seed`, which needs the car): each wheel
/// copies its setup into its definitions (`E51Wheel::CopyCF30`, with the ports of its three calls and the CRT's sine
/// and cosine).
pub fn copy_setups(mem: &mut Mem, wheels: [u32; 4], setups: &Setups) {
    for (wheel, setup) in wheels.into_iter().zip(&setups.wheels) {
        wheel::layout::copy_cf30(
            &mut wheel::layout::Ported,
            &mut wheel::axes::X87Trig,
            mem,
            wheel,
            setup.wheel,
            setup.spring,
            setup.tyre,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::POSITIONS;
    use rbr_formats::lsp_keys::key_offset;

    #[test]
    fn the_members_are_where_the_key_tables_put_them() {
        let setup = |key: &str| key_offset(Class::CompleteCarSetup, key).unwrap();
        let option = |key: &str| key_offset(Class::CompleteCarSetupOptions, key).unwrap();
        let s = setups(0);
        assert_eq!([s.car, s.drive, s.vcu], [setup("Car"), setup("Drive"), setup("VehicleControlUnit")]);
        assert_eq!(ENGINE_SETUP, setup("Engine"));
        for (n, position) in POSITIONS.iter().enumerate() {
            let w = s.wheels[n];
            assert_eq!(w.wheel, setup(&format!("Wheel{position}")));
            assert_eq!(w.spring, setup(&format!("SpringDamper{position}")));
            assert_eq!(w.tyre, setup(&format!("Tyre{position}")));
            let n = n as u32;
            assert_eq!(nth(0, SPRING_OPTIONS, n), option(&format!("SpringDamperOptions{position}")));
            assert_eq!(nth(0, TYRE_OPTIONS, n), option(&format!("TyreOptions{position}")));
            assert_eq!(nth(0, WHEEL_OPTIONS, n), option(&format!("WheelOptions{position}")));
        }
        assert_eq!(
            [CAR_OPTIONS, DRIVE_OPTIONS, VCU_OPTIONS, ENGINE_OPTIONS],
            ["CarOptions", "DriveOptions", "ControlOptions", "EngineOptions"].map(option)
        );
        for n in 0..TYRE_TYPE_COUNT {
            assert_eq!(tyre_type(0, n), option(&format!("TyreType{n:02}")));
        }
    }
}
