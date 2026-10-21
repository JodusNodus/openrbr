//! The sensor sampler, called by the publish step. It fills the 16-word sensor payload from the engine and drive
//! getters, the drive's sensor block, the filtered steering torque, the timer gap and the speed sum, then emits it on
//! the sensor signal. The order is the original's: the getters, the timer gap, the emit; the getters and the timer gap
//! are answered by `SensorCalls` (from the original's records in the test).

use super::layout;
use crate::car_body::store::CarStore;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait SensorCalls {
    /// The engine's speed (payload word 0).
    fn engine_speed(&mut self, engine: u32) -> f32;
    /// The drive's gear word (payload word 1).
    fn drive_gear(&mut self, drive: u32) -> u32;
    /// The drive's speed, times the wheel radius for payload word 2.
    fn drive_speed(&mut self, drive: u32) -> f32;
    /// The engine's float for payload word 3 (a constant when the engine has no source for it). Named by use.
    fn engine_word_3(&mut self, engine: u32) -> f32;
    /// The engine's float for payload word 5. Named by use.
    fn engine_word_5(&mut self, engine: u32) -> f32;
    /// The engine's float for payload word 10 (a constant when the engine has no source for it). Named by use.
    fn engine_word_10(&mut self, engine: u32) -> f32;
    /// Payload word 14: 1 when the engine's speed is above a fixed limit, else 0.
    fn engine_word_14(&mut self, engine: u32) -> u32;
    /// The timer gap as a double (`obj` is the call's `this`, `at` the address of the timer's double).
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64;
    /// Emits the payload on the sensor signal.
    fn emit_sensor(&mut self, payload: &[u32; 16]);
}

/// The sensor block's scale is multiplied by it for payload word 12.
const SCALE_FACTOR: f32 = 12.0;

/// Payload word 13.
const ONE: f32 = 1.0;

/// The speed sum is scaled by it for payload word 9.
const SPEED_SUM_SCALE: f32 = 0.001;

/// `2^32`, added to a negative count, so the count reads as unsigned.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// Fills and emits the sensor payload of `car`.
pub fn sample_sensors<S: CarStore>(calls: &mut impl SensorCalls, mem: &S, car: u32) {
    let engine = layout::engine_at(car);
    let drive = layout::drive_at(car);
    let block = mem.sensor_block(car);
    let mut p = [0u32; 16];
    p[0] = calls.engine_speed(engine).to_bits();
    p[1] = calls.drive_gear(drive);
    p[2] = (calls.drive_speed(drive) * mem.eff_wheel_radius(car)).to_bits();
    p[3] = calls.engine_word_3(engine).to_bits();
    p[5] = calls.engine_word_5(engine).to_bits();
    p[8] = mem.ffb_torque(car).value.to_bits();
    p[10] = calls.engine_word_10(engine).to_bits();
    p[11] = count_as_float(mem.sensor_count(block));
    p[12] = (mem.sensor_scale(block) * SCALE_FACTOR).to_bits();
    p[13] = ONE.to_bits();
    p[14] = calls.engine_word_14(engine);
    let (obj, at) = mem.timer_gap_args();
    p[15] = (calls.timer_gap(obj, at) as f32).to_bits();
    p[9] = (mem.speed_sum(car) * SPEED_SUM_SCALE).to_bits();
    calls.emit_sensor(&p);
}

/// The signed count as a float: exact conversion, plus `2^32` when negative, then one rounding to single.
fn count_as_float(count: u32) -> u32 {
    let v = count as i32;
    let f = if v < 0 { (f64::from(v) + TWO_POW_32) as f32 } else { v as f32 };
    f.to_bits()
}
