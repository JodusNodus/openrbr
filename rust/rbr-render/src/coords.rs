//! Coordinate conventions. Written down once here; nothing else in the crate flips an axis.
//!
//! - Positions are the stage file's values as stored: left-handed, y up, metres
//!   (`notes/formats.md` "Shaders and vertex layouts", Camera). The original projects with D3D left-handed maths
//!   (`_34 = 1`).
//! - The view and the projection are left-handed too ([`view_matrix`], [`projection`]). wgpu's clip
//!   space matches D3D's (x right, y up, depth 0 to 1), so the image is the one the game draws with
//!   no mirroring.
//! - Depth is reversed ([`perspective`]): 1 at the near plane, 0 at the far one, tested with `Greater`, cleared to
//!   [`DEPTH_CLEAR`]. With a float depth buffer that keeps about the same relative precision at every distance; the
//!   game's `0..1` depth (D24, near planes of 0.1 m from the cars' camera files, far 10 km) left about a millimetre
//!   at 50 m, and signs fought their backings.
//! - Front faces: D3D culls counter-clockwise triangles by default (`D3DCULL_CCW`), so the front
//!   face is the one that looks clockwise on screen. That is [`FRONT_FACE`] with back-face culling.
//! - Vertex colour is a D3DCOLOR `u32`, stored little-endian as bytes B, G, R, A.
//!   [`rgba_from_d3dcolor`] reorders them.

use glam::camera::lh::{proj::directx, view};
use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

/// The default horizontal field of view, degrees (`viewer --fov`): the free-fly and orbit cameras and the bonnet,
/// bumper and cockpit views. The chase has its own ([`crate::playback::CHASE_FOV_DEGREES`]).
pub const FOV_DEGREES: f32 = 50.0;
/// The horizontal fields of view `viewer --fov` and the launcher take, degrees. Ours.
pub const FOV_RANGE_DEGREES: std::ops::RangeInclusive<f32> = 30.0..=120.0;
/// Near plane, metres.
pub const NEAR: f32 = 0.15;
/// Far plane, metres (`notes/formats.md` "Shaders and vertex layouts": about 10,000).
pub const FAR: f32 = 10_000.0;
/// The depth the main view's buffers are cleared to: the far plane under the reversed depth.
pub const DEPTH_CLEAR: f32 = 0.0;
/// Winding of triangles that face the viewer (see the module comment).
pub const FRONT_FACE: wgpu::FrontFace = wgpu::FrontFace::Cw;

/// The world-to-view matrix for an eye at `eye` looking along `forward`, with y up. `forward` may have
/// any length; the view basis needs it unit.
pub fn view_matrix(eye: Vec3, forward: Vec3) -> Mat4 {
    view::look_to_mat4(eye, forward.normalize(), Vec3::Y)
}

/// The vertical field of view of an image `aspect` wide per high whose horizontal one is `fov`, radians.
pub fn vertical_fov(fov: f32, aspect: f32) -> f32 {
    2.0 * ((fov * 0.5).tan() / aspect).atan()
}

/// The view-to-clip matrix for horizontal field of view `fov` (radians) and an image `aspect` wide per high.
pub fn projection(fov: f32, aspect: f32) -> Mat4 {
    perspective(fov, aspect, NEAR)
}

/// [`projection`] with near plane `near`, metres, and reversed depth (1 at `near`, 0 at [`FAR`]).
pub fn perspective(fov: f32, aspect: f32, near: f32) -> Mat4 {
    reversed(vertical_fov(fov, aspect), aspect, near)
}

/// The reversed-depth perspective for vertical field of view `fovy`: D3D's with the near and far planes swapped.
pub fn reversed(fovy: f32, aspect: f32, near: f32) -> Mat4 {
    directx::perspective(fovy, aspect, FAR, near)
}

/// A world matrix as stored in `.sgc`: three rows of `[R | t]`. The rotation and the translation act on
/// column vectors, so the matrix takes node positions to the car's frame. (The loader stores the same
/// numbers transposed, rbr-recomp `notes/formats/sgc.md`, section 4.)
pub fn affine(rows: [[f32; 4]; 3]) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(rows[0][0], rows[1][0], rows[2][0], 0.0),
        Vec4::new(rows[0][1], rows[1][1], rows[2][1], 0.0),
        Vec4::new(rows[0][2], rows[1][2], rows[2][2], 0.0),
        Vec4::new(rows[0][3], rows[1][3], rows[2][3], 1.0),
    )
}

