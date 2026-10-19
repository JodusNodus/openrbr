//! Captures the B10 HUD states (`rbr_render::hud`): `hud_states <out-dir>` writes
//! - from scripted live drives on `rbr-standalone --live --boot` (track-54_E, COROLLA): `wrong-way.png` (the car
//!   turned round and driven back down the stage until the banner is up) and `false-start-warning.png` (a
//!   championship start, where the live boot's early jump start counts as the rally's first false start);
//! - typed states over the stage: `result-page.png` (the quick rally's result page with a new stage record),
//!   `retired.png` (end page 0x12), `helicopter.png` (the rescue helicopter flying) and
//!   `false-start-warning-{2,3}.png`.
//!
//! Run from the repository root with `RBR_DATA` and `RBR_PATCH` set, after `cargo build --release -p rbr-standalone`.

use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_recording::camera::MODE_START_ORBIT;
use rbr_recording::splits::{Mark, Split, Splits, Standing, Verdict};
use rbr_recording::{pose, Frames};
use rbr_render::camera::FreeFly;
use rbr_render::coords::FOV_DEGREES;
use rbr_render::hud::model::HudView;
use rbr_render::hud::{Overlay, Readout};
use rbr_render::live::{Controls, Live, RaceHud, SELECT_GEAR};
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, Selection};

