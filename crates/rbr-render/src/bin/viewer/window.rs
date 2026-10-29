//! The window: its event loop, and each frame of the drive or the free-fly camera.

mod feed;
mod input;
mod results;

use crate::args::Args;
use crate::draw::{dial_values, draw_car, draw_particles, readout, scripted_camera, start_camera, step_characters};
use crate::latency::Latency;
use crate::loading::{Loaded, Loading};
use crate::scene::{take_damage, Scene};
use crate::screenshot::SIZE;
use feed::Feed;
use gilrs::ev::filter::{Filter, Repeat};
use gilrs::Gilrs;
use input::{movement, select_gear, shifter};
use rbr_formats::data::Data;
use rbr_render::bindings::{self, Action, Bindings};
use rbr_render::camera::FreeFly;
use rbr_render::ffb;
use rbr_render::hud::pause::{self, Nav, PauseMenu};
use rbr_render::hud::results::{self as result_actions, ResultsMenu};
use rbr_render::hud::stock::Stock;
use rbr_render::hud::{self as hud_overlay, Overlay};
use rbr_render::icon;
use rbr_render::live::Controls;
use rbr_render::perf_log;
use rbr_render::playback::ChaseLook;
use rbr_render::present::Acquired;
use rbr_render::renderer::{self, Gpu, Renderer};
use rbr_render::shifter::{self, Shifter};
use rbr_render::views::{CarView, Shown};
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

/// The loading screen's frame period.
const LOADING_FRAME: Duration = Duration::from_millis(16);
/// The least time between frames while the window is hidden ([`rbr_render::present::Acquired::Hidden`]).
const HIDDEN_FRAME: Duration = Duration::from_millis(16);
/// Free-fly speed, metres per second, and the multiplier while Shift is held.
const WALK_SPEED: f32 = 20.0;
const FAST_FACTOR: f32 = 5.0;
/// Radians of turn per mouse pixel.
const LOOK_SENSITIVITY: f32 = 0.003;
/// Longest frame step used for movement, seconds. Longer stalls do not jump the camera.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Orbit zoom rate: the distance changes by a factor of `e` per second at 1.0.
const ZOOM_RATE: f32 = 1.0;
/// Chase zoom per mouse-wheel line: each line changes the distance by a factor of `e^WHEEL_ZOOM`.
const WHEEL_ZOOM: f32 = 0.1;
/// Pixels of trackpad scroll counted as one wheel line.
const PIXELS_PER_LINE: f32 = 20.0;
/// A held gamepad button repeats its menu move after this and then this often, as a held key does.
const PAD_REPEAT: Repeat = Repeat { after: Duration::from_millis(400), every: Duration::from_millis(80) };

/// Rendered frames per second, averaged over about half a second so the shown number stays readable.
/// Ours: the original has no FPS counter.
#[derive(Debug, Clone, Copy, Default)]
struct FpsMeter {
    frames: u32,
    elapsed: f32,
    shown: Option<f32>,
}

impl FpsMeter {
    /// One more frame took `dt` seconds; the smoothed rate, once a full window has passed.
    fn update(&mut self, dt: f32) -> Option<f32> {
        self.frames += 1;
        self.elapsed += dt;
        if self.elapsed >= 0.5 {
            self.shown = Some(self.frames as f32 / self.elapsed.max(f32::EPSILON));
            (self.frames, self.elapsed) = (0, 0.0);
        }
        self.shown
    }
}

/// Opens the window, shows the loading screen while a thread loads what `args` name, then runs the drive in it.
pub fn run_window(args: Args, data: Data, fmod_bank: Option<PathBuf>) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut viewer = Viewer { args: Arc::new(args), data, fmod_bank, phase: Phase::Opening, failed: None };
    event_loop.run_app(&mut viewer).map_err(|e| e.to_string())?;
    viewer.failed.map_or(Ok(()), Err)
}

/// The window's handler: the loading screen, then the drive.
struct Viewer {
    args: Arc<Args>,
    data: Data,
    /// The RSF car's FMOD bank, for the sound the load starts.
    fmod_bank: Option<PathBuf>,
    phase: Phase,
    /// Why the window or the load failed.
    failed: Option<String>,
}

enum Phase {
    Opening,
    Loading(Box<(Shell, Loading)>),
    Running(Box<App>),
}

