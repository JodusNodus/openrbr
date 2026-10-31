//! The mirrors' passes ([`crate::mirrors`], ours): each due mirror's view drawn into its image, a layer of
//! [`Mirrors::images`] that the main view's glass samples (`mirrors.wgsl`).

use super::frame::{mark, span, View};
use super::textures::depth_view;
use super::Renderer;
use crate::mirrors::{self, Placed, Rate, MOST, MOST_MIRRORS};
use crate::post::SCENE_FORMAT;
use crate::profile::Profiler;
use crate::taa;
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::Mat4;
use std::cell::{Cell, RefCell};

/// How far the mirrors draw the stage, metres (ours: the brief's "far level of detail, capped at about 300 m").
pub(super) const REACH: f32 = 300.0;

/// The image layers: the mirrors', then the windscreen's (the cabin reflected in it, which the windscreen's pane
/// samples: `glass.wgsl`).
const LAYERS: usize = MOST_MIRRORS + 1;
const WINDSCREEN: usize = MOST_MIRRORS;
/// The windscreen's image is drawn every this many frames (user's call: its pass cost 1.2-1.5 ms a frame); between,
/// the glass holds it still with the car as it does the mirrors'.
const WINDSCREEN_RATE: Rate = Rate::Every(2);
/// Each layer's span in `--profile`.
const LABELS: [&str; LAYERS] = ["mirror 1", "mirror 2", "mirror 3", "mirror 4", "windscreen reflection"];

/// One mirror's view in `mirrors.wgsl`'s `MirrorViews`.
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct ViewUniform {
    view_projection: [[f32; 4]; 4],
    scale: [f32; 4],
}

/// The image a mirror's layer holds: the view it was drawn with, its size, the car body's model space to stage
/// space then, and the layer's draws before it (its jitter's phase).
#[derive(Debug, Clone, Copy)]
struct Held {
    view_projection: Mat4,
    size: (u32, u32),
    body: Mat4,
    draws: u32,
}

/// A mirror's pass's targets at one size: the image it draws and its depth, the image copied into the mirror's layer,
/// and the effects reading the depth (with
/// [`Enhance::particles`](super::Enhance::particles)).
struct Target {
    size: (u32, u32),
    depth: wgpu::TextureView,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    effects: Option<wgpu::BindGroup>,
}

/// The mirrors' images, their views for the glass, and what each pass needs.
pub(super) struct Mirrors {
    rate: Rate,
    /// Whether the windscreen's image is drawn ([`Enhance::windscreen`](super::Enhance::windscreen)), at
    /// [`WINDSCREEN_RATE`].
    windscreen_on: bool,
    /// [`LAYERS`] layers of [`MOST`] (one texel without mirrors), and their view.
    images: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    /// `MirrorViews`.
    pub(super) uniform: wgpu::Buffer,
    /// The glass this frame ([`Renderer::set_mirrors`]); none outside the cockpit.
    placed: Vec<Placed>,
    /// The windscreen's mean plane this frame ([`Renderer::set_windscreen_mirror`]).
    windscreen: Option<Placed>,
    targets: RefCell<Vec<Option<Target>>>,
    held: RefCell<[Option<Held>; LAYERS]>,
    frame: Cell<u64>,
}

impl Mirrors {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, rate: Rate, windscreen_on: bool) -> Mirrors {
        let (size, layers) = match (rate, windscreen_on) {
            (Rate::Off, false) => ((1, 1), 1),
            _ => (MOST, LAYERS as u32),
        };
        let images = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirror images"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = images.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mirror views"),
            size: std::mem::size_of::<[ViewUniform; LAYERS]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Mirrors {
            rate,
            windscreen_on,
            images,
            view,
            uniform,
            placed: Vec::new(),
            windscreen: None,
            targets: RefCell::new((0..LAYERS).map(|_| None).collect()),
            held: RefCell::new([None; LAYERS]),
            frame: Cell::new(0),
        }
    }
}

impl Mirrors {
    /// The glass drawn this frame with its image layer: the mirrors', then the windscreen's.
    fn layers(&self) -> impl Iterator<Item = (usize, Placed)> + '_ {
        self.placed.iter().copied().enumerate().chain(self.windscreen.map(|placed| (WINDSCREEN, placed)))
    }

    /// Whether layer `k`'s image is drawn on frame `frame`: the mirrors' at their rate, the windscreen's at
    /// [`WINDSCREEN_RATE`].
    fn due(&self, k: usize, frame: u64) -> bool {
        match k {
            WINDSCREEN => WINDSCREEN_RATE.due(k, frame),
            _ => self.rate.due(k, frame),
        }
    }
}

impl Renderer {
    /// The cockpit's mirrors' glass this frame, at most [`MOST_MIRRORS`] in the order [`mirrors::mark`] numbered
    /// them (none: no mirror passes). Only with [`Enhance::mirrors`](super::Enhance::mirrors) on.
    pub fn set_mirrors(&mut self, placed: &[Placed]) {
        let mirrors = &mut self.mirrors;
        if mirrors.rate == Rate::Off {
            return;
        }
        mirrors.placed = placed.iter().take(MOST_MIRRORS).copied().collect();
        if mirrors.placed.is_empty() {
            mirrors.held.borrow_mut()[..MOST_MIRRORS].fill(None);
        }
    }