const SIZE: (u32, u32) = (1280, 720);
const STAGE: &str = "track-54_E";

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    fs::create_dir_all(&out)?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, STAGE, Selection::default())?;
    stage.load_textures(&maps)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());
    let mut overlay = Overlay::new(&gpu.device, format);
    let mut shoot = |readout: &Readout, camera: Option<FreeFly>, path: &Path| -> Result<(), Box<dyn Error>> {
        let aspect = SIZE.0 as f32 / SIZE.1 as f32;
        let fov = FOV_DEGREES.to_radians();
        let (view, sky) = camera.map_or((Mat4::IDENTITY, Mat4::IDENTITY), |c| {
            (c.view_projection(fov, aspect), c.sky_view_projection(fov, aspect))
        });
        let (_, pixels) = renderer.render_offscreen_with(SIZE, view, sky, |encoder, target| {
            overlay.draw(&gpu.device, &gpu.queue, encoder, target, SIZE, readout)
        })?;
        write(path, &pixels)?;
        eprintln!("{}", path.display());
        Ok(())
    };

    // Turned round: up the road for room behind, stopped, a few cuts (forward on right lock, back on left lock) until
    // a back cut leaves the odometer lower, then forward at about 3.4 m/s until the banner is up.
    let mut live = spawn(&[])?;
    let mut shot = false;
    let forward = Controls { flags: SELECT_GEAR, gear: 2, ..Controls::default() };
    let back = Controls { flags: SELECT_GEAR, gear: 0, ..Controls::default() };
    let (mut turned, mut cut_from) = (false, 0.0);
    const CUTS: usize = 1500;
    for k in 0..8000 {
        let odometer = live.hud_model.view().odometer.map_or(0.0, |o| o.0);
        let cut = (k.max(CUTS) - CUTS) / 300;
        if !turned && k >= CUTS && (k - CUTS).is_multiple_of(300) {
            if !cut.is_multiple_of(2) && odometer < cut_from - 1.0 {
                turned = true;
            }
            cut_from = odometer;
        }
        let controls = if k < 1100 {
            Controls { throttle: 0.5, ..forward }
        } else if k < CUTS {
            Controls { brake: 1.0, ..forward }
        } else if turned {
            Controls { throttle: if live.hud.speed < 3.4 { 0.45 } else { 0.0 }, ..forward }
        } else if cut.is_multiple_of(2) {
            Controls { throttle: 0.35, steering: 1.0, ..forward }
        } else {
            Controls { throttle: 0.35, steering: -1.0, ..back }
        };
        let frame = drive(&mut live, &controls)?;
        if live.hud_model.view().wrong_way.is_some_and(|a| a >= 0.95) {
            eprintln!("payload {k}: wrong way, speed {:.1} m/s", live.hud.speed);
            shoot(&readout(&live, &controls), chase(&frame), &out.join("wrong-way.png"))?;
            shot = true;
            break;
        }
    }

    if !shot {
        eprintln!("the scripted drive never went the wrong way");
    }

    // A championship start: the boot's jump start is the rally's first.
    let mut live = spawn(&["--championship", "rookie"])?;
    for k in 0..400 {
        let controls = Controls { throttle: 0.3, flags: SELECT_GEAR, gear: 2, ..Controls::default() };
        let frame = drive(&mut live, &controls)?;
        if live.hud_model.view().warning.is_some_and(|(_, a)| a >= 0.5) {
            eprintln!("payload {k}: false-start warning {:?}", live.hud_model.view().warning);
            shoot(&readout(&live, &controls), chase(&frame), &out.join("false-start-warning.png"))?;
            break;
        }
    }

    // Typed states.
    let base = Readout {
        view: "chase",
        adjust: None,
        view_keys: String::new(),
        time: 0.0,
        speed: 0.0,
        mph: false,
        fps: None,
        gear: Some("2".into()),
        rpm: Some(3000.0),
        throttle: 0.0,
        brake: 0.0,
        handbrake: 0.0,
        clutch: 0.0,
        steering: 0.0,
        race: Some(RaceHud { time: 182.4, countdown: -180.0, penalties: 0.0, running: true, jump_start: false }),
        splits: None,
        state: HudView::default(),
        pace_3d: None,
    };
    let split = |time, reference| Split { time: Some(time), reference: Some(reference) };
    let finished = Splits {
        finished: true,
        over: true,
        elapsed: 182.4,
        race_time: 182.4,
        penalties: 0.0,
        reference_finish: Some(185.0),
        splits: vec![split(57.1, 58.0), split(121.3, 122.0)],
        standing: Some(Standing {
            record: Mark { car: 6, splits: [57.1, 121.3], time: 182.4, holder: "thomas".into() },
            personal: Some(Mark { car: 6, splits: [57.1, 121.3], time: 182.4, holder: String::new() }),
            verdict: Verdict::StageRecord,
        }),
        ..Splits::default()
    };
    let state = HudView { end_page: Some(0x10), ..HudView::default() };
    shoot(&Readout { splits: Some(finished), state, ..base.clone() }, None, &out.join("result-page.png"))?;
    let retired = Splits { retired: true, over: true, race_time: 97.3, ..Splits::default() };
    let state = HudView { end_page: Some(0x12), helicopter: true, ..HudView::default() };
    shoot(&Readout { splits: Some(retired), state, ..base.clone() }, None, &out.join("retired.png"))?;
    let state = HudView { helicopter: true, ..HudView::default() };
    shoot(&Readout { state, ..base.clone() }, None, &out.join("helicopter.png"))?;
    for count in [2, 3] {
        let state = HudView { warning: Some((count, 0.5)), ..HudView::default() };
        shoot(&Readout { state, ..base.clone() }, None, &out.join(format!("false-start-warning-{count}.png")))?;
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
    let mut live = Live::spawn(&command)?;
    live.first_frame()?;
    while live.hud.camera.is_some_and(|c| c.mode == MODE_START_ORBIT) {
        live.step_payload(&Controls::default().payload())?;
    }
    Ok(live)
}

/// One payload of `controls`; the answer's frame.
fn drive(live: &mut Live, controls: &Controls) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(live.step_payload(&controls.payload())?)
}

/// The viewer's readout for a live frame (`viewer.rs`'s `readout`).
fn readout(live: &Live, controls: &Controls) -> Readout {
    let hud = live.hud;
    Readout {
        view: "chase",
        adjust: None,
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
        state: live.hud_model.view(),
        pace_3d: None,
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
