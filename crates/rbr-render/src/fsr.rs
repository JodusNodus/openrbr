//! AMD FidelityFX FSR 3's temporal upscaler ([`AntiAliasing::Fsr`](crate::renderer::AntiAliasing::Fsr), through the
//! vendored `wgpu-ffx`) in place of [`crate::taa`]'s resolve: the same jittered frame and depth, and each pixel's motion
//! as `taa.wgsl` works it out (`fs_motion`: the camera's, the car's, the steering wheel's), gathered at the output's
//! size, so the frame may be drawn below it (the render scale).
//!
//! What the history cannot carry FSR takes new where it is marked reactive: the car glass's mud and drops (as the
//! resolve does), and the rain and the particles, which write no depth. Those are drawn over the frame before the
//! dispatch ([`Fsr::keep_opaque`] keeps the frame without them): where they changed it is reactive.

use glam::{Mat4, Vec2};
use wgpu_ffx::{FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo, FsrView};

use crate::post::SCENE_FORMAT;
use crate::renderer::{texture_entry, Gpu, TEXTURE};

const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
const REACTIVE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// RCAS's strength after the upscale (0 to 1; `RBR_FSR_SHARPNESS` overrides, 0 turns it off).
const SHARPNESS: f32 = 0.8;

/// What [`Fsr::resolve`] takes of a frame.
pub(crate) struct Dispatch {
    /// The main view's unjittered view-projection.
    pub(crate) view_projection: Mat4,
    /// The frame's jitter, pixels (x right, y down).
    pub(crate) jitter: Vec2,
    /// The history starts again.
    pub(crate) reset: bool,
    /// Since the last frame, milliseconds.
    pub(crate) delta: f32,
}

pub(crate) struct Fsr {
    queue: wgpu::Queue,
    device: wgpu::Device,
    context: FsrContext,
    view: FsrView,
    /// `fs_motion`'s group 1: the frame before the late effects.
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    targets: Targets,
    /// The frame's size and the output's.
    size: (u32, u32),
    output: (u32, u32),
    /// The frame had the effects drawn over it since [`Fsr::keep_opaque`].
    over: bool,
    sharpness: f32,
    /// `RBR_FSR_DEBUG`: FSR's debug view instead of the frame.
    debug: bool,
}

/// What depends on the sizes.
struct Targets {
    color: wgpu::Texture,
    depth: wgpu::Texture,
    motion: wgpu::TextureView,
    reactive: wgpu::TextureView,
    opaque: wgpu::TextureView,
    dilated_depth: wgpu::Texture,
    dilated_motion: wgpu::Texture,
    previous_depth: wgpu::Buffer,
    output: wgpu::Texture,
    /// `fs_motion`'s group 1 reading the frame itself (nothing over it), then the frame before the effects.
    groups: [wgpu::BindGroup; 2],
}

impl Fsr {
    /// The upscaler for frames of `size` drawn into `raw` with their `depth`, to `output`; `fs_motion` in `module`
    /// takes the resolve's group 0 of `taa_layout`.
    pub(crate) fn new(
        gpu: &Gpu,
        taa_layout: &wgpu::BindGroupLayout,
        module: &wgpu::ShaderModule,
        [raw, depth]: [&wgpu::TextureView; 2],
        size: (u32, u32),
        output: (u32, u32),
    ) -> Fsr {
        let device = &gpu.device;
        let context = FsrContext::new(FsrContextInfo {
            adapter: gpu.adapter.clone(),
            device: device.clone(),
            flags: FsrContextFlags::HIGH_DYNAMIC_RANGE
                | FsrContextFlags::DEPTH_INVERTED
                | FsrContextFlags::AUTO_EXPOSURE,
            format_profile: None,
        });
        let view = context.create_view(&gpu.queue, sides(size), sides(output));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fsr motion"),
            entries: &[texture_entry(0, TEXTURE)],
        });
        let target = |format| Some(wgpu::ColorTargetState::from(format));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fsr motion"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fsr motion"),
                bind_group_layouts: &[Some(taa_layout), Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_motion"),
                compilation_options: Default::default(),
                targets: &[target(MOTION_FORMAT), target(REACTIVE_FORMAT)],
            }),
            multiview_mask: None,
            cache: None,
        });
        let targets = targets(device, &context, &layout, [raw, depth], size, output);
        let sharpness = std::env::var("RBR_FSR_SHARPNESS").ok().and_then(|s| s.parse().ok()).unwrap_or(SHARPNESS);
        Fsr {
            queue: gpu.queue.clone(),
            device: device.clone(),
            context,
            view,
            layout,
            pipeline,
            targets,
            size,
            output,
            over: false,
            sharpness,
            debug: std::env::var_os("RBR_FSR_DEBUG").is_some(),
        }
    }

    /// For frames of `size` drawn into `raw` with their `depth`, to `output`; the history starts again.
    pub(crate) fn resize(&mut self, [raw, depth]: [&wgpu::TextureView; 2], size: (u32, u32), output: (u32, u32)) {
        self.view.resize(&self.queue, sides(size), sides(output));
        self.targets = targets(&self.device, &self.context, &self.layout, [raw, depth], size, output);
        (self.size, self.output) = (size, output);
    }

    /// The jitter's phases for the frame's size and the output's (8 at the same size, more the further below).
    pub(crate) fn phases(&self) -> u32 {
        wgpu_ffx::get_jitter_phase_count(self.size.0 as i32, self.output.0 as i32).max(1) as u32
    }

    /// Keeps the frame in `raw` before the rain and the particles are drawn over it: where they change it, the
    /// resolve takes it new.
    pub(crate) fn keep_opaque(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let t = &self.targets;
        encoder.copy_texture_to_texture(t.color.as_image_copy(), t.opaque.texture().as_image_copy(), t.color.size());
        self.over = true;
    }

    /// Gathers the frame into `scene` (the output's size): its motion and masks (`fs_motion`, reading the resolve's
    /// `taa_group`), then FSR.
    pub(crate) fn resolve(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        taa_group: &wgpu::BindGroup,
        scene: &wgpu::TextureView,
        frame: &Dispatch,
    ) {
        let t = &self.targets;
        {
            let attachment = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fsr motion"),
                color_attachments: &[attachment(&t.motion), attachment(&t.reactive)],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, taa_group, &[]);
            pass.set_bind_group(1, &t.groups[usize::from(self.over)], &[]);
            pass.draw(0..3, 0..1);
        }
        self.over = false;
        let (near, far, fov_y) = lens(frame.view_projection);
        let info = FsrDispatchInfo {
            color: t.color.clone(),
            depth: t.depth.clone(),
            motion_vectors: t.motion.texture().clone(),
            exposure: None,
            reactive_mask: Some(t.reactive.texture().clone()),
            transparency_and_composition: None,
            dilated_depth: t.dilated_depth.clone(),
            dilated_motion_vectors: t.dilated_motion.clone(),
            reconstructed_previous_depth: t.previous_depth.clone(),
            output: t.output.clone(),
            jitter_offset: frame.jitter.to_array(),
            // `fs_motion`'s motion is in uv; FSR's scale takes it to pixels.
            motion_vector_scale: [self.size.0 as f32, self.size.1 as f32],
            render_size: sides(self.size),
            upscale_size: sides(self.output),
            enable_sharpening: self.sharpness > 0.0,
            sharpness: self.sharpness.min(1.0),
            frame_time_delta: frame.delta,
            pre_exposure: 1.0,
            reset_history: frame.reset,
            // Under inverted depth FSR takes the planes swapped (its `DEPTH_INVERTED` check).
            camera_near: far,
            camera_far: near,
            camera_fov_y: fov_y,
            view_space_to_meters_factor: 1.0,
            flags: if self.debug { FsrDispatchFlags::DRAW_DEBUG_VIEW } else { FsrDispatchFlags::empty() },
        };
        if let Err(error) = self.context.dispatch(&mut self.view, encoder, &info) {
            eprintln!("fsr: {error}");
            return;
        }
        encoder.copy_texture_to_texture(t.output.as_image_copy(), scene.texture().as_image_copy(), t.output.size());
    }
}

