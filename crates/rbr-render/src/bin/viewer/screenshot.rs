//! `--screenshot`: one frame drawn offscreen into a PNG, after the `--frames` timings.

use crate::args::Args;
use crate::draw::{dial_values, readout, scripted_camera, show_crew, start_camera, step_characters, throw_smoke};
use crate::scene::Scene;
use glam::Mat4;
use rbr_counters::{Counts, Mark};
use rbr_render::hud::camera_page::CameraPage;
use rbr_render::hud::pause::{self, Outcome, PauseMenu};
use rbr_render::hud::results::{self, ResultsMenu};
use rbr_render::hud::{self as hud_overlay, Overlay};
use rbr_render::playback::ChaseLook;
use rbr_render::renderer::{self, Gpu, Renderer};
use rbr_render::views::Shown;
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

/// Window and screenshot size, pixels.
pub const SIZE: (u32, u32) = (1280, 720);

pub fn screenshot(scene: &mut Scene, args: &Args, path: &Path) -> Result<(), Box<dyn Error>> {
    let Args { camera, fov, enhance, frames, profile, time, .. } = *args;
    let view = scene.cameras.start_view(args.view);
    let menu = match (&scene.live, &args.pause_menu) {
        (Some(live), Some(moves)) => {
            let mut menu = PauseMenu::open(live.menu, CameraPage::new(view, scene.cameras.nudges()));
            if moves.iter().any(|&nav| matches!(menu.nav(nav), Some(Outcome::Chosen(_)))) {
                return Err("--pause-menu: the moves close the menu".into());
            }
            Some(menu)
        }
        _ => None,
    };
    // The Camera page's view and nudges, shown as the window shows them (the file is not written).
    let page = menu.as_ref().and_then(PauseMenu::camera);
    let view = page.map_or(view, |page| page.view);
    if let Some(page) = page {
        scene.cameras.set(page.view, page.nudges);
        if let (Some(mounts), Some(playback)) = (&mut scene.mounts, &mut scene.playback) {
            scene.cameras.apply(mounts, playback);
        }
    }
    let size = args.size.unwrap_or(SIZE);
    let instance = renderer::instance();
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = scene.renderer(&gpu, format, enhance, args.render_scale, &mut |_| {});
    renderer.set_time(time);
    renderer.set_start_clock(scene.live.as_ref().and_then(|l| l.hud_model.view().start_clock));
    let shots = args.sequence.max(1);
    for shot in 0..shots {
        // A drive's frame at its time, with the car's velocity standing for the eye's (the rain's streaks).
        if let Some(playback) = &mut scene.playback {
            let (index, now, here) = (playback.index(), playback.time(), playback.position());
            playback.step(-1);
            let (then, there) = (playback.time(), playback.position());
            playback.step((index - playback.index()) as isize);
            if args.time == 0.0 {
                renderer.set_time(now as f32);
            }
            if now > then {
                renderer.set_eye_velocity((here - there) / (now - then) as f32);
            }
        }
        if let Some(characters) = &mut scene.characters {
            step_characters(characters, scene.playback.as_ref(), 0.0, scene.live.as_ref().and_then(|l| l.hud.race));
        }
        // The game's start orbit replaces the view until the go, and the helicam while the helicopter runs.
        let camera_record = scene.live.as_ref().and_then(|live| live.hud.camera);
        let (heli, flip, scripted) = scripted_camera(scene.characters.as_ref());
        let shown = match page {
            Some(_) => Shown::Player(view),
            None => Shown::of(view, camera_record, heli, flip),
        };
        let (view, look) = (shown.car_view(), ChaseLook::default());
        if let Some(playback) = &mut scene.playback {
            playback.follow_game_chase(shown.swing(), args.look_back);
        }
        let aspect = size.0 as f32 / size.1 as f32;
        let (view_projection, sky_view_projection) = match camera {
            Some(camera) => (camera.view_projection(fov, aspect), camera.sky_view_projection(fov, aspect)),
            None => view_projections(scene, shown, look, fov, aspect),
        };
        if let Some(characters) = &mut scene.characters {
            characters.apply(&mut renderer, view_projection);
        }
        // The particles first: the glass takes their tyre spray.
        if let (Some(drawn), Some(playback)) = (&mut scene.particles, &scene.playback) {
            throw_smoke(scene.characters.as_mut(), drawn);
            drawn.draw(&mut renderer, playback, scene.live.as_mut(), view_projection);
        }
        if let (Some(playback), Some(glass)) = (&mut scene.playback, &mut scene.glass) {
            glass.draw(&mut renderer, playback, view.interior(), scene.particles.as_ref().map(|p| &p.particles.spray));
        }
        if let (Some(playback), Some(crew)) = (&mut scene.playback, &mut scene.crew) {
            show_crew(crew, playback, view, None, &mut renderer);
        }
        // The flares are seen through the car's camera (not the `--camera` one).
        if let (Some(playback), Some(mounts)) = (&mut scene.playback, &scene.mounts) {
            playback.interior = view.interior();
            renderer.set_outside_view(!view.interior());
            playback.reverse = scene.live.as_ref().is_some_and(|live| live.hud.gear == 0);
            if let Some(lamps) = scene.live.as_ref().and_then(|live| live.lamps) {
                playback.set_lamps(lamps);
            }
            let lens = shown.lens(playback, mounts, scripted, look, fov);
            playback.apply(&mut renderer, &lens);
        }
        if let (Some(ghost), Some(playback)) = (&mut scene.ghost, &scene.playback) {
            ghost.apply(&mut renderer, playback, view_projection);
        }
        if let (Some(drawn), Some(playback)) = (&mut scene.fences, &scene.playback) {
            drawn.draw(&mut renderer, playback, view_projection);
        }
        if let (Some(dials), Some(playback)) = (&mut scene.dials, &scene.playback) {
            dials.update(&dial_values(playback, scene.live.as_ref()), None, &mut renderer);
        }
        let readout = scene
            .playback
            .as_ref()
            .filter(|_| scene.hud.show)
            .map(|p| readout(p, scene.live.as_ref(), shown, &args.bindings, scene.hud, None, view_projection));
        let mut overlay = Overlay::new(&gpu.device, format);
        // The probes show a first frame's sky and shadows (as the window's load draws them).
        if shot == 0 && renderer.has_probes() {
            renderer.render_offscreen(size, view_projection, sky_view_projection)?;
            renderer.bake_probes();
        }
        if frames > 0 && shot == 0 {
            time_frames(&gpu, &mut renderer, format, size, (view_projection, sky_view_projection), frames, profile)?;
        }
        let (stats, pixels) =
            renderer.render_offscreen_with(size, view_projection, sky_view_projection, |encoder, target| {
                overlay.draw_with(&gpu.device, &gpu.queue, encoder, target, size, |ctx, hud| {
                    readout.iter().for_each(|r| hud_overlay::paint_readout(ctx, hud, r));
                    menu.iter().for_each(|m| pause::paint(ctx, m));
                    if menu.is_none() {
                        if let Some(actions) = scene.live.as_ref().and_then(|live| ResultsMenu::open(live.menu)) {
                            results::paint(ctx, &actions);
                        }
                    }
                });
            })?;
        eprintln!("screenshot: {stats:?}");
        let path = match args.sequence {
            0 => path.to_path_buf(),
            _ => {
                path.with_file_name(format!("{}-{shot:03}.png", path.file_stem().unwrap_or_default().to_string_lossy()))
            }
        };
        write_png(&path, size, &pixels)?;
        // `--sequence`: the next frame is the drive's next.
        if let Some(playback) = &mut scene.playback {
            playback.step(1);
        }
    }
    Ok(())
}

