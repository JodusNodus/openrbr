//! Renders a replay to a video file offline (`--video OUT.mp4`): the live command runs the replay's frames back
//! to back (`--replay-pace free`), each output frame waits for the answers' game time to reach it, and ffmpeg encodes
//! the frames while the sound renders offline to a WAV that is muxed in at the end. No window opens; the mouse does
//! nothing.

use crate::args::Args;
use crate::draw::{dial_values, draw_car, draw_particles, readout, scripted_camera, step_characters};
use crate::scene::{take_damage, Scene};
use crate::screenshot::view_projections;
use glam::Mat4;
use rbr_formats::data::Data;
use rbr_recording::STEP_SECONDS;
use rbr_render::characters::scripted::Event;
use rbr_render::hud::results::{self, ResultsMenu};
use rbr_render::hud::stock::Stock;
use rbr_render::hud::{self as hud_overlay, Overlay};
use rbr_render::playback::ChaseLook;
use rbr_render::renderer::{self, Renderer};
use rbr_render::sound::Player;
use rbr_render::views::Shown;
use rbr_sound::Scene as AudioScene;
use std::error::Error;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// The video's size, pixels, without `--size`.
const SIZE: (u32, u32) = (1920, 1080);
/// The sound's sample rate (the `rbr-sound` binary's).
const RATE: u32 = 44100;
/// Seconds between progress lines.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// Renders the replay the live command runs (`--replay`) to `path` at `args.video_fps` frames a second, with the
/// game's sound unless `--no-sound`.
pub fn render(
    scene: &mut Scene,
    args: &Args,
    data: &Data,
    fmod_bank: Option<&Path>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    if scene.live.is_none() {
        return Err("--video needs --live (it renders a replay)".into());
    }
    let size = args.size.unwrap_or(SIZE);
    let fps = args.video_fps;
    let instance = renderer::instance();
    let gpu = renderer::request_gpu(&instance, None)?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = scene.renderer(&gpu, format, args.enhance, args.render_scale, &mut |_| {});
    let mut overlay = Overlay::with_hud(&gpu.device, format, Box::new(Stock::new(data)));
    // The live drive's sound with no device, rendered into a WAV that is muxed in at the end (`Encoder`).
    let player = crate::start_sound(args, data, fmod_bank, scene.precipitation, Some(RATE));
    let wav_path =
        player.as_ref().map(|_| std::env::temp_dir().join(format!("rerbr-video-{}.wav", std::process::id())));
    let mut audio = match (player, &wav_path) {
        (Some(player), Some(path)) => {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            Some(Audio { player, wav: hound::WavWriter::create(path, spec)?, rendered: 0 })
        }
        _ => None,
    };
    // The HUD follows the custom co-driver's fired calls instead of the stage's notes when one calls them.
    if let Some(command) = &args.live {
        if let Some(live) = &mut scene.live {
            crate::scene::use_custom_hud(live, args, data, command);
        }
    }
    let mut encoder = Encoder::start(size, fps, path, wav_path.as_deref())?;
    if let Some(live) = &mut scene.live {
        live.resume();
    }
    let mut shown = Shown::Player(scene.cameras.start_view(args.view));
    let mut game_time = 0.0;
    let mut periods = 0u64;
    let mut frames = 0u64;
    let mut pending = Vec::new();
    let mut probed = false;
    let started = Instant::now();
    let mut last_progress = started;
    loop {
        let t = frames as f32 / fps;
        let mut ended = false;
        while game_time < f64::from(t) {
            let next = scene.live.as_mut().and_then(|live| live.next_answer());
            let Some((frame, answered)) = next else {
                ended = true;
                break;
            };
            take_answer(scene, &mut pending, frame)?;
            game_time += f64::from(answered) * f64::from(STEP_SECONDS);
            periods += u64::from(answered);
        }
        if ended {
            break;
        }
        let events = draw_frame(scene, args, &mut renderer, size, t, &mut shown);
        let (view, sky) = views(scene, args, shown, size);
        // Bake the reflection probes after the first frame's sky and shadows, as the window's load does.
        if !probed {
            probed = true;
            if renderer.has_probes() {
                renderer.render_offscreen(size, view, sky)?;
                renderer.bake_probes();
            }
        }
        // The frame's scene hears the sound stepped since the last frame, and the samples it fills.
        if let Some(audio) = &mut audio {
            let helicopter = scene.characters.as_ref().and_then(|c| c.rescue_position());
            audio.step(audio_scene(scene, args, shown), &mut pending, &rbr_render::sound::events(&events, helicopter));
            audio.samples(periods)?;
        }
        let readout = scene
            .playback
            .as_ref()
            .filter(|_| scene.hud.show)
            .map(|p| readout(p, scene.live.as_ref(), shown, &args.bindings, scene.hud, None));
        let results = scene.live.as_ref().and_then(|live| ResultsMenu::open(live.menu));
        let (_, pixels) = renderer.render_offscreen_with(size, view, sky, |encoder, target| {
            overlay.draw_with(&gpu.device, &gpu.queue, encoder, target, size, |ctx, hud| {
                readout.iter().for_each(|r| hud_overlay::paint_readout(ctx, hud, r));
                results.iter().for_each(|m| results::paint(ctx, m));
            });
        })?;
        encoder.video(&pixels)?;
        frames += 1;
        if last_progress.elapsed() >= PROGRESS_EVERY {
            last_progress = Instant::now();
            eprintln!(
                "video: {frames} frames, {game_time:.1} s of game time, {:.1} fps render",
                frames as f64 / (last_progress - started).as_secs_f64().max(1e-9)
            );
        }
    }
    let wall = started.elapsed().as_secs_f64();
    eprintln!(
        "video: {frames} frames ({:.1} s) from {game_time:.2} s of game time in {wall:.0} s wall ({:.1} fps render)",
        frames as f64 / f64::from(fps),
        frames as f64 / wall.max(1e-9)
    );
    // The WAV's header is written when it closes: finalize it before the mux reads it.
    if let Some(audio) = audio {
        audio.wav.finalize()?;
    }
    encoder.finish()?;
    // A replay that stopped early (a frame that differs from the drive's) leaves the video short.
    scene.live.as_mut().map_or(Ok(()), |live| live.wait()).map_err(|e| format!("{e}: the video ends early").into())
}

