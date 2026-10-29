//! The loading screen (ours): the window opens at once and shows the stage's picture, the stage and car and the load's
//! progress, while a thread loads the scene, makes the renderer, draws one frame and starts the sound. On Semetin with
//! the Fiesta and every enhancement (M3 Pro) the scene takes 1.2 s, the renderer 3.3 s (nearly all of it the stage's
//! meshes), the frame 0.17 s and the sound 0.85 s (its FMOD bank 0.45 s, opening the audio device 0.37 s).

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use egui::{pos2, Align2, Color32, ColorImage, FontId, LayerId, Rect, TextureHandle, TextureOptions, Vec2};
use rbr_formats::data::Data;
use rbr_formats::stage_files;
use rbr_render::perf_log;
use rbr_render::playback::ChaseLook;
use rbr_render::renderer::{Gpu, Renderer};
use rbr_render::sound::Player;
use rbr_render::stage_picture::{self, Pixels};
use rbr_render::views::Shown;
use std::path::PathBuf;

use crate::args::Args;
use crate::draw::dial_values;
use crate::scene::{self, Scene};
use crate::screenshot::view_projections;

/// A step of the load, in order.
#[derive(Clone, Copy)]
pub enum Step {
    Stage,
    Characters,
    /// Starting `rbr-standalone` (`--live`).
    Physics,
    Car,
    Particles,
    /// The car's textures not cached yet, `done` of `total`, upscaled ([`rbr_render::upscale`]).
    Upscale {
        done: usize,
        total: usize,
    },
    /// The renderer's meshes, uploaded and their pipelines built.
    Meshes,
    /// The first frame ([`warm_up`]).
    Frame,
    /// The reflection probes along the driveline ([`Renderer::bake_probes`]), after the first frame.
    Probes,
    Sound,
}

impl Step {
    fn label(self) -> String {
        match self {
            Step::Stage => "Loading the stage",
            Step::Characters => "Placing the spectators",
            Step::Physics => "Starting the physics",
            Step::Car => "Loading the car",
            Step::Particles => "Loading the particles",
            Step::Upscale { done, total } => {
                return format!("Upscaling textures, {} of {total} (once per car and stage)", done + 1)
            }
            Step::Meshes => "Preparing the stage",
            Step::Frame => "Drawing the first frame",
            Step::Probes => "Baking the reflections",
            Step::Sound => "Starting the sound",
        }
        .to_owned()
    }

    /// Where the step starts on the bar, 0 to 1: the share of a Semetin load before it.
    fn start(self) -> f32 {
        match self {
            Step::Stage => 0.0,
            Step::Characters => 0.05,
            Step::Physics => 0.06,
            Step::Car => 0.08,
            Step::Particles => 0.16,
            // Before the meshes, by its share of the textures done.
            Step::Upscale { done, total } => 0.2 + 0.04 * done as f32 / total.max(1) as f32,
            Step::Meshes => 0.24,
            Step::Frame => 0.82,
            Step::Probes => 0.84,
            Step::Sound => 0.85,
        }
    }
}

/// The bar moves this share of the way to the load's progress each frame, so that it fills smoothly.
const EASE: f32 = 0.2;
/// The mesh progress the thread sends, in steps of this share.
const MESH_STEP: f32 = 0.01;

/// What the loading thread sends.
enum Message {
    Picture(Pixels),
    Step(Step),
    /// The share of the renderer's meshes made.
    Meshes(f32),
    Done(Result<Box<Loaded>, String>),
}

/// What the load makes.
pub struct Loaded {
    pub scene: Scene,
    /// The scene's renderer, with a frame drawn.
    pub renderer: Renderer,
    /// The live drive's sound, playing.
    pub sound: Option<Player>,
}

pub struct Loading {
    /// The stage and car named.
    title: String,
    messages: Receiver<Message>,
    /// The stage's picture until it is uploaded, then its texture.
    picture: Option<Pixels>,
    texture: Option<TextureHandle>,
    step: Step,
    /// The load's progress, 0 to 1, and what the bar shows.
    done: f32,
    shown: f32,
}

