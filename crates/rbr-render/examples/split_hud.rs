//! Captures the split panel (`rbr_render::hud::splits`) over the stage: `split_hud <out-dir>` writes `split.png` (split 1
//! passed 3 s of stage time ago) and `finish.png` (the finish panel). The numbers are the tanner-corolla boot's
//! (`rbr-standalone --boot --recording drive-tanner-corolla.bin --stage track-54_E --steps 40000`): splits 87.02 s and
//! 169.24 s against 57.00 s and 119.00 s, the finish at 257.68 s with 10 s of penalties, against 185.00 s. The records
//! are typed in here, not read from a live process; the stage behind them is drawn with an identity camera.

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use glam::Mat4;
use rbr_formats::data::Data;
use rbr_recording::hud::RaceHud;
use rbr_recording::splits::{Mark, Split, Splits, Standing, Verdict};
use rbr_render::hud::{Overlay, Readout};
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, Selection};

const SIZE: (u32, u32) = (1280, 720);
const FINISH: f32 = 257.68;
const PENALTIES: f32 = 10.0;

/// The record after split 1, or after the finish when `finished`.
fn record(elapsed: f32, finished: bool) -> Splits {
    let second = finished.then_some(169.24);
    Splits {
        finished,
        elapsed,
        race_time: FINISH,
        penalties: PENALTIES,
        reference_finish: Some(185.0),
        splits: vec![
            Split { time: Some(87.02), reference: Some(57.0) },
            Split { time: second, reference: Some(119.0) },
        ],
        ..Splits::default()
    }
}

fn readout(splits: Splits, race: RaceHud) -> Readout {
    Readout {
        view: "chase",
        view_keys: String::new(),
        time: f64::from(splits.elapsed),
        speed: 0.0,
        mph: false,
        fps: None,
        gear: None,
        rpm: None,
        throttle: 0.0,
        brake: 0.0,
        handbrake: 0.0,
        clutch: 0.0,
        steering: 0.0,
        race: Some(race),
        splits: Some(splits),
        state: Default::default(),
        pace_3d: None,
    }
}

/// The finished record with a standing: the stage record of 3:05.00 by "Richard Burns", a personal best of 4:12.50
/// (its own splits and the 257.68 s race) and the `verdict`.
fn with_standing(mut record: Splits, verdict: Verdict) -> Splits {
    let mark = |car, splits, time, holder: &str| Mark { car, splits, time, holder: holder.to_owned() };
    record.standing = Some(Standing {
        record: mark(0, [57.0, 119.0], 185.0, "Richard Burns"),
        personal: Some(mark(6, [88.0, 170.0], 252.5, "")),
        verdict,
    });
    record
}

fn write_png(path: &Path, (width, height): (u32, u32), rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args_os().nth(1).ok_or("output directory required")?);
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let stage = stage::load(&data.install().join("Maps"), "track-54_E", Selection::default())?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());
    let mut overlay = Overlay::new(&gpu.device, format);
    let captures = [
        (
            "split.png",
            readout(
                record(90.0, false),
                RaceHud { time: 90.0, running: true, penalties: PENALTIES, ..RaceHud::default() },
            ),
        ),
        (
            "finish.png",
            readout(
                record(FINISH - PENALTIES, true),
                RaceHud { time: FINISH, penalties: PENALTIES, ..RaceHud::default() },
            ),
        ),
        (
            "finish-personal-best.png",
            readout(
                with_standing(record(FINISH - PENALTIES, true), Verdict::PersonalBest),
                RaceHud { time: FINISH, penalties: PENALTIES, ..RaceHud::default() },
            ),
        ),
        (
            "finish-stage-record.png",
            readout(
                with_standing(record(FINISH - PENALTIES, true), Verdict::StageRecord),
                RaceHud { time: FINISH, penalties: PENALTIES, ..RaceHud::default() },
            ),
        ),
    ];
    for (name, readout) in captures {
        let (_, pixels) = renderer.render_offscreen_with(SIZE, Mat4::IDENTITY, Mat4::IDENTITY, |encoder, target| {
            overlay.draw(&gpu.device, &gpu.queue, encoder, target, SIZE, &readout);
        })?;
        write_png(&out.join(name), SIZE, &pixels)?;
        eprintln!("{name}: written");
    }
    Ok(())
}
