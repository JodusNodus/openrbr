//! `--screenshot DIR`: draws the pages offscreen and writes them to PNGs, for reports.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use futures_executor::block_on;

use crate::car_browser::{Collection as CarCollection, Target};
use crate::pages::Page;
use crate::stage_browser::Collection;
use crate::{preview, Form};

const SIZE: (u32, u32) = (1100, 760);

/// Draws the pages to `dir`, and with `rally_damage` (`--screenshot-damage`) the service park on that car.
pub fn pages(form: &mut Form, dir: &Path, rally_damage: Option<PathBuf>) -> Result<(), String> {
    // The pages change the selection as they are drawn (the Championship page sets the team's car): the file is
    // kept (`read_only`).
    // The previews are made on their own threads: the Drive page waits for them.
    let (map, car) = (form.catalog.maps[form.map].clone(), form.car().cloned());
    if let Some(car) = &car {
        form.previews.wait_for(&map, car, &form.rsf());
    }
    let pages = [
        (Page::Drive, "drive"),
        (Page::Replays, "replays"),
        (Page::Controls, "controls"),
        (Page::Graphics, "graphics-sound"),
        (Page::Settings, "settings"),
    ];
    for (page, name) in pages {
        form.page = page;
        shot(&dir.join(format!("{name}.png")), |ctx| form.ui(ctx))?;
    }
    // The Drive page with the Custom weather, its controls closed and open.
    let saved =
        (form.clouds, form.precipitation, form.fog, form.wetness, form.options.weather, form.weather_row.clone());
    for (name, adjust) in [("drive-weather-custom", false), ("drive-weather-adjust", true)] {
        form.page = Page::Drive;
        form.screenshot_custom_weather(adjust);
        fresh_previews(form, &map, car.as_ref());
        shot(&dir.join(format!("{name}.png")), |ctx| form.ui(ctx))?;
    }
    (form.clouds, form.precipitation, form.fog, form.wetness, form.options.weather, form.weather_row) = saved;
    form.weather_adjust = false;
    // The Settings page with the NGP download's confirmation and gvrc.de's car list open (from a fixed list).
    form.page = Page::Settings;
    form.downloads.screenshot();
    shot(&dir.join("settings-downloads.png"), |ctx| form.ui(ctx))?;
    form.downloads = Default::default();
    // The Drive page with the car list open on the selection.
    form.page = Page::Drive;
    fresh_previews(form, &map, car.as_ref());
    if form.favourite_cars.is_empty() {
        let startable = form.catalog.cars.iter().filter(|c| c.unavailable().is_none());
        form.favourite_cars = startable.map(|c| c.name.clone()).step_by(9).take(3).collect();
    }
    let classes = ["cars", "favourites"];
    for (name, collection) in classes.into_iter().zip([CarCollection::All, CarCollection::Favourites]) {
        fresh_previews(form, &map, car.as_ref());
        form.car_browser.open = false;
        shot(&dir.join(format!("drive-{name}.png")), |ctx| form.screenshot_cars(ctx, collection.clone()))?;
    }
    form.car_browser.open = false;
    // The Drive page with some stars, then the stage browser on them and on the country of the selection.
    if form.favourite_maps.is_empty() {
        let startable = (form.catalog.maps.iter()).filter(|m| form.catalog.availability(&form.season, m).is_ok());
        form.favourite_maps = startable.map(|m| m.number).step_by(7).take(4).chain([map.number]).collect();
    }
    fresh_previews(form, &map, car.as_ref());
    shot(&dir.join("drive-stars.png"), |ctx| form.ui(ctx))?;
    let country = form.stage_browser.row(form.map).map(|r| r.country.clone());
    for (name, collection) in
        [("favourites", Collection::Favourites), ("country", Collection::Country(country.flatten()))]
    {
        fresh_previews(form, &map, car.as_ref());
        form.stage_browser.open = false;
        shot(&dir.join(format!("drive-stages-{name}.png")), |ctx| form.screenshot_stages(ctx, collection.clone()))?;
    }
    form.stage_browser.open = false;
    form.page = Page::Setup;
    for (n, area) in rbr_gameplay::setup::Area::ALL.into_iter().enumerate() {
        form.setup.show_area(area);
        shot(&dir.join(format!("setup-{n}.png")), |ctx| form.ui(ctx))?;
    }
    // The Championship page: a season after its first stage, then (with the damage file's car) its service.
    form.page = Page::Championship;
    form.rally.screenshot_season(&form.season, None);
    shot(&dir.join("championship.png"), |ctx| form.ui(ctx))?;
    // Its car list open, and a season saved without an RSF car asking for one.
    form.car_browser.open_on(Target::Season, form.season_pick, &form.favourite_cars, None);
    shot(&dir.join("championship-cars.png"), |ctx| form.ui(ctx))?;
    form.car_browser.open = false;
    let seasons = std::mem::take(&mut form.season_cars);
    shot(&dir.join("championship-no-car.png"), |ctx| form.ui(ctx))?;
    form.season_cars = seasons;
    if let Some(path) = rally_damage {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let damage = rbr_gameplay::car_damage::CarriedDamage::from_bytes(&bytes).ok_or("not a damage file")?;
        form.rally.screenshot_season(&form.season, Some(damage));
        shot(&dir.join("service.png"), |ctx| form.ui(ctx))?;
    }
    Ok(())
}

