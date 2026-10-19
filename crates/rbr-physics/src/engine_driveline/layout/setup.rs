//! The drive setup's gear rows and brakes, and `CDrive::M8770`, which copies the setup into the definition.

use crate::fpu;
use crate::mem::Mem;
use crate::ntweak::set_dirty;

/// The gear rows (`E13Mixed50`, 0x50 bytes): the first in the options and the setup; in a row, the ratio (a real) and
/// the pinion's and gear's teeth (integers, +0x10 and +0x20).
const OPTIONS_ROWS: u32 = 0xC;
const SETUP_ROWS: u32 = 0x3C;
const ROW: u32 = 0x50;
const ROW_PINION: u32 = 0x10;
const ROW_GEAR: u32 = 0x20;
/// The rows `E53SetupRows::Normalize` recomputes.
const NORMALIZED_ROWS: u32 = 32;
/// The options' gear count, and the setup's gear ids (ten, then the final drive's and the drop gear's).
const OPTIONS_GEARS: u32 = 0xB9C;
const SETUP_GEAR_IDS: u32 = 0x3FC;
const SETUP_FINAL_DRIVE_ID: u32 = 0x49C;
const SETUP_DROP_GEAR_ID: u32 = 0x4AC;
/// The setup rows of the final drive and the drop gear.
const SETUP_FINAL_DRIVE: u32 = 0x35C;
const SETUP_DROP_GEAR: u32 = 0x3AC;
/// A tweak's value, after its header.
const TWEAK_VALUE: u32 = 0xC;

/// `E53SetupRows::Normalize` (0x546f70) on the drive options at `options`: each of the first 32 gear rows gets its
/// ratio ([`drive::gear_ratio`]), marked dirty.
pub fn normalize_gear_rows(mem: &mut Mem, options: u32) {
    for k in 0..NORMALIZED_ROWS {
        let row = options + OPTIONS_ROWS + k * ROW;
        let pinion = mem.u32(row + ROW_PINION + TWEAK_VALUE) as i32;
        let gear = mem.u32(row + ROW_GEAR + TWEAK_VALUE) as i32;
        set_dirty(mem, row);
        mem.set_f32(row + TWEAK_VALUE, crate::engine_driveline::drive::gear_ratio(gear, pinion));
    }
}

/// Copies the options' gear row `id` over the setup's row at `to`.
fn copy_gear_row(mem: &mut Mem, options: u32, id: u32, to: u32) {
    let row = mem.read(options + OPTIONS_ROWS + id.wrapping_mul(ROW), ROW as usize);
    mem.map(to, &row);
}

/// `CCarSettingsManager::NormalizeSetup` (0x547300) for the drive setup at `setup` and the drive options at `options`:
/// the options' rows normalized, then the setup's first gear rows (as many as the options' gear count), its final drive
/// and its drop gear each become a copy of the options' row their id names.
pub fn select_gear_rows(mem: &mut Mem, setup: u32, options: u32) {
    normalize_gear_rows(mem, options);
    for k in 0..mem.u32(options + OPTIONS_GEARS + TWEAK_VALUE) {
        let id = mem.u32(setup + SETUP_GEAR_IDS + 0x10 * k + TWEAK_VALUE);
        copy_gear_row(mem, options, id, setup + SETUP_ROWS + k * ROW);
    }
    for (id, to) in [(SETUP_FINAL_DRIVE_ID, SETUP_FINAL_DRIVE), (SETUP_DROP_GEAR_ID, SETUP_DROP_GEAR)] {
        let id = mem.u32(setup + id + TWEAK_VALUE);
        copy_gear_row(mem, options, id, setup + to);
    }
}

