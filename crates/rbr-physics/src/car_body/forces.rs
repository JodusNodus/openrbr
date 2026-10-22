//! The car's per-step force composite. It applies the drag, the brake wall's friction and the gyroscopic torque
//! (the walked angular velocity crossed with its product by the local inertia, added to the torque accumulator). Then
//! the smoothed vector is blended with the walk of the car's vector (`0.7 * v`, then `+ 0.3 * walk`, in the original's
//! order), two externals run on the car, and when the debug graph option is set and the car has a graph, its first
//! two lanes are pushed to it. Last, the four mounted bodies each pass a test and then an action. The order is the
//! original's; the blend runs over all four lanes (the walk's lane 3 is what the emulator's walk copies from its
//! input).

use super::drag::{self, DragCalls};
use super::layout;
use super::wall_friction::{self, WallFrictionCalls};
use crate::car_body::store::CarStore;
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;

/// The externals this function calls itself (the drag and the wall friction are `DragCalls` and
/// `WallFrictionCalls`).
pub trait ForcesCalls<S = Mem>: DragCalls<S> + WallFrictionCalls<S> {
    /// A no-op on the car's passenger object.
    fn passenger_idle(&mut self, passenger: u32);
    /// An external on the car, before the walk: it writes the vector the walk reads, so it gets the memory.
    fn update_walked_vector(&mut self, mem: &mut S, car: u32);
    /// Walks the car's vector (the walk reads all four lanes of it) with the car as the identity entity. All four lanes
    /// of the output: lane 3 is the blend's fourth lane.
    fn walk_car_vector(&mut self, car: u32) -> Vec4;
    /// Pushes two floats to the debug graph `graph`: the smoothed vector's first two lanes times the graph scale.
    fn push_graph(&mut self, graph: u32, first: f32, second: f32);
    /// The mounted body's step gate: nonzero when the body holds (its state word is 1).
    fn mounted_step_ready(&mut self, body: u32) -> u32;
    /// The mounted body's action.
    fn mounted_action(&mut self, body: u32);
}

/// The blend's decay of the smoothed vector.
const DECAY: f32 = 0.7;

/// The walk's share in the blend.
const BLEND_SHARE: f32 = 0.3;

/// The scale of the two floats pushed to the graph (`-0.10193679`).
const GRAPH_SCALE: f32 = -0.101_936_795;

/// The settings flag that enables the debug graph.
const GRAPH_FLAG: u32 = 0x10;

/// Applies the step's forces. `drag_factor` is the drag's (`drag::apply_drag`).
pub fn apply_forces<S: CarStore>(calls: &mut impl ForcesCalls<S>, mem: &mut S, car: u32, drag_factor: f32) {
    drag::apply_drag(calls, mem, car, drag_factor);
    wall_friction::wall_friction(calls, mem, car);
    let spin = DragCalls::walk_vector(calls, mem.angular_velocity(car), car);
    let inertia = mem.local_inverse_inertia(car);
    let momentum =
        Vec3 { x: (-1.0 / inertia.x) * spin.x, y: (-1.0 / inertia.y) * spin.y, z: (-1.0 / inertia.z) * spin.z };
    calls.spin_damping(mem, car, spin.cross3(momentum));
    calls.passenger_idle(layout::passenger_at(car));
    calls.update_walked_vector(mem, car);
    let walk = calls.walk_car_vector(car);
    let decayed = mem.smoothed_vector(car).scale(DECAY);
    let blend = Vec4 {
        x: walk.x * BLEND_SHARE + decayed.x,
        y: walk.y * BLEND_SHARE + decayed.y,
        z: walk.z * BLEND_SHARE + decayed.z,
        w: walk.w * BLEND_SHARE + decayed.w,
    };
    mem.set_smoothed_vector(car, blend);
    let graph = mem.graph_2d(car);
    if mem.settings_flags() & GRAPH_FLAG != 0 && graph != 0 {
        calls.push_graph(graph, blend.x * GRAPH_SCALE, blend.y * GRAPH_SCALE);
    }
    for i in 0..4 {
        let body = mem.mounted_body(car, i);
        if body != 0 && calls.mounted_step_ready(body) != 0 {
            calls.mounted_action(body);
        }
    }
}