    /// The windscreen's pane seen from inside this frame, its mean plane facing the eye (none: no pass). Drawn as the
    /// centre mirror's is, the cabin and the stage behind it, at [`WINDSCREEN_RATE`]; only with
    /// [`Enhance::windscreen`](super::Enhance::windscreen).
    pub(crate) fn set_windscreen_mirror(&mut self, placed: Option<Placed>) {
        let mirrors = &mut self.mirrors;
        if !mirrors.windscreen_on {
            return;
        }
        mirrors.windscreen = placed;
        if placed.is_none() {
            mirrors.held.borrow_mut()[WINDSCREEN] = None;
        }
    }

    /// Whether a mirror's pass is due this frame.
    pub(super) fn mirrors_due(&self) -> bool {
        let m = &self.mirrors;
        let held = m.held.borrow();
        m.layers().any(|(k, _)| held[k].is_none() || m.due(k, m.frame.get()))
    }

    /// Draws the due mirrors' passes from the main view's `[view_projection, sky_view_projection]`, each in
    /// its own submission (the frame uniforms are rewritten per pass), and writes the views the main view's glass
    /// samples them by. The particles' uniform is set back to the main view's after.
    pub(super) fn draw_mirrors(
        &self,
        profiler: &mut Option<Profiler>,
        [view_projection, sky_view_projection]: [Mat4; 2],
    ) {
        let m = &self.mirrors;
        if m.rate == Rate::Off && !m.windscreen_on {
            return;
        }
        let frame = m.frame.get();
        m.frame.set(frame + 1);
        let body = self.car_body.unwrap_or(Mat4::IDENTITY);
        let mut held = m.held.borrow_mut();
        let mut drawn = false;
        // One span for the frame's passes, also when none is due (the rates' mean cost per frame).
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let t = m.layers().next().is_some().then(|| mark(profiler, &mut encoder));
        for (k, placed) in m.layers() {
            if held[k].is_some() && !m.due(k, frame) {
                continue;
            }
            let Some(view) = mirrors::view(view_projection, sky_view_projection, self.depth_size, &placed) else {
                continue;
            };
            // Under the temporal anti-aliasing each image is drawn with a sub-texel jitter of its own, which the glass
            // samples it by, so that the main view's resolve gathers its edges over the draws.
            let draws = held[k].map_or(0, |held| held.draws.wrapping_add(1));
            let shift = self.taa.as_ref().map_or(Mat4::IDENTITY, |_| taa::shift(taa::jitter(draws, false), view.size));
            let view = mirrors::View {
                view_projection: shift * view.view_projection,
                sky_view_projection: shift * view.sky_view_projection,
                ..view
            };
            let layer = mark(profiler, &mut encoder);
            let mut targets = m.targets.borrow_mut();
            let target = match &targets[k] {
                Some(target) if target.size == view.size => target,
                _ => targets[k].insert(self.mirror_target(view.size)),
            };
            let pass = View::Mirror { side: placed.side };
            self.write_frames(view.view_projection, view.sky_view_projection, self.shadow_matrices(), pass);
            if let Some(particles) = &self.particles {
                particles.prepare(&self.queue, view.view_projection, &self.game_light().0);
            }
            // The rain goes over the scene, reading its depth.
            let rain = self.effects.as_ref().filter(|effects| effects.precipitating()).zip(target.effects.as_ref());
            let attachments = [&target.resolved_view, &target.depth];
            self.draw_scene(&mut encoder, attachments, view.view_projection, pass, [rain.is_some(); 2]);
            if let Some((effects, group)) = rain {
                let reflect = mirrors::reflection(placed.corners.iter().sum::<glam::Vec3>() / 4.0, placed.normal);
                effects.write_mirror(&self.queue, view.view_projection, reflect, view.size);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirror rain"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.resolved_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.depth,
                        depth_ops: None,
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                effects.draw_mirror_rain(&mut pass, group);
            }
            encoder.copy_texture_to_texture(
                target.resolved.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    texture: &m.images,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: k as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d { width: view.size.0, height: view.size.1, depth_or_array_layers: 1 },
            );
            span(profiler, &mut encoder, LABELS[k], layer);
            let done = std::mem::replace(&mut encoder, self.device.create_command_encoder(&Default::default()));
            self.queue.submit([done.finish()]);
            held[k] = Some(Held { view_projection: view.view_projection, size: view.size, body, draws });
            drawn = true;
        }
        if let Some(t) = t {
            span(profiler, &mut encoder, "mirrors", t);
            self.queue.submit([encoder.finish()]);
        }
        if let (true, Some(particles)) = (drawn, &self.particles) {
            particles.prepare(&self.queue, view_projection, &self.game_light().0);
        }
        // Each image's view moved with the car since it was drawn, so that the glass holds it still.
        let mut views = [ViewUniform::default(); LAYERS];
        for (k, _) in m.layers() {
            if let Some(held) = &held[k] {
                views[k] = ViewUniform {
                    view_projection: (held.view_projection * held.body * body.inverse()).to_cols_array_2d(),
                    scale: [held.size.0 as f32 / MOST.0 as f32, held.size.1 as f32 / MOST.1 as f32, 1.0, 0.0],
                };
            }
        }
        self.queue.write_buffer(&m.uniform, 0, bytes_of(&views));
    }

    fn mirror_target(&self, size: (u32, u32)) -> Target {
        let resolved = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirror image"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = depth_view(&self.device, size);
        Target {
            size,
            effects: self.effects.as_ref().map(|effects| effects.mirror_group(&self.device, &depth)),
            depth,
            resolved_view: resolved.create_view(&Default::default()),
            resolved,
        }
    }
}
