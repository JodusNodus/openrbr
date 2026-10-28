//! Car and wheel poses read from a recorded frame.
//!
//! The car block is `CCar` (0x1700 bytes). Each wheel block is the first 0xa00 bytes of `CWheel`: the recorder copies
//! no more (`harness/plugin/RBRRecompHarness.cpp`, `kWheelBytes`), so the spin rate (`+0xa50`) and the ground flag
//! (`+0xebc`) are not in the recordings. The offsets below are the original's. Each cites its evidence: `src/rbr` (the
//! C++ reconstruction, `file:line`) or `notes/physics/`. Where a field's meaning is only inferred, the doc says so.
//!
//! Frame, proven on the reference drives (`tests/drives.rs`, `geometry`):
//! - The physics is z-up (`.lbs` is y-up, `.trk` z-up, `rbr_formats::lbs`). Body `+Z` is world up: its dot with world
//!   up averages 0.998 over Tanner, and the car is upright in every frame (dot above 0.9).
//! - The car drives along body `-Y`: the world direction of `-Y` (see [`CarPose::forward`]) against the velocity
//!   averages 0.99 over Tanner and Lancer and 0.95 over arctic snow, for frames above 5 m/s.
//! - Orientation is the active rotation of the quaternion `[x, y, z, w]`, so `world = position + R·local`. The
//!   velocity alignment above holds only for this convention.
//! - Wheel anchors are car-frame points. Wheels 0 and 1 sit at `y ≈ -1.09` (the front, along `-Y`), wheels 2 and 3 at
//!   `y ≈ +1.37`; the world track is 1.46 to 1.52 m and the wheelbase 2.46 m on every Tanner and arctic frame, and the
//!   four wheels are below the body centre in the upright frames. The lateral sign of wheels 0 and 2 is not proven.
//!
//! Units are metres, m/s, rad and rad/s. Quaternions are `[x, y, z, w]`.

use crate::read_f32;

/// Car orientation quaternion, four floats. `CCar::Snap9360` copies the 16 bytes here next to the position
/// (`src/rbr/RBR_E34_CarD.cpp:1372`); the rigid-body layout keeps the normalised quaternion at `+0x100`
/// (`crates/rbr-physics/src/rigid_body/layout.rs`, `Body::orientation`). Unit length is checked on the recordings.
const CAR_ORIENTATION: usize = 0x100;
/// Car position, three floats (the fourth word is unused). `Snap9360` copies it (`RBR_E34_CarD.cpp:1371`), and
/// rbr-recomp `notes/physics/car-body.md` 2a gives it as the car's position in the world (inferred).
const CAR_POSITION: usize = 0x140;
/// Car velocity, three floats in m/s (`CCar::m_velocity`; rbr-recomp `notes/physics/car-body.md` 2a, strong).
const CAR_VELOCITY: usize = 0x1c0;

/// Wheel orientation quaternion, four floats. The roll step multiplies a quaternion into this block
/// (`src/rbr/RBR_E51_WheelB.cpp:268-276`, `E15Shell::M83DD0` on `o+0x100`), and `Copy100` copies it
/// (rbr-recomp `notes/physics/wheel.md` 1). Frame not proven.
const WHEEL_ORIENTATION: usize = 0x100;
/// Wheel anchor, three floats in the car's frame. `M3DD30` sets it from the mount vector `def+0x3a0` through a 3x3
/// matrix plus a strut offset (`src/rbr/RBR_E51_WheelC.cpp:275-276`), then passes it to the car's `WorldCompose`
/// (`src/rbr/RBR_E51_WheelD.cpp:375`). The car frame is confirmed by the geometry checks in the module docs.
const WHEEL_ANCHOR: usize = 0x140;
/// The wheel axle, the unit vector `M3B900` stores from the first row of the hub's rotation (`wheel/layout/refresh_pose.rs`),
/// outward on both sides (the recorded wheels' orientations; the 208's left wheels +x, its right wheels -x).
const WHEEL_AXLE: usize = 0x9e0;
/// Roll angle about the axle, rad. Each step adds `dt * Fwd6DB0` and wraps at 20π (`RBR_E51_WheelD.cpp:747-760`,
/// `:918-935`). `Snap9360` emits it per wheel (`RBR_E34_CarD.cpp:1385`), and the half-angle sine and cosine make it
/// a rotation about the axle (`RBR_E51_WheelB.cpp:268-275`).
const WHEEL_ROLL: usize = 0x5ac;
/// Strut travel, m: the spring coordinate, clamped each step (`RBR_E51_WheelD.cpp:408-412`,
/// rbr-recomp `notes/physics/wheel.md` 2a, strong). `Snap9360` emits it per wheel (`RBR_E34_CarD.cpp:1384`).
const WHEEL_STRUT_TRAVEL: usize = 0x5bc;

