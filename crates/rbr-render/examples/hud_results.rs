//! Captures stage-result layouts without game data: `cargo run -p rbr-render --example hud_results -- OUT`.
//! An optional `--without-actions` captures the HUD alone for before/after comparisons.

use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;

use glam::Mat4;
use rbr_recording::menu::Menu;
use rbr_recording::splits::{Mark, Split, Splits, Standing};
use rbr_render::hud::model::HudView;
use rbr_render::hud::results::{self, ResultsMenu};
use rbr_render::hud::{self, Overlay, Readout};
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::Stage;

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let actions = !std::env::args().any(|a| a == "--without-actions");
    fs::create_dir_all(&out)?;
    let gpu = renderer::request_gpu(&renderer::instance(), None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut overlay = Overlay::new(&gpu.device, format);
    let stage = Stage {
        name: "HUD capture".into(),
        selection: Default::default(),
        meshes: Vec::new(),
        interactive: Vec::new(),
        texture_names: Default::default(),
        textures: Default::default(),
        start_digits: None,
        start: None,
        atmosphere: Default::default(),
        driveline: Vec::new(),
        report: Default::default(),
    };
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());
    let mark = Mark { splits: [69.85, 143.57], time: 233.68, holder: "Richard Burns".into(), ..Default::default() };
    let splits = Splits {
        finished: true,
        over: true,
        race_time: 238.0,
        penalties: 10.0,
        reference_finish: Some(mark.time),
        splits: mark.splits.iter().map(|&t| Split { time: Some(t + 1.0), reference: Some(t) }).collect(),
        standing: Some(Standing { record: mark, ..Default::default() }),
        ..Default::default()
    };
    let mut readout = Readout {
        view: "chase",
        view_keys: String::new(),
        time: 238.0,
        speed: 0.0,
        mph: false,
        fps: None,
        gear: Some("N".into()),
        rpm: Some(1000.0),
        throttle: 0.0,
        brake: 0.0,
        handbrake: 0.0,
        clutch: 0.0,
        steering: 0.0,
        race: None,
        splits: Some(splits.clone()),
        state: HudView::default(),
        pace_3d: None,
        parts: Default::default(),
    };
    for (name, mode, retired, failed, standing, show_hud, size) in [
        ("quick", 1, false, false, true, true, (1280, 720)),
        ("quick-no-records", 1, false, false, false, true, (1280, 720)),
        ("retired", 1, true, false, true, true, (1280, 720)),
        ("season", 3, false, false, true, true, (1280, 720)),
        ("season-retired", 3, true, false, true, true, (1280, 720)),
        ("season-failed", 3, false, true, true, true, (1280, 720)),
        ("hud-hidden", 1, false, false, true, false, (1280, 720)),
        ("quick-4k", 1, false, false, true, true, (3840, 2160)),
        ("quick-4-3", 1, false, false, true, true, (960, 720)),
    ] {
        let menu = ResultsMenu::open(Menu { mode, over: true, driving: false, help: false }).unwrap();
        let mut record = splits.clone();
        record.retired = retired;
        record.finished = !retired;
        if !standing {
            record.standing = None;
        }
        readout.splits = Some(record);
        readout.state.end_page = Some(if failed {
            0x18
        } else if retired {
            0x12
        } else {
            0x10
        });
        let (_, pixels) = renderer.render_offscreen_with(size, Mat4::IDENTITY, Mat4::IDENTITY, |encoder, target| {
            overlay.draw_with(&gpu.device, &gpu.queue, encoder, target, size, |ctx, hud| {
                if show_hud {
                    hud::paint_readout(ctx, hud, &readout);
                }
                if actions {
                    results::paint(ctx, &menu);
                }
            });
        })?;
        let mut png = png::Encoder::new(BufWriter::new(File::create(out.join(format!("{name}.png")))?), size.0, size.1);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()?.write_image_data(&pixels)?;
    }
    Ok(())
}
