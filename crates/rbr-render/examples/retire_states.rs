//! Captures a retire's screens (`rbr_render::hud::retire`): `retire_states <out-dir> [payloads]` drives a championship
//! stage at the champion level (the assistance gate shut) on `rbr-standalone --live --boot` (track-54_E, COROLLA) off
//! the road and on against whatever stops the car, calls for help there (not helped at that level: a retire code 4),
//! and writes what the race flow shows on the way:
//! - `fade.png`: the off-stage fade (a retire code 4) at half black, or `incident.png` for the rally incident's red;
//! - `message-box.png`: the retire's message box over the paused game;
//! - `retired-page.png`: the end page after the box's accept (0x17).
//!
//! What the drive does not reach is typed over its last frame, with the same names (`message-box-bystand.png`,
//! `message-box-offstage.png` for the boxes), and `failed-page.png`: page 0x18 (a finish in a car that can't drive).
//!
//! Run from the repository root with `RBR_DATA`, `RBR_PATCH` and `RBR_NGP_PHYSICS` set, after `cargo build --release -p rbr-standalone`.

use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_recording::camera::MODE_START_ORBIT;
use rbr_recording::flow::MessageBox;
use rbr_recording::splits::Splits;
use rbr_recording::{pose, Frames};
use rbr_render::camera::FreeFly;
use rbr_render::coords::FOV_DEGREES;
use rbr_render::hud::model::HudView;
use rbr_render::hud::{stock, Overlay, Readout};
use rbr_render::live::{Controls, Live, ACCEPT_MESSAGE, CALL_FOR_HELP, SELECT_GEAR};
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, Selection};

const SIZE: (u32, u32) = (1280, 720);
const STAGE: &str = "track-54_E";

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let payloads: usize = std::env::args().nth(2).map_or(Ok(4000), |n| n.parse())?;
    fs::create_dir_all(&out)?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, STAGE, Selection::default())?;
    stage.load_textures(&maps)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());
    let mut overlay = Overlay::with_hud(&gpu.device, format, Box::new(stock::Stock::new(&data)));
    let mut shoot = |readout: &Readout, camera: Option<FreeFly>, path: &Path| -> Result<(), Box<dyn Error>> {
        let aspect = SIZE.0 as f32 / SIZE.1 as f32;
        let fov = FOV_DEGREES.to_radians();
        let (view, sky) = camera.map_or((Mat4::IDENTITY, Mat4::IDENTITY), |c| {
            (c.view_projection(fov, aspect), c.sky_view_projection(fov, aspect))
        });
        renderer.set_start_clock(readout.state.start_clock);
        let (_, pixels) = renderer.render_offscreen_with(SIZE, view, sky, |encoder, target| {
            overlay.draw(&gpu.device, &gpu.queue, encoder, target, SIZE, readout)
        })?;
        write(path, &pixels)?;
        eprintln!("{}", path.display());
        Ok(())
    };

    let mut live = spawn(&["--championship", "champion", "--rally-next", "service"])?;
    let gear = Controls { flags: SELECT_GEAR, gear: 2, ..Controls::default() };
    let mut last = Vec::new();
    let mut controls = gear;
    let (mut tinted, mut boxed) = (false, false);
    for k in 0..payloads {
        // Up the road, then hard right off it and on.
        controls = Controls { throttle: 0.8, steering: if k < 700 { 0.0 } else { 0.7 }, ..gear };
        let view = live.hud_model.view();
        // Stuck: the call for help, which at the champion level is not helped and retires the car (code 4).
        if k >= 3000 && k % 500 == 0 && live.hud.speed < 0.5 {
            eprintln!("payload {k}: the call for help");
            controls = Controls { flags: CALL_FOR_HELP, ..Controls::default() };
        }
        if boxed {
            controls = Controls { flags: ACCEPT_MESSAGE, ..Controls::default() };
        }
        let frame = live.step_payload(&controls.payload())?;
        if Frames::new(&frame).next().is_some_and(|f| f.is_ok()) {
            last = frame.clone();
        }
        let view_after = live.hud_model.view();
        if !tinted && (view_after.black.is_some_and(|a| a >= 0.5) || view_after.red.is_some_and(|a| a >= 0.5)) {
            let name = if view_after.black.is_some() { "fade.png" } else { "incident.png" };
            eprintln!("payload {k}: {name}, red {:?}, black {:?}", view_after.red, view_after.black);
            shoot(&readout(&live, &controls), chase(&last), &out.join(name))?;
            tinted = true;
        }
        if !boxed && view_after.message_box.is_some() {
            eprintln!("payload {k}: the message box {:?} is up", view_after.message_box);
            shoot(&readout(&live, &controls), chase(&last), &out.join("message-box.png"))?;
            boxed = true;
        }
        if view_after.end_page.is_some() {
            eprintln!("payload {k}: the race ended, page {:#x?}", view_after.end_page);
            shoot(&readout(&live, &controls), chase(&last), &out.join("retired-page.png"))?;
            break;
        }
        if k % 2000 == 0 {
            eprintln!(
                "payload {k}: {:.1} m/s, helicopter {}, box {:?}",
                live.hud.speed, view.helicopter, view.message_box
            );
        }
    }
    // Typed over the last frame: what the drive did not reach.
    let base = readout(&live, &controls);
    let typed = |state: HudView, splits: Option<Splits>| Readout { splits, state, ..base.clone() };
    let view = live.hud_model.view();
    let shots = [
        ("incident.png", HudView { red: Some(0.6), ..view.clone() }, None),
        ("fade.png", HudView { black: Some(0.6), ..view.clone() }, None),
        ("message-box-bystand.png", HudView { message_box: Some(MessageBox::RetireBystand), ..view.clone() }, None),
        ("message-box-offstage.png", HudView { message_box: Some(MessageBox::RetireOffstage), ..view.clone() }, None),
        (
            "retired-page.png",
            HudView { end_page: Some(0x17), ..view.clone() },
            Some(Splits { retired: true, over: true, race_time: 97.3, ..Splits::default() }),
        ),
        (
            "failed-page.png",
            HudView { end_page: Some(0x18), service_next: true, ..view.clone() },
            Some(Splits { finished: true, over: true, race_time: 251.7, elapsed: 251.7, ..Splits::default() }),
        ),
    ];
    for (name, state, splits) in shots {
        let path = out.join(name);
        if !path.exists() {
            shoot(&typed(state, splits), chase(&last), &path)?;
        }
    }
    Ok(())
}

