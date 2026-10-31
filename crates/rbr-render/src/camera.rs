//! Free-fly camera and the view frustum used to cull blocks.
//!
//! Headings follow [`crate::coords`]: left-handed, y up. Yaw 0 looks down +z, and yaw grows
//! towards +x.

use crate::coords;
use glam::{Mat3, Mat4, Vec3, Vec4};

/// Pitch limit, radians, so the view never flips over the top.
const PITCH_LIMIT: f32 = 1.5;

/// A camera position and heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeFly {
    pub position: Vec3,
    /// Radians, about +y.
    pub yaw: f32,
    /// Radians, up is positive.
    pub pitch: f32,
}

/// Movement input in the camera's own axes, each between -1 and 1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Movement {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
}

impl FreeFly {
    /// A camera at `position` looking along `forward` (not necessarily unit length).
    pub fn looking_along(position: Vec3, forward: Vec3) -> FreeFly {
        let f = forward.normalize_or(Vec3::Z);
        FreeFly { position, yaw: f.x.atan2(f.z), pitch: f.y.asin() }
    }

    /// Unit vector the camera looks along.
    pub fn forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch)
    }

    /// Unit vector to the camera's right, level with the ground.
    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    /// Turns the camera by mouse deltas, radians.
    pub fn turn(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch = (self.pitch + pitch_delta).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Moves the camera by `movement` scaled by `distance` metres along its own axes. Up is world up.
    pub fn step(&mut self, movement: Movement, distance: f32) {
        self.position += self.forward() * (movement.forward * distance)
            + self.right() * (movement.right * distance)
            + Vec3::Y * (movement.up * distance);
    }

    /// World-to-clip matrix for horizontal field of view `fov` (radians) and an image `aspect` wide per high.
    pub fn view_projection(&self, fov: f32, aspect: f32) -> Mat4 {
        coords::projection(fov, aspect) * coords::view_matrix(self.position, self.forward())
    }

    /// The clip matrix for camera-anchored sky: the same projection, with the view's rotation only.
    pub fn sky_view_projection(&self, fov: f32, aspect: f32) -> Mat4 {
        coords::projection(fov, aspect) * coords::sky_view_matrix(self.forward())
    }

    /// The camera as a [`Lens`]: y up, horizontal field of view `fov` (radians) and [`coords::NEAR`].
    pub fn lens(&self, fov: f32) -> Lens {
        Lens { eye: self.position, forward: self.forward(), up: Vec3::Y, fov, near: coords::NEAR }
    }
}

/// Pitch limit of the orbit camera, radians.
const ORBIT_PITCH_LIMIT: f32 = 1.4;
/// Eye distance limits of the orbit camera, metres.
const ORBIT_MIN_DISTANCE: f32 = 0.5;
const ORBIT_MAX_DISTANCE: f32 = 100.0;

/// A camera that looks at `target` from `distance` metres away. The eye is on the side that `yaw` and
/// `pitch` give, with the same axes as [`FreeFly`]: yaw 0 puts the eye on +z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    pub target: Vec3,
    pub distance: f32,
    /// Radians, about +y.
    pub yaw: f32,
    /// Radians, up is positive.
    pub pitch: f32,
}

impl Orbit {
    /// A camera that frames the box `lo`..`hi` from the front and slightly above, across the image's width at
    /// horizontal field of view `fov` (radians).
    pub fn framing(lo: Vec3, hi: Vec3, fov: f32) -> Orbit {
        let radius = (hi - lo).length() * 0.5;
        let half_fov = fov * 0.5;
        Orbit {
            target: (lo + hi) * 0.5,
            distance: (radius / half_fov.sin() * 1.1).clamp(ORBIT_MIN_DISTANCE, ORBIT_MAX_DISTANCE),
            // A three-quarter view from the front (-z) and the car's left (+x).
            yaw: std::f32::consts::PI - 0.7,
            pitch: 0.25,
        }
    }

