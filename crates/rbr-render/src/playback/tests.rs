use super::frames::{model_to_body, WHEEL_ROLL_AXIS};
use super::*;
use crate::coords::physics_to_stage;
use crate::damage::{self, Cracks};
use crate::game_camera::Follow;
use crate::views::{CarView, Mount, Mounts};
use glam::{Mat4, Vec3};
use rbr_recording::detach;
use rbr_recording::WHEELS;

#[test]
fn chase_look_default_keeps_the_chase_view_and_turns_around_the_target() {
    let offset = Vec3::new(0.0, 2.0, -6.0);
    assert!((ChaseLook::default().apply(offset) - offset).length() < 1e-5);
    let half_turn = ChaseLook { yaw: std::f32::consts::PI, ..ChaseLook::default() };
    assert!((half_turn.apply(offset) - Vec3::new(0.0, 2.0, 6.0)).length() < 1e-4);
    let mut far = ChaseLook::default();
    far.zoom_by(100.0);
    assert!((far.apply(offset).length() - offset.length() * LOOK_ZOOM_MAX).abs() < 1e-3);
}

const CAR_BYTES: usize = 0x1700;
const WHEEL_BYTES: usize = 0xa00;
/// The car block offsets of the position and the orientation (`rbr_recording::pose`).
const CAR_POSITION: usize = 0x140;
const CAR_ORIENTATION: usize = 0x100;

/// One recorded frame at `x` metres with `substeps` substeps, as `rbr_recording` reads it.
fn frame(post: u32, substeps: u32, x: f32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [rbr_recording::MAGIC, 0, post, substeps, CAR_BYTES as u32] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend(std::iter::repeat_n(0, rbr_recording::CONTROL_BYTES));
    let mut car = vec![0u8; CAR_BYTES];
    car[CAR_POSITION..CAR_POSITION + 4].copy_from_slice(&x.to_le_bytes());
    car[CAR_ORIENTATION + 12..CAR_ORIENTATION + 16].copy_from_slice(&1.0f32.to_le_bytes());
    bytes.extend(car);
    bytes.extend(std::iter::repeat_n(0, WHEELS * WHEEL_BYTES));
    bytes
}

fn playback(frames: &[Vec<u8>]) -> Playback {
    let bytes: Vec<u8> = frames.concat();
    Playback {
        drive: Drive::parse(&bytes).unwrap(),
        parts: Vec::new(),
        offset: Vec3::ZERO,
        clock: 0.0,
        paused: false,
        rate: 1.0,
        interior: false,
        zones: [0.0; damage::ZONES],
        cracks: Cracks::default(),
        lamp_points: Vec::new(),
        reverse: false,
        lamps: Default::default(),
        headlights: lights::Headlights::Main,
        detached: [None; detach::PARTS.len()],
        turn: None,
        wheel: None,
        hide_wheel: false,
        game_chase: Follow::new(crate::game_camera::Settings::default()),
        head: crate::head::Head::new(true),
        probe: None,
        shadow: None,
        mirrors: Vec::new(),
    }
}

#[test]
fn model_front_and_up_map_to_the_body_frame() {
    let m = model_to_body();
    assert!(m.transform_vector3(Vec3::NEG_Z).abs_diff_eq(Vec3::NEG_Y, 1e-6));
    assert!(m.transform_vector3(Vec3::Y).abs_diff_eq(Vec3::Z, 1e-6));
    // The model to the stage is a proper rotation, as the static placement is, so the outer faces stay front faces.
    let to_stage = physics_to_stage() * m;
    assert!(!crate::coords::is_mirror(to_stage));
    assert!(!crate::coords::is_mirror(crate::coords::car_to_stage(Vec3::ZERO, Vec3::Z)));
}

#[test]
fn a_live_frame_of_several_periods_says_so() {
    let mut late = frame(2, 3, 0.0);
    let input = rbr_recording::input::Input { frames: 3, ..Default::default() }.to_bytes();
    late[rbr_recording::HEADER_BYTES..rbr_recording::HEADER_BYTES + rbr_recording::CONTROL_BYTES]
        .copy_from_slice(&input);
    let playback = playback(&[frame(1, 1, 0.0), late]);
    assert_eq!((playback.periods(0), playback.periods(1)), (1, 3), "a recording's frame is one period");
}

