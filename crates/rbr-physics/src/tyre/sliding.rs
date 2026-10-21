//! The sliding model of one contact patch: the friction forces of the patch, and the carcass deflection and torsion
//! that the friction can hold.

use super::grip::clamp_deflection;
use super::layout::{patch_input, Frame, Outside};
use super::patch_forces::patch_forces;

/// Evaluates the sliding model. When the sliding weight is zero (or NaN: the compare is unordered, and takes the other
/// path too) the results are cleared. Otherwise the patch forces set the pneumatic trail and the two slip phases, and
/// the deflection and the torsion are limited by the friction forces (plus the soil's lateral force, and the radius
/// times the longitudinal force). `Outside` when the patch's table search reads past its table.
pub fn solve_sliding(f: &mut Frame) -> Result<(), Outside> {
    if f.scratch.sliding_weight == 0.0 {
        let s = &mut f.scratch;
        s.pneumatic_trail = 0.0;
        s.sliding_deflection = 0.0;
        s.sliding_torsion = 0.0;
        s.lateral_phase = 0.0;
        s.longitudinal_phase = 0.0;
        s.lateral_friction_force = 0.0;
        s.longitudinal_friction_force = 0.0;
        return Ok(());
    }

    let out = patch_forces(&patch_input(f)?);
    f.scratch.pneumatic_trail = out.pneumatic_trail;
    f.scratch.lateral_phase = out.lateral_phase;
    f.scratch.longitudinal_phase = out.longitudinal_phase;

    let (lateral, longitudinal) = (out.lateral_force, out.longitudinal_force);
    let (s, st) = (&f.scratch, &f.state);
    let sliding_deflection = clamp_deflection(
        s.lateral_soil_force + lateral,
        s.inclination_cos * s.lateral_stiffness,
        st.lateral_deflection,
    );
    let sliding_torsion = clamp_deflection(
        st.effective_radius * longitudinal + s.longitudinal_soil_force,
        s.torsional_stiffness,
        st.torsion_angle,
    );
    f.scratch.sliding_deflection = sliding_deflection;
    f.scratch.sliding_torsion = sliding_torsion;
    f.scratch.lateral_friction_force = lateral;
    f.scratch.longitudinal_friction_force = longitudinal;
    Ok(())
}
