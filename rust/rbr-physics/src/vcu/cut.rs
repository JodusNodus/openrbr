//! Rev-limit cut (`CScratchSim::RevLimitCut`, 0x536ee0) and the slip check of the gear logic
//! (`CScratchSim::CheckSlip`, 0x537110). Both read the drive through its accessors, which the caller
//! passes in as values.

use super::layout::{Def, State, RPM_PER_RAD_S};

/// The four arguments of `CEngine::SetState4` for the rev-limit cut.
///
/// Over the limit by more than `margin` rpm (300; NGP's 25, `Tuning::cut_margin`): all off. Over by up to the
/// margin: the first and third on. At or under the limit with a shift cooldown running: the second and fourth on.
/// Otherwise all on.
pub fn rev_limit_cut(engine_speed: f32, def: &Def, state: &State, margin: f32) -> [u32; 4] {
    let over = engine_speed * RPM_PER_RAD_S - def.rpm_limit;
    if over > 0.0 {
        if over > margin {
            [0, 0, 0, 0]
        } else {
            [1, 0, 1, 0]
        }
    } else if state.shift_cooldown > 0.0 {
        [0, 1, 0, 1]
    } else {
        [1, 1, 1, 1]
    }
}

/// 1 when the unit is enabled, the drive's slip flag (`drive + 8`, `+0x55c`) is set and `0 < x < 7`.
pub fn check_slip(vcu_enabled: u32, drive_slip_flag: u32, x: f32) -> bool {
    vcu_enabled != 0 && drive_slip_flag != 0 && x > 0.0 && 7.0 > x
}

#[cfg(test)]
mod tests {
    use super::super::layout::{Knot, SpeedKnot};
    use super::*;

    fn def(rpm_limit: f32) -> Def {
        Def {
            gear_upshift_rpm: [0.0; 10],
            gear_downshift_rpm: [0.0; 10],
            shift_lockout_up_s: 0.0,
            shift_lockout_down_s: 0.0,
            clutch_ramp_rate: 0.0,
            rpm_limit,
            centre_diff_handbrake_release: 0.0,
            left_foot_brake_threshold: 0.0,
            gear_count: 0,
            centre_throttle_lock: Vec::<Knot>::new(),
            centre_brake_lock: Vec::<Knot>::new(),
            centre_speed_lock: Vec::<SpeedKnot>::new(),
            lf_centre_throttle_lock: Vec::<Knot>::new(),
            lf_centre_brake_lock: Vec::<Knot>::new(),
            lf_centre_speed_lock: Vec::<SpeedKnot>::new(),
            front_throttle_lock: Vec::<Knot>::new(),
            front_brake_lock: Vec::<Knot>::new(),
            front_speed_lock: Vec::<SpeedKnot>::new(),
            rear_throttle_lock: Vec::<Knot>::new(),
            rear_brake_lock: Vec::<Knot>::new(),
            rear_speed_lock: Vec::<SpeedKnot>::new(),
        }
    }

    /// Engine speed (rad/s) that is `rpm` rpm.
    fn rad(rpm: f32) -> f32 {
        rpm / RPM_PER_RAD_S
    }

    #[test]
    fn rev_limit_cut_branches() {
        let st = |cooldown| State { shift_cooldown: cooldown, ..State::default() };
        let d = def(7800.0);
        assert_eq!(rev_limit_cut(rad(8200.0), &d, &st(0.0), 300.0), [0, 0, 0, 0]);
        assert_eq!(rev_limit_cut(rad(7900.0), &d, &st(0.0), 300.0), [1, 0, 1, 0]);
        assert_eq!(rev_limit_cut(rad(7000.0), &d, &st(0.05), 300.0), [0, 1, 0, 1]);
        assert_eq!(rev_limit_cut(rad(7000.0), &d, &st(0.0), 300.0), [1, 1, 1, 1]);
        // NaN is not over the limit (the compare fails), so the cooldown decides.
        assert_eq!(rev_limit_cut(f32::NAN, &d, &st(0.05), 300.0), [0, 1, 0, 1]);
        assert_eq!(rev_limit_cut(f32::NAN, &d, &st(f32::NAN), 300.0), [1, 1, 1, 1]);
    }

    #[test]
    fn check_slip_conditions() {
        assert!(check_slip(1, 1, 3.0));
        assert!(!check_slip(0, 1, 3.0));
        assert!(!check_slip(1, 0, 3.0));
        assert!(!check_slip(1, 1, 0.0));
        assert!(!check_slip(1, 1, 7.0));
        assert!(!check_slip(1, 1, f32::NAN));
    }
}
