//! The launcher's window: winit, wgpu and egui, and the page without a game.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::disc_install::DiscSetup;
use crate::install;
use crate::pages::WARN;
use crate::settings::{load, read_settings};
use crate::settings_page::folder_row;
use crate::{Form, Start};

/// Window sizes offered besides the monitor's modes, when they fit the monitor.
const COMMON_SIZES: [(u32, u32); 7] =
    [(3840, 2160), (2560, 1440), (1920, 1080), (1600, 900), (1280, 720), (1024, 768), (800, 600)];

impl Form {
    /// The window sizes: the monitor's modes and the common sizes that fit it, largest first, with the current one.
    fn set_monitor(&mut self, window: &Window) {
        // Wayland names no monitor for a window it hasn't shown yet: the first one listed instead.
        let Some(monitor) = window.current_monitor().or_else(|| window.available_monitors().next()) else {
            return;
        };
        let full = monitor.size();
        let mut sizes: Vec<(u32, u32)> = monitor
            .video_modes()
            .map(|m| (m.size().width, m.size().height))
            .chain([(full.width, full.height)])
            .chain(COMMON_SIZES.into_iter().filter(|&(w, h)| w <= full.width && h <= full.height))
            .chain([self.size])
            .collect();
        sizes.sort_by_key(|&(w, h)| std::cmp::Reverse((w * h, w)));
        sizes.dedup();
        self.sizes = sizes;
    }
}

/// The window, its surface and egui's state and renderer.
struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// `config` is not yet the surface's ([`rbr_render::present::configure`]).
    stale: bool,
    egui: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl Gfx {
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Gfx, String> {
        let attributes = Window::default_attributes()
            .with_title(concat!("ReRBR ", env!("CARGO_PKG_VERSION")))
            .with_inner_size(LogicalSize::new(1100.0, 760.0))
            .with_window_icon(rbr_render::icon::window_icon());
        #[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes,
            rbr_render::icon::APP_ID,
            rbr_render::icon::APP_ID,
        );
        let window = Arc::new(event_loop.create_window(attributes).map_err(|e| e.to_string())?);
        let instance = rbr_render::renderer::instance();
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = futures_executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let (device, queue) = futures_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| e.to_string())?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = rbr_render::present::surface_format(&capabilities.formats);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
        };
        let stale = !rbr_render::present::configure(&surface, &device, &config);
        let context = egui::Context::default();
        let egui = egui_winit::State::new(
            context.clone(),
            context.viewport_id(),
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
        Ok(Gfx { window, surface, device, queue, config, stale, egui, renderer })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.stale = !rbr_render::present::configure(&self.surface, &self.device, &self.config);
    }

    /// Runs the form and draws it; returns when egui wants the next frame.
    fn redraw(&mut self, mut ui: impl FnMut(&egui::Context)) -> Duration {
        let input = self.egui.take_egui_input(&self.window);
        let context = self.egui.egui_ctx().clone();
        let mut output = context.run_pass(input, |ctx| ui(ctx));
        self.egui.handle_platform_output(&self.window, output.platform_output);
        let primitives = context.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: output.pixels_per_point,
        };
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.renderer.update_texture(&self.device, &self.queue, id, &delta);
            }
        }
        let early = self.draw(&primitives, &screen);
        // After drawing, or when the frame was not drawn (egui checks that no delta is dropped unapplied).
        for id in output.textures_delta.free.drain() {
            self.renderer.free_texture(&id);
        }
        early.unwrap_or_else(|| {
            output.viewport_output.get(&context.viewport_id()).map_or(Duration::MAX, |v| v.repaint_delay)
        })
    }

    /// Draws `primitives` into the window; when the frame cannot be drawn, how soon to try again.
    fn draw(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        screen: &egui_wgpu::ScreenDescriptor,
    ) -> Option<Duration> {
        if self.stale {
            self.stale = !rbr_render::present::configure(&self.surface, &self.device, &self.config);
            if self.stale {
                return Some(Duration::ZERO);
            }
        }
        let frame = match rbr_render::present::acquire(&self.surface) {
            rbr_render::present::Acquired::Frame(frame) => frame,
            // Hidden: look again in a while rather than at once.
            rbr_render::present::Acquired::Hidden => return Some(Duration::from_millis(100)),
            rbr_render::present::Acquired::Failed => {
                self.stale = !rbr_render::present::configure(&self.surface, &self.device, &self.config);
                return Some(Duration::ZERO);
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut buffers = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, primitives, screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.renderer.render(&mut pass.forget_lifetime(), primitives, screen);
        }
        buffers.push(encoder.finish());
        self.queue.submit(buffers);
        self.queue.present(frame);
        None
    }
}

/// The window: the form, or without a game the install folder's field (`folder`, at first the install found in the
/// usual places, `found`), why the last one did not load, and the files copied from the launcher's folder (`home`).
pub struct App {
    start: Start,
    notice: Option<String>,
    form: Option<Form>,
    folder: String,
    disc_setup: DiscSetup,
    found: Option<String>,
    error: String,
    gfx: Option<Gfx>,
}

impl App {
    pub fn new(start: Start, mut form: Option<Form>, folder: Option<String>, notice: Option<String>) -> App {
        if let Some(form) = &mut form {
            form.log_missing_programs();
            form.page = form.first_page();
        }
        // A saved folder that does not load says why.
        let error = match (&form, &folder) {
            (None, Some(folder)) => install::problem(folder),
            _ => String::new(),
        };
        // At the first start, the usual install places' first with NGP.
        let found = folder.is_none().then(install::detect).flatten().map(|dir| dir.display().to_string());
        let folder = folder.or_else(|| found.clone()).unwrap_or_default();
        App { start, notice, form, folder, disc_setup: DiscSetup::default(), found, error, gfx: None }
    }

