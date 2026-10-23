//! Weather and particle effects beyond the game's look (`Enhance::particles`): rain and snow drawn on the GPU about
//! the eye (`rain.wgsl`), in place of the precipitation particles, and the light and depth the particles read to be
//! lit by the sun, shaded by its cascades and faded where they meet the stage (`fs_soft` in `particles.wgsl`). Both
//! share one bind group (`effects.wgsl`): the camera, the sun, its cascades and the frame's multisampled depth, so they
//! draw in a pass that reads the depth instead of writing it.

use crate::atmosphere::Atmosphere;
use crate::particles::Precipitation;
use crate::renderer::{depth_binding, depth_source, LampUniform, DEPTH_FORMAT, MAX_LAMPS};
use crate::sun::Cascades;
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{Mat4, Vec3};

/// The shared header (`effects.wgsl`), appended to the shaders that bind the effects at group 1.
pub(crate) const HEADER: &str = concat!(include_str!("effects.wgsl"), include_str!("lamps.wgsl"));
const RAIN: &str = include_str!("rain.wgsl");

/// The rain's shader for a depth of `samples` ([`depth_source`]).
pub(crate) fn rain_source(samples: u32) -> String {
    depth_source(&format!("{RAIN}\n{HEADER}"), samples)
}
/// Drops or flakes in the near and far layers at full rain or snow (`NEAR_COUNT` in `rain.wgsl`).
const NEAR_COUNT: u32 = 16000;
const FAR_COUNT: u32 = 40000;
/// The eye's velocity follows its motion by this share a frame, for steady streaks (`rain.wgsl`).
const VELOCITY_SMOOTHING: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    motion: [f32; 4],
    sun: [f32; 4],
    sun_colour: [f32; 4],
    ambient: [f32; 4],
    sky: [f32; 4],
    fog: [f32; 4],
    screen: [f32; 4],
    sun_matrices: [[[f32; 4]; 4]; 3],
    sun_splits: [f32; 4],
    lamps: [LampUniform; MAX_LAMPS],
}

pub(crate) struct Effects {
    /// The effects' bind group layout, group 1 of the passes that use them.
    pub(crate) layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    /// The frame's uniform as last written, and the mirrors' passes' own (`Effects::write_mirror`).
    written: Uniform,
    mirror_uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    sun_map: wgpu::TextureView,
    group: wgpu::BindGroup,
    /// Group 0 of the rain pipeline, which binds nothing there.
    empty: wgpu::BindGroup,
    rain: wgpu::RenderPipeline,
    precipitation: Option<Precipitation>,
    /// The eye and the time of the last frame, and the eye's velocity.
    last: Option<(Vec3, f32)>,
    velocity: Vec3,
    /// The car's lamps at night (`Renderer::set_lamps`).
    lamps: [LampUniform; MAX_LAMPS],
}