/// What one live answer does, as the window's live branch does per pump: the drive grows, the car dents and loses
/// parts, the ghost watches, and the answer's sound records wait for the output frame's scene.
fn take_answer(scene: &mut Scene, pending: &mut Vec<rbr_sound::Inputs>, frame: Vec<u8>) -> Result<(), String> {
    let live = scene.live.as_mut().ok_or("the live process ended")?;
    let drive = live.parse(&frame)?;
    let playback = scene.playback.as_mut().ok_or("the live process ended")?;
    playback.push(drive);
    take_damage(playback, live);
    if let Some(ghost) = &mut scene.ghost {
        let penalties = live.hud.race.map_or(0.0, |race| race.penalties);
        ghost.observe(live.splits.as_ref(), penalties, playback.sample().step);
    }
    playback.reverse = live.hud.gear == 0;
    if let Some(lamps) = live.lamps {
        playback.set_lamps(lamps);
    }
    if live.take_rescue_call() {
        if let Some(characters) = &mut scene.characters {
            characters.call_rescue();
        }
    }
    pending.extend(live.take_sounds());
    Ok(())
}

/// The frame's matrices.
fn views(scene: &Scene, args: &Args, shown: Shown, size: (u32, u32)) -> (Mat4, Mat4) {
    view_projections(scene, shown, ChaseLook::default(), args.fov, size.0 as f32 / size.1 as f32)
}

/// Updates the scene for the output frame at game time `t` (the window's per-frame work, without any input), and
/// returns the scripted events the sound hears.
fn draw_frame(
    scene: &mut Scene,
    args: &Args,
    renderer: &mut Renderer,
    size: (u32, u32),
    t: f32,
    shown: &mut Shown,
) -> Vec<Event> {
    if let Some(playback) = &mut scene.playback {
        // The car's velocity stands for the eye's (the rain's streaks) when the camera rides with the car.
        let (index, now, here) = (playback.index(), playback.time(), playback.position());
        playback.step(-1);
        let (then, there) = (playback.time(), playback.position());
        playback.step((index - playback.index()) as isize);
        renderer.set_time(t);
        if now > then && args.flyby.is_none() && args.camera.is_none() {
            renderer.set_eye_velocity((here - there) / (now - then) as f32);
        }
    }
    let mut events = Vec::new();
    if let Some(characters) = &mut scene.characters {
        let race = scene.live.as_ref().and_then(|l| l.hud.race);
        events = step_characters(characters, scene.playback.as_ref(), f64::from(t), race);
    }
    // The game's start orbit replaces the view until the go, and the helicam while the helicopter runs.
    let camera_record = scene.live.as_ref().and_then(|live| live.hud.camera);
    let (heli, flip, _) = scripted_camera(scene.characters.as_ref());
    let before = *shown;
    *shown = Shown::of(scene.cameras.start_view(args.view), camera_record, heli, flip);
    if !before.same_camera(*shown) {
        renderer.reset_history();
    }
    let camera = match (args.flyby, &scene.playback) {
        (Some(offset), Some(playback)) => Some(playback.flyby(offset)),
        _ => args.camera,
    };
    if let Some(playback) = &mut scene.playback {
        playback.follow_game_chase(shown.swing(), args.look_back);
    }
    let aspect = size.0 as f32 / size.1 as f32;
    let (view_projection, _) = match camera {
        Some(camera) => (camera.view_projection(args.fov, aspect), camera.sky_view_projection(args.fov, aspect)),
        None => views(scene, args, *shown, size),
    };
    if let Some(characters) = &mut scene.characters {
        characters.apply(renderer, view_projection);
    }
    draw_particles(scene, renderer, view_projection);
    draw_car(scene, renderer, *shown, ChaseLook::default(), args.fov);
    if let (Some(ghost), Some(playback)) = (&mut scene.ghost, &scene.playback) {
        ghost.apply(renderer, playback, view_projection);
    }
    if let (Some(drawn), Some(playback)) = (&mut scene.fences, &scene.playback) {
        drawn.draw(renderer, playback, view_projection);
    }
    if let (Some(dials), Some(playback)) = (&mut scene.dials, &scene.playback) {
        dials.update(&dial_values(playback, scene.live.as_ref()), Some(1.0 / args.video_fps), renderer);
    }
    renderer.set_start_clock(scene.live.as_ref().and_then(|l| l.hud_model.view().start_clock));
    renderer.set_start_lamps(scene.live.as_ref().and_then(|l| l.hud.race.as_ref()));
    if let Some(live) = &scene.live {
        renderer.set_interactive_poses(&live.interactive_poses);
    }
    events
}

