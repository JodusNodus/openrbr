//! The sun's shadow and the contact shading over the main view ([`Enhance::sun`](super::Enhance::sun),
//! [`Enhance::occlusion`](super::Enhance::occlusion); ours): two values per pixel, worked out by `fs_sun_mask`
//! (`sun_mask.wgsl`) from the depth and the normals every opaque stage draw laid first (`fs_prepass`), read back by
//! those draws' shading (`SUN_MASK`). The soft shadows' search and taps run once per pixel there, out of the stage's
//! shader.

use super::pipelines::{texture_entry, TEXTURE};
use super::{depth_binding, Context};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;
/// The normals' clear colour: up, where a sample was left uncovered.
const UP: wgpu::Color = wgpu::Color { r: 0.5, g: 1.0, b: 0.5, a: 1.0 };

pub(super) struct SunMask {
    /// The stage draws' group 2: the mask.
    layout: wgpu::BindGroupLayout,
    /// The mask pass's group 2: the normals and the depth, at `scene_colour`'s and `scene_depth`'s bindings.
    depth_layout: wgpu::BindGroupLayout,
    /// The mask pass's empty group 1, `None` without the mask.
    pass: Option<(wgpu::RenderPipeline, wgpu::BindGroup)>,
    view: wgpu::TextureView,
    /// The prepass's normals, resolved, in the frame's colour format (the prepass's pipelines draw in it).
    normals: wgpu::TextureView,
    format: wgpu::TextureFormat,
    /// The mask for the stage draws, and the normals and the depth for the pass.
    pub(super) group: wgpu::BindGroup,
    depth: wgpu::BindGroup,
}

impl SunMask {
    /// The stage draws' group 2 layout.
    pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun mask"),
            entries: &[texture_entry(3, TEXTURE)],
        })
    }

    /// The mask over a `depth` of `size` pixels, with the `contact` shading; its pass only with the context's
    /// [`Context::sun_mask`] (else one texel the draws never read).
    pub(super) fn new(
        context: &Context<'_>,
        (layout, frame_layout): (wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
        depth: &wgpu::TextureView,
        size: (u32, u32),
        contact: bool,
    ) -> SunMask {
        let (device, on) = (context.device, context.sun_mask);
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun mask depth"),
            entries: &[texture_entry(0, TEXTURE), texture_entry(1, depth_binding(context.samples))],
        });
        let pass = on.then(|| {
            let empty = device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("sun mask"), entries: &[] });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sun mask"),
                layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sun mask"),
                    bind_group_layouts: &[frame_layout, &empty, &depth_layout],
                    push_constant_ranges: &[],
                })),
                vertex: wgpu::VertexState {
                    module: context.shader,
                    entry_point: Some("vs_sky"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: context.shader,
                    entry_point: Some("fs_sun_mask"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[
                            ("MASK_SAMPLES", f64::from(context.samples)),
                            ("MASK_CONTACT", f64::from(u8::from(contact))),
                        ],
                        ..Default::default()
                    },
                    targets: &[Some(FORMAT.into())],
                }),
                multiview: None,
                cache: None,
            });
            let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sun mask"),
                layout: &empty,
                entries: &[],
            });
            (pipeline, empty)
        });
        let size = if on { size } else { (1, 1) };
        let format = context.format;
        let (view, normals, group, depth) = targets(device, (&layout, &depth_layout), (depth, format), size);
        SunMask { layout, depth_layout, pass, view, normals, format, group, depth }
    }

    pub(super) fn on(&self) -> bool {
        self.pass.is_some()
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView, size: (u32, u32)) {
        if self.on() {
            let layouts = (&self.layout, &self.depth_layout);
            (self.view, self.normals, self.group, self.depth) = targets(device, layouts, (depth, self.format), size);
        }
    }

    /// The prepass's colour attachment: the frame's multisampled `color` resolved into the normals, or the normals
    /// themselves with one sample.
    pub(super) fn normals<'a>(
        &'a self,
        color: &'a wgpu::TextureView,
        samples: u32,
    ) -> wgpu::RenderPassColorAttachment<'a> {
        let (view, resolve_target, store) = match samples {
            1 => (&self.normals, None, wgpu::StoreOp::Store),
            _ => (color, Some(&self.normals), wgpu::StoreOp::Discard),
        };
        let ops = wgpu::Operations { load: wgpu::LoadOp::Clear(UP), store };
        wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target, ops }
    }

    /// Works the mask out from the depth and normals the main view's prepass stored, with the world-anchored `frame`.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::BindGroup) {
        let Some((pipeline, empty)) = &self.pass else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sun mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, empty, &[]);
        pass.set_bind_group(2, &self.depth, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The mask and the normals of `size`, the stage draws' group reading the mask and the pass's group reading the
/// normals and `depth`.
fn targets(
    device: &wgpu::Device,
    (layout, depth_layout): (&wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
    (depth, format): (&wgpu::TextureView, wgpu::TextureFormat),
    size: (u32, u32),
) -> (wgpu::TextureView, wgpu::TextureView, wgpu::BindGroup, wgpu::BindGroup) {
    let view = texture(device, FORMAT, size);
    let normals = texture(device, format, size);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sun mask"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&view) }],
    });
    let depth = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sun mask depth"),
        layout: depth_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&normals) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
        ],
    });
    (view, normals, group, depth)
}

fn texture(device: &wgpu::Device, format: wgpu::TextureFormat, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("sun mask"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