/// The brakes `CBrakesPage::vf48` copies: (the setup's choice, the options' first brake, the setup's brake). A choice is
/// a real, truncated; each options brake is an `E13Real10` (0xa0 bytes) and the copy takes its last nine reals.
const BRAKE_CHOICES: [(u32, u32, u32); 2] = [(0x61C, 0xDDC, 0x4BC), (0x62C, 0xF5C, 0x56C)];
const OPTIONS_BRAKE: u32 = 0xA0;
const BRAKE_COPY: usize = 0x90;

/// `CBrakesPage::vf48` (0x5487d0) for the drive setup at `setup` and the drive options at `options`: the front and rear
/// brakes the setup chooses, copied from the options into the setup.
pub fn select_brakes(mem: &mut Mem, setup: u32, options: u32) {
    for (choice, from, to) in BRAKE_CHOICES {
        let k = fpu::trunc_i32(f64::from(mem.f32(setup + choice + TWEAK_VALUE))) as u32;
        let brake = mem.read(options + from + k.wrapping_mul(OPTIONS_BRAKE), BRAKE_COPY);
        mem.map(setup + to, &brake);
    }
}

/// The tweaks `CDrive::M8770` copies from the drive setup into the drive definition (setup offset, definition offset):
/// the three differentials' torques, then the twelve gear rows' five tweaks each.
fn setup_tweaks_in_def() -> impl Iterator<Item = (u32, u32)> {
    [(0xC, 0x4C), (0x1C, 0xAC), (0x2C, 0x10C)]
        .into_iter()
        .chain((0..12 * 5).map(|k| (SETUP_ROWS + 0x10 * k, 0x688 + 0x10 * k)))
}
/// The definition's tweaks `M8770` sets from [`drive::SetupTerms`](crate::engine_driveline::drive::SetupTerms): the final ratio, the
/// footbrake pressure, and the four wheels' brake ratios (front, front, rear, rear).
const DEF_FINAL_RATIO: u32 = 0xA48;
const DEF_FOOTBRAKE: u32 = 0x614;
const DEF_BRAKE_RATIOS: [u32; 4] = [0x20C, 0x2FC, 0x3EC, 0x4DC];
/// The setup's front and rear brake pressures.
const SETUP_FRONT_PRESSURE: u32 = 0x55C;
const SETUP_REAR_PRESSURE: u32 = 0x60C;

/// `E39_Link3AB0(worker)` (0x503ab0) on the drive `drive`: the worker object (`+0x170`) stored and enabled (its `+0x78`).
pub fn link_worker(mem: &mut Mem, drive: u32, worker: u32) {
    mem.set_u32(drive + 0x170, worker);
    mem.set_u32(worker + 0x78, 1);
}

/// `CDrive::M8770(setup)` (0x508770) on the drive definition `def` (the drive's `+4`): each copied tweak's name, dirty
/// flag and value words, then the derived values, each tweak marked dirty first.
pub fn drive_setup_into_def(mem: &mut Mem, def: u32, setup: u32) {
    for (from, to) in setup_tweaks_in_def() {
        for word in [4, 8, 0xC] {
            mem.set_u32(def + to + word, mem.u32(setup + from + word));
        }
    }
    let terms = crate::engine_driveline::drive::setup_terms(
        mem.f32(setup + SETUP_FINAL_DRIVE + TWEAK_VALUE),
        mem.f32(setup + SETUP_DROP_GEAR + TWEAK_VALUE),
        mem.f32(setup + SETUP_FRONT_PRESSURE + TWEAK_VALUE),
        mem.f32(setup + SETUP_REAR_PRESSURE + TWEAK_VALUE),
    );
    let ratios = [terms.front, terms.front, terms.rear, terms.rear];
    for (at, value) in [(DEF_FINAL_RATIO, terms.final_ratio), (DEF_FOOTBRAKE, terms.footbrake)]
        .into_iter()
        .chain(DEF_BRAKE_RATIOS.into_iter().zip(ratios))
    {
        set_dirty(mem, def + at);
        mem.set_f32(def + at + TWEAK_VALUE, value);
    }
}