/// The sound the output frames hear: the live drive's player with no device, rendered into the WAV for the game
/// time elapsed.
struct Audio {
    player: Player,
    wav: hound::WavWriter<BufWriter<std::fs::File>>,
    /// Samples per channel rendered so far.
    rendered: usize,
}

impl Audio {
    /// Steps the player with the periods' records and the scripted events since, under the frame's scene.
    fn step(&mut self, scene: AudioScene, pending: &mut Vec<rbr_sound::Inputs>, events: &rbr_sound::Events) {
        self.player.set_scene(scene);
        pending.drain(..).for_each(|inputs| self.player.step(&inputs));
        self.player.events(events);
    }

    /// Renders the samples of the game time elapsed (`periods` periods) into the WAV.
    fn samples(&mut self, periods: u64) -> Result<(), Box<dyn Error>> {
        let target = (periods as f64 * f64::from(RATE) * f64::from(STEP_SECONDS)).round() as usize;
        if target > self.rendered {
            let mut block = vec![[0.0; 2]; target - self.rendered];
            self.player.render(&mut block);
            self.rendered = target;
            for &s in block.iter().flatten() {
                self.wav.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
            }
        }
        Ok(())
    }
}

/// The sound's scene for the output frame: the camera the frame draws with and the car it hears.
fn audio_scene(scene: &Scene, args: &Args, shown: Shown) -> AudioScene {
    match (&scene.playback, &scene.mounts) {
        (Some(playback), Some(mounts)) => {
            let (_, _, scripted) = scripted_camera(scene.characters.as_ref());
            let lens = shown.lens(playback, mounts, scripted, ChaseLook::default(), args.fov);
            rbr_render::sound::scene(&lens, &playback.sample().car, shown.car_view().interior())
        }
        _ => AudioScene::default(),
    }
}

/// The video file's encoder: ffmpeg reads raw RGB frames on stdin, then the WAV is muxed in.
struct Encoder {
    video_in: Option<std::process::ChildStdin>,
    child: Option<std::process::Child>,
    /// The video and the WAV muxed into `out` at the end (`None` without sound: ffmpeg writes `out` itself).
    tmp_video: Option<std::path::PathBuf>,
    wav: Option<std::path::PathBuf>,
    out: std::path::PathBuf,
    size: (u32, u32),
}

impl Encoder {
    /// Starts ffmpeg writing `out` (`wav`: muxed in at the end).
    fn start(size: (u32, u32), fps: f32, out: &Path, wav: Option<&Path>) -> Result<Encoder, Box<dyn Error>> {
        let tmp_video = wav.map(|_| std::env::temp_dir().join(format!("rerbr-video-{}.mp4", std::process::id())));
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{}x{}", size.0, size.1),
            "-r",
            &fps.to_string(),
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
        ]);
        command.arg(tmp_video.as_deref().unwrap_or(out));
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("ffmpeg: {e} (is ffmpeg on PATH?)"))?;
        let video_in = child.stdin.take();
        Ok(Encoder {
            video_in,
            child: Some(child),
            tmp_video,
            wav: wav.map(Path::to_path_buf),
            out: out.to_path_buf(),
            size,
        })
    }

    /// One frame's pixels (the alpha channel dropped, as `write_png` does).
    fn video(&mut self, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        debug_assert_eq!(rgb.len(), (self.size.0 * self.size.1 * 3) as usize);
        self.video_in
            .as_mut()
            .ok_or("the video encoder ended")?
            .write_all(&rgb)
            .map_err(|e| format!("ffmpeg: {e}").into())
    }

    /// Ends the video stream, muxes the WAV in, and removes the temp files.
    fn finish(mut self) -> Result<(), Box<dyn Error>> {
        drop(self.video_in.take());
        if let Some(mut child) = self.child.take() {
            let status = child.wait().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("ffmpeg: {status}").into());
            }
        }
        if let (Some(video), Some(wav)) = (self.tmp_video.as_deref(), self.wav.as_deref()) {
            let status = Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .arg("-i")
                .arg(video)
                .arg("-i")
                .arg(wav)
                .args(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "faststart"])
                .arg(&self.out)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| format!("ffmpeg: {e} (is ffmpeg on PATH?)"))?
                .wait()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("ffmpeg: {status}").into());
            }
        }
        Ok(())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        drop(self.video_in.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for path in [&self.tmp_video, &self.wav].into_iter().flatten() {
            let _ = std::fs::remove_file(path);
        }
    }
}
