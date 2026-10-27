//! The sun's shadow and the contact shading over the main view ([`Enhance::lighting`](super::Enhance::lighting); ours),
//! worked out from the depth and the normals every opaque stage draw laid first (`fs_prepass`) and read back by those
//! draws' shading (`SUN_MASK`): the sun's share per pixel (`fs_sun_mask`; the soft shadows' search and taps run once
//! per pixel there, out of the stage's shader), and the contact shading per 2x2 pixels (`fs_contact`).

use super::pipelines::{texture_entry, TEXTURE};
use super::{depth_binding, Context};
use crate::sun::Quality;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// The prepass's normals (`fs_prepass`): 10 bits a channel, half the bytes of the frame's colour.
pub(super) const NORMALS: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
/// The normals' clear colour: up, where nothing was drawn.
const UP: wgpu::Color = wgpu::Color { r: 0.5, g: 1.0, b: 0.5, a: 1.0 };

pub(super) struct SunMask {
    /// The stage draws' group 2: the mask and the contact shading.
    layout: wgpu::BindGroupLayout,
    /// The passes' group 2: the normals and the depth, at `scene_colour`'s and `scene_depth`'s bindings.
    depth_layout: wgpu::BindGroupLayout,
    /// The passes' pipelines; `None` without the mask.
    passes: Option<Passes>,
    targets: Targets,
    /// The sun's first cascade's depth range (`sun_ranges`), a texel standing in without the cascades.
    ranges: wgpu::TextureView,
}

struct Passes {
    mask: wgpu::RenderPipeline,
    contact: wgpu::RenderPipeline,
    /// Their empty group 1.
    empty: wgpu::BindGroup,
}

/// What [`SunMask`] draws into at the frame's size, and the groups reading them.
struct Targets {
    mask: wgpu::TextureView,
    /// At half the size.
    contact: wgpu::TextureView,
    /// The prepass's normals (the prepass's pipelines draw in [`NORMALS`]).
    normals: wgpu::TextureView,
    /// The mask and the contact shading for the stage draws, and the normals and the depth for the passes.
    group: wgpu::BindGroup,
    depth: wgpu::BindGroup,
}

impl SunMask {
    /// The stage draws' group 2 layout.
    pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun mask"),
            entries: &[texture_entry(3, TEXTURE), texture_entry(4, TEXTURE)],
        })
    }

    /// The mask over a `depth` of `size` pixels, with the contact shading as the context says; its passes only with
    /// the context's [`Context::lighting`] (else texels the draws never read).
    pub(super) fn new(
        context: &Context<'_>,
        (layout, frame_layout): (wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
        depth: &wgpu::TextureView,
        ranges: Option<&wgpu::TextureView>,
        size: (u32, u32),
        quality: Quality,
    ) -> SunMask {
        let device = context.device;
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun mask depth"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(1, depth_binding()),
                texture_entry(
                    5,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
            ],
        });
        let passes = context.lighting.then(|| {
            let empty = device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("sun mask"), entries: &[] });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sun mask"),
                bind_group_layouts: &[Some(frame_layout), Some(&empty), Some(&depth_layout)],
                immediate_size: 0,
            });
            let pipeline = |entry: &str| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
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
                        entry_point: Some(entry),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &[
                                ("SEARCH_TEXELS", f64::from(quality.search_texels)),
                                ("SOFT_TEXELS", f64::from(quality.soft_texels)),
                            ],
                            ..Default::default()
                        },
                        targets: &[Some(FORMAT.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
            let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sun mask"),
                layout: &empty,
                entries: &[],
            });
            Passes { mask: pipeline("fs_sun_mask"), contact: pipeline("fs_contact"), empty }
        });
        let size = if passes.is_some() { size } else { (1, 1) };
        let ranges = ranges.cloned().unwrap_or_else(|| no_ranges(device));
        let targets = Targets::new(device, (&layout, &depth_layout), (depth, &ranges), size);
        SunMask { layout, depth_layout, passes, targets, ranges }
    }

    pub(super) fn on(&self) -> bool {
        self.passes.is_some()
    }

    /// The stage draws' group 2.
    pub(super) fn group(&self) -> &wgpu::BindGroup {
        &self.targets.group
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView, size: (u32, u32)) {
        if self.on() {
            self.targets = Targets::new(device, (&self.layout, &self.depth_layout), (depth, &self.ranges), size);
        }
    }

    /// The prepass's colour attachment: the normals.
    pub(super) fn normals(&self) -> wgpu::RenderPassColorAttachment<'_> {
        let ops = wgpu::Operations { load: wgpu::LoadOp::Clear(UP), store: wgpu::StoreOp::Store };
        wgpu::RenderPassColorAttachment { view: &self.targets.normals, depth_slice: None, resolve_target: None, ops }
    }

    /// Works the mask and the contact shading out from the depth and normals the main view's prepass stored, with the
    /// world-anchored `frame`.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::BindGroup) {
        let Some(passes) = &self.passes else { return };
        for (pipeline, view) in [(&passes.mask, &self.targets.mask), (&passes.contact, &self.targets.contact)] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun mask"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.set_bind_group(1, &passes.empty, &[]);
            pass.set_bind_group(2, &self.targets.depth, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

impl Targets {
    /// The targets for `size`, the passes reading the normals and `depth`.
    fn new(
        device: &wgpu::Device,
        (layout, depth_layout): (&wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
        (depth, ranges): (&wgpu::TextureView, &wgpu::TextureView),
        (width, height): (u32, u32),
    ) -> Targets {
        let mask = texture(device, FORMAT, (width, height));
        let contact = texture(device, FORMAT, (width.div_ceil(2), height.div_ceil(2)));
        let normals = texture(device, NORMALS, (width, height));
        let view = wgpu::BindingResource::TextureView;
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun mask"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 3, resource: view(&mask) },
                wgpu::BindGroupEntry { binding: 4, resource: view(&contact) },
            ],
        });
        let depth = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun mask depth"),
            layout: depth_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: view(&normals) },
                wgpu::BindGroupEntry { binding: 1, resource: view(depth) },
                wgpu::BindGroupEntry { binding: 5, resource: view(ranges) },
            ],
        });
        Targets { mask, contact, normals, group, depth }
    }
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

/// A texel standing in for the cascade's depth range.
fn no_ranges(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("sun range (off)"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
}