    /// The page without a game: the install folder's field; whether a folder was saved, to load.
    fn choose_folder(&mut self, ctx: &egui::Context) -> bool {
        let mut saved = false;
        ctx.root_ui(|root| egui::CentralPanel::default().show(root, |ui| {
            ui.heading("Welcome to ReRBR");
            ui.add_space(8.0);
            ui.label("ReRBR plays Richard Burns Rally with NGP's physics. It needs your own copy of the game: choose your game folder.");
            ui.add_space(4.0);
            ui.label("• A RallySimFans install, or Richard Burns Rally 1.02 with NGP (copied from Windows, for example): Browse… to the folder with RichardBurnsRally_SSE.exe.");
            ui.label("• Only the disc? Set up from the disc… makes the game folder from the disc, the 1.02 patch and NGP.");
            ui.colored_label(egui::Color32::GRAY, "ReRBR only reads the game folder; it never changes it.");
            ui.add_space(8.0);
            if let Some(notice) = &self.notice {
                ui.colored_label(egui::Color32::GRAY, notice);
            }
            let set = folder_row(ui, &mut self.folder, &mut self.disc_setup);
            let found = self.found.as_deref() == Some(self.folder.as_str());
            if found {
                ui.colored_label(egui::Color32::GRAY, "Found Richard Burns Rally with NGP there.");
            }
            let use_it = found && ui.button("Use this folder").clicked();
            if (set || use_it) && !self.folder.trim().is_empty() {
                let mut settings = read_settings(&self.start.settings);
                settings.install_folder = Some(self.folder.trim().to_owned());
                if let Err(e) = std::fs::write(&self.start.settings, settings.to_text()) {
                    self.error = format!("{}: {e}", self.start.settings.display());
                } else {
                    saved = true;
                }
            }
            if !self.error.is_empty() {
                ui.colored_label(WARN, &self.error);
            }
        }));
        saved
    }

    /// `--screenshot` without a game (`error`): the page as it opens, and with the disc setup open.
    pub fn screenshots(mut self, error: &str, dir: &Path) -> Result<(), String> {
        eprintln!("launcher: {error}");
        crate::capture::shot(&dir.join("first-start.png"), |ctx| _ = self.choose_folder(ctx))?;
        self.disc_setup.open = true;
        crate::capture::shot(&dir.join("first-start-disc.png"), |ctx| _ = self.choose_folder(ctx))
    }

    fn on_event(&mut self, gfx: &mut Gfx, event_loop: &ActiveEventLoop, event: WindowEvent) {
        // While a binding is being set, the keys go to it and not to egui (Enter would start, Space click).
        if let Some(form) = self.form.as_mut().filter(|f| f.controls.capturing()) {
            if let WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(key), state, .. },
                ..
            } = event
            {
                if state == ElementState::Pressed && form.controls.key(&mut form.bindings, key) {
                    form.save();
                }
                gfx.window.request_redraw();
                return;
            }
        }
        let response = gfx.egui.on_window_event(&gfx.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let delay = match &mut self.form {
                    Some(form) => {
                        let delay = gfx.redraw(|ctx| form.ui(ctx));
                        form.menu_sound();
                        delay
                    }
                    None => {
                        let mut saved = false;
                        let delay = gfx.redraw(|ctx| saved = self.choose_folder(ctx));
                        if saved {
                            self.load(&gfx.window);
                        }
                        delay
                    }
                };
                // A new install folder: the form is loaded again (once the viewer has closed).
                if self.form.as_ref().is_some_and(|f| f.reload && f.child.is_none()) {
                    self.load(&gfx.window);
                }
                if delay.is_zero() {
                    gfx.window.request_redraw();
                } else if let Some(at) = std::time::Instant::now().checked_add(delay) {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(at));
                }
                return;
            }
            _ => {}
        }
        if response.repaint {
            gfx.window.request_redraw();
        }
    }

    /// Loads the form from the settings file.
    pub fn load(&mut self, window: &Window) {
        let settings = read_settings(&self.start.settings);
        self.folder = settings.install_folder.clone().unwrap_or_default();
        match load(&self.start, settings) {
            Ok(mut form) => {
                form.set_monitor(window);
                form.log_missing_programs();
                form.page = self.form.as_ref().map_or_else(|| form.first_page(), |f| f.page);
                self.form = Some(form);
                self.error.clear();
            }
            Err(e) => {
                eprintln!("launcher: {e}");
                self.form = None;
                self.error = install::problem(&self.folder);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        match Gfx::new(event_loop) {
            Ok(gfx) => {
                if let Some(form) = &mut self.form {
                    form.set_monitor(&gfx.window);
                }
                gfx.window.request_redraw();
                self.gfx = Some(gfx);
            }
            Err(e) => {
                eprintln!("launcher: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // Out of `self` while it draws, so that the pages can load the form.
        let Some(mut gfx) = self.gfx.take() else {
            return;
        };
        self.on_event(&mut gfx, event_loop, event);
        self.gfx = Some(gfx);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let (ControlFlow::WaitUntil(at), Some(gfx)) = (event_loop.control_flow(), &self.gfx) {
            if std::time::Instant::now() >= at {
                event_loop.set_control_flow(ControlFlow::Wait);
                gfx.window.request_redraw();
            }
        }
    }
}