/// `rbr-standalone --live --boot` on the stage, past its start orbit, with `extra` arguments.
fn spawn(extra: &[&str]) -> Result<Live, Box<dyn Error>> {
    let standalone = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/rbr-standalone");
    let mut command: Vec<String> =
        [standalone.to_str().unwrap(), "--live", "--boot", "--stage", STAGE, "--car", "6", "--tyre", "4"]
            .map(str::to_owned)
            .to_vec();
    command.extend(extra.iter().map(|s| (*s).to_owned()));
    command.extend(["--ngp-physics".to_owned(), std::env::var("RBR_NGP_PHYSICS")?]);
    let mut live = Live::lockstep(&command)?;
    live.first_frame()?;
    while live.hud.camera.is_some_and(|c| c.mode == MODE_START_ORBIT) {
        live.step_payload(&Controls::default().payload())?;
    }
    Ok(live)
}

/// The viewer's readout for a live frame (`viewer.rs`'s `readout`).
fn readout(live: &Live, controls: &Controls) -> Readout {
    let hud = live.hud;
    Readout {
        view: "chase",
        view_keys: String::new(),
        time: 0.0,
        speed: hud.speed,
        mph: false,
        fps: None,
        gear: Some(hud.gear_label()),
        rpm: Some(hud.rpm()),
        throttle: controls.throttle,
        brake: controls.brake,
        handbrake: controls.handbrake,
        clutch: controls.clutch,
        steering: controls.steering,
        race: hud.race,
        splits: live.splits.clone(),
        state: live.hud_view(),
        parts: Default::default(),
    }
}

/// A chase camera behind the answer's car.
fn chase(frame: &[u8]) -> Option<FreeFly> {
    let frame = Frames::new(frame).next()?.ok()?;
    let car = pose::car_pose(frame.car)?;
    let position = Vec3::new(car.position[0], car.position[2], car.position[1]);
    let forward = car.rotate([0.0, -1.0, 0.0]);
    let forward = Vec3::new(forward[0], forward[2], forward[1]).normalize();
    let eye = position - forward * 7.0 + Vec3::Y * 2.5;
    Some(FreeFly::looking_along(eye, position + forward * 3.0 + Vec3::Y - eye))
}

fn write(path: &Path, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut png = png::Encoder::new(BufWriter::new(File::create(path)?), SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(pixels)?;
    Ok(())
}
