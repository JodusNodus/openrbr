//! The window: its event loop, and each frame of the drive or the free-fly camera.

mod input;

use crate::args::Args;
use crate::draw::{dial_values, readout, scripted_camera, show_crew, start_camera, step_characters, throw_smoke};
use crate::scene::{take_damage, Scene};
use crate::screenshot::SIZE;
use gilrs::ev::filter::{Filter, Repeat};
use gilrs::Gilrs;
use input::{movement, select_gear, shifter};
use rbr_render::bindings::{self, Action, Bindings};
use rbr_render::camera::FreeFly;
use rbr_render::ffb;
use rbr_render::hud::pause::{self, Nav, PauseMenu};
use rbr_render::hud::{self as hud_overlay, Overlay};
use rbr_render::live::Controls;
use rbr_render::playback::{ChaseLook, Drive};
use rbr_render::present::Present;
use rbr_render::renderer::{self, Enhance, Gpu, Renderer};
use rbr_render::shifter::{self, Shifter};
use rbr_render::views::{CarView, Shown};
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

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
/// The window's Wayland app id and X11 class.
#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
const APP_ID: &str = "rbr-recomp";

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

pub fn run_window(scene: Scene, args: &Args, sound: Option<rbr_render::sound::Player>) -> Result<(), String> {
    let view = scene.cameras.start_view(args.view);
    let (size, fov, enhance, ffb) = (args.size, args.fov, args.enhance, args.ffb.clone());
    let (render_scale, present, fullscreen) = (args.render_scale, args.present, args.fullscreen);
    let frame_period = args.max_fps.map(|fps| Duration::from_secs_f32(1.0 / fps));
    let (bindings, quit_after) = (args.bindings.clone(), args.quit_after);
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let shifter = shifter(args, scene.live.is_some());
    // Gamepads are read in live mode only.
    let gilrs = scene.live.as_ref().and_then(|_| match Gilrs::new() {
        Ok(gilrs) => Some(gilrs),
        Err(e) => {
            eprintln!("viewer: no gamepads: {e}");
            None
        }
    });
    // The wheel's force, in live mode with force feedback on.
    let wheel = scene.live.as_ref().filter(|_| ffb.settings.enabled).and_then(|_| match ffb::Wheel::open(ffb) {
        Ok(wheel) => {
            eprintln!("viewer: force feedback on {}", wheel.name);
            Some(wheel)
        }
        Err(e) => {
            eprintln!("viewer: no force feedback: {e}");
            None
        }
    });
    let mut app = App {
        scene,
        clock: 0.0,
        meter: FpsMeter::default(),
        view,
        shown: Shown::Player(view),
        size,
        fov,
        enhance,
        render_scale,
        present,
        frame_period,
        fullscreen,
        controls: Controls::default(),
        bindings,
        gilrs,
        wheel,
        pads: [0.0; Action::ALL.len()],
        gear_held: false,
        shifter,
        running: None,
        held: HashSet::new(),
        looking: false,
        look: ChaseLook::default(),
        last_frame: Instant::now(),
        started: Instant::now(),
        sound,
        quit_after,
        ended: None,
        menu: None,
        paused_at: None,
        paused: Duration::ZERO,
        exit: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

/// Everything that exists once the window does.
struct Running {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
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
    /// The window's size in pixels (`--size`), else [`SIZE`] in points.
    size: Option<(u32, u32)>,
    /// Every camera's horizontal field of view, radians (`--fov`).
    fov: f32,
    /// Renderer additions (`--enhance`).
    enhance: Enhance,
    /// `--render-scale`.
    render_scale: Option<f32>,
    present: Present,
    /// `--max-fps`: the shortest time between drawn frames.
    frame_period: Option<Duration>,
    fullscreen: bool,
    controls: Controls,
    /// The live mode's keys and gamepad inputs (`--bindings`).
    bindings: Bindings,
    gilrs: Option<Gilrs>,
    /// The wheel's force feedback (live mode).
    wheel: Option<ffb::Wheel>,
    /// Each action's gamepad value in the last frame, for the gear and restart presses.
    pads: [f32; Action::ALL.len()],
    /// Whether a direct gear action was held in the last frame, for the release to neutral.
    gear_held: bool,
    /// The advanced H-shifter (`h-shifter = advanced`), in live mode.
    shifter: Option<Shifter>,
    running: Option<Running>,
    /// Keys currently held.
    held: HashSet<KeyCode>,
    /// Whether the left mouse button is held (mouse look).
    looking: bool,
    /// The chase camera's turn around the car (`--drive`, `--live`): left drag turns, the wheel zooms, V resets.
    look: ChaseLook,
    last_frame: Instant,
    /// When the window opened (the wind's time).
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
    /// When the open menu paused the drive, and the time paused before it (the wind's time leaves it out).
    paused_at: Option<Instant>,
    paused: Duration,
    /// The menu's exit was chosen: the window closes.
    exit: bool,
}

impl App {
    fn create(&self, event_loop: &ActiveEventLoop) -> Result<Running, Box<dyn Error>> {
        let attributes = Window::default_attributes().with_title("ReRBR");
        let attributes = match self.size {
            Some((width, height)) => attributes.with_inner_size(PhysicalSize::new(width, height)),
            None => attributes.with_inner_size(LogicalSize::new(SIZE.0, SIZE.1)),
        };
        // Borderless on the current monitor: the surface follows the monitor's size (`WindowEvent::Resized`), and a
        // Wayland compositor can scan it out directly.
        let attributes = attributes.with_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        // The Wayland app id and the X11 class (one field in winit), which compositor rules can match.
        #[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(attributes, APP_ID, APP_ID);
        let window = Arc::new(event_loop.create_window(attributes)?);
        let instance = renderer::instance();
        let surface = instance.create_surface(window.clone())?;
        let gpu = renderer::request_gpu(&instance, Some(&surface))?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = rbr_render::present::surface_format(&capabilities.formats);
        let size = window.inner_size();
        let present_mode = self.present.pick(&capabilities.present_modes);
        eprintln!("viewer: presenting with {present_mode:?} (asked for {})", self.present.name());
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
            // One frame queued: the drive is shown as soon as it is drawn (2 added a 60 Hz frame of lag).
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&gpu.device, &config);
        let renderer = self.scene.renderer(&gpu, format, self.enhance, self.render_scale);
        let overlay = Overlay::new(&gpu.device, format);
        Ok(Running {
            window,
            surface,
            config,
            gpu,
            renderer,
            camera: start_camera(&self.scene.stage, self.scene.chase),
            overlay,
        })
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(MAX_FRAME_SECONDS);
        self.last_frame = now;
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
            let pads = self.bindings.read_pads(gilrs);
            let before = std::mem::replace(&mut self.pads, pads);
            for action in bindings::pad_presses(before, pads) {
                self.press(action);
            }
            if self.menu.is_some() {
                navs.into_iter().for_each(|nav| self.menu_nav(nav));
            } else if navs.contains(&Nav::Select) {
                self.accept_message();
            }
        }
        let Some(running) = &mut self.running else {
            return;
        };
        let speed = WALK_SPEED * if self.held.contains(&KeyCode::ShiftLeft) { FAST_FACTOR } else { 1.0 };
        let movement = movement(&self.held);
        let (width, height) = (running.config.width, running.config.height);
        let aspect = width as f32 / height as f32;
        let mut live_failed = false;
        if self.menu.is_some() {
            // Paused: the stepping process gets no payload, so its game, clocks and car stand still.
        } else if let (Some(process), Some(playback)) = (&mut self.scene.live, &mut self.scene.playback) {
            let held = Action::ALL.map(|a| self.bindings.get(a).key.is_some_and(|k| self.held.contains(&k)));
            self.controls = self.bindings.controls(held, self.pads, self.controls);
            if let Some(shifter) = &mut self.shifter {
                let held = |a: Action| held[a as usize] || self.pads[a as usize] > bindings::PRESS;
                let action = |word| Action::ALL.into_iter().find(|a| a.gear() == Some(word)).is_some_and(held);
                let inputs = shifter::Inputs {
                    lever: if held(Action::Neutral) {
                        Some(shifter::NEUTRAL)
                    } else {
                        shifter::lever(action, shifter.gearbox.dogleg)
                    },
                    clutch: self.controls.clutch,
                    throttle: self.controls.throttle,
                    engine_speed: process.hud.engine_speed,
                    gearbox: process.hud.gearbox,
                };
                if let Some(gear) = shifter.update((now - self.started).as_secs_f32(), inputs) {
                    select_gear(&mut self.controls, gear);
                }
                if let Some(sound) = &mut self.sound {
                    sound.grind(shifter.grind());
                }
            } else {
                let gear_held = bindings::gear_held(held, self.pads);
                if std::mem::replace(&mut self.gear_held, gear_held) && !gear_held && self.bindings.neutral_on_release()
                {
                    select_gear(&mut self.controls, shifter::NEUTRAL);
                }
            }
            match process.pump(&mut self.controls) {
                Ok(frames) => {
                    for frame in frames {
                        match Drive::parse_with(&frame, process.clock) {
                            Ok(drive) => playback.push(drive),
                            Err(e) => eprintln!("viewer: live frame: {e}"),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("viewer: {e}");
                    live_failed = true;
                }
            }
            if let Some(wheel) = &self.wheel {
                wheel.set(process.hud.force.unwrap_or(0.0), process.hud.ngp, self.controls.steering);
            }
            running.renderer.set_interactive_poses(&process.interactive_poses);
            take_damage(playback, process);
            if let Some(ghost) = &mut self.scene.ghost {
                ghost.observe(process.splits.as_ref(), playback.sample().step);
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
        let (heli, flip, scripted) = scripted_camera(self.scene.characters.as_ref());
        self.shown = Shown::of(self.view, camera_record, heli, flip);
        // The Camera page shows the view it sets, also over the start orbit and the helicam.
        if self.menu.as_ref().is_some_and(|menu| menu.camera().is_some()) {
            self.shown = Shown::Player(self.view);
        }
        let looking_back = self.bindings.get(Action::LookBack).key.is_some_and(|k| self.held.contains(&k))
            || self.pads[Action::LookBack as usize] > bindings::PRESS;
        let (car_view, look) = (self.shown.car_view(), self.look);
        if let Some(playback) = &mut self.scene.playback {
            playback.follow_game_chase(self.shown.swing(), looking_back);
        }
        if let (Some(playback), Some(glass)) = (&mut self.scene.playback, &mut self.scene.glass) {
            let spray = self.scene.particles.as_ref().map(|p| &p.particles.spray);
            glass.draw(&mut running.renderer, playback, car_view.interior(), spray);
        }
        if let (Some(playback), Some(crew)) = (&mut self.scene.playback, &mut self.scene.crew) {
            show_crew(crew, playback, car_view, &mut running.renderer);
        }
        let fov = self.fov;
        let car_lens = match (&self.scene.playback, &self.scene.mounts) {
            (Some(playback), Some(mounts)) => Some(self.shown.lens(playback, mounts, scripted, look, fov)),
            _ => None,
        };
        if let (Some(playback), Some(lens)) = (&mut self.scene.playback, &car_lens) {
            playback.interior = car_view.interior();
            running.renderer.set_outside_view(!car_view.interior());
            playback.apply(&mut running.renderer, lens);
        }
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
        if let (Some(drawn), Some(playback)) = (&mut self.scene.particles, &self.scene.playback) {
            throw_smoke(self.scene.characters.as_mut(), drawn);
            drawn.draw(&mut running.renderer, playback, self.scene.live.as_mut(), view_projection);
        }
        if let (Some(drawn), Some(playback)) = (&mut self.scene.fences, &self.scene.playback) {
            drawn.draw(&mut running.renderer, playback, view_projection);
        }
        let frame = match running.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                running.surface.configure(&running.gpu.device, &running.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = running.gpu.device.create_command_encoder(&Default::default());
        let paused = self.paused + self.paused_at.map_or(Duration::ZERO, |at| at.elapsed());
        let elapsed = self.started.elapsed() - paused;
        running.renderer.set_time(elapsed.as_secs_f32());
        if let Some(clock) = &self.scene.day_clock {
            // The drive's time, else the window's.
            let seconds = self.scene.playback.as_ref().map_or(elapsed.as_secs_f64(), |p| p.time());
            running.renderer.set_atmosphere(clock.atmosphere(seconds).0);
        }
        running.renderer.set_start_clock(self.scene.live.as_ref().and_then(|l| l.hud_model.view().start_clock));
        if let (Some(dials), Some(playback)) = (&mut self.scene.dials, &self.scene.playback) {
            dials.update(&dial_values(playback, self.scene.live.as_ref()), Some(dt), &mut running.renderer);
        }
        running.renderer.render(&mut encoder, &view, (width, height), view_projection, sky_view_projection);
        let readout = self.scene.playback.as_ref().filter(|_| self.scene.hud.show).map(|playback| {
            let fps = if self.scene.hud.fps { self.meter.update(dt) } else { None };
            let (live, shown, bindings) = (self.scene.live.as_ref(), self.shown, &self.bindings);
            readout(playback, live, shown, bindings, self.scene.hud, fps, view_projection)
        });
        if readout.is_some() || self.menu.is_some() {
            let (gpu, menu) = (&running.gpu, &self.menu);
            running.overlay.draw_with(&gpu.device, &gpu.queue, &mut encoder, &view, (width, height), |ctx| {
                readout.iter().for_each(|r| hud_overlay::paint_readout(ctx, r));
                menu.iter().for_each(|m| pause::paint(ctx, m));
            });
        }
        running.gpu.queue.submit([encoder.finish()]);
        running.window.pre_present_notify();
        frame.present();
    }

    /// Writes the live drive's ghost (`--save-ghost`).
    fn finish_ghost(&self) {
        if let (Some(ghost), Some(playback)) = (&self.scene.ghost, &self.scene.playback) {
            ghost.finish(playback);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match self.create(event_loop) {
            Ok(running) => self.running = Some(running),
            Err(error) => {
                eprintln!("viewer: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.finish_ghost();
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                if let Some(running) = &mut self.running {
                    running.config.width = size.width.max(1);
                    running.config.height = size.height.max(1);
                    running.surface.configure(&running.gpu.device, &running.config);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. }, ..
            } => match state {
                ElementState::Pressed
                    if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) && self.accept_message() => {}
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
                            KeyCode::KeyR => playback.restart(),
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
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (true, DeviceEvent::MouseMotion { delta: (dx, dy) }) = (self.looking, &event) {
            let (yaw, pitch) = (
                *dx as f32 * LOOK_SENSITIVITY,
                // Mouse up (negative dy) looks up.
                -*dy as f32 * LOOK_SENSITIVITY,
            );
            match (&mut self.scene.orbit, &self.scene.playback, &mut self.running) {
                (Some(orbit), _, _) => orbit.turn(yaw, pitch),
                (None, Some(_), _) => self.look.turn(yaw, pitch),
                (None, None, Some(running)) => running.camera.turn(yaw, pitch),
                (None, None, None) => {}
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
        if let Some(running) = &self.running {
            // Under `--max-fps` the next frame waits for its time.
            match self.frame_period.map(|period| self.last_frame + period).filter(|due| *due > Instant::now()) {
                Some(due) => event_loop.set_control_flow(ControlFlow::WaitUntil(due)),
                None => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    running.window.request_redraw();
                }
            }
        }
    }
}
