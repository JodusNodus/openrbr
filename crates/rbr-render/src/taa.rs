//! Temporal anti-aliasing ([`AntiAliasing::Taa`](crate::renderer::AntiAliasing::Taa); ours, after gp4's), the
//! renderer's default.
//!
//! The main view's projection is shifted every frame by a sub-pixel offset from the Halton (2, 3) sequence
//! ([`jitter`]: [`phases`], 8 at full size, more below it), so successive frames sample different points inside each
//! pixel; the mirrors, the reflection cubes and the sun's cascades keep the unjittered view. The frame is drawn into
//! [`Taa::raw`]; the resolve (`taa.wgsl`) blends it into a history of the previous ones, carried by each pixel's
//! motion: where something that moves of itself was drawn (the car and its parts, the ghost, the characters, the props)
//! the motion the renderer's motion pass wrote (`renderer/motion.rs`), elsewhere the camera's (the point from the
//! depth, seen by the previous frame's unjittered view-projection).
//! The history is clipped to the current neighbourhood's colour so that what was uncovered or changed does not ghost.
//! The result goes to the post pass's scene and is kept as the next frame's history.
//!
//! Drawn below the output's size (the render scale), the resolve upscales (`fs_upscale`, after Qualcomm's Snapdragon
//! Game Super Resolution 2): the history is the output's size, and each of its pixels gathers the frame's samples by
//! how near their jittered points fell to it. The rain and the particles are then drawn into the frame, at the depth's
//! size, which the resolve adds over the gathered frame, so the history keeps the frame without them ([`Taa::keep`]).
//!
//! The history starts again (the new frame alone) after a resize, on a switch between the in-car and outside views, when
//! the eye jumps further than [`CUT`] in a frame (a camera change, a restart), or on
//! [`Renderer::reset_history`](crate::renderer::Renderer::reset_history). A frame drawn again of the same view (a
//! pause, a screenshot's frames) keeps gathering samples.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec4};

use crate::post::SCENE_FORMAT;
use crate::renderer::{depth_binding, texture_entry, TEXTURE};

pub(crate) const SHADER: &str = include_str!("taa.wgsl");
/// The new frame's base share of the history.
const BLEND: f32 = 0.1;
/// The clip box's half size, in standard deviations of the neighbourhood.
const CLIP_SIGMAS: f32 = 1.25;
/// How far the eye may move in a frame before the history is dropped, metres (200 km/h at 10 frames a second is 5.6).
const CUT: f32 = 8.0;
/// The upscale's sample weight over the plane, output pixels squared: its kernel `exp(-2.29 d^2)` (`gathered` in
/// `taa.wgsl`) integrated.
const KERNEL_AREA: f32 = std::f32::consts::PI / 2.29;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_to_prev: [f32; 16],
    size: [f32; 4],
    output: [f32; 4],
    jitter: [f32; 4],
    params: [f32; 4],
}

/// Element `i` (from 1) of the Halton sequence of base `b`, in 0..1.
fn halton(mut i: u32, b: u32) -> f32 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= b as f32;
        r += f * (i % b) as f32;
        i /= b;
    }
    r
}

/// The jitter's phases for a frame of `size` resolved to `output`: 8 an output pixel, so the more the further below the
/// output the frame is (AMD's FSR's rule).
pub(crate) fn phases(size: (u32, u32), output: (u32, u32)) -> u32 {
    let ratio = output.0 as f32 / size.0.max(1) as f32;
    (8.0 * ratio * ratio).ceil() as u32
}

/// The jitter of frame `n` of `phases` in pixels (x right, y down, -0.5..0.5): Halton (2, 3).
pub(crate) fn jitter(n: u32, phases: u32) -> Vec2 {
    let i = n % phases.max(1) + 1;
    Vec2::new(halton(i, 2) - 0.5, halton(i, 3) - 0.5)
}

/// The clip space shift that moves what a projection draws by `jitter` pixels of a `width` x `height` target (left
/// multiplied onto the view-projection).
pub(crate) fn shift(jitter: Vec2, (width, height): (u32, u32)) -> Mat4 {
    let x = 2.0 * jitter.x / width.max(1) as f32;
    let y = -2.0 * jitter.y / height.max(1) as f32;
    Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::new(x, y, 0.0, 1.0))
}

/// The frame the resolve drew last.
struct Last {
    view_projection: Mat4,
    outside: bool,
}