/// `--frames` and `--profile`: times `frames` frames read back, drawn only, and with the GPU idle, then each stage's.
fn time_frames(
    gpu: &Gpu,
    renderer: &mut Renderer,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    (view_projection, sky_view_projection): (Mat4, Mat4),
    frames: u32,
    profile: bool,
) -> Result<(), Box<dyn Error>> {
    // The first frame builds the reflection cube; time the steady ones after it.
    renderer.render_offscreen(size, view_projection, sky_view_projection)?;
    let start = Instant::now();
    for _ in 0..frames {
        renderer.render_offscreen(size, view_projection, sky_view_projection)?;
    }
    let mean = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
    // As a window draws them: submitted back to back into one target, nothing read back.
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("timed frames"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let start = Instant::now();
    let mut encoding = Duration::ZERO;
    for _ in 0..frames {
        let encode = Instant::now();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render(&mut encoder, &view, size, view_projection, sky_view_projection);
        let commands = encoder.finish();
        encoding += encode.elapsed();
        gpu.queue.submit([commands]);
    }
    gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
    let drawn = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
    let encoding = encoding.as_secs_f64() * 1000.0 / f64::from(frames);
    // The CPU's share alone: each frame recorded and encoded with the GPU idle, so that no wait for it counts.
    let mut idle = Duration::ZERO;
    let mut counts = Counts::default();
    for _ in 0..frames {
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        let counting = Mark::now();
        let encode = Instant::now();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render(&mut encoder, &view, size, view_projection, sky_view_projection);
        let commands = encoder.finish();
        idle += encode.elapsed();
        counts += counting.elapsed();
        gpu.queue.submit([commands]);
    }
    let idle = idle.as_secs_f64() * 1000.0 / f64::from(frames);
    eprintln!(
        "frames: {frames}, {mean:.2} ms each read back, {drawn:.2} ms each drawn only ({encoding:.2} encoding, \
         {idle:.2} with the GPU idle)"
    );
    let per_frame = |n: u64| n / u64::from(frames);
    let cpu = if counts.instructions > 0 {
        format!("{} instructions, {} cycles, ", per_frame(counts.instructions), per_frame(counts.cycles))
    } else {
        String::new()
    };
    eprintln!(
        "  with the GPU idle, per frame: {cpu}{} allocations ({} bytes)",
        per_frame(counts.allocations),
        per_frame(counts.allocated)
    );
    if profile {
        renderer.enable_profiling();
        for _ in 0..frames {
            renderer.render_offscreen(size, view_projection, sky_view_projection)?;
        }
        for (stage, ms) in renderer.profile() {
            eprintln!("  {stage}: {ms:.2} ms");
        }
    }
    Ok(())
}

/// The view-projection and the sky's view-projection for the scene's camera at horizontal field of view `fov` and
/// `aspect`: the orbit camera when there is one, otherwise the stage start view.
pub fn view_projections(scene: &Scene, shown: Shown, look: ChaseLook, fov: f32, aspect: f32) -> (Mat4, Mat4) {
    match (&scene.orbit, &scene.playback, &scene.mounts) {
        (Some(orbit), _, _) => (orbit.view_projection(fov, aspect), orbit.sky_view_projection(fov, aspect)),
        (None, Some(playback), Some(mounts)) => {
            let lens = shown.lens(playback, mounts, scripted_camera(scene.characters.as_ref()).2, look, fov);
            (lens.view_projection(aspect), lens.sky_view_projection(aspect))
        }
        _ => {
            let camera = start_camera(&scene.stage, scene.chase);
            (camera.view_projection(fov, aspect), camera.sky_view_projection(fov, aspect))
        }
    }
}

/// Writes the colour without the alpha channel. The game's back buffer is `D3DFMT_X8R8G8B8` (the capture's
/// `CreateDevice`), so what the draws leave in alpha is never shown: opaque car draws write their texture's alpha,
/// which is 0 on COROLLA's number plates and showed as a black or white patch in the PNG.
fn write_png(path: &Path, (width, height): (u32, u32), rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
