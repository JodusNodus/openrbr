//! Draws HUD states that a live drive does not reach: `hud_symbols <out-dir>` writes `symbols.png`, every pace-note
//! symbol (`rbr_render::hud::pace`) with its label and tag, and `countdown-<n>.png`, the countdown calls for typed race
//! clocks (a live boot starts the car running, without the countdown). Drawn over the stage with an identity camera
//! (`RBR_DATA`).

use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2};
use glam::Mat4;
use rbr_formats::data::Data;
use rbr_render::hud::model::HudView;
use rbr_render::hud::{pace, Overlay, Readout};
use rbr_render::live::RaceHud;
use rbr_render::renderer::{self, Renderer};
use rbr_render::stage::{self, Selection};

const SIZE: (u32, u32) = (1280, 720);

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    fs::create_dir_all(&out)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut overlay = Overlay::new(&gpu.device, format);
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let stage = stage::load(&data.install().join("Maps"), "track-54_E", Selection::default())?;
    let mut renderer = Renderer::new(&gpu, format, &stage, Default::default());

    let tags: Vec<u32> = (0..33).filter(|&t| pace::symbol(t).is_some()).collect();
    let pixels = render(&mut renderer, |encoder, view| {
        overlay.draw_with(&gpu.device, &gpu.queue, encoder, view, SIZE, |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for (k, &tag) in tags.iter().enumerate() {
                let symbol = pace::symbol(tag).unwrap();
                let cell = Pos2::new(20.0 + 140.0 * (k % 9) as f32, 20.0 + 150.0 * (k / 9) as f32);
                let panel = Rect::from_min_size(cell, Vec2::new(120.0, 124.0));
                painter.rect_filled(panel, 8.0, Color32::from_black_alpha(150));
                let rect = Rect::from_center_size(Pos2::new(panel.center().x, cell.y + 44.0), pace::SIZE);
                pace::paint(&painter, rect, &symbol, 1.0);
                let label = format!("{} ({tag})", pace::label(&symbol));
                let at = Pos2::new(panel.center().x, rect.max.y + 4.0);
                painter.text(at, Align2::CENTER_TOP, label, FontId::proportional(13.0), Color32::from_gray(235));
            }
        })
    })?;
    write(&out.join("symbols.png"), &pixels)?;

    // Before the calls, each digit, and GO (the game's countdown starts at 7 s; the clock starts at GO).
    let clocks = [
        ("ready", RaceHud { countdown: 5.0, ..Default::default() }),
        ("3", RaceHud { countdown: 2.5, ..Default::default() }),
        ("2", RaceHud { countdown: 1.5, ..Default::default() }),
        ("1", RaceHud { countdown: 0.5, ..Default::default() }),
        ("go", RaceHud { time: 0.5, countdown: -0.5, running: true, ..Default::default() }),
    ];
    for (name, race) in clocks {
        let readout = Readout {
            view: "chase",
            view_keys: String::new(),
            time: 0.0,
            speed: 0.0,
            mph: false,
            fps: None,
            gear: Some("1".into()),
            rpm: Some(1000.0),
            throttle: 0.0,
            brake: 0.0,
            handbrake: 1.0,
            clutch: 0.0,
            steering: 0.0,
            race: Some(race),
            splits: None,
            state: HudView::default(),
            pace_3d: None,
        };
        let pixels = render(&mut renderer, |encoder, view| {
            overlay.draw(&gpu.device, &gpu.queue, encoder, view, SIZE, &readout)
        })?;
        write(&out.join(format!("countdown-{name}.png")), &pixels)?;
    }
    Ok(())
}

fn render(
    renderer: &mut Renderer,
    draw: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(renderer.render_offscreen_with(SIZE, Mat4::IDENTITY, Mat4::IDENTITY, draw)?.1)
}

fn write(path: &Path, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut png = png::Encoder::new(BufWriter::new(File::create(path)?), SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(pixels)?;
    Ok(())
}
