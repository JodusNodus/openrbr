use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_recording::{pose, Frames};
use rbr_render::camera::FreeFly;
use rbr_render::coords::{physics_pose_to_stage, FOV_DEGREES};
use rbr_render::live::Live;
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, InteractiveInstance, InteractivePose, Selection};
use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const TARGET_PRE: u32 = 7915;
const SIZE: (u32, u32) = (1280, 720);

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let recording = PathBuf::from(args.next().ok_or("recording path required")?);
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    let bytes = fs::read(&recording)?;
    let frames = Frames::new(&bytes).collect::<Result<Vec<_>, _>>()?;
    let target = frames.iter().find(|frame| frame.pre == TARGET_PRE).ok_or("target step is absent")?;

    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, "track-44_E", Selection::default())?;
    stage.load_textures(&maps)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let gpu = renderer::request_gpu(&instance, None)?;
    let mut renderer = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm, &stage, Default::default());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).ok_or("repo root")?;
    let standalone = root.join("scripts/cachy-rust.sh");
    let command = vec![
        standalone.to_string_lossy().into_owned(),
        "rbr-standalone".into(),
        "--live".into(),
        "--boot".into(),
        "--stage".into(),
        "track-44_E".into(),
        "--car".into(),
        "1".into(),
    ];
    let mut live = Live::spawn(&command).map_err(std::io::Error::other)?;
    // The live boot performs the recording's four 40-substep load-settle frames internally and starts at step 160.
    // Send the first recorded one-substep control directly; `first_frame` would consume it with default controls.
    for frame in frames.iter().filter(|frame| (160..TARGET_PRE).contains(&frame.pre)) {
        let live_frame = live.step_payload(&frame.control).map_err(std::io::Error::other)?;
        compare_steps(&live_frame, frame.pre, frame.post)?;
    }
    capture(&mut renderer, &mut live, target, &output.join("sign-before.png"))?;
    let hit = live.step_payload(&target.control).map_err(std::io::Error::other)?;
    compare_steps(&hit, target.pre, target.post)?;
    capture(&mut renderer, &mut live, target, &output.join("sign-after.png"))?;
    eprintln!("hit step {} -> {}, {} sticky interactive poses", target.pre, target.post, live.interactive_poses.len());
    print_moved(&stage.interactive, &live.interactive_poses);
    Ok(())
}

fn compare_steps(bytes: &[u8], expected_pre: u32, expected_post: u32) -> Result<(), Box<dyn Error>> {
    let actual = Frames::new(bytes).next().ok_or("live frame missing")??;
    if (actual.pre, actual.post) != (expected_pre, expected_post) {
        return Err(format!(
            "live step {} -> {} does not match recording {} -> {}",
            actual.pre, actual.post, expected_pre, expected_post
        )
        .into());
    }
    Ok(())
}

fn capture(
    renderer: &mut Renderer,
    live: &mut Live,
    frame: &rbr_recording::Frame<'_>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let car = pose::car_pose(frame.car).ok_or("recorded car pose missing")?;
    let car_position = Vec3::new(car.position[0], car.position[2], car.position[1]);
    let car_forward = car.rotate([0.0, -1.0, 0.0]);
    let forward = Vec3::new(car_forward[0], car_forward[2], car_forward[1]).normalize();
    let eye = car_position - forward * 7.0 + Vec3::Y * 2.5;
    let aim = car_position + forward * 3.0 + Vec3::Y * 1.0;
    let camera = FreeFly::looking_along(eye, aim - eye);
    renderer.set_interactive_poses(&live.interactive_poses);
    let (stats, pixels) = renderer.render_offscreen(
        SIZE,
        camera.view_projection(FOV_DEGREES.to_radians(), SIZE.0 as f32 / SIZE.1 as f32),
        camera.sky_view_projection(FOV_DEGREES.to_radians(), SIZE.0 as f32 / SIZE.1 as f32),
    )?;
    fs::create_dir_all(path.parent().ok_or("screenshot has no parent")?)?;
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), SIZE.0, SIZE.1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    eprintln!("{}: {stats:?}", path.display());
    Ok(())
}

fn print_moved(instances: &[InteractiveInstance], poses: &std::collections::HashMap<u32, InteractivePose>) {
    for instance in instances {
        let Some(pose) = poses.get(&instance.handle) else {
            continue;
        };
        let original = Mat4::from_cols_array(&instance.placement).w_axis.truncate();
        let (position, _) = physics_pose_to_stage(
            Vec3::from(pose.position),
            glam::Quat::from_xyzw(pose.orientation[0], pose.orientation[1], pose.orientation[2], pose.orientation[3]),
        );
        let moved = (position - original).length();
        if moved > 0.05 {
            eprintln!("moved handle {:#x}: {moved:.3} m", instance.handle);
        }
    }
}