impl Loading {
    /// Starts loading what `args` name (`fmod_bank`: the RSF car's sound), the renderer on `gpu` for frames of
    /// `format` and `size`.
    pub fn start(
        args: Arc<Args>,
        data: Data,
        fmod_bank: Option<PathBuf>,
        gpu: Gpu,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Loading {
        let title = args.title.clone().unwrap_or_else(|| match (&args.stage, &args.car) {
            (Some(stage), _) => stage_files::name(stage).to_owned(),
            (None, Some(car)) => car.clone(),
            (None, None) => String::new(),
        });
        let (send, messages) = channel();
        std::thread::Builder::new()
            .name("load".into())
            .spawn(move || {
                perf_log::name_thread("loading");
                let send = |message| {
                    // The window closed: nobody waits for the load.
                    let _ = send.send(message);
                };
                if let Some(picture) = args.stage.as_deref().and_then(|stage| stage_picture::of_stage(&data, stage)) {
                    send(Message::Picture(picture));
                }
                let mut span = perf_log::span("load stage and car");
                let loaded = scene::load(&args, &data, &mut |step| send(Message::Step(step))).map(|mut scene| {
                    send(Message::Step(Step::Meshes));
                    span.next("upload meshes and textures");
                    let mut sent = 0.0;
                    let mut renderer = scene.renderer(&gpu, format, args.enhance, args.render_scale, &mut |made| {
                        if made - sent >= MESH_STEP {
                            sent = made;
                            send(Message::Meshes(made));
                        }
                    });
                    send(Message::Step(Step::Frame));
                    span.next("first frame");
                    warm_up(&mut scene, &mut renderer, &args, &gpu, format, size);
                    send(Message::Step(Step::Probes));
                    span.next("bake probes");
                    renderer.bake_probes();
                    send(Message::Step(Step::Sound));
                    span.next("start sound");
                    let sound = crate::start_sound(&args, &data, fmod_bank.as_deref(), scene.precipitation, None);
                    Box::new(Loaded { scene, renderer, sound })
                });
                send(Message::Done(loaded));
            })
            .expect("a thread");
        Loading { title, messages, picture: None, texture: None, step: Step::Stage, done: 0.0, shown: 0.0 }
    }

    /// What the thread loaded once it is done, or why it could not.
    pub fn poll(&mut self) -> Option<Result<Loaded, String>> {
        loop {
            match self.messages.try_recv() {
                Ok(Message::Picture(picture)) => self.picture = Some(picture),
                Ok(Message::Step(step)) => (self.step, self.done) = (step, step.start()),
                Ok(Message::Meshes(made)) => {
                    let (start, end) = (Step::Meshes.start(), Step::Frame.start());
                    self.done = start + (end - start) * made;
                }
                Ok(Message::Done(loaded)) => return Some(loaded.map(|loaded| *loaded)),
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Some(Err("the load stopped (see the messages above)".to_owned()))
                }
            }
        }
    }

    /// The screen: the stage's picture with the stage and car under it, then the step and the bar, centred.
    pub fn paint(&mut self, ctx: &egui::Context) {
        if let Some((width, height, rgba)) = self.picture.take() {
            let image = ColorImage::from_rgba_unmultiplied([width, height], &rgba);
            self.texture = Some(ctx.load_texture("stage picture", image, TextureOptions::LINEAR));
        }
        self.shown += (self.done - self.shown) * EASE;
        let painter = ctx.layer_painter(LayerId::background());
        let screen = ctx.content_rect();
        let picture = self.texture.as_ref().map(|texture| {
            let size = texture.size_vec2();
            let scale = (screen.width() * 0.6 / size.x).min(screen.height() * 0.5 / size.y);
            (texture.id(), size * scale)
        });
        let width = picture.map_or(screen.width() * 0.4, |(_, size)| size.x);
        // The picture, a gap, the title, a gap, the bar, a gap and the step's line.
        let height = picture.map_or(0.0, |(_, size)| size.y + 24.0) + 28.0 + 16.0 + 6.0 + 10.0 + 16.0;
        let mut y = screen.center().y - height / 2.0;
        let left = screen.center().x - width / 2.0;
        if let Some((id, size)) = picture {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(id, Rect::from_min_size(pos2(left, y), size), uv, Color32::WHITE);
            y += size.y + 24.0;
        }
        let title = FontId::proportional(24.0);
        painter.text(pos2(screen.center().x, y), Align2::CENTER_TOP, &self.title, title, Color32::from_gray(235));
        y += 28.0 + 16.0;
        let bar = Rect::from_min_size(pos2(left, y), Vec2::new(width, 6.0));
        painter.rect_filled(bar, 3.0, Color32::from_gray(60));
        let filled = Rect::from_min_size(bar.min, Vec2::new(width * self.shown.clamp(0.0, 1.0), bar.height()));
        painter.rect_filled(filled, 3.0, Color32::from_gray(220));
        y += 6.0 + 10.0;
        let line = FontId::proportional(14.0);
        painter.text(pos2(left, y), Align2::LEFT_TOP, self.step.label(), line, Color32::from_gray(150));
    }
}

/// Draws the start view once, offscreen at the window's `size`, so that the window's first frame finds the frame's
/// targets, the reflection cube and the dials' atlases made: it took 160 ms where the next took 50 (4K, Semetin, M3
/// Pro), at the start orbit.
fn warm_up(
    scene: &mut Scene,
    renderer: &mut Renderer,
    args: &Args,
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) {
    let shown = Shown::Player(scene.cameras.start_view(args.view));
    if let (Some(playback), Some(mounts)) = (&scene.playback, &scene.mounts) {
        playback.apply(renderer, &shown.lens(playback, mounts, None, ChaseLook::default(), args.fov));
    }
    if let (Some(dials), Some(playback)) = (&mut scene.dials, &scene.playback) {
        dials.update(&dial_values(playback, scene.live.as_ref()), None, renderer);
    }
    let aspect = size.0 as f32 / size.1 as f32;
    let (view_projection, sky_view_projection) = view_projections(scene, shown, ChaseLook::default(), args.fov, aspect);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("first frame"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(&mut encoder, &target.create_view(&Default::default()), size, view_projection, sky_view_projection);
    gpu.queue.submit([encoder.finish()]);
    if let Err(e) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        eprintln!("viewer: first frame: {e}");
    }
}
