//! Adapters for the backend's original memory layout and recorded control payload.

use rbr_gameplay::control::CarControl;
use rbr_gameplay::stage::{State, LOAD_SUBSTEPS};
use rbr_hybrid::pure::{self, Playback, CAR_PTR};
use rbr_physics::mem::Mem;
use rbr_recording::sound::{TyreMax, CAR_WHEELS, WHEEL_TYRE_SCRATCH};
use rbr_recording::{read_f32, CONTROL_BYTES};

use super::CarState;

pub const DRIVELINE_OFFSET: usize = 0x17c;
const VELOCITY_OFFSET: usize = 0x1c0;
const POSITION_OFFSET: usize = 0x140;
const NOTIFY_A_SPEED_OFFSET: usize = 0x15f8;
/// Keep the same full read footprint as the recording adapter, including its mapped-byte checks.
const SAMPLE_BYTES: usize = NOTIFY_A_SPEED_OFFSET + 4;

pub(super) fn recorded_car(car: &[u8]) -> CarState {
    CarState {
        driveline_parameter: read_f32(car, DRIVELINE_OFFSET).expect("car +0x17c"),
        velocity: std::array::from_fn(|lane| read_f32(car, VELOCITY_OFFSET + 4 * lane).expect("car +0x1c0")),
        position: std::array::from_fn(|lane| read_f32(car, POSITION_OFFSET + 4 * lane).expect("car +0x140")),
        // Vf6cx writes -MB560[1] to +0x15f8; NotifyA reads that scalar from p[0].
        notify_a_speed: read_f32(car, NOTIFY_A_SPEED_OFFSET).expect("car +0x15f8"),
    }
}

pub(super) fn car_state(playback: &Playback) -> CarState {
    let mem = &playback.pure.mem;
    recorded_car(&mem.read(mem.u32(CAR_PTR), SAMPLE_BYTES))
}

pub(super) fn accumulate_tyre_terms(playback: &Playback, tyre_max: &mut TyreMax) {
    let mem = &playback.pure.mem;
    let wheels: [u32; 4] = std::array::from_fn(|k| mem.u32(mem.u32(CAR_PTR) + CAR_WHEELS + 4 * k as u32));
    for (scratch, term) in playback.pure.take_tyre_terms() {
        if let Some(k) = wheels.iter().position(|&w| w != 0 && w + WHEEL_TYRE_SCRATCH == scratch) {
            tyre_max.add(k, term);
        }
    }
}

/// The drive state's +0x570 (car +0x4b0, its +8), compared by NGP's live gear handler.
pub(super) fn drive_gear(playback: &Playback) -> u32 {
    let mem = &playback.pure.mem;
    mem.u32(mem.u32(mem.u32(CAR_PTR) + 0x4b0 + 8) + 0x570)
}

/// The state goes where the control emit reads its slot (S2).
pub(super) fn set_app_state(playback: &Playback, state: State) {
    pure::set_app_state(&mut playback.pure.mem.clone(), state.value());
}

/// The 0x20-byte CSigCarControl payload, preserving word 6 when no gear is selected.
pub fn payload(control: &CarControl, word6: u32) -> [u8; CONTROL_BYTES] {
    let mut out = [0; CONTROL_BYTES];
    for (k, w) in control.words().iter().enumerate() {
        out[4 * k..4 * k + 4].copy_from_slice(&w.unwrap_or(word6).to_le_bytes());
    }
    out
}

/// The NGP boot's load budget, or the countdown captured in a validation snapshot's instruction at 0x482bc0.
pub fn load_substeps(mem: &Mem) -> u32 {
    if mem.ngp_runtime().is_some() {
        return rbr_gameplay::stage::NGP_LOAD_SUBSTEPS;
    }
    mem.try_read(0x48_2bc0, 4).map_or(LOAD_SUBSTEPS, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_uses_the_ngp_load_budget_without_executable_code() {
        use rbr_physics::ngp::layout::runtime::Runtime;
        let mut mem = Mem::new();
        assert_eq!(load_substeps(&mem), LOAD_SUBSTEPS);
        mem.set_u32(0x482bc0, rbr_gameplay::stage::NGP_LOAD_SUBSTEPS);
        assert_eq!(load_substeps(&mem), 1500);
        let mut boot = Mem::new();
        boot.set_ngp_runtime(Runtime::new([0; 3], 1));
        assert_eq!(load_substeps(&boot), load_substeps(&mem));
    }

    #[test]
    fn the_payload_takes_the_recorded_word_6_unless_a_gear_is_selected() {
        let control = CarControl { throttle: 1.0, flags: 0x10, ..CarControl::default() };
        let words = |bytes: [u8; CONTROL_BYTES]| -> Vec<u32> {
            bytes.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
        };
        assert_eq!(words(payload(&control, 0xdead)), [1.0f32.to_bits(), 0, 0, 0, 0, 0x10, 0xdead, 0]);
        let select = CarControl { flags: 0x04, gear: Some(3), ..control };
        assert_eq!(words(payload(&select, 0xdead))[6], 3);
    }

    #[test]
    fn recorded_state_preserves_signed_zero_and_nan_payloads() {
        let mut bytes = [0u8; SAMPLE_BYTES];
        let values = [
            (DRIVELINE_OFFSET, 0x8000_0000u32),
            (NOTIFY_A_SPEED_OFFSET, 0x7fc0_1234),
            (VELOCITY_OFFSET, 3.0f32.to_bits()),
            (VELOCITY_OFFSET + 4, 4.0f32.to_bits()),
            (POSITION_OFFSET, 0x8000_0000),
            (POSITION_OFFSET + 4, 0xffc0_5678),
            (POSITION_OFFSET + 8, 42.0f32.to_bits()),
        ];
        for (at, word) in values {
            bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        let sample = super::super::sample(&bytes, &[]);
        assert_eq!(sample.driveline_parameter.to_bits(), 0x8000_0000);
        assert_eq!(sample.notify_a_speed.to_bits(), 0x7fc0_1234);
        assert_eq!(sample.position.map(f32::to_bits), [0x8000_0000, 0xffc0_5678, 42.0f32.to_bits()]);
        assert_eq!(sample.speed, 5.0);
        assert!(sample.outside_assistance.is_none());
        assert!(sample.retire.is_none());
    }
}
