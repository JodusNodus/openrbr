//! Captures the live HUD (`rbr_render::hud`) over a recorded drive played through `rbr-standalone --live --boot`:
//! `hud_capture <recording> <stage> <car> <out-dir> [last-step]` sends the recording's controls from step 160 on (the
//! live boot runs the load settle itself), feeds the HUD from the live records, and writes `start.png` (the first
//! frame), `pace.png` (the first pace note fully shown), `split.png` (2 s after the first split), `message.png` (the
//! first lit lamp or message), `damage.png` (3 s after a damage zone first reaches 0.5) and `last.png`. The camera is a chase view from the
//! recorded car pose. Run from the repository root with `RBR_DATA` set, after `cargo build --release -p
//! rbr-standalone`.

use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use glam::Vec3;
use rbr_formats::data::Data;
use rbr_recording::{pose, Frames};
use rbr_render::camera::FreeFly;
use rbr_render::coords::FOV_DEGREES;
use rbr_render::hud::model::Lamp;
use rbr_render::hud::{Overlay, Readout};
use rbr_render::live::Live;
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, Selection};

const SIZE: (u32, u32) = (1280, 720);
/// The live boot's first step (four 40-substep settle frames).
const FIRST: u32 = 160;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let recording = PathBuf::from(args.next().ok_or("recording path required")?);
    let stage_name = args.next().ok_or("stage required")?;
    let car = args.next().ok_or("car index required")?;
    let out = PathBuf::from(args.next().ok_or("output directory required")?);
    let last: u32 = args.next().map_or(Ok(u32::MAX), |s| s.parse())?;
    fs::create_dir_all(&out)?;
    let bytes = fs::read(&recording)?;
    let frames = Frames::new(&bytes).collect::<Result<Vec<_>, _>>()?;

    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, &stage_name, Selection::default())?;
    stage.load_textures(&maps)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());
    let mut overlay = Overlay::new(&gpu.device, format);

    let standalone = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/rbr-standalone");
    let command: Vec<String> =
        [standalone.to_str().unwrap(), "--live", "--boot", "--stage", &stage_name, "--car", &car]
            .map(str::to_owned)
            .to_vec();
    let mut live = Live::spawn(&command).map_err(std::io::Error::other)?;
    let mut taken = Vec::new();
    let mut split_at = None;
    let mut hit_at = None;
    let drive: Vec<_> = frames.iter().filter(|f| f.pre >= FIRST && f.pre < last).collect();
    for (k, frame) in drive.iter().enumerate() {
        live.step_payload(&frame.control).map_err(std::io::Error::other)?;
        let state = live.hud_model.view();
        let mut want = Vec::new();
        if k == 0 {
            want.push("start");
        }
        if state.note.is_some_and(|(_, a)| a >= 1.0) && !taken.contains(&"pace") {
            want.push("pace");
        }
        let passed = live.splits.as_ref().and_then(|s| s.splits.first()?.time);
        if passed.is_some() && split_at.is_none() {
            split_at = Some(k);
            eprintln!("step {}: split 1 passed at {:?}", frame.pre, passed);
        }
        if split_at.is_some_and(|at| k == at + 200) {
            want.push("split");
        }
        let alarm = state.message.is_some() || state.lamps.is_some_and(|l| l.iter().any(|l| *l != Lamp::Off));
        if alarm && !taken.contains(&"message") {
            eprintln!("step {}: lamps {:?}, message {:?}", frame.pre, state.lamps, state.message);
            want.push("message");
        }
        let damage = live.damage.zones.iter().copied().fold(0.0, f32::max);
        if damage >= 0.5 && hit_at.is_none() {
            hit_at = Some(k);
            eprintln!("step {}: a zone at {damage:.2}", frame.pre);
        }
        if hit_at.is_some_and(|at| k == at + 300) {
            want.push("damage");
        }
        if k + 1 == drive.len() {
            want.push("last");
        }
        for name in want {
            capture(&mut renderer, &mut overlay, &gpu, &live, frame, &out.join(format!("{name}.png")))?;
            eprintln!(
                "step {}: {name}.png (note {:?}, damage {damage:.2})",
                frame.pre,
                state.note.map(|n| n.0.symbol.kind)
            );
            taken.push(name);
        }
    }
    Ok(())
}

/// The viewer's readout for the live frame (`viewer.rs`'s `readout`).
fn readout(live: &Live, frame: &rbr_recording::Frame<'_>) -> Readout {
    let [throttle, brake, handbrake, clutch, steering, ..] = frame.control_f32s();
    let hud = live.hud;
    Readout {
        view: "chase",
        adjust: None,
        view_keys: String::new(),
        time: f64::from(frame.pre) * 0.007,
        speed: hud.speed,
        mph: false,
        fps: None,
        gear: Some(hud.gear_label()),
        rpm: Some(hud.rpm()),
        throttle,
        brake,
        handbrake,
        clutch,
        steering,
        race: hud.race,
        splits: live.splits.clone(),
        state: live.hud_model.view(),
        pace_3d: None,
    }
}

fn capture(
    renderer: &mut Renderer,
    overlay: &mut Overlay,
    gpu: &renderer::Gpu,
    live: &Live,
    frame: &rbr_recording::Frame<'_>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let car = pose::car_pose(frame.car).ok_or("recorded car pose missing")?;
    let position = Vec3::new(car.position[0], car.position[2], car.position[1]);
    let forward = car.rotate([0.0, -1.0, 0.0]);
    let forward = Vec3::new(forward[0], forward[2], forward[1]).normalize();
    let eye = position - forward * 7.0 + Vec3::Y * 2.5;
    let camera = FreeFly::looking_along(eye, position + forward * 3.0 + Vec3::Y - eye);
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    let readout = readout(live, frame);
    let (_, pixels) = renderer.render_offscreen_with(
        SIZE,
        camera.view_projection(FOV_DEGREES.to_radians(), aspect),
        camera.sky_view_projection(FOV_DEGREES.to_radians(), aspect),
        |encoder, target| overlay.draw(&gpu.device, &gpu.queue, encoder, target, SIZE, &readout),
    )?;
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), SIZE.0, SIZE.1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