pub(crate) struct Taa {
    layout: wgpu::BindGroupLayout,
    /// The resolve at the frame's size (`fs_taa`) and the upscale (`fs_upscale`).
    pipelines: [wgpu::RenderPipeline; 2],
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// The frame's targets.
    targets: Targets,
    current: usize,
    /// Frames begun (the jitter's phase).
    frame: u32,
    /// This frame's jitter, the frame's pixels.
    jitter: Vec2,
    /// The frame's size and the output's.
    size: (u32, u32),
    output: (u32, u32),
    last: Option<Last>,
    /// `RBR_TAA_DEBUG=motion` or `mask`: show the motion or the car glass's mask instead of the frame (0: the frame).
    debug: f32,
}

/// What depends on the frame's size and the output's.
struct Targets {
    /// The frame drawn this frame, jittered.
    raw: wgpu::Texture,
    raw_view: wgpu::TextureView,
    /// Upscaling, the frame before the rain and the particles ([`Taa::keep`]); else 1x1, unused.
    opaque: wgpu::Texture,
    /// The resolved frames, the output's size: the one [`Taa::current`] holds the latest.
    history: [wgpu::TextureView; 2],
    /// Reading history `i` (and the frame and its depth).
    groups: [wgpu::BindGroup; 2],
}

impl Taa {
    /// The resolve for frames of `size` resolved to `output` (upscaled when larger), their `depth` and their `motion`
    /// (`renderer::motion`).
    pub(crate) fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        output: (u32, u32),
        depth: &wgpu::TextureView,
        motion: &wgpu::TextureView,
    ) -> Taa {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                texture_entry(1, TEXTURE),
                texture_entry(2, depth_binding()),
                texture_entry(3, TEXTURE),
                texture_entry(4, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                texture_entry(5, TEXTURE),
                texture_entry(6, TEXTURE),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("taa"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let target = Some(wgpu::ColorTargetState::from(SCENE_FORMAT));
        let pipelines = ["fs_taa", "fs_upscale"].map(|entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[target.clone(), target.clone()],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("taa"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("taa history"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let targets = targets(device, &layout, &uniform, &sampler, [depth, motion], [size, output]);
        Taa {
            layout,
            pipelines,
            uniform,
            sampler,
            targets,
            current: 0,
            frame: 0,
            jitter: Vec2::ZERO,
            size,
            output,
            last: None,
            debug: match std::env::var("RBR_TAA_DEBUG").as_deref() {
                Ok("motion") => 1.0,
                Ok("mask") => 2.0,
                _ => 0.0,
            },
        }
    }

    /// For frames of `size` resolved to `output`, their `depth` and `motion`; the history starts again.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        output: (u32, u32),
        depth: &wgpu::TextureView,
        motion: &wgpu::TextureView,
    ) {
        self.targets = targets(device, &self.layout, &self.uniform, &self.sampler, [depth, motion], [size, output]);
        (self.size, self.output, self.last) = (size, output, None);
    }

    /// The target the frame is drawn into.
    pub(crate) fn raw(&self) -> &wgpu::TextureView {
        &self.targets.raw_view
    }

    /// Whether the resolve upscales: the frame is drawn below the output's size.
    pub(crate) fn upscaling(&self) -> bool {
        self.size != self.output
    }

    /// Upscaling, keeps the frame as drawn so far, before the rain and the particles are drawn into it: the history
    /// gathers the kept frame, and the resolve adds what they changed.
    pub(crate) fn keep(&self, encoder: &mut wgpu::CommandEncoder) {
        let t = &self.targets;
        encoder.copy_texture_to_texture(t.raw.as_image_copy(), t.opaque.as_image_copy(), t.raw.size());
    }

    /// Starts the history again with the next frame.
    pub(crate) fn reset(&mut self) {
        self.last = None;
    }

    /// Begins a frame: the shift its main view's view-projection takes ([`shift`]).
    pub(crate) fn begin(&mut self) -> Mat4 {
        self.frame = self.frame.wrapping_add(1);
        self.jitter = jitter(self.frame, phases(self.size, self.output));
        shift(self.jitter, self.size)
    }

    /// Writes the resolve's uniform for the frame of `view_projection` (the main view's, unjittered), seen from
    /// `outside` the car or in it.
    pub(crate) fn prepare(&mut self, queue: &wgpu::Queue, view_projection: Mat4, outside: bool) {
        let eye = crate::camera::eye(view_projection);
        let last = self
            .last
            .take()
            .filter(|last| last.outside == outside && (crate::camera::eye(last.view_projection) - eye).length() <= CUT);
        let current = view_projection.as_dmat4();
        let previous = last.as_ref().map_or(current, |last| last.view_projection.as_dmat4());
        let mut uniform = Uniform::zeroed();
        uniform.view_to_prev = (previous * current.inverse()).as_mat4().to_cols_array();
        let texels = |(width, height): (u32, u32)| {
            let (width, height) = (width as f32, height as f32);
            [width, height, 1.0 / width, 1.0 / height]
        };
        (uniform.size, uniform.output) = (texels(self.size), texels(self.output));
        // The upscale's new frame's share per unit of its samples' weight: on average the base share over the scale
        // (a longer history, which gathers about as many samples an output pixel).
        let scale = self.output.0 as f32 / self.size.0 as f32;
        uniform.jitter = [self.jitter.x, self.jitter.y, scale, BLEND * scale / KERNEL_AREA];
        let reset = f32::from(u8::from(last.is_none()));
        uniform.params = [BLEND, CLIP_SIGMAS, reset, self.debug];
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.last = Some(Last { view_projection, outside });
    }

    /// Resolves the frame in [`Taa::raw`] into `scene` (the output's size) and the next history.
    pub(crate) fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &wgpu::TextureView) {
        let write = 1 - self.current;
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("taa"),
            color_attachments: &[attachment(scene), attachment(&self.targets.history[write])],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipelines[usize::from(self.upscaling())]);
        pass.set_bind_group(0, &self.targets.groups[self.current], &[]);
        pass.draw(0..3, 0..1);
        self.current = write;
    }
}

/// The frame's target and its kept copy for frames of `size`, the two histories for `output`, and the bind groups
/// reading each.
fn targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    [depth, motion]: [&wgpu::TextureView; 2],
    [size, output]: [(u32, u32); 2],
) -> Targets {
    let texture = |label, (width, height): (u32, u32)| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            // The reflections pass copies into the frame (as into the post pass's scene); [`Taa::keep`] copies it.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let raw = texture("taa frame", size);
    let raw_view = raw.create_view(&Default::default());
    let opaque = texture("taa frame kept", if size == output { (1, 1) } else { size });
    let opaque_view = opaque.create_view(&Default::default());
    let history = [0, 1].map(|_| texture("taa history", output).create_view(&Default::default()));
    // The resolve reads the frame (upscaling, its copy before the effects) and, upscaling, the frame with them.
    let frame = if size == output { &raw_view } else { &opaque_view };
    let groups = [0, 1].map(|read| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(frame) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&history[read]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(motion) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&raw_view) },
            ],
        })
    });
    Targets { raw, raw_view, opaque, history, groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn jitter_covers_the_pixel_evenly() {
        let points: Vec<Vec2> = (1..=8).map(|n| jitter(n, 8)).collect();
        assert!(points.iter().all(|p| p.x.abs() <= 0.5 && p.y.abs() <= 0.5));
        let mean = points.iter().copied().sum::<Vec2>() / 8.0;
        assert!(mean.length() < 0.1, "{mean}");
        assert_eq!(jitter(3, 8), jitter(11, 8));
        assert_ne!(jitter(3, 16), jitter(11, 16));
    }

    #[test]
    fn the_phases_grow_with_the_upscale() {
        assert_eq!(phases((1920, 1080), (1920, 1080)), 8);
        assert_eq!(phases((960, 540), (1920, 1080)), 32);
        assert_eq!(phases((1286, 724), (1920, 1080)), 18);
    }

    #[test]
    fn the_shift_moves_a_point_by_the_jitter_in_pixels() {
        let view_projection = crate::coords::reversed(1.0, 1.5, 0.1);
        let p = Vec4::new(0.3, -0.2, 5.0, 1.0);
        let (width, height) = (300, 200);
        let pixel =
            |c: Vec4| Vec2::new((c.x / c.w * 0.5 + 0.5) * width as f32, (0.5 - c.y / c.w * 0.5) * height as f32);
        let moved = shift(Vec2::new(0.5, 0.25), (width, height)) * view_projection * p;
        let d = pixel(moved) - pixel(view_projection * p);
        assert!((d - Vec2::new(0.5, 0.25)).length() < 1e-3, "{d}");
    }

    #[test]
    fn the_shift_keeps_the_eye() {
        let view_projection =
            crate::coords::reversed(1.0, 1.5, 0.1) * Mat4::from_translation(Vec3::new(-3.0, -1.0, 2.0));
        let eye = crate::camera::eye(shift(Vec2::new(0.4, -0.3), (640, 360)) * view_projection);
        assert!((eye - Vec3::new(3.0, 1.0, -2.0)).length() < 1e-3, "{eye}");
    }
}