fn sides((width, height): (u32, u32)) -> [u32; 2] {
    [width.max(2), height.max(2)]
}

/// The near and far planes (metres) and the vertical field of view (radians) of a reversed-depth perspective
/// `view_projection` with a rigid view ([`crate::coords::reversed`]): its rows are the projection's scaled by the
/// view's unit axes, and its depth is `a + b / z`, 1 at the near plane and 0 at the far.
fn lens(view_projection: Mat4) -> (f32, f32, f32) {
    let (up, depth, w) = (view_projection.row(1), view_projection.row(2), view_projection.row(3));
    let forward = w.truncate().length_squared();
    let a = depth.truncate().dot(w.truncate()) / forward;
    let b = depth.w - a * w.w;
    (b / (1.0 - a), -b / a, 2.0 * (forward.sqrt() / up.truncate().length()).atan())
}

fn targets(
    device: &wgpu::Device,
    context: &FsrContext,
    layout: &wgpu::BindGroupLayout,
    [raw, depth]: [&wgpu::TextureView; 2],
    size: (u32, u32),
    output: (u32, u32),
) -> Targets {
    let formats = context.formats();
    let texture = |label, (width, height): (u32, u32), format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(2), height: height.max(2), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let drawn = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let stored = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
    let view = |texture: wgpu::Texture| texture.create_view(&Default::default());
    let opaque = view(texture(
        "fsr opaque",
        size,
        SCENE_FORMAT,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    ));
    let groups = [raw, &opaque].map(|frame| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fsr motion"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(frame) }],
        })
    });
    Targets {
        color: raw.texture().clone(),
        depth: depth.texture().clone(),
        motion: view(texture("fsr motion", size, MOTION_FORMAT, drawn)),
        reactive: view(texture("fsr reactive", size, REACTIVE_FORMAT, drawn)),
        opaque,
        dilated_depth: texture("fsr dilated depth", size, formats.dilated_depth, stored),
        dilated_motion: texture("fsr dilated motion", size, formats.dilated_motion_vectors, stored),
        previous_depth: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fsr previous depth"),
            size: u64::from(size.0.max(2)) * u64::from(size.1.max(2)) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        output: texture("fsr output", output, formats.output, stored | wgpu::TextureUsages::COPY_SRC),
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn the_lens_comes_back_from_the_view_projection() {
        let projection = crate::coords::reversed(0.9, 1.6, 0.25);
        let view = Mat4::from_rotation_x(0.2)
            * Mat4::from_rotation_y(0.7)
            * Mat4::from_translation(Vec3::new(-3.0, -1.0, 2.0));
        let (near, far, fov_y) = lens(projection * view);
        assert!((near - 0.25).abs() < 1e-3, "{near}");
        assert!((far / crate::coords::FAR - 1.0).abs() < 1e-2, "{far}");
        assert!((fov_y - 0.9).abs() < 1e-4, "{fov_y}");
    }
}
