//! The car's store (`crate::store`): the layout's getters and setters (`CCar`, its parts, wheels and body points, the
//! world's timer words). `Mem` implements it in `layout/car.rs`.

use super::layout::{Real, TimeKeeperDiff};
use crate::math::quat::Quat;
use crate::math::vec::{Vec3, Vec4};
use crate::math::Mat3;
use crate::store::Raw;

pub trait CarStore: Raw {
    /// Reads the matrix of `at` (nine words, row by row).
    fn load_mat3(&self, at: u32) -> Mat3;
    /// `CCar::Get30` (0x4e67f0): the word at `this + 0x30`.
    fn get_30(&self, car: u32) -> u32;
    /// `CCar::Get15F8` (0x4e6770, thiscall, st0): the float at `this + 0x15f8`, the car speed the slip query reads.
    fn get_15f8(&self, car: u32) -> f32;
    /// `E60Flow::M5666b0` (0x5666b0, fastcall, `flow` in ecx): the host object's word at `+0x17854` when its first word is
    /// 0x7b (the `short` compare), else the global `DAMAGE_SELECT`. `MotionProbe` asks it before its probe.
    fn damage_select(&self, flow: u32) -> u32;
    /// `E60Flow::M566640` (0x566640, thiscall, `flow` in ecx): 1 when the flow holds, else 0. The state word is the host's
    /// +0x17850 when the host's first word is 0x7b (the `short` compare), else the flow's own first word. The flow holds
    /// unless that word is 3 and the mode word (the host's +0x17858, else the global's +0x8e84) is 2 or more.
    fn flow_holds(&self, flow: u32) -> u32;
    /// The lane word at `+0xa48 + 0x40 * i` (`CCar::GetA48`). Meaning unknown; the drive calls it with `i` 0 and 1.
    fn lane_a48(&self, car: u32, i: u32) -> f32;
    /// The CReal at +0xad0: its value (+0xadc) is the steered wheels' torque sum (`AddADC`; `notes/ngp.md` "Force feedback").
    fn steer_torque_sum(&self, car: u32) -> Real;
    /// The CReal at `+0xa1c + 0x10 * (j + 4 * i)` (`CCar::SetA1C`). Meaning unknown.
    fn matrix_cell(&self, car: u32, i: u32, j: u32) -> Real;
    /// The word at +0xb0c (`CCar::GetB0C`): the steering rack position (`notes/ngp.md` "Force feedback").
    fn target_b0c(&self, car: u32) -> f32;
    /// The word at +0xb1c (`CCar::GetB1C`): the rate of `target_b0c`, by `notes/physics.md` "Car body".
    fn rate_b1c(&self, car: u32) -> f32;
    /// The target the slew moves toward (`m_steerInput`, +0x16a4).
    fn steer_input(&self, car: u32) -> f32;
    /// The setup word at `def + 0x22c`, `MaxSteeringLock` (the value of the key's CReal at `+0x220`).
    fn max_steering_lock(&self, car: u32) -> f32;
    /// The snapshot flag (+0x9e4, `m_snapshotOn`): `StepEngine` calls `Snap9360` while it is set.
    fn snapshot_on(&self, car: u32) -> bool;
    /// The override timer (+0x1670): while above 0 the filtered steering torque is held at 0 (`CCar::UpdateFfbTorque`).
    fn t_override(&self, car: u32) -> f32;
    /// The car speed (+0x15fc), m/s (rbr-recomp `notes/physics/car-body.md` 2a, inferred).
    fn speed(&self, car: u32) -> f32;
    /// The filtered steering torque, which force feedback reads: the CReal at +0xae0 (its value at +0xaec).
    fn ffb_torque(&self, car: u32) -> Real;
    /// The car's setup (`CCar::CDef`) address: the pointer at +0x34.
    fn def_address(&self, car: u32) -> u32;
    /// The setup word at `def + 0x23c`, `SteeringRackRatio` (the value of the key's CReal at `+0x230`).
    fn steering_rack_ratio(&self, car: u32) -> f32;
    /// The drag coefficient `Cd` (`def + 0x4490`, `notes/physics.md` "Car body": `DragCoefficient`).
    fn drag_coefficient(&self, car: u32) -> f32;
    /// The frontal area `A` (`def + 0x4480`, `notes/physics.md` "Car body": `FrontArea`).
    fn frontal_area(&self, car: u32) -> f32;
    /// The car's velocity (`CCar::m_velocity`, three lanes at +0x1c0).
    fn velocity(&self, car: u32) -> Vec3;
    /// The car's angular velocity (`CCar::m_angularVelocity`, three lanes at +0x190).
    fn angular_velocity(&self, car: u32) -> Vec3;
    /// The first and fifth lanes of damper `i` (at +0xa28 + 0x40 i and +0x10 more): their difference is its force.
    fn damper_lanes(&self, car: u32, i: u32) -> (f32, f32);
    /// The setup gain of damper `i` (`def + 0x4460 + 0x10 i`).
    fn damper_gain(&self, car: u32, i: u32) -> f32;
    /// The three inertia-like words of the def (`def + 0x130 + 4 i`, `CCar::Vf64x`'s divisors; their meaning is open).
    fn def_inertia(&self, car: u32) -> Vec3;
    /// The vector at +0x15d0 (`CCar::Vf64x` blends it; its meaning is open). All four lanes are modelled: the blend runs
    /// `mulps` and `addps` over four lanes, and lane 3 comes from the walk's copy of its input's lane 3.
    fn aux_vector(&self, car: u32) -> Vec4;
    /// Stores the four lanes of the vector at +0x15d0 (`aux_vector`).
    fn set_aux_vector(&mut self, car: u32, v: Vec4);
    /// The scalar at +0x15d4 (`CCar::Vf64x`, the second argument of its external 0x495090).
    fn aux_scalar(&self, car: u32) -> f32;
    /// The word at +0x16ac (`CCar::Vf64x`): zero or not decides the external call.
    fn aux_flag(&self, car: u32) -> u32;
    /// The keeper's word at +0x160e0 (`CCar::Vf64x` tests bit 0x10 of it).
    fn keeper_flags(&self) -> u32;
    /// The object pointer at +0x490 + 4 i (`CCar::Vf64x`'s loop over four objects).
    fn car_object(&self, car: u32, i: u32) -> u32;
    /// The brake-wall penetration (`m_wallPenetration`, +0x166c, m): `Sim7AD0` acts only while it is above 0.
    fn wall_penetration(&self, car: u32) -> f32;
    /// The vector at +0x370, the seed `E46_Get370` returns (`Sim7AD0`'s `ToWorld` input). S70 adds into it.
    fn accumulator(&self, car: u32) -> Vec3;
    /// The inverse mass (`def + 0x16c`, `notes/physics.md` "Car body"): `Sim7AD0` divides by it.
    fn inverse_mass(&self, car: u32) -> f32;
    /// The wind (`Sim7AD0`'s gust): the world keeper's vector at `+0x160a0 + 0x10`.
    fn wind(&self) -> Vec3;
    /// The time keeper's gate (`CCar::DragAndSpinDamp`): bit 0x4000 of the keeper's word at +0x160f4 or of the world's
    /// word at +0x38. The keeper is the world's pointer at +0x24.
    fn time_keeper_gate(&self) -> bool;
    /// The body the car's transform reads (`+0x4a0`, `CCar::Xform6870`). The asm checks it for null first (the assert path).
    fn body(&self, car: u32) -> u32;
    /// Stores the body (`+0x4a0`): `Vf150x` makes a wheel the body when its flags say so.
    fn set_body(&mut self, car: u32, wheel: u32);
    /// `E19IsState2` (0x499900, fastcall on the replay object at the global 0x893634): 1 when its state word (+0x10) is 2,
    /// else 0.
    fn replay_is_state2(&self, replay: u32) -> u32;
    /// The world's timer card (`*(world + 0x20)`, the timers' owner).
    fn timer_card(&self) -> u32;
    /// The timer's double (`*(timer card + 0x20)`), which the timekeeper records and the drive line's thunk reads.
    fn timer_double(&self) -> f64;
    /// The timer card's word at +0x18 (the position payload's last word).
    fn timer_word_18(&self) -> u32;
    /// The car's position lanes (`car + 0x140`, four words), the start of the position payload.
    fn position_words(&self, car: u32) -> [u32; 4];
    /// The payload's owner word (`car + 0x30`).
    fn owner_word(&self, car: u32) -> u32;
    /// The float at `*(car + 0x4b8) + 0x45c` (the MrR payload's last word, as bits).
    fn mrr_word(&self, car: u32) -> u32;
    /// The E39 block (+0x620): its count word (+0x80) and scale (+0x8c) are the sensor payload's words 11 and 12.
    fn sensor_block(&self, car: u32) -> u32;
    /// The count word of the E39 block (`+0x80`, read as a signed integer by `Sample9800`).
    fn sensor_count(&self, block: u32) -> u32;
    /// The scale word of the E39 block (`+0x8c`, a float).
    fn sensor_scale(&self, block: u32) -> f32;
    /// The timer gap's arguments (`0x4a0290`): the keeper, which is the world's word at +0x40 (`ecx`), and the address of
    /// the timer's double, the card's +0x20 plus 0x20 (pushed).
    fn timer_gap_args(&self) -> (u32, u32);
    fn time_keeper_diff(&self, keeper: u32, current: u32) -> TimeKeeperDiff;
    fn load_f64(&self, at: u32) -> f64;
    /// The word at +0x650 of a wheel's object, dereferenced `k` words: `*(wheel + 0x650) + 4 k` (`Vf6cx`'s tests).
    fn wheel_flag(&self, wheel: u32, k: u32) -> u32;
    /// The wheel at +0x470 + 4 i (`CCar::m_wheels`), or None when the slot is empty (`GetDir` reads wheels 0 and 1).
    fn wheel(&self, car: u32, i: u32) -> Option<u32>;
    /// The wheel's attach flags (+0x60): the bits `Vf150x` tests.
    fn attach_flags(&self, wheel: u32) -> u32;
    /// The wheel's def (+0x34, a `CDef`); its slot indices are at +0x4fc and +0x23c.
    fn wheel_def(&self, wheel: u32) -> u32;
    /// Stores the wheel's index in the car's wheel list (+0xeb4).
    fn set_wheel_index(&mut self, wheel: u32, i: u32);
    /// The direction input of a wheel: the vector at +0x9e0 (lanes 0 to 2; `GetDir` sums them).
    fn wheel_direction(&self, wheel: u32) -> Vec3;
    /// The grid object the car pushes its edge to (`CCar` +0x64, an `E37GridOps`); zero when there is none.
    fn grid_object(&self, car: u32) -> u32;
    /// The grid flag `m_gridPushed` (+0x16a8): nonzero once the car's edge is in the grid (`PushGridEdge` sets it).
    fn grid_pushed(&self, car: u32) -> bool;
    /// Stores the grid flag (`layout::grid_pushed`), 1 when set.
    fn set_grid_pushed(&mut self, car: u32, pushed: bool);
    /// The `+0xebc` word of the wheel at +0x470 + 4 i (`Vf6cx` counts the wheels that are on).
    fn wheel_ebc(&self, car: u32, i: u32) -> u32;
    /// Stores the brake-wall penetration (+0x166c, `m_wallPenetration`).
    fn set_wall_penetration(&mut self, car: u32, v: f32);
    /// The time the wall has been in contact (+0x9ec, `m_tWallContact`).
    fn wall_contact(&self, car: u32) -> f32;
    /// Stores the time the wall has been in contact (+0x9ec).
    fn set_wall_contact(&mut self, car: u32, v: f32);
    /// The partial-contact time (+0x9e8, `m_tPartialContact`): `Vf6cx` adds the step to it.
    fn partial_contact(&self, car: u32) -> f32;
    /// Stores the partial-contact time (+0x9e8).
    fn set_partial_contact(&mut self, car: u32, v: f32);
    /// Stores the override timer (+0x1670, `m_tOverride`).
    fn set_t_override(&mut self, car: u32, v: f32);
    /// Sets the 50g flag (+0x1668, `m_flag50g`) to 1.
    fn set_flag_50g(&mut self, car: u32);
    /// Stores the speed before the update (+0x1600, `m_speedPre`).
    fn set_speed_pre(&mut self, car: u32, v: f32);
    /// Stores the speed (+0x15fc, `m_speed`).
    fn set_speed(&mut self, car: u32, v: f32);
    /// Stores the negated y of the walk of the velocity (+0x15f8; its meaning is open).
    fn set_neg_walk_y(&mut self, car: u32, v: f32);
    /// The direction opposite the velocity, four lanes (+0x1610, `m_dirBack`).
    fn set_dir_back(&mut self, car: u32, v: Vec4);
    /// The velocity with its fourth lane (+0x1c0, four lanes; `velocity` is the first three).
    fn velocity4(&self, car: u32) -> Vec4;
    /// The three words of the def at +0x230, +0x234, +0x238 (`Vf6cx`'s 50g test squares them).
    fn accel_lanes(&self, car: u32) -> Vec3;
    /// The dangerous acceleration (`CDef::m_dangerousAcceleration`, def +0x44b0).
    fn dangerous_acceleration(&self, car: u32) -> f32;
    /// The setup word at `def + 0x44a0`, `MaxFFTorque` (the value of the key's CReal at `+0x4494`), which `FfbNorm`
    /// divides by.
    fn max_ff_torque(&self, car: u32) -> f32;
    /// The setup word at `def + 0x24c` (a working name): the gain of the drive's torque term in `Vf6cx`'s position.
    fn def_gain_24c(&self, car: u32) -> f32;
    /// The word at +0x15f4 (the position's running value, `Vf6cx` and the payload's `* 0.001`).
    fn value_15f4(&self, car: u32) -> f32;
    /// The car's four wheel pointers (+0x470 to +0x47c), read as they are (`CCar::UpdateA720` calls each one).
    fn wheel_pointers(&self, car: u32) -> [u32; 4];
    /// The object the seed's frame words land in (the car's +0x34).
    fn frame_object(&self, car: u32) -> u32;
    /// The body gate of the point scatters (`CCarBody::Vf9Cx` and `VfA0x`): bit 0x1000 of the keeper's word at +0x160f4 or
    /// of the world's word at +0x38. The keeper is the world's pointer at +0x24, as in `time_keeper_gate`.
    fn body_gate(&self) -> bool;
    /// The base the scatters add or subtract: the lanes at +0x130 of the object at `body + 0x34`. None when that object's
    /// word at +0x120 is null: the original faults then.
    fn body_base(&self, body: u32) -> Option<Vec3>;
    /// The `n` points at `out` (sixteen bytes each, lanes 0 to 3).
    fn load_points(&self, out: u32, n: u32) -> Vec<Vec4>;
    /// Stores lanes 0 to 2 of each point at `out` (lane 3 is not written, as the original).
    fn store_points(&mut self, out: u32, points: &[Vec4]);
    /// The drive line index at +0x17c, the word the drive line's thunk (`0x50a4f0`) takes.
    fn drive_line_word(&self, car: u32) -> u32;
    /// `E36Triple::MinPick` (0x4f3ec0) on the triple at `at`: the int pair and the float.
    fn min_pick(&self, at: u32) -> i32;
    fn load_quat(&self, at: u32) -> Quat;
    fn store_quat(&mut self, at: u32, q: Quat);
    fn zero_vec4(&mut self, at: u32);
}