/// Below this speed, m/s, the direction of travel is noise and [`CarPose::heading`] is `None`.
const MOVING_SPEED: f32 = 0.1;

/// A car's pose and velocity at one physics step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarPose {
    /// Position, m.
    pub position: [f32; 3],
    /// Orientation quaternion `[x, y, z, w]`.
    pub orientation: [f32; 4],
    /// Velocity, m/s.
    pub velocity: [f32; 3],
}

impl CarPose {
    /// Speed, m/s: the length of the velocity.
    pub fn speed(&self) -> f32 {
        let [x, y, z] = self.velocity;
        (x * x + y * y + z * z).sqrt()
    }

    /// A body-frame vector in world coordinates, without the position: `R·v` for the active rotation of
    /// `orientation`.
    pub fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = self.orientation;
        let m = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
    }

    /// A car-frame point in world coordinates: `position + R·local`. Wheel world positions are
    /// `car.to_world(wheel.anchor)`.
    pub fn to_world(&self, local: [f32; 3]) -> [f32; 3] {
        let r = self.rotate(local);
        let p = self.position;
        [p[0] + r[0], p[1] + r[1], p[2] + r[2]]
    }

    /// The car's forward direction in world coordinates: body `-Y`, proven in the module docs.
    pub fn forward(&self) -> [f32; 3] {
        self.rotate([0.0, -1.0, 0.0])
    }

    /// Direction of travel, rad: the yaw of the horizontal velocity, `atan2(y, x)`. The physics is z-up, so the
    /// horizontal plane is x and y. This is the course over the ground (it differs from [`CarPose::forward`] when
    /// the car slides). `None` below [`MOVING_SPEED`].
    pub fn heading(&self) -> Option<f32> {
        let [x, y, _] = self.velocity;
        (self.speed() >= MOVING_SPEED).then(|| y.atan2(x))
    }
}

/// One wheel's state at one physics step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelPose {
    /// Mount point, m, in the car's frame. The world position is `CarPose::to_world(anchor)`.
    pub anchor: [f32; 3],
    /// Wheel orientation quaternion `[x, y, z, w]`. Its frame is not proven.
    pub orientation: [f32; 4],
    /// The wheel axle, a unit vector in the car's frame, pointing out of the car.
    pub axle: [f32; 3],
    /// The tyre's `HubOffset` (HUB1; 0 for a recording, which does not carry it).
    pub hub_offset: f32,
    /// Roll angle about the axle, rad, wrapped at 20π.
    pub roll: f32,
    /// Strut travel, m.
    pub strut_travel: f32,
}

impl WheelPose {
    /// The tyre's centre, m, in the car's frame: the anchor moved along the axle by `HubOffset`, as `Copy100`
    /// (0x5390d0) does for the wheel's pose. `M3B900` stored the hub `HubOffset` the other way, so the anchor sits
    /// inboard or outboard of the tyre by it.
    pub fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|i| self.anchor[i] + self.hub_offset * self.axle[i])
    }
}

/// Reads `N` consecutive floats at `offset` in `block`, or `None` if any is missing.
fn floats<const N: usize>(block: &[u8], offset: usize) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = read_f32(block, offset + 4 * i)?;
    }
    Some(out)
}

/// Reads the car pose from a `CCar` block, or `None` if the block is too short for it.
pub fn car_pose(car: &[u8]) -> Option<CarPose> {
    Some(CarPose {
        position: floats(car, CAR_POSITION)?,
        orientation: floats(car, CAR_ORIENTATION)?,
        velocity: floats(car, CAR_VELOCITY)?,
    })
}

