//! The soil's drag on the tyre: the soil the slipping tyre pushes aside, and the forces and averages that follow.

/// How much of the previous average the smoothing keeps, and how much of the new value it takes.
const KEEP: f32 = 0.9;
const TAKE: f32 = 0.1;

/// What the soil drag reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoilInput {
    /// How far the tyre sinks into the soil, m, and the carcass compression (`Indentation`); the cross-section of the
    /// furrow is their blend, weighted by `tread_coverage`, times the carcass width.
    pub sink_depth: f32,
    pub carcass_compression: f32,
    pub tread_coverage: f32,
    pub carcass_width: f32,
    /// The contact patch's length, m.
    pub patch_length: f32,
    /// The normal force, N.
    pub normal_force: f32,
    /// The soil's density.
    pub soil_density: f32,
    /// The contact point's lateral and longitudinal velocities and its longitudinal slip velocity, m/s.
    pub lateral_velocity: f32,
    pub longitudinal_velocity: f32,
    pub long_slip_velocity: f32,
    /// The lateral deflection as a fraction of the sidewall height, and the cosine of the inclination.
    pub lateral_deflection_ratio: f32,
    pub inclination_cos: f32,
    /// The effective radius, m.
    pub effective_radius: f32,
    /// The averages before this step.
    pub smoothed_soil_drag: f32,
    pub smoothed_drag_weight: f32,
    pub smoothed_furrow_ratio: f32,
}

/// What the soil drag produces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoilDrag {
    /// The volume of soil the longitudinal slip moves per second, m³/s, where the slip velocity has the opposite sign
    /// to the contact point's velocity (`opposed`) and where it has the same sign (`aligned`).
    pub flow_opposed: f32,
    pub flow_aligned: f32,
    /// Always ±0: the original multiplies it by 0.0.
    pub longitudinal_force: f32,
    /// The normal force plus the drag force.
    pub normal_force: f32,
    /// The drag force, N.
    pub drag: f32,
    /// The volume the lateral slip sweeps through the soil per second, m³/s.
    pub lateral_flow: f32,
    /// The drag along the lateral axis, N.
    pub lateral_force: f32,
    /// The new averages.
    pub smoothed_soil_drag: f32,
    pub smoothed_drag_weight: f32,
    pub smoothed_furrow_ratio: f32,
}

/// The soil drag, all SSE f32 in the original's operation order. The original multiplies two terms by a zeroed
/// register (`vz` and the longitudinal force), and the multiplications stay.
pub fn soil_drag(x: SoilInput) -> SoilDrag {
    let abs = |v: f32| if v > 0.0 { v } else { 0.0 - v };
    let side_area = x.patch_length * x.sink_depth;
    let section = ((x.sink_depth - x.carcass_compression) * x.tread_coverage + x.carcass_compression) * x.carcass_width;
    let lateral_flow = abs(x.lateral_velocity) * side_area;
    let lateral_mass_flow = x.soil_density * lateral_flow;
    let alignment = x.long_slip_velocity * x.longitudinal_velocity;
    let longitudinal_mass_flow = abs(x.longitudinal_velocity) * section * x.soil_density;
    let slip = x.long_slip_velocity;
    let (flow_opposed, flow_aligned) =
        if 0.0 > alignment { (abs(slip) * section, 0.0) } else { (0.0, abs(slip) * section) };
    let lateral_drag = x.lateral_velocity * lateral_mass_flow;
    let longitudinal_drag = longitudinal_mass_flow * x.longitudinal_velocity * 0.0;
    let push = 0.0 - lateral_drag;
    let furrow_ratio = 1.0 - side_area / (x.effective_radius * 2.0);
    let lateral_share = (1.0 - x.lateral_deflection_ratio) * x.inclination_cos;
    let drag_weight = 1.0 - lateral_share;
    let lateral_part = 1.0 - drag_weight;
    let rest = 1.0 - furrow_ratio;
    let lateral_force = lateral_part * push;
    let longitudinal_zero = rest * longitudinal_drag;
    let longitudinal_force = longitudinal_zero * 0.0;
    let drag = abs(push) * drag_weight + abs(longitudinal_drag) * furrow_ratio;
    SoilDrag {
        flow_opposed,
        flow_aligned,
        longitudinal_force,
        normal_force: drag + x.normal_force,
        drag,
        lateral_flow,
        lateral_force,
        smoothed_soil_drag: x.smoothed_soil_drag * KEEP + drag * TAKE,
        smoothed_drag_weight: drag_weight * TAKE + x.smoothed_drag_weight * KEEP,
        smoothed_furrow_ratio: x.smoothed_furrow_ratio * KEEP + furrow_ratio * TAKE,
    }
}