#[test]
fn physics_frame_maps_to_stage_by_swapping_y_and_z() {
    let p = physics_to_stage().transform_point3(Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(p, Vec3::new(1.0, 3.0, 2.0));
}

#[test]
fn a_wheel_roll_turns_the_top_forward() {
    // The top of the tyre (model +Y) moves toward the front (model -Z) for a small positive roll.
    let eps = 1e-3;
    let top = Mat4::from_axis_angle(WHEEL_ROLL_AXIS, eps).transform_point3(Vec3::Y);
    assert!(top.z < 0.0, "{top:?}");
}

#[test]
fn chase_has_its_own_field_of_view_and_the_views_the_setting() {
    let mut p = playback(&[frame(10, 1, 0.0)]);
    p.follow_cameras(None, false);
    let mount = |eye: Vec3, near| Mount { eye, target: eye + Vec3::NEG_Z, up: Vec3::Y, near };
    let mounts = Mounts {
        bonnet: mount(Vec3::new(0.0, 0.8, -2.8), 0.08),
        bumper: mount(Vec3::new(0.0, 0.2, -3.4), 0.05),
        cockpit: mount(Vec3::new(0.3, 1.0, -1.2), 0.19),
        nudges: Default::default(),
    };
    let fov = 50.0f32.to_radians();
    for view in CarView::ALL {
        let lens = p.lens(&mounts, view, ChaseLook::default(), fov);
        let expected = match view {
            CarView::Chase => CHASE_FOV_DEGREES.to_radians(),
            _ => fov,
        };
        assert_eq!(lens.fov, expected, "{view:?}");
        // The horizontal scale is the field of view's at any aspect; the vertical one follows the aspect.
        for aspect in [16.0 / 9.0, 4.0 / 3.0] {
            let projection = lens.projection(aspect);
            assert!((projection.x_axis.x - 1.0 / (expected * 0.5).tan()).abs() < 1e-5, "{view:?}");
            assert!((projection.y_axis.y - aspect * projection.x_axis.x).abs() < 1e-4, "{view:?}");
        }
    }
}

#[test]
fn frames_are_read_with_their_drive_time() {
    let p = playback(&[frame(10, 1, 0.0), frame(30, 2, 1.0), frame(31, 1, 2.0)]);
    assert_eq!(p.drive.ends, [1.0, 3.0, 4.0].map(|n| n * f64::from(rbr_recording::STEP_SECONDS)));
    assert_eq!(p.duration(), 4.0 * f64::from(rbr_recording::STEP_SECONDS));
    assert_eq!(p.sample().step, 10);
}

#[test]
fn stepping_and_seeking_move_whole_frames() {
    let mut p = playback(&[frame(10, 1, 0.0), frame(30, 2, 1.0), frame(31, 1, 2.0)]);
    p.step(1);
    assert!(p.paused);
    assert_eq!(p.index(), 1);
    p.step(5);
    assert_eq!(p.index(), 2);
    p.step(-9);
    assert_eq!(p.index(), 0);
    p.seek_to_step(30);
    assert_eq!(p.sample().step, 30);
    p.seek_to_step(999);
    assert_eq!(p.sample().step, 31);
}

#[test]
fn advance_follows_the_rate_and_stops_when_paused() {
    let mut p = playback(&[frame(10, 1, 0.0), frame(30, 2, 1.0), frame(31, 1, 2.0)]);
    p.rate = 2.0;
    p.advance(0.02);
    assert_eq!(p.index(), 2);
    p.restart();
    p.paused = true;
    p.advance(1.0);
    assert_eq!(p.index(), 0);
}

#[test]
fn the_position_is_the_body_in_stage_space() {
    let p = playback(&[frame(10, 1, 5.0)]);
    assert_eq!(p.position(), Vec3::new(5.0, 0.0, 0.0));
}