/// Replaces an interactive record's placement rotation and translation with its live solid pose while
/// preserving the record's scale (and any reflected axis). At the solid's load pose this returns the
/// `.lbs` matrix unchanged; both matrices use left-handed, y-up stage coordinates.
pub fn interactive_pose_matrix(placement: [f32; 16], position: Vec3, orientation: Quat) -> Mat4 {
    let base = Mat4::from_cols_array(&placement);
    let columns = [base.x_axis.truncate(), base.y_axis.truncate(), base.z_axis.truncate()];
    let mut scale = Vec3::new(columns[0].length(), columns[1].length(), columns[2].length());
    if Mat3::from_cols(columns[0], columns[1], columns[2]).determinant() < 0.0 {
        scale.x = -scale.x;
    }
    let base_rotation =
        Quat::from_mat3(&Mat3::from_cols(columns[0] / scale.x, columns[1] / scale.y, columns[2] / scale.z)).normalize();
    Mat4::from_translation(position)
        * Mat4::from_quat(orientation.normalize())
        * Mat4::from_quat(base_rotation.conjugate())
        * Mat4::from_translation(-base.w_axis.truncate())
        * base
}

/// Stage space from the physics frame: `(x, y, z)` to `(x, z, y)`.
pub fn physics_to_stage() -> Mat4 {
    Mat4::from_cols(Vec4::X, Vec4::Z, Vec4::Y, Vec4::W)
}

/// Converts the physics body's z-up pose to the renderer's `.lbs` y-up stage axes.
pub fn physics_pose_to_stage(position: Vec3, orientation: Quat) -> (Vec3, Quat) {
    let swap = physics_to_stage();
    let rotation = swap * Mat4::from_quat(orientation.normalize()) * swap;
    (swap.transform_point3(position), Quat::from_mat4(&rotation).normalize())
}

/// A mesh's placement in its car: `F * L`. `F` is the node's chained world matrix; `L` is its transform block
/// composed as the loader does (`NLB_ComposeTransformEH`, 0x1008d420, and `CGeomObject::Update`: `m_world = m_local *
/// animObj`; the same composition as `rbr_physics::load::car_grids::compose`). `words` are the block's 14 words
/// (`sgc::Transform::words`), read by `HH_H7ReadTransform` (0x1000c7a0) as a translation (0-2), an angle and axis
/// (3, 4-6), a scale (7-9) and a second angle and axis (10, 11-13); each rotation is the quaternion
/// `axis * sin(angle / 2), cos(angle / 2)`. On column vectors `L = T * R1 * R2 * S * R2^-1` (the loader's row-vector
/// `R2^-1 * S * R2 * R1 * T`). On the body the block is the identity; the wheels turn by pi about y and the doors
/// undo their node's rotation (rbr-recomp `notes/formats/sgc.md`, section 5.2).
pub fn part_to_car(world: [[f32; 4]; 3], words: [f32; 14]) -> Mat4 {
    let rotation = |angle: f32, axis: [f32; 3]| {
        let (sin, cos) = (angle * 0.5).sin_cos();
        Mat4::from_quat(Quat::from_xyzw(axis[0] * sin, axis[1] * sin, axis[2] * sin, cos))
    };
    let r1 = rotation(words[3], [words[4], words[5], words[6]]);
    let r2 = rotation(words[10], [words[11], words[12], words[13]]);
    let scale = Mat4::from_scale(Vec3::new(words[7], words[8], words[9]));
    affine(world) * Mat4::from_translation(Vec3::new(words[0], words[1], words[2])) * r1 * r2 * scale * r2.inverse()
}

/// `HingeDefaultAngle` of the side doors in every retail car's `common.lsp` (radians).
pub const HINGE_DEFAULT_ANGLE: f32 = 0.01;

/// A door panel at rest: its placement ([`part_to_car`], which keeps the car's orientation: the block's rotation
/// undoes the node's in all eight retail cars), turned by the hinge's default angle about the exterior door's hinge
/// point `hinge` (its node translation). `side` is +1 for the left door and -1
/// for the right, the sign of `vecHingeAxis_RF`'s z. The rotation is about the car's up axis, mirrored for the
/// right door.
pub fn hinged_panel_to_car(world: [[f32; 4]; 3], words: [f32; 14], hinge: [f32; 3], side: f32) -> Mat4 {
    let hinge = Vec3::from(hinge);
    Mat4::from_translation(hinge)
        * Mat4::from_rotation_y(-side * HINGE_DEFAULT_ANGLE)
        * Mat4::from_translation(-hinge)
        * part_to_car(world, words)
}