impl Effects {
    /// For colour targets of `format`, reading the `depth` of `samples` and the sun's cascades `sun_map`.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        sun_map: &wgpu::TextureView,
        samples: u32,
    ) -> Effects {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, depth_binding(samples)),
                entry(
                    2,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
            ],
        });
        let buffer = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let (uniform, mirror_uniform) = (buffer("effects"), buffer("mirror effects"));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effects sun"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let group = bind_group(device, &layout, &uniform, &sampler, depth, sun_map);
        let empty_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] });
        let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("empty"),
            layout: &empty_layout,
            entries: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rain"),
            source: wgpu::ShaderSource::Wgsl(rain_source(samples).into()),
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain"),
            bind_group_layouts: &[&empty_layout, &layout],
            push_constant_ranges: &[],
        });
        let hdr = format == crate::post::SCENE_FORMAT;
        let rain = {
            let constants = [("HDR", f64::from(u8::from(hdr)))];
            let options = wgpu::PipelineCompilationOptions { constants: &constants, ..Default::default() };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rain"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_rain"),
                    compilation_options: options.clone(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_rain"),
                    compilation_options: options,
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                multiview: None,
                cache: None,
            })
        };
        Effects {
            layout,
            uniform,
            written: Uniform::default(),
            mirror_uniform,
            sampler,
            sun_map: sun_map.clone(),
            group,
            empty,
            rain,
            precipitation: None,
            last: None,
            velocity: Vec3::ZERO,
            lamps: [LampUniform::default(); MAX_LAMPS],
        }
    }

    /// Rebinds the frame's depth after a resize.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView) {
        self.group = bind_group(device, &self.layout, &self.uniform, &self.sampler, depth, &self.sun_map);
    }

    pub(crate) fn set_precipitation(&mut self, precipitation: Option<Precipitation>) {
        self.precipitation = precipitation;
    }

    /// How hard it rains (0 when it does not, or snows).
    /// Whether rain or snow falls (the draws [`Effects::draw_rain`] makes).
    pub(crate) fn precipitating(&self) -> bool {
        self.precipitation.is_some_and(|p| p.intensity > 0.0)
    }

    pub(crate) fn rain(&self) -> f32 {
        self.precipitation.filter(|p| !p.snow).map_or(0.0, |p| p.intensity)
    }

    /// The car's lamps, which light the rain, snow and particles at night.
    pub(crate) fn set_lamps(&mut self, lamps: [LampUniform; MAX_LAMPS]) {
        self.lamps = lamps;
    }

    /// The eye's velocity for the next frame, where the frames do not move it in time (a screenshot).
    pub(crate) fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    /// Writes the frame's uniform: the view, the time, the frame's `size`, the stage's light (the atmosphere, its sky's
    /// colour times the level beside it) and the sun's cascades (`None` without them). The eye's velocity follows its motion
    /// when the time moves on.
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        time: f32,
        size: (u32, u32),
        (atmosphere, sky_level): (&Atmosphere, f32),
        cascades: Option<&Cascades>,
    ) {
        let eye = crate::camera::eye(view_projection);
        if let Some((last, then)) = self.last {
            if time > then {
                let moved = (eye - last) / (time - then);
                self.velocity = self.velocity.lerp(moved, VELOCITY_SMOOTHING);
            }
        }
        self.last = Some((eye, time));
        let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let precipitation = self.precipitation.map_or(0.0, |p| p.intensity);
        let snow = self.precipitation.is_some_and(|p| p.snow);
        let sky = (atmosphere.zenith() * sky_level + atmosphere.fog_color) * 0.5;
        let (matrices, shade) = cascades.map_or(([Mat4::IDENTITY; 3], 0.0), |c| (c.matrices, 1.0));
        let splits = crate::sun::SPLITS;
        let uniform = Uniform {
            view_projection: view_projection.to_cols_array_2d(),
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            eye: eye.extend(time).to_array(),
            motion: self.velocity.extend(precipitation).to_array(),
            sun: atmosphere.sun_direction.extend(shade).to_array(),
            sun_colour: atmosphere.diffuse.extend(f32::from(u8::from(snow))).to_array(),
            ambient: atmosphere.ambient.extend(0.0).to_array(),
            sky: sky.extend(atmosphere.fog_end).to_array(),
            fog: atmosphere.fog_color.extend(atmosphere.haze()).to_array(),
            screen: [width, height, 1.0 / width, 1.0 / height],
            sun_matrices: matrices.map(|m| m.to_cols_array_2d()),
            sun_splits: [splits[0], splits[1], splits[2], 0.0],
            lamps: self.lamps,
        };
        queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
        self.written = uniform;
    }

    /// The bind group of a mirror's pass ([`crate::mirrors`]) reading its `depth`.
    pub(crate) fn mirror_group(&self, device: &wgpu::Device, depth: &wgpu::TextureView) -> wgpu::BindGroup {
        bind_group(device, &self.layout, &self.mirror_uniform, &self.sampler, depth, &self.sun_map)
    }

    /// Writes the mirrors' uniform for a mirror's view `view_projection` of `size`, the frame's reflected by `reflect`
    /// (so that the rain streaks along the mirrored motion). Each mirror's pass is its own submission.
    pub(crate) fn write_mirror(&self, queue: &wgpu::Queue, view_projection: Mat4, reflect: Mat4, size: (u32, u32)) {
        let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let mut uniform = self.written;
        uniform.view_projection = view_projection.to_cols_array_2d();
        uniform.inverse_view_projection = view_projection.inverse().to_cols_array_2d();
        uniform.eye = crate::camera::eye(view_projection).extend(uniform.eye[3]).to_array();
        uniform.motion = reflect.transform_vector3(self.velocity).extend(uniform.motion[3]).to_array();
        uniform.screen = [width, height, 1.0 / width, 1.0 / height];
        queue.write_buffer(&self.mirror_uniform, 0, bytes_of(&uniform));
    }

    /// Draws the rain or snow into a mirror's `pass` with its [`Effects::mirror_group`].
    pub(crate) fn draw_mirror_rain(&self, pass: &mut wgpu::RenderPass<'_>, group: &wgpu::BindGroup) {
        self.draw_with(pass, &self.rain, group);
    }

    /// The bind group the particles' soft pipeline takes at group 1.
    pub(crate) fn group(&self) -> &wgpu::BindGroup {
        &self.group
    }

    /// Draws the rain or snow into `pass` (the multisampled scene with the depth read only).
    pub(crate) fn draw_rain(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_with(pass, &self.rain, &self.group);
    }

    fn draw_with(&self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
        let Some(precipitation) = self.precipitation else { return };
        let count = ((NEAR_COUNT + FAR_COUNT) as f32 * precipitation.intensity.clamp(0.0, 1.0)) as u32;
        if count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.empty, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.draw(0..6, 0..count);
    }
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    depth: &wgpu::TextureView,
    sun_map: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("effects"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(sun_map) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}
