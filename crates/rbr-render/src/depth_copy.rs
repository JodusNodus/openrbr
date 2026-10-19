//! The frame's depth as a single-sample texture: the multisampled depth's first sample, copied once per frame for the
//! passes that read it many times per pixel (the wet roads' screen-space reflections). The
//! same values, cheaper to read scattered over the screen. Not made for the water's reflections alone, which read too
//! little of it to pay for the copy.

pub(crate) const SHADER: &str = "
@group(0) @binding(0) var depth: texture_depth_multisampled_2d;

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_copy(@builtin(position) clip: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(depth, vec2<i32>(clip.xy), 0));
}
";
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

pub(crate) struct DepthCopy {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    view: wgpu::TextureView,
}

impl DepthCopy {
    /// The copy of the multisampled `depth` (`size` pixels).
    pub(crate) fn new(device: &wgpu::Device, size: (u32, u32), depth: &wgpu::TextureView) -> DepthCopy {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth copy"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: crate::renderer::depth_binding(),
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth copy"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth copy"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("depth copy"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_copy"),
                compilation_options: Default::default(),
                targets: &[Some(FORMAT.into())],
            }),
            multiview: None,
            cache: None,
        });
        let (group, view) = targets(device, &layout, size, depth);
        DepthCopy { layout, pipeline, group, view }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, size: (u32, u32), depth: &wgpu::TextureView) {
        (self.group, self.view) = targets(device, &self.layout, size, depth);
    }

    /// The copy, an `R32Float` texture.
    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// A texel of the copy's format, to bind where a pass reads the copy if there is one.
    pub(crate) fn texel(device: &wgpu::Device) -> wgpu::TextureView {
        texture(device, (1, 1))
    }

    /// Copies the frame's depth, which the frame's pass stored.
    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth copy"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: (u32, u32),
    depth: &wgpu::TextureView,
) -> (wgpu::BindGroup, wgpu::TextureView) {
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("depth copy"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(depth) }],
    });
    (group, texture(device, size))
}

fn texture(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth copy"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