/// Whether a transform mirrors (negative determinant). The triangles of a mirrored part are
/// reversed, so their outer side stays the front face (`cull_mode` back). The retail mirrored parts
/// are the right-hand mudflap and its counterparts; the game's own handling is not checked.
pub fn is_mirror(transform: Mat4) -> bool {
    transform.determinant() < 0.0
}

/// The car's local frame to the stage. The car's front is its `-z` (the front lights and wheels are at
/// negative z, `notes/formats.md` "Car models"), so `forward` is the car's `-z` and up is `+y`. The car's `+x`
/// is its left (the `lf` parts), which keeps the frame the left-handed camera's: a proper rotation,
/// no mirror. `origin` is the car's origin on the stage.
pub fn car_to_stage(origin: Vec3, forward: Vec3) -> Mat4 {
    let forward = forward.normalize_or(Vec3::Z);
    let right = Vec3::Y.cross(forward);
    Mat4::from_cols((-right).extend(0.0), Vec3::Y.extend(0.0), (-forward).extend(0.0), origin.extend(1.0))
}

/// The world-to-view matrix for sky drawn at infinity: the rotation for `forward`, with the eye at the
/// origin.
pub fn sky_view_matrix(forward: Vec3) -> Mat4 {
    view_matrix(Vec3::ZERO, forward)
}