/// Reads one wheel's pose from a `CWheel` block, or `None` if the block is too short for it.
pub fn wheel_pose(wheel: &[u8]) -> Option<WheelPose> {
    Some(WheelPose {
        anchor: floats(wheel, WHEEL_ANCHOR)?,
        orientation: floats(wheel, WHEEL_ORIENTATION)?,
        axle: floats(wheel, WHEEL_AXLE)?,
        hub_offset: 0.0,
        roll: read_f32(wheel, WHEEL_ROLL)?,
        strut_travel: read_f32(wheel, WHEEL_STRUT_TRAVEL)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAR_BYTES: usize = 0x1700;
    const WHEEL_BYTES: usize = 0xa00;
    const HALF_TURN: f32 = std::f32::consts::FRAC_1_SQRT_2;

    fn put(block: &mut [u8], offset: usize, values: &[f32]) {
        for (i, v) in values.iter().enumerate() {
            block[offset + 4 * i..offset + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    fn car_at(position: [f32; 3], orientation: [f32; 4]) -> CarPose {
        CarPose { position, orientation, velocity: [0.0; 3] }
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn car_pose_reads_its_offsets() {
        let mut car = vec![0; CAR_BYTES];
        put(&mut car, CAR_POSITION, &[1.0, 2.0, 3.0]);
        put(&mut car, CAR_ORIENTATION, &[0.0, 0.0, 0.6, 0.8]);
        put(&mut car, CAR_VELOCITY, &[3.0, 4.0, 0.0]);
        let pose = car_pose(&car).unwrap();
        assert_eq!(pose.position, [1.0, 2.0, 3.0]);
        assert_eq!(pose.orientation, [0.0, 0.0, 0.6, 0.8]);
        assert_eq!(pose.velocity, [3.0, 4.0, 0.0]);
        assert_eq!(pose.speed(), 5.0);
        assert_eq!(pose.heading(), Some(4.0f32.atan2(3.0)));
    }

    #[test]
    fn car_pose_needs_the_velocity_words() {
        assert!(car_pose(&vec![0; CAR_VELOCITY + 12]).is_some());
        assert!(car_pose(&vec![0; CAR_VELOCITY + 11]).is_none());
    }

    #[test]
    fn heading_is_none_when_stopped() {
        let stopped = CarPose { position: [0.0; 3], orientation: [0.0, 0.0, 0.0, 1.0], velocity: [0.05, 0.0, 0.0] };
        assert_eq!(stopped.heading(), None);
    }

    #[test]
    fn identity_rotation_keeps_the_axes() {
        let car = car_at([10.0, 20.0, 30.0], [0.0, 0.0, 0.0, 1.0]);
        assert!(near(car.to_world([1.0, 2.0, 3.0]), [11.0, 22.0, 33.0]));
        assert!(near(car.forward(), [0.0, -1.0, 0.0]));
    }

    #[test]
    fn quarter_turn_about_z_maps_x_to_y() {
        // Active rotation of 90 degrees about +Z: x goes to y, and -Y (forward) goes to +X.
        let car = car_at([0.0; 3], [0.0, 0.0, HALF_TURN, HALF_TURN]);
        assert!(near(car.rotate([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
        assert!(near(car.forward(), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn wheel_pose_reads_its_offsets() {
        let mut wheel = vec![0; WHEEL_BYTES];
        put(&mut wheel, WHEEL_ANCHOR, &[-0.8, -0.3, 1.4]);
        put(&mut wheel, WHEEL_ORIENTATION, &[0.7, 0.0, 0.0, 0.7]);
        put(&mut wheel, WHEEL_AXLE, &[-1.0, 0.0, 0.0]);
        put(&mut wheel, WHEEL_ROLL, &[1.5]);
        put(&mut wheel, WHEEL_STRUT_TRAVEL, &[0.12]);
        let pose = wheel_pose(&wheel).unwrap();
        assert_eq!(pose.anchor, [-0.8, -0.3, 1.4]);
        assert_eq!(pose.orientation, [0.7, 0.0, 0.0, 0.7]);
        assert_eq!(pose.axle, [-1.0, 0.0, 0.0]);
        assert_eq!(pose.roll, 1.5);
        assert_eq!(pose.strut_travel, 0.12);
    }

    #[test]
    fn wheel_pose_needs_every_word_it_reads() {
        assert!(wheel_pose(&vec![0; WHEEL_AXLE + 12]).is_some());
        assert!(wheel_pose(&vec![0; WHEEL_AXLE + 11]).is_none());
    }

    #[test]
    fn the_centre_is_the_anchor_moved_along_the_axle_by_the_hub_offset() {
        let mut wheel = vec![0; WHEEL_BYTES];
        put(&mut wheel, WHEEL_ANCHOR, &[-0.795, -0.3, 1.4]);
        put(&mut wheel, WHEEL_AXLE, &[-1.0, 0.0, 0.0]);
        let pose = WheelPose { hub_offset: -0.045, ..wheel_pose(&wheel).unwrap() };
        assert!(near(pose.centre(), [-0.75, -0.3, 1.4]));
        assert_eq!(wheel_pose(&wheel).unwrap().centre(), [-0.795, -0.3, 1.4]);
    }
}