/// New previews of `map` and `car`, waited for: each capture has its own textures.
fn fresh_previews(form: &mut Form, map: &crate::catalog::Map, car: Option<&crate::catalog::Car>) {
    form.previews = preview::Previews::new(form.data.clone());
    if let Some(car) = car {
        form.previews.wait_for(map, car, &form.rsf());
    }
}

/// Draws `ui` to the PNG `path`.
pub fn shot(path: &Path, ui: impl FnMut(&egui::Context)) -> Result<(), String> {
    write_png(path, &draw(ui)?)?;
    eprintln!("launcher: wrote {}", path.display());
    Ok(())
}

/// Draws `ui` into an RGBA image of [`SIZE`]: a few passes, so the layout settles, and the last one drawn.
pub fn draw(mut ui: impl FnMut(&egui::Context)) -> Result<Vec<u8>, String> {
    let (width, height) = SIZE;
    let instance = rbr_render::renderer::instance();
    let adapter = block_on(instance.request_adapter(&Default::default())).map_err(|e| e.to_string())?;
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let context = egui::Context::default();
    let mut renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [width, height], pixels_per_point: 1.0 };
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32))),
        ..Default::default()
    };
    let mut output = context.run_pass(input(), &mut ui);
    for _ in 0..3 {
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(&device, &queue, *id, delta);
            }
        }
        output.textures_delta.clear();
        output = context.run_pass(input(), &mut ui);
    }
    for (id, deltas) in &output.textures_delta.set {
        for delta in deltas {
            renderer.update_texture(&device, &queue, *id, delta);
        }
    }
    // Applied (egui checks that none are dropped unapplied).
    output.textures_delta.clear();
    let primitives = context.tessellate(output.shapes, output.pixels_per_point);
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut buffers = renderer.update_buffers(&device, &queue, &mut encoder, &primitives, &screen);
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        renderer.render(&mut pass.forget_lifetime(), &primitives, &screen);
    }
    let row = (4 * width).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
        },
        extent,
    );
    buffers.push(encoder.finish());
    queue.submit(buffers);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    let mapped = readback.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
    Ok(mapped.chunks(row as usize).flat_map(|r| &r[..4 * width as usize]).copied().collect())
}

/// Writes `rgba` (the capture's size) to `path`, making its folder first.
pub fn write_png(path: &Path, rgba: &[u8]) -> Result<(), String> {
    path.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), SIZE.0, SIZE.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut w| w.write_image_data(rgba)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_makes_its_folder() {
        let root = std::env::temp_dir().join(format!("rbr-launcher-capture-{}", std::process::id()));
        let path = root.join("new").join("page.png");
        write_png(&path, &vec![0; (4 * SIZE.0 * SIZE.1) as usize]).unwrap();
        assert!(path.is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
