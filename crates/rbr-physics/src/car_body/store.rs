//! The car's store (`crate::store`): the typed reads and writes of the car, its parts, wheels and body points, and the
//! world's timer words, in physical terms. `Mem` implements it in `layout/car.rs`, which knows where each value lives.

use super::layout::{Real, TimeKeeperDiff};
use crate::math::quat::Quat;
use crate::math::vec::{Vec3, Vec4};
use crate::math::Mat3;
use crate::store::Raw;

pub trait CarStore: Raw {
    /// Reads the matrix of `at` (nine words, row by row).
    fn load_mat3(&self, at: u32) -> Mat3;
    /// The car entity's sort key, which the car's signals send to identify it.
    fn sort_key(&self, car: u32) -> u32;
    /// The car's speed along its own forward direction, m/s: the speed the slip query reads, and the signal payload's
    /// speed (clamped at 0).
    fn forward_speed(&self, car: u32) -> f32;
    /// The flow object's damage selection: the host object's word when the host is the tagged kind, else the global
    /// selection. The motion probe asks it before its probe.
    fn damage_select(&self, flow: u32) -> u32;
    /// 1 when the flow holds, else 0. The state word is the host's when the host is the tagged kind, else the flow's
    /// own. The flow holds unless that word is 3 and the mode word (the host's, else the global's) is 2 or more.
    fn flow_holds(&self, flow: u32) -> u32;
    /// The force of anti-roll bar `i`: the value of its force cell (the wheel's strut step takes it, negated for the
    /// bar's second side).
    fn roll_bar_force(&self, car: u32, i: u32) -> f32;
    /// The cell `(i, j)` of the anti-roll bars' matrix, a dirty-flagged value. Named by use: columns 0 and 1 hold the
    /// travels of bar `i`'s two sides, column 2 its force.
    fn roll_bar_cell(&self, car: u32, i: u32, j: u32) -> Real;
    /// The cell of the steered wheels' torque sum (`notes/ngp.md` "Force feedback").
    fn steer_torque_sum(&self, car: u32) -> Real;
    /// The steering target: the rack position (`notes/ngp.md` "Force feedback").
    fn steer_target(&self, car: u32) -> f32;
    /// The rate of `steer_target`, per second.
    fn steer_target_rate(&self, car: u32) -> f32;
    /// The steering input the slew moves toward.
    fn steer_input(&self, car: u32) -> f32;
    /// The setup's `MaxSteeringLock`.
    fn max_steering_lock(&self, car: u32) -> f32;
    /// The setup's `SteeringRackRatio`.
    fn steering_rack_ratio(&self, car: u32) -> f32;
    /// The snapshot flag: the controls step emits the replay snapshot while it is set.
    fn snapshot_on(&self, car: u32) -> bool;
    /// The override timer, s: while above 0 the car's controls are replaced by a neutral block and the filtered steering
    /// torque is held at 0.
    fn override_timer(&self, car: u32) -> f32;
    fn set_override_timer(&mut self, car: u32, v: f32);
    /// The car's speed, m/s.
    fn speed(&self, car: u32) -> f32;
    /// The filtered steering torque, which force feedback reads.
    fn ffb_torque(&self, car: u32) -> Real;
    /// Whether the force-feedback output is muted: the entity's output gate.
    fn ffb_muted(&self, car: u32) -> bool;
    /// The car's setup (`CCar::CDef`) address.
    fn def_address(&self, car: u32) -> u32;
    /// The drag coefficient `Cd` (`DragCoefficient`).
    fn drag_coefficient(&self, car: u32) -> f32;
    /// The frontal area `A`, m^2 (`FrontArea`).
    fn frontal_area(&self, car: u32) -> f32;
    /// The car's velocity, m/s.
    fn velocity(&self, car: u32) -> Vec3;
    /// The car's angular velocity, rad/s.
    fn angular_velocity(&self, car: u32) -> Vec3;
    /// The travels of the two sides of anti-roll bar `i`: their difference acts on the bar.
    fn roll_bar_travels(&self, car: u32, i: u32) -> (f32, f32);
    /// The stiffness of anti-roll bar `i` (`AntiRollBarStiffness0` and `1`).
    fn roll_bar_stiffness(&self, car: u32, i: u32) -> f32;
    /// The car's local inverse inertia diagonal (`vecLocalInverseInertiaDiagonal`).
    fn local_inverse_inertia(&self, car: u32) -> Vec3;
    /// The low-pass filtered vector the force step keeps (`0.7` old, `0.3` new). All four lanes are modelled: the blend
    /// runs over four lanes, and lane 3 comes from the walk's copy of its input's lane 3.
    fn smoothed_vector(&self, car: u32) -> Vec4;
    fn set_smoothed_vector(&mut self, car: u32, v: Vec4);
    /// The debug graph the smoothed vector is pushed to: zero when there is none.
    fn graph_2d(&self, car: u32) -> u32;
    /// The world settings' flag word: debug and option bits (the graphs, the keeper branch of the publish step, the
    /// damage handler's material name).
    fn settings_flags(&self) -> u32;
    /// The mounted body in slot `i` of the car's four, or 0.
    fn mounted_body(&self, car: u32, i: u32) -> u32;
    /// The brake-wall penetration, m: the wall drag acts only while it is above 0.
    fn wall_penetration(&self, car: u32) -> f32;
    fn set_wall_penetration(&mut self, car: u32, v: f32);
    /// The car's force accumulator (the wall drag's `ToWorld` input; the force adds into it).
    fn force_accumulator(&self, car: u32) -> Vec3;
    /// The inverse mass, 1/kg (`InverseMass`).
    fn inverse_mass(&self, car: u32) -> f32;
    /// The gravity (the wall drag's gust): the world settings' vector.
    fn gravity(&self) -> Vec3;
    /// The gate of the drag's pre-step: a bit of the world settings' flags or of the world's own.
    fn drag_pre_step_gate(&self) -> bool;
    /// The gate of the body scatters' pre-step, from the same words.
    fn body_pre_step_gate(&self) -> bool;
    /// The body entity the car's transforms read. The original checks it for null first (its assert).
    fn body(&self, car: u32) -> u32;
    /// Makes `wheel` the body.
    fn set_body(&mut self, car: u32, wheel: u32);
    /// 1 when the replay object's state word is 2, else 0.
    fn replay_is_state2(&self, replay: u32) -> u32;
    /// The world's timer card, the timers' owner.
    fn timer_card(&self) -> u32;
    /// The timer's double, which the timekeeper records and the drive line's thunk reads.
    fn timer_double(&self) -> f64;
    /// The timer card's word the position payload ends with.
    fn timer_word(&self) -> u32;
    /// The car's position lanes (four words), the start of the position payload.
    fn position_words(&self, car: u32) -> [u32; 4];
    /// The last word of the MrR car state payload, as bits: a float of the drive.
    fn mrr_word(&self, car: u32) -> u32;
    /// The drive's sensor block: its count and scale are the sensor payload's words 11 and 12.
    fn sensor_block(&self, car: u32) -> u32;
    /// The block's count, read as a signed integer by the sensor sampler.
    fn sensor_count(&self, block: u32) -> u32;
    /// The block's scale, a float.
    fn sensor_scale(&self, block: u32) -> f32;
    /// The timer gap's arguments: the keeper (the call's `this`) and the address of the timer's double.
    fn timer_gap_args(&self) -> (u32, u32);
    fn time_keeper_diff(&self, keeper: u32, current: u32) -> TimeKeeperDiff;
    fn load_f64(&self, at: u32) -> f64;
    /// Word `k` of the hinged body's damage triple (the hinged bodies are the car's doors: the damage handler clears
    /// the triple's words as they fail).
    fn hinged_state_word(&self, hinged: u32, k: u32) -> u32;
    /// The wheel in slot `i` of the car's four, or None when the slot is empty.
    fn wheel(&self, car: u32, i: u32) -> Option<u32>;
    /// The wheel's attach flags: the bits the attach tests.
    fn attach_flags(&self, wheel: u32) -> u32;
    /// The wheel's setup (`CDef`); its slot indices say where the wheel enters the car's lists.
    fn wheel_def(&self, wheel: u32) -> u32;
    /// The index in the car's hinged list that the wheel's setup gives.
    fn hinged_index(&self, wheel: u32) -> u32;
    /// The index in the car's mounted list that the wheel's setup gives.
    fn mounted_index(&self, wheel: u32) -> u32;
    /// Stores the wheel's index in the car's wheel list.
    fn set_wheel_index(&mut self, wheel: u32, i: u32);
    /// The direction input of a wheel, three lanes (the car's ground direction sums them).
    fn wheel_direction(&self, wheel: u32) -> Vec3;
    /// The grid object the car pushes its edge to; zero when there is none.
    fn grid_object(&self, car: u32) -> u32;
    /// The grid flag: set once the car's edge is in the grid.
    fn grid_pushed(&self, car: u32) -> bool;
    fn set_grid_pushed(&mut self, car: u32, pushed: bool);
    /// Whether wheel `i` is in contact with the ground (nonzero), as the step counts the wheels that are on.
    fn wheel_contact(&self, car: u32, i: u32) -> u32;
    /// The time the car has been in contact with the wall, s.
    fn wall_contact(&self, car: u32) -> f32;
    fn set_wall_contact(&mut self, car: u32, v: f32);
    /// The time fewer than three wheels have been in contact, s.
    fn partial_contact(&self, car: u32) -> f32;
    fn set_partial_contact(&mut self, car: u32, v: f32);
    /// Whether the flag that the car's acceleration passed the dangerous limit is set.
    fn overload_flag(&self, car: u32) -> bool;
    /// Sets that flag.
    fn set_overload_flag(&mut self, car: u32);
    /// The car's damage handler: the damage object the car is wired to.
    fn damage_handler(&self, car: u32) -> u32;
    /// Stores the speed before the update.
    fn set_speed_pre(&mut self, car: u32, v: f32);
    fn set_speed(&mut self, car: u32, v: f32);
    fn set_forward_speed(&mut self, car: u32, v: f32);
    /// Stores the direction opposite the velocity, four lanes.
    fn set_dir_back(&mut self, car: u32, v: Vec4);
    /// The velocity with its fourth lane (`velocity` is the first three).
    fn velocity4(&self, car: u32) -> Vec4;
    /// The car's acceleration, m/s^2: its squared length is tested against the dangerous acceleration.
    fn acceleration(&self, car: u32) -> Vec3;
    /// The dangerous acceleration, m/s^2 (`DangerousAcceleration`).
    fn dangerous_acceleration(&self, car: u32) -> f32;
    /// The setup's `MaxFFTorque`, which the force-feedback curve divides by.
    fn max_ff_torque(&self, car: u32) -> f32;
    /// The setup's `EffWheelRadius`, m.
    fn eff_wheel_radius(&self, car: u32) -> f32;
    /// The running sum, per step, of the drive speed times `eff_wheel_radius` (named by use: the sensor payload scales it
    /// by 0.001).
    fn speed_sum(&self, car: u32) -> f32;
    /// The car's four wheel pointers, read as they are.
    fn wheel_pointers(&self, car: u32) -> [u32; 4];
    /// The object the setup's words land in: the car's setup.
    fn setup_target(&self, car: u32) -> u32;
    /// The base the body scatters add or subtract: the body's centre of gravity (`vecCenterOfGravity`). None when the
    /// body's owner has none: the original faults then.
    fn body_base(&self, body: u32) -> Option<Vec3>;
    /// The `n` points at `out` (four lanes each).
    fn load_points(&self, out: u32, n: u32) -> Vec<Vec4>;
    /// Stores lanes 0 to 2 of each point at `out` (lane 3 is not written, as the original).
    fn store_points(&mut self, out: u32, points: &[Vec4]);
    /// The car's drive line position: an index with a fraction, which the drive line's lookup takes.
    fn drive_line_word(&self, car: u32) -> u32;
    /// The dominant surface of the blend stored at `at`: two surface ids and a weight.
    fn dominant_surface(&self, at: u32) -> i32;
    fn load_quat(&self, at: u32) -> Quat;
    fn store_quat(&mut self, at: u32, q: Quat);
    fn zero_vec4(&mut self, at: u32);
}