/// Reorders a D3DCOLOR's bytes (B, G, R, A in memory) to RGBA.
pub fn rgba_from_d3dcolor(bytes: [u8; 4]) -> [u8; 4] {
    [bytes[2], bytes[1], bytes[0], bytes[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn d3dcolor_bytes_become_rgba() {
        // ARGB 0x80FF8010 is stored as 10 80 FF 80: blue 0x10, green 0x80, red 0xFF, alpha 0x80.
        assert_eq!(rgba_from_d3dcolor([0x10, 0x80, 0xff, 0x80]), [0xff, 0x80, 0x10, 0x80]);
    }

    #[test]
    fn car_front_points_along_forward_without_mirroring() {
        let forward = Vec3::new(0.6, 0.0, -0.8);
        let frame = car_to_stage(Vec3::new(10.0, 2.0, -5.0), forward);
        let front = frame.transform_point3(Vec3::new(0.0, 0.0, -1.0));
        assert!((front - (Vec3::new(10.0, 2.0, -5.0) + forward)).length() < 1e-5);
        assert!(frame.determinant() > 0.9, "a proper rotation");
        // The car's left (+x) is the camera's left when the camera looks along forward.
        let left = frame.transform_vector3(Vec3::X);
        assert!(left.dot(Vec3::Y.cross(forward)) < 0.0);
    }

    /// The block's words with translation `t`, the first rotation `angle` about `axis` and a unit scale.
    fn block(t: [f32; 3], angle: f32, axis: [f32; 3]) -> [f32; 14] {
        let mut w = [0.0; 14];
        w[0..3].copy_from_slice(&t);
        w[3] = angle;
        w[4..7].copy_from_slice(&axis);
        w[7..10].copy_from_slice(&[1.0; 3]);
        w[11] = 1.0;
        w
    }

    #[test]
    fn mesh_offset_is_added_before_the_world_matrix() {
        let world = [[1.0, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.25], [0.0, 0.0, 1.0, -3.0]];
        let placed = part_to_car(world, block([-1.0, 0.0, 0.0], 0.0, [0.0, 1.0, 0.0]));
        assert_eq!(placed.transform_point3(Vec3::new(1.0, 2.0, 3.0)), Vec3::new(0.5, 2.25, 0.0));
    }

    #[test]
    fn block_rotation_turns_the_vertices_before_the_offset() {
        // COROLLA's e_wheel_tarmac_lf: pi about -y, then 0.059 along x, at the node (0.733, 0.160, -2.496). The rim
        // (model x -0.034, facing -x) ends up on the wheel's outer side, facing out.
        let world = [[1.0, 0.0, 0.0, 0.733], [0.0, 1.0, 0.0, 0.160], [0.0, 0.0, 1.0, -2.496]];
        let placed = part_to_car(world, block([0.059, 0.0, 0.0], PI, [0.0, -1.0, 0.0]));
        let rim = placed.transform_point3(Vec3::new(-0.034, 0.0, 0.0));
        assert!((rim - Vec3::new(0.826, 0.160, -2.496)).length() < 1e-5);
        assert!(placed.transform_vector3(Vec3::NEG_X).x > 0.99);
        // COROLLA's e_door_r: the block's quarter turn about +z undoes the node's, so the panel keeps the car's axes.
        let door = [[0.0, 1.0, 0.0, -0.819], [-1.0, 0.0, 0.0, 0.485], [0.0, 0.0, 1.0, -2.061]];
        let placed = part_to_car(door, block([-0.543, 0.819, -1.415], FRAC_PI_2, [0.0, 0.0, 1.0]));
        assert!(placed.abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, 1.028, -3.476)), 1e-3));
    }

    #[test]
    fn stored_matrix_maps_column_vectors() {
        let m = affine([[1.0, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.25], [0.0, 0.0, -1.0, 3.0]]);
        assert_eq!(m.transform_point3(Vec3::new(1.0, 2.0, 3.0)), Vec3::new(1.5, 2.25, 0.0));
        assert!(is_mirror(m));
        assert!(!is_mirror(affine([[0.0, 1.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]])));
    }

    #[test]
    fn the_horizontal_field_of_view_sets_the_vertical_one_by_the_aspect() {
        let degrees = |fov: f32, aspect: f32| vertical_fov(fov.to_radians(), aspect).to_degrees();
        assert!((degrees(50.0, 16.0 / 9.0) - 29.3950).abs() < 1e-3, "{}", degrees(50.0, 16.0 / 9.0));
        assert!((degrees(50.0, 4.0 / 3.0) - 38.5526).abs() < 1e-3, "{}", degrees(50.0, 4.0 / 3.0));
        assert!((degrees(50.0, 1.0) - 50.0).abs() < 1e-4);
        // The clip x of a point on the image's edge is 1 at any aspect.
        for aspect in [16.0 / 9.0, 4.0 / 3.0, 21.0 / 9.0] {
            let edge = Vec3::new((25.0f32).to_radians().tan(), 0.0, 1.0) * 10.0;
            let clip = projection(50.0f32.to_radians(), aspect) * edge.extend(1.0);
            assert!((clip.x / clip.w - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn interactive_pose_reuses_the_placement_matrix_convention() {
        let position = Vec3::new(12.0, 3.0, -8.0);
        let orientation = Quat::from_rotation_y(0.7);
        let placement = Mat4::from_scale_rotation_translation(Vec3::new(1.2, 0.8, 1.1), orientation, position);
        let at_load = interactive_pose_matrix(placement.to_cols_array(), position, orientation);
        assert!(at_load.abs_diff_eq(placement, 1e-5));

        let moved =
            interactive_pose_matrix(placement.to_cols_array(), Vec3::new(4.0, 5.0, 6.0), Quat::from_rotation_y(-0.3));
        assert!(moved.transform_point3(Vec3::ZERO).abs_diff_eq(Vec3::new(4.0, 5.0, 6.0), 1e-5));
        assert!((moved.transform_vector3(Vec3::X).length() - 1.2).abs() < 1e-5);
    }

    #[test]
    fn physics_pose_uses_the_recorded_z_up_to_stage_y_up_swap() {
        let (position, orientation) =
            physics_pose_to_stage(Vec3::new(1.0, 2.0, 3.0), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        assert_eq!(position, Vec3::new(1.0, 3.0, 2.0));
        let moved_x = Mat4::from_quat(orientation).transform_vector3(Vec3::X);
        assert!(moved_x.abs_diff_eq(Vec3::Z, 1e-5));
    }

    #[test]
    fn view_space_is_left_handed_and_unmirrored() {
        // Looking down +z with y up: +x is on the right of the image (left-handed).
        let view = view_matrix(Vec3::ZERO, Vec3::Z);
        let right = view.transform_point3(Vec3::X);
        let up = view.transform_point3(Vec3::Y);
        assert!(right.x > 0.0 && right.y.abs() < 1e-6);
        assert!(up.y > 0.0 && up.x.abs() < 1e-6);
        let ahead =
            projection(FOV_DEGREES.to_radians(), 1.0) * view.transform_point3(Vec3::new(0.0, 0.0, 10.0)).extend(1.0);
        assert!(ahead.z / ahead.w >= 0.0 && ahead.z / ahead.w <= 1.0);
    }
}
