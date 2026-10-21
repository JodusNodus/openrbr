//! `CCar::Sample9800` (exe 0x4e9800, `ret 0`): the sensor sampler, called by `Vf28x`. It fills the 16-word sensor
//! payload from the engine and drive getters, the E39 block, the filtered steering torque, the timer gap and the position value, then
//! emits it on the sensor signal (`0x4e8270`). The asm fixes the order: the getters, the timer gap, the emit; the
//! getters and the timer gap are answered by `Sample9800Calls` (from the original's records in the test).

use super::layout;
use crate::car_body::store::CarStore;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait Sample9800Calls {
    /// `0x50eb80` (x87 float): the engine's word at +0x188.
    fn engine_state_188(&mut self, engine: u32) -> f32;
    /// `0x504530` (eax): the drive's word at +0x54c.
    fn drive_4530(&mut self, drive: u32) -> u32;
    /// `0x504540` (x87 float): the drive's float at +0x384.
    fn drive_4540(&mut self, drive: u32) -> f32;
    /// `0x50ee60` (x87 float): the engine's float at +0x68 (or the constant at 0x795e50 when the word at +0x26f8 is 0).
    fn engine_sim_ee60(&mut self, engine: u32) -> f32;
    /// `0x50e970` (x87 float): the value of `0x52e390` on the engine's word at +0x28.
    fn engine_sim_e970(&mut self, engine: u32) -> f32;
    /// `0x50d380` (x87 float): the engine's float at +0x58 (or the constant at 0x795e50 when the pointer at +0x48 is 0).
    fn engine_48_58(&mut self, engine: u32) -> f32;
    /// `0x50e980` (eax): 1 when the engine's float at +0x188 is above the constant at 0x73089c, else 0.
    fn engine_sim_e980(&mut self, engine: u32) -> u32;
    /// `0x4a0290` (x87 double): the timer gap (`obj` = `ecx`, `at` = the pushed address).
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64;
    /// `0x4e8270` on the sensor signal object (the global 0x893600 plus 0x12d8) with the payload.
    fn emit_sensor(&mut self, payload: &[u32; 16]);
}

/// `12.0` (asm 0x7316bc): the E39 block's scale word is multiplied by it.
const TWELVE: f32 = 12.0;

/// `1.0` (asm 0x795e14): payload word 13.
const ONE: f32 = 1.0;

/// `0.001` (asm 0x795e94): the position value is scaled by it for payload word 9.
const POSITION_SCALE: f32 = f32::from_bits(0x3A83_126F);

/// `2^32`, added to a negative count (asm 0x795e10), so the count reads as unsigned.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// `CCar::Sample9800(this)` (name ours).
pub fn sample_sensors<S: CarStore>(calls: &mut impl Sample9800Calls, mem: &S, car: u32) {
    let engine = layout::engine_at(car);
    let drive = layout::drive_at(car);
    let block = mem.sensor_block(car);
    let mut p = [0u32; 16];
    p[0] = calls.engine_state_188(engine).to_bits();
    p[1] = calls.drive_4530(drive);
    p[2] = (calls.drive_4540(drive) * mem.eff_wheel_radius(car)).to_bits();
    p[3] = calls.engine_sim_ee60(engine).to_bits();
    p[5] = calls.engine_sim_e970(engine).to_bits();
    p[8] = mem.ffb_torque(car).value.to_bits();
    p[10] = calls.engine_48_58(engine).to_bits();
    p[11] = count_as_float(mem.sensor_count(block));
    p[12] = (mem.sensor_scale(block) * TWELVE).to_bits();
    p[13] = ONE.to_bits();
    p[14] = calls.engine_sim_e980(engine);
    let (obj, at) = mem.timer_gap_args();
    p[15] = (calls.timer_gap(obj, at) as f32).to_bits();
    p[9] = (mem.speed_sum(car) * POSITION_SCALE).to_bits();
    calls.emit_sensor(&p);
}

/// The signed count as a float: `fild` (exact), plus `2^32` when negative, then one rounding to single.
fn count_as_float(count: u32) -> u32 {
    let v = count as i32;
    let f = if v < 0 { (f64::from(v) + TWO_POW_32) as f32 } else { v as f32 };
    f.to_bits()
}