impl Viewer {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        self.failed = Some(error);
        event_loop.exit();
    }

    /// The loaded scene's drive, in the loading screen's window.
    fn run(&self, shell: Shell, Loaded { scene, renderer, sound }: Loaded) -> App {
        let args = &*self.args;
        let Shell { window, surface, config, stale, gpu, overlay } = shell;
        let camera = start_camera(&scene.stage, scene.chase);
        App::new(scene, args, sound, Running { window, surface, config, stale, gpu, renderer, camera, overlay })
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !matches!(self.phase, Phase::Opening) {
            return;
        }
        match Shell::open(event_loop, &self.args, &self.data) {
            Ok(shell) => {
                let (gpu, config) = (shell.gpu.clone(), &shell.config);
                let (args, data, fmod_bank) = (self.args.clone(), self.data.clone(), self.fmod_bank.clone());
                let loading = Loading::start(args, data, fmod_bank, gpu, config.format, (config.width, config.height));
                self.phase = Phase::Loading(Box::new((shell, loading)));
            }
            Err(error) => self.fail(event_loop, error.to_string()),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let (shell, loading) = match &mut self.phase {
            Phase::Opening => return,
            Phase::Loading(loading) => &mut **loading,
            Phase::Running(app) => return app.window_event(event_loop, id, event),
        };
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Escape), state: ElementState::Pressed, .. },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(size) => shell.resize(size),
            WindowEvent::RedrawRequested => shell.draw_loading(loading),
            _ => {}
        }
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let Phase::Running(app) = &mut self.phase {
            app.device_event(event_loop, id, event);
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Phase::Running(app) = &mut self.phase {
            self.failed = app.replay_failure.take();
            app.exiting(event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (shell, loading) = match &mut self.phase {
            Phase::Opening => return,
            Phase::Loading(loading) => &mut **loading,
            Phase::Running(app) => return app.about_to_wait(event_loop),
        };
        let Some(loaded) = loading.poll() else {
            shell.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + LOADING_FRAME));
            return;
        };
        let Phase::Loading(loading) = std::mem::replace(&mut self.phase, Phase::Opening) else { unreachable!() };
        match loaded {
            Ok(loaded) => self.phase = Phase::Running(Box::new(self.run(loading.0, loaded))),
            Err(error) => self.fail(event_loop, error),
        }
    }
}

/// The window with its surface, GPU and overlay: what the loading screen draws in, and the drive after it.
struct Shell {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// `config` is not yet the surface's ([`rbr_render::present::configure`]).
    stale: bool,
    gpu: Gpu,
    overlay: Overlay,
}