    /// The eye position.
    pub fn eye(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        self.target + Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch) * self.distance
    }

    /// Turns the eye around the target by mouse deltas, radians.
    pub fn turn(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch = (self.pitch + pitch_delta).clamp(-ORBIT_PITCH_LIMIT, ORBIT_PITCH_LIMIT);
    }

    /// Multiplies the eye distance by `factor`, within the limits.
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(ORBIT_MIN_DISTANCE, ORBIT_MAX_DISTANCE);
    }

    /// World-to-clip matrix for horizontal field of view `fov` (radians) and an image `aspect` wide per high.
    pub fn view_projection(&self, fov: f32, aspect: f32) -> Mat4 {
        let eye = self.eye();
        coords::projection(fov, aspect) * coords::view_matrix(eye, self.target - eye)
    }

    /// The sky's view-projection: the view's rotation only, with the eye at the origin.
    pub fn sky_view_projection(&self, fov: f32, aspect: f32) -> Mat4 {
        coords::projection(fov, aspect) * coords::sky_view_matrix(self.target - self.eye())
    }
}

/// A camera with its own up and near plane: the views of a car, the fixed ones rolling with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lens {
    pub eye: Vec3,
    /// Unit.
    pub forward: Vec3,
    pub up: Vec3,
    /// Horizontal field of view, radians; the vertical one follows from the image's aspect.
    pub fov: f32,
    /// Metres.
    pub near: f32,
}

impl Lens {
    /// World-to-clip matrix for an image `aspect` wide per high.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * look_to(self.eye, self.forward, self.up)
    }

    /// The sky's view-projection: the view's rotation only, with the eye at the origin.
    pub fn sky_view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * look_to(Vec3::ZERO, self.forward, self.up)
    }

    /// View-to-clip matrix for an image `aspect` wide per high.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        coords::perspective(self.fov, aspect, self.near)
    }
}

/// The left-handed world-to-view matrix of an eye looking along `forward` with `up` (neither unit length).
fn look_to(eye: Vec3, forward: Vec3, up: Vec3) -> Mat4 {
    glam::camera::lh::view::look_to_mat4(eye, forward.normalize(), up.normalize())
}

/// The eye position of a perspective `view_projection`: the point that clip x, y and w all send to zero.
pub fn eye(view_projection: Mat4) -> Vec3 {
    let row = |i| view_projection.row(i);
    let (r0, r1, r3) = (row(0), row(1), row(3));
    let rows = Mat3::from_cols(r0.truncate(), r1.truncate(), r3.truncate()).transpose();
    -(rows.inverse() * Vec3::new(r0.w, r1.w, r3.w))
}

/// The six clip planes of a view-projection matrix, as `(normal, distance)` with normals pointing
/// inwards: a point is inside when `normal . p + distance >= 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum([Vec4; 6]);

impl Frustum {
    /// Planes of `view_projection` (Gribb and Hartmann), for depth range 0 to 1.
    pub fn from_view_projection(view_projection: Mat4) -> Frustum {
        let row = |i| view_projection.row(i);
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        Frustum([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2])
    }

    /// Whether the box `center` +- `half_extent` touches the frustum.
    pub fn intersects_box(&self, center: Vec3, half_extent: Vec3) -> bool {
        self.0.iter().all(|plane| {
            let normal = plane.truncate();
            normal.dot(center) + plane.w + half_extent.dot(normal.abs()) >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_round_trips_through_yaw_and_pitch() {
        let camera = FreeFly::looking_along(Vec3::ZERO, Vec3::new(0.6, 0.2, 0.8));
        let forward = camera.forward();
        assert!((forward - Vec3::new(0.6, 0.2, 0.8).normalize()).length() < 1e-5);
        assert!(camera.right().x > 0.0 && camera.right().z < 0.0);
    }

    #[test]
    fn eye_is_the_camera_position() {
        let camera = FreeFly::looking_along(Vec3::new(10.0, 2.0, -3.0), Vec3::new(0.6, 0.2, 0.8));
        let eye = eye(camera.view_projection(coords::FOV_DEGREES.to_radians(), 16.0 / 9.0));
        assert!((eye - Vec3::new(10.0, 2.0, -3.0)).length() < 1e-3, "{eye}");
    }

    #[test]
    fn frustum_keeps_boxes_in_front_and_drops_boxes_behind() {
        let camera = FreeFly::looking_along(Vec3::ZERO, Vec3::Z);
        let frustum =
            Frustum::from_view_projection(camera.view_projection(coords::FOV_DEGREES.to_radians(), 16.0 / 9.0));
        let half = Vec3::splat(1.0);
        assert!(frustum.intersects_box(Vec3::new(0.0, 0.0, 100.0), half));
        assert!(!frustum.intersects_box(Vec3::new(0.0, 0.0, -100.0), half));
        assert!(!frustum.intersects_box(Vec3::new(1000.0, 0.0, 100.0), half));
    }
}