impl Shell {
    fn open(event_loop: &ActiveEventLoop, args: &Args, data: &Data) -> Result<Shell, Box<dyn Error>> {
        let attributes = Window::default_attributes().with_title("ReRBR").with_window_icon(icon::window_icon());
        let attributes = match args.size {
            Some((width, height)) => attributes.with_inner_size(PhysicalSize::new(width, height)),
            None => attributes.with_inner_size(LogicalSize::new(SIZE.0, SIZE.1)),
        };
        // Borderless on the current monitor: the surface follows the monitor's size (`WindowEvent::Resized`), and a
        // Wayland compositor can scan it out directly.
        let attributes = attributes.with_fullscreen(args.fullscreen.then_some(Fullscreen::Borderless(None)));
        // The Wayland app id and the X11 class (one field in winit), which compositor rules can match.
        #[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
        let attributes =
            winit::platform::wayland::WindowAttributesExtWayland::with_name(attributes, icon::APP_ID, icon::APP_ID);
        let window = Arc::new(event_loop.create_window(attributes)?);
        let instance = renderer::instance();
        let surface = instance.create_surface(window.clone())?;
        let gpu = renderer::request_gpu(&instance, Some(&surface))?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = rbr_render::present::surface_format(&capabilities.formats);
        let size = window.inner_size();
        let present_mode = args.present.pick(&capabilities.present_modes);
        eprintln!("viewer: presenting with {present_mode:?} (asked for {})", args.present.name());
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            // The frame's alpha is not shown, as with the game's X8R8G8B8 back buffer (see `write_png`).
            alpha_mode: if capabilities.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                capabilities.alpha_modes[0]
            },
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
            // Two frames queued: the frame waits for its texture before it reads the input ([`App::frame`]), so the
            // second adds no lag, and with one the frame missed every other refresh once the wait came first (27
            // frames a second against 60 at 4K on cachy, `RBR_LATENCY`).
            desired_maximum_frame_latency: 2,
        };
        let stale = !rbr_render::present::configure(&surface, &gpu.device, &config);
        let overlay = Overlay::with_hud(&gpu.device, format, Box::new(Stock::new(data)));
        Ok(Shell { window, surface, config, stale, gpu, overlay })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.stale = !rbr_render::present::configure(&self.surface, &self.gpu.device, &self.config);
    }

    /// A frame of the loading screen: black under it.
    fn draw_loading(&mut self, loading: &mut Loading) {
        if self.stale {
            self.stale = !rbr_render::present::configure(&self.surface, &self.gpu.device, &self.config);
            if self.stale {
                return;
            }
        }
        let frame = match rbr_render::present::acquire(&self.surface) {
            Acquired::Frame(frame) => frame,
            // The loading screen's frames are paced by `about_to_wait`.
            Acquired::Hidden => return,
            Acquired::Failed => {
                self.stale = !rbr_render::present::configure(&self.surface, &self.gpu.device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("loading"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        let size = (self.config.width, self.config.height);
        self.overlay
            .draw_with(&self.gpu.device, &self.gpu.queue, &mut encoder, &view, size, |ctx, _| loading.paint(ctx));
        self.gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
    }
}

/// Everything that exists once the window does.
struct Running {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// `config` is not yet the surface's ([`rbr_render::present::configure`]).
    stale: bool,
    gpu: Gpu,
    renderer: Renderer,
    camera: FreeFly,
    /// The HUD, drawn with a drive.
    overlay: Overlay,
}

pub struct App {
    /// What the viewer shows.
    scene: Scene,
    /// The smoothed frame rate, for the HUD's FPS counter.
    meter: FpsMeter,
    /// The camera view of the drive (C cycles it).
    view: CarView,
    /// What the last frame showed: the view, or a camera the game forced over it.
    shown: Shown,
    /// Every camera's horizontal field of view, radians (`--fov`).
    fov: f32,
    /// `--max-fps`: the shortest time between drawn frames.
    frame_period: Option<Duration>,
    controls: Controls,
    /// The live mode's keys and device inputs (`--bindings`).
    bindings: Bindings,
    /// The gamepads, for the pause menu's navigation.
    gilrs: Option<Gilrs>,
    /// The wheel's force feedback (live mode), which the live process's step records set as they come.
    wheel: Option<Arc<ffb::Wheel>>,
    /// Each action's device input in the last frame, for the gear and restart presses.
    pads: [f32; Action::ALL.len()],
    /// The advanced H-shifter (`h-shifter = advanced`), in live mode.
    shifter: Option<Shifter>,
    /// The input thread (live mode).
    feed: Option<Feed>,
    /// Watching a replay (`--replay`): the stepping process serves its frames, and the window takes no device
    /// input, opens no menus and saves no ghost.
    replay: bool,
    /// The live drive whose last stage result is saved when the window closes.
    save_replay: Option<PathBuf>,
    /// A failed replay closes the window and reports failure to the launcher.
    replay_failure: Option<String>,
    running: Running,
    /// Keys currently held.
    held: HashSet<KeyCode>,
    /// Whether the left mouse button is held (mouse look).
    looking: bool,
    pointer: Option<PhysicalPosition<f64>>,
    /// The chase camera's turn around the car (`--drive`, `--live`): left drag turns, the wheel zooms, V resets.
    look: ChaseLook,
    last_frame: Instant,
    /// When the window opened (the renderer's time).
    started: Instant,
    /// The viewer time that runs the characters without a drive.
    clock: f64,
    /// The live drive's sound (`--no-sound` turns it off).
    sound: Option<rbr_render::sound::Player>,
    quit_after: Option<(f32, bool)>,
    /// When the live race was first seen over (`--quit-after end+S`).
    ended: Option<Instant>,
    /// The live drive's pause menu, while it is open (the drive is paused).
    menu: Option<PauseMenu>,
    /// Actions alongside the stage result, independent of HUD visibility.
    results: Option<ResultsMenu>,
    /// When the open menu paused the drive, and the time paused before it (the renderer's time leaves it out).
    paused_at: Option<Instant>,
    paused: Duration,
    /// The menu's exit was chosen: the window closes.
    exit: bool,
    /// The live input's timing (`RBR_LATENCY`), printed when the app ends.
    latency: Option<Latency>,
}

impl App {
    fn new(scene: Scene, args: &Args, sound: Option<rbr_render::sound::Player>, running: Running) -> App {
        let view = scene.cameras.start_view(args.view);
        let ffb = args.ffb.clone();
        let replay = args.replay.is_some();
        let shifter = shifter(args, scene.live.is_some() && !replay);
        // Gamepads navigate the menu in live mode only.
        let gilrs = scene.live.as_ref().filter(|_| !replay).and_then(|_| match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(e) => {
                eprintln!("viewer: no gamepads: {e}");
                None
            }
        });
        // The wheel's rotation, in live mode when set (again here: a wheel plugged in again has its own).
        if let Some(degrees) = ffb.rotation.filter(|_| scene.live.is_some() && !replay) {
            match rbr_render::wheel_rotation::set(degrees) {
                Ok(wheels) => wheels.iter().for_each(|w| eprintln!("viewer: {} turns {} degrees", w.name, w.degrees)),
                Err(e) => eprintln!("viewer: wheel rotation not set: {e}"),
            }
        }
        // The wheel's force, in live mode with force feedback on.
        let wheel =
            scene.live.as_ref().filter(|_| !replay && ffb.settings.enabled).and_then(|_| match ffb::Wheel::open(ffb) {
                Ok(wheel) => {
                    eprintln!("viewer: force feedback on {}", wheel.name);
                    Some(Arc::new(wheel))
                }
                Err(e) => {
                    eprintln!("viewer: no force feedback: {e}");
                    None
                }
            });
        let mut latency = scene.live.as_ref().and_then(|_| Latency::from_env());
        if let Some(latency) = &mut latency {
            latency.watch_screen(&running.window);
        }
        let feed = scene.live.as_ref().filter(|_| !replay).map(|_| {
            let script = latency.as_ref().and_then(Latency::script);
            Feed::start(args.bindings.clone(), script, latency.is_some())
        });
        let mut app = App {
            scene,
            clock: 0.0,
            meter: FpsMeter::default(),
            view,
            shown: Shown::Player(view),
            fov: args.fov,
            frame_period: args.max_fps.map(|fps| Duration::from_secs_f32(1.0 / fps)),
            controls: Controls::default(),
            bindings: args.bindings.clone(),
            gilrs,
            wheel,
            pads: [0.0; Action::ALL.len()],
            shifter,
            feed,
            replay,
            save_replay: args.save_replay.clone().filter(|_| !replay),
            replay_failure: None,
            running,
            held: HashSet::new(),
            looking: false,
            pointer: None,
            look: ChaseLook::default(),
            last_frame: Instant::now(),
            started: Instant::now(),
            sound,
            quit_after: args.quit_after,
            ended: None,
            menu: None,
            results: None,
            paused_at: None,
            paused: Duration::ZERO,
            exit: false,
            latency,
        };
        app.attach_live();
        if perf_log::enabled() {
            let running = &mut app.running;
            let (config, asked) = (&running.config, args.present.name());
            let present = format!("{:?} (asked for {asked})", config.present_mode);
            crate::perf::begin(
                args,
                &running.gpu.adapter,
                (config.width, config.height),
                present,
                &mut running.renderer,
            );
        }
        app
    }

    /// Points the input thread and the wheel at the live process (started, or started over), whose step records then
    /// set the wheel's force and the dead zone's steering as they come.
    fn attach_live(&mut self) {
        let Some(process) = &self.scene.live else { return };
        if let Some(feed) = &self.feed {
            feed.attach(process.sender());
        }
        if let Some(wheel) = &self.wheel {
            let wheel = Arc::clone(wheel);
            process.on_step(move |step| wheel.set(step.force, step.steering()));
        }
        if self.latency.is_some() {
            process.keep_steps();
        }
    }

    fn frame(&mut self) {
        perf_log::frame();
        let mut span = perf_log::span("acquire surface");
        // The surface's texture first: acquiring it is where the loop waits (for a free image, the display's
        // refresh under fifo), so the input read and the live answers after it are the newest the frame can show.
        let acquire = Instant::now();
        let running = &mut self.running;
        if running.stale {
            running.stale = !rbr_render::present::configure(&running.surface, &running.gpu.device, &running.config);
            if running.stale {
                return;
            }
        }
        let frame = match rbr_render::present::acquire(&running.surface) {
            Acquired::Frame(frame) => Some(frame),
            // Hidden, the frame runs all the same (the input, the live process's steps, the sound) without being
            // drawn, at most once a `HIDDEN_FRAME`: nothing waits for a free image then.
            Acquired::Hidden => {
                if let Some(wait) = (self.last_frame + HIDDEN_FRAME).checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
                None
            }
            Acquired::Failed => {
                running.stale = !rbr_render::present::configure(&running.surface, &running.gpu.device, &running.config);
                return;
            }
        };
        let now = Instant::now();
        if let Some(latency) = &mut self.latency {
            latency.acquired(acquire, now);
        }
        let dt = (now - self.last_frame).as_secs_f32().min(MAX_FRAME_SECONDS);
        self.last_frame = now;
        span.next("input");
        if let Some(gilrs) = &mut self.gilrs {
            let mut navs = Vec::new();
            while let Some(event) = gilrs.next_event().filter_ev(&PAD_REPEAT, gilrs) {
                match event.event {
                    gilrs::EventType::ButtonPressed(button, _) => navs.extend(Nav::of_button(button)),
                    // A held d-pad repeats its move, as a held key does (the filter's events are not in the state).
                    gilrs::EventType::ButtonRepeated(button, _) => {
                        gilrs.update(&event);
                        navs.extend(Nav::of_button(button).filter(|nav| !matches!(nav, Nav::Select | Nav::Back)));
                    }
                    _ => {}
                }
            }
            if self.menu.is_some() {
                navs.into_iter().for_each(|nav| self.menu_nav(nav));
            } else if self.results.is_some() {
                navs.into_iter().for_each(|nav| self.results_nav(nav));
            } else if navs.contains(&Nav::Select) {
                self.accept_message();
            }
        }
        // The devices' presses, as the input thread last polled them.
        let polled = self.feed.as_ref().map(Feed::polled);
        if let Some((pads, _)) = polled {
            let before = std::mem::replace(&mut self.pads, pads);
            for action in bindings::pad_presses(before, pads) {
                self.press(action);
            }
        }
        if !self.replay {
            self.sync_results();
        }
        let running = &mut self.running;
        let speed = WALK_SPEED * if self.held.contains(&KeyCode::ShiftLeft) { FAST_FACTOR } else { 1.0 };
        let movement = movement(&self.held);
        let (width, height) = (running.config.width, running.config.height);
        let aspect = width as f32 / height as f32;
        let mut live_failed = false;
        if self.menu.is_some() {
            // Paused: the stepping process's clock is stopped, so its game, clocks and car stand still.
        } else if let (Some(process), Some(playback)) = (&mut self.scene.live, &mut self.scene.playback) {
            let held = Action::ALL.map(|a| self.bindings.get(a).key.is_some_and(|k| self.held.contains(&k)));
            // The input thread polls the devices (and `RBR_LATENCY`'s script) over these: the frame gives the flags
            // and the gear it selects.
            if let Some(shifter) = &mut self.shifter {
                // The advanced H-shifter selects itself: NGP's poll sees no direct gear actions.
                let polled = polled.map_or(self.controls, |(_, c)| c);
                let held = |a: Action| held[a as usize] || self.pads[a as usize] > bindings::PRESS;
                let action = |word| Action::ALL.into_iter().find(|a| a.gear() == Some(word)).is_some_and(held);
                let inputs = shifter::Inputs {
                    lever: if held(Action::Neutral) { Some(shifter::NEUTRAL) } else { shifter::lever(action) },
                    engaged: u8::try_from(process.hud.gear).unwrap_or(shifter::NEUTRAL),
                    clutch: polled.clutch,
                    throttle: polled.throttle,
                    engine_speed: process.hud.engine_speed,
                    gearbox: process.hud.gearbox,
                };
                if let Some(gear) = shifter.update((now - self.started).as_secs_f32(), inputs) {
                    select_gear(&mut self.controls, gear);
                }
                if let Some(sound) = &mut self.sound {
                    sound.grind(shifter.grind());
                }
            }
            if let Some(feed) = &self.feed {
                feed.post(&mut self.controls, &self.held, self.shifter.is_some());
            }
            span.next("live link");
            let pumped = process.pump();
            if let (Some(latency), Some(feed)) = (&mut self.latency, &self.feed) {
                latency.input(feed.take_sends(), process.take_steps(), process.newest_step());
            }
            match pumped {
                Ok(frames) => {
                    for frame in frames {
                        match process.parse(&frame) {
                            Ok(drive) => playback.push(drive),
                            Err(e) => eprintln!("viewer: live frame: {e}"),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("viewer: {e}");
                    if self.replay {
                        self.replay_failure = Some(e);
                        self.exit = true;
                    }
                    live_failed = true;
                }
            }
            span.next("live state and sound");
            running.renderer.set_interactive_poses(&process.interactive_poses);
            take_damage(playback, process);
            if !self.replay {
                if let Some(ghost) = &mut self.scene.ghost {
                    let penalties = process.hud.race.map_or(0.0, |race| race.penalties);
                    ghost.observe(process.splits.as_ref(), penalties, playback.sample().step);
                }
            }
            if process.splits.as_ref().is_some_and(|s| s.over) {
                self.ended.get_or_insert(now);
            }
            playback.reverse = process.hud.gear == 0;
            if let Some(lamps) = process.lamps {
                playback.set_lamps(lamps);
            }
            // The race flow's retire codes 1 and 2 (and a rally incident with the gate shut) call the helicopter.
            if process.take_rescue_call() {
                if let Some(characters) = &mut self.scene.characters {
                    characters.call_rescue();
                }
            }
            let sounds = process.take_sounds();
            if let Some(sound) = &mut self.sound {
                if let Some(mounts) = &self.scene.mounts {
                    let lens = self.shown.lens(playback, mounts, None, self.look, self.fov);
                    let interior = self.shown.car_view().interior();
                    sound.set_scene(rbr_render::sound::scene(&lens, &playback.sample().car, interior));
                }
                sounds.iter().for_each(|s| sound.step(s));
            }
        } else if let Some(playback) = &mut self.scene.playback {
            playback.advance(dt as f64);
        }
        span.next("scene update");
        if live_failed {
            self.scene.live = None;
            // Dropping the wheel stops its force.
            self.wheel = None;
        }
        if self.menu.is_none() {
            self.clock += f64::from(dt);
        }
        if let Some(characters) = &mut self.scene.characters {
            let race = self.scene.live.as_ref().and_then(|l| l.hud.race);
            let events = step_characters(characters, self.scene.playback.as_ref(), self.clock, race);
            if let Some(sound) = &mut self.sound {
                sound.events(&rbr_render::sound::events(&events, characters.rescue_position()));
            }
        }
        // The game's start orbit replaces the player's view until the go, and the helicam while the helicopter runs.
        let camera_record = self.scene.live.as_ref().and_then(|live| live.hud.camera);
        let (heli, flip, _) = scripted_camera(self.scene.characters.as_ref());
        let before = self.shown;
        self.shown = Shown::of(self.view, camera_record, heli, flip);
        // The Camera page shows the view it sets, also over the start orbit and the helicam.
        if self.menu.as_ref().is_some_and(|menu| menu.camera().is_some()) {
            self.shown = Shown::Player(self.view);
        }
        // A cut to another camera: the temporal anti-aliasing's history is of the last one's view.
        if !before.same_camera(self.shown) {
            running.renderer.reset_history();
        }
        let looking_back = self.bindings.get(Action::LookBack).key.is_some_and(|k| self.held.contains(&k))
            || self.pads[Action::LookBack as usize] > bindings::PRESS;
        if let Some(playback) = &mut self.scene.playback {
            playback.follow_game_chase(self.shown.swing(), looking_back);
        }
        let fov = self.fov;
        let car_lens = draw_car(&mut self.scene, &mut running.renderer, self.shown, self.look, fov);
        let (view_projection, sky_view_projection) = match (&mut self.scene.orbit, car_lens) {
            (Some(orbit), _) => {
                orbit.zoom((movement.forward * -ZOOM_RATE * dt).exp());
                (orbit.view_projection(fov, aspect), orbit.sky_view_projection(fov, aspect))
            }
            (None, Some(lens)) => (lens.view_projection(aspect), lens.sky_view_projection(aspect)),
            (None, None) => {
                if self.scene.playback.is_none() {
                    running.camera.step(movement, speed * dt);
                }
                (running.camera.view_projection(fov, aspect), running.camera.sky_view_projection(fov, aspect))
            }
        };
        if let Some(characters) = &mut self.scene.characters {
            characters.apply(&mut running.renderer, view_projection);
        }
        if let (Some(ghost), Some(playback)) = (&mut self.scene.ghost, &self.scene.playback) {
            ghost.apply(&mut running.renderer, playback, view_projection);
        }
        draw_particles(&mut self.scene, &mut running.renderer, view_projection);
        if let (Some(drawn), Some(playback)) = (&mut self.scene.fences, &self.scene.playback) {
            drawn.draw(&mut running.renderer, playback, view_projection);
        }
        let Some(frame) = frame else {
            return;
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = running.gpu.device.create_command_encoder(&Default::default());
        let paused = self.paused + self.paused_at.map_or(Duration::ZERO, |at| at.elapsed());
        let elapsed = self.started.elapsed() - paused;
        running.renderer.set_time(elapsed.as_secs_f32());
        running.renderer.set_start_clock(self.scene.live.as_ref().and_then(|l| l.hud_model.view().start_clock));
        running.renderer.set_start_lamps(self.scene.live.as_ref().and_then(|l| l.hud.race.as_ref()));
        if let (Some(dials), Some(playback)) = (&mut self.scene.dials, &self.scene.playback) {
            dials.update(&dial_values(playback, self.scene.live.as_ref()), Some(dt), &mut running.renderer);
        }
        span.next("render");
        running.renderer.render(&mut encoder, &view, (width, height), view_projection, sky_view_projection);
        span.next("HUD");
        let readout = self.scene.playback.as_ref().filter(|_| self.scene.hud.show).map(|playback| {
            let fps = if self.scene.hud.fps { self.meter.update(dt) } else { None };
            let (live, shown, bindings) = (self.scene.live.as_ref(), self.shown, &self.bindings);
            readout(playback, live, shown, bindings, self.scene.hud, fps)
        });
        if readout.is_some() || self.menu.is_some() || self.results.is_some() {
            let (gpu, menu) = (&running.gpu, &self.menu);
            running.overlay.draw_with(&gpu.device, &gpu.queue, &mut encoder, &view, (width, height), |ctx, hud| {
                readout.iter().for_each(|r| hud_overlay::paint_readout(ctx, hud, r));
                menu.iter().for_each(|m| pause::paint(ctx, m));
                self.results.iter().for_each(|m| result_actions::paint(ctx, m));
            });
        }
        span.next("submit");
        running.gpu.queue.submit([encoder.finish()]);
        let submitted = Instant::now();
        span.next("GPU timing readback");
        running.renderer.end_frame();
        span.next("present");
        if let Some(latency) = &mut self.latency {
            latency.presenting();
        }
        running.window.pre_present_notify();
        running.gpu.queue.present(frame);
        if let Some(latency) = &mut self.latency {
            if latency.drawn(submitted) {
                latency.watch_gpu(&running.gpu.device, &running.gpu.queue);
            }
        }
    }

    /// Writes the live drive's ghost (`--save-ghost`); a replay saves none.
    fn finish_ghost(&self) {
        if self.replay {
            return;
        }
        if let (Some(ghost), Some(playback)) = (&self.scene.ghost, &self.scene.playback) {
            ghost.finish(playback);
        }
    }
}

impl ApplicationHandler for App {
    /// The window is open before the app is made ([`Viewer`]).
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    /// The live input's timing, while the Wayland connection its feedback shares is still open.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let (Some(path), Some(splits)) =
            (&self.save_replay, self.scene.live.as_ref().and_then(|live| live.splits.as_ref()))
        {
            if let Err(e) = rbr_recording::replay::save_result(path, splits) {
                eprintln!("viewer: replay result: {e}");
            }
        }
        if let Some(latency) = self.latency.take() {
            eprintln!("viewer: {}", latency.summary());
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let events = perf_log::span("window events");
        match event {
            WindowEvent::CloseRequested => {
                self.finish_ghost();
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                let running = &mut self.running;
                running.config.width = size.width.max(1);
                running.config.height = size.height.max(1);
                running.stale = !rbr_render::present::configure(&running.surface, &running.gpu.device, &running.config);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, repeat, .. },
                ..
            } => match state {
                ElementState::Pressed if self.results.is_some() => {
                    self.results_key(code, repeat);
                }
                ElementState::Pressed
                    if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) && self.accept_message() => {}
                ElementState::Pressed if self.replay => {
                    self.replay_key(code, event_loop);
                }
                ElementState::Pressed if self.scene.live.is_some() && code == KeyCode::Escape => {
                    self.press(Action::Pause);
                }
                ElementState::Pressed if self.menu.is_some() => {
                    match self.bindings.action_for_key(code) {
                        Some(Action::Pause) => self.press(Action::Pause),
                        _ => Nav::of_key(code).into_iter().for_each(|nav| self.menu_nav(nav)),
                    }
                    self.held.insert(code);
                }
                ElementState::Pressed => {
                    if code == KeyCode::Escape {
                        self.finish_ghost();
                        event_loop.exit();
                    }
                    if code == KeyCode::KeyV {
                        self.look = ChaseLook::default();
                    }
                    if self.scene.live.is_some() {
                        if let Some(action) = self.bindings.action_for_key(code) {
                            self.press(action);
                        }
                    } else if let Some(
                        action @ (Action::Wipers | Action::Headlights | Action::Camera | Action::GaugeMode),
                    ) = self.bindings.action_for_key(code)
                    {
                        self.press(action);
                    } else if let Some(playback) = &mut self.scene.playback {
                        match code {
                            KeyCode::KeyP => playback.paused = !playback.paused,
                            KeyCode::ArrowRight => playback.step(1),
                            KeyCode::ArrowLeft => playback.step(-1),
                            KeyCode::ArrowUp => playback.rate = (playback.rate * 2.0).min(16.0),
                            KeyCode::ArrowDown => playback.rate = (playback.rate / 2.0).max(1.0 / 16.0),
                            KeyCode::KeyR => {
                                playback.restart();
                                self.running.renderer.reset_history();
                            }
                            // Ours: calls the rescue helicopter, as the game flow does on a retire.
                            KeyCode::KeyK => {
                                if let Some(characters) = &mut self.scene.characters {
                                    characters.call_rescue();
                                }
                            }
                            _ => {}
                        }
                    }
                    self.held.insert(code);
                }
                ElementState::Released => {
                    self.held.remove(&code);
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some(position);
                self.results_hover();
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } if self.results.is_some() => {
                if state == ElementState::Pressed {
                    self.results_click();
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.looking = state == ElementState::Pressed && !self.accept_message()
            }
            WindowEvent::MouseWheel { delta, .. } if self.scene.playback.is_some() => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_LINE,
                };
                // Scrolling up (positive) moves the eye in.
                self.look.zoom_by((-lines * WHEEL_ZOOM).exp());
            }
            WindowEvent::RedrawRequested => {
                drop(events);
                self.frame()
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        let _events = perf_log::span("window events");
        if let (true, DeviceEvent::MouseMotion { delta: (dx, dy) }) = (self.looking, &event) {
            let (yaw, pitch) = (
                *dx as f32 * LOOK_SENSITIVITY,
                // Mouse up (negative dy) looks up.
                -*dy as f32 * LOOK_SENSITIVITY,
            );
            match (&mut self.scene.orbit, &self.scene.playback) {
                (Some(orbit), _) => orbit.turn(yaw, pitch),
                (None, Some(_)) => self.look.turn(yaw, pitch),
                (None, None) => self.running.camera.turn(yaw, pitch),
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let since = |(_, end): (f32, bool)| if end { self.ended } else { Some(self.started) };
        let quit = self.quit_after.is_some_and(|q| since(q).is_some_and(|t| t.elapsed().as_secs_f32() > q.0));
        if quit || self.exit {
            self.finish_ghost();
            event_loop.exit();
        }
        // Under `--max-fps` the next frame waits for its time.
        match self.frame_period.map(|period| self.last_frame + period).filter(|due| *due > Instant::now()) {
            Some(due) => event_loop.set_control_flow(ControlFlow::WaitUntil(due)),
            // The next frame is drawn now, not on `RedrawRequested`: Wayland compositors send that only after
            // they took the last frame, which waits for the GPU, so the CPU's half of a frame never overlapped
            // the GPU's (49 fps against 68 in a cockpit on cachy's Hyprland). Acquiring the surface's texture
            // holds the loop back instead. macOS keeps the redraw request.
            None if cfg!(not(target_os = "macos")) => {
                event_loop.set_control_flow(ControlFlow::Poll);
                self.frame();
            }
            None => {
                event_loop.set_control_flow(ControlFlow::Wait);
                self.running.window.request_redraw();
            }
        }
    }
}
