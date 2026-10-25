//! The volumetric clouds ([`Enhance::clouds`](super::Enhance::clouds) with [`Enhance::sky`](super::Enhance::sky);
//! ours, `cloud_volume.wgsl`): their noise made once (`cloud_noise.wgsl`), and each main-view frame by day the weather
//! map around the eye and the march at half the frame's size, blended into the last frame's, which the sky's draw
//! lays over the sky (`fs_sky`). The game's cloud faces are not drawn while they are.

use super::pipelines::{texture_entry, TEXTURE};
use super::{depth_binding, Context};
use crate::atmosphere::Atmosphere;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use std::cell::Cell;

const NOISE: &str = include_str!("../cloud_noise.wgsl");
const SHAPE_SIZE: u32 = 128;
const DETAIL_SIZE: u32 = 32;
const WEATHER_SIZE: u32 = 512;
/// The dome's side, texels: the clouds over the upper hemisphere for the reflections and mirrors (`fs_cloud_dome`).
const DOME_SIZE: u32 = 256;
/// The weather map's side, metres: twice the farthest cloud marched (`VOLUME_REACH` in `cloud_volume.wgsl`).
const WEATHER_REACH: f32 = 120_000.0;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WEATHER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

/// How fast the clouds drift over the stage, metres a second (ours).
const DRIFT_SPEED: f32 = 11.0;

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Params {
    previous: [[f32; 4]; 4],
    state: [f32; 4],
    weather: [f32; 4],
    layer: [f32; 4],
    shape: [f32; 4],
}

/// The sky's own clouds (ours), from a hash of the section and the cover chosen over it, so that each stage and weather
/// has its own: the direction they drift in (metres a second) and where their field starts (units of the layer,
/// `cloud_at` in `clouds.wgsl`).
pub(super) fn drift(a: &Atmosphere) -> ([f32; 2], [f32; 2]) {
    let key = format!("{}/{:?}", a.section.as_deref().unwrap_or(""), a.clouds);
    let hash = key.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    let angle = (hash & 0xffff) as f32 / 65536.0 * std::f32::consts::TAU;
    let start = [((hash >> 16) & 0xff) as f32 * 7.3, (hash >> 24) as f32 * 7.3];
    ([angle.cos() * DRIFT_SPEED, angle.sin() * DRIFT_SPEED], start)
}

/// The clouds' layer by the sky's cover (ours): its base over the eye and thickness (metres), the cirrus' cover and
/// the overcast deck's light (`cloud_params.layer`), and the shapes' tile (metres) and lowest and highest tops
/// (`cloud_params.shape`). A clear sky has small fair-weather cumulus under a veil of cirrus; a partly cloudy one
/// cumulus, the thickest towering; light cloud a lower, flatter broken stratocumulus; an overcast sky a closed deck,
/// lower, thicker and darker as it rains.
fn layer(a: &Atmosphere) -> ([f32; 4], [f32; 4]) {
    let rain = a.precipitation();
    let cover = a.cloud_cover();
    if a.overcast() {
        ([900.0 - 400.0 * rain, 1200.0 + 600.0 * rain, 0.0, 1.0 - 0.45 * rain], [2000.0, 0.5, 1.0, 0.0])
    } else if cover <= 0.2 {
        ([1200.0, 1000.0, 0.8, 1.0], [1600.0, 0.2, 0.7, 0.0])
    } else if cover <= 0.45 {
        ([1400.0, 2200.0, 0.4, 1.0], [2500.0, 0.3, 1.0, 0.0])
    } else {
        ([1100.0, 800.0, 0.15, 1.0], [2500.0, 0.55, 1.0, 0.0])
    }
}

/// The height over the eye the volumetric clouds' shadows fall from (`cloud_shadow` in `clouds.wgsl`): low in the
/// layer, where most of a cloud is.
pub(super) fn shadow_height(a: &Atmosphere) -> f32 {
    let (layer, _) = layer(a);
    layer[0] + 0.3 * layer[1]
}

pub(super) struct CloudVolume {
    /// The sky draw's group 1 layout, and the group bound without the clouds (a texel that leaves the sky as it is).
    sky_layout: wgpu::BindGroupLayout,
    placeholder: wgpu::BindGroup,
    passes: Option<Passes>,
}

struct Passes {
    march_layout: wgpu::BindGroupLayout,
    march: wgpu::RenderPipeline,
    dome: wgpu::RenderPipeline,
    weather_pass: wgpu::RenderPipeline,
    resources: Resources,
    /// The weather pass's group 2 (the parameters) and both passes' empty group 1.
    params_group: wgpu::BindGroup,
    empty: wgpu::BindGroup,
    targets: Targets,
    /// The frames marched, the image the last one wrote, the last frame's rotation-only clip matrix.
    frame: Cell<u32>,
    current: Cell<usize>,
    previous: Cell<Option<Mat4>>,
}

/// What the march reads besides the depth and its history.
struct Resources {
    shape: wgpu::TextureView,
    detail: wgpu::TextureView,
    weather: wgpu::TextureView,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
}

/// The two images the frames alternate between, at half the frame's size, and the two domes; the march's and the
/// dome's groups writing each (reading the other as its history) and the sky's group reading both.
struct Targets {
    images: [wgpu::TextureView; 2],
    domes: [wgpu::TextureView; 2],
    march: [wgpu::BindGroup; 2],
    dome_march: [wgpu::BindGroup; 2],
    sky: [wgpu::BindGroup; 2],
}

impl CloudVolume {
    /// The clouds for `context` over the main view's `depth` of `size` pixels when `on`, else the placeholder the sky
    /// draw binds.
    pub(super) fn new(
        context: &Context<'_>,
        queue: &wgpu::Queue,
        frame_layout: &wgpu::BindGroupLayout,
        (depth, size): (&wgpu::TextureView, (u32, u32)),
        on: bool,
    ) -> CloudVolume {
        let device = context.device;
        let sky_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud image"),
            entries: &[texture_entry(7, TEXTURE), texture_entry(8, TEXTURE)],
        });
        let clear = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("no clouds"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Nothing over the sky, all of it showing (alpha 1, as half floats).
        let texel: [u16; 4] = [0, 0, 0, 0x3c00];
        queue.write_texture(
            clear.as_image_copy(),
            bytemuck::cast_slice(&texel),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(8), rows_per_image: None },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let clear = clear.create_view(&Default::default());
        let placeholder = sky_group(device, &sky_layout, (&clear, &clear));
        let passes = on.then(|| Passes::new(context, queue, frame_layout, &sky_layout, (depth, size)));
        CloudVolume { sky_layout, placeholder, passes }
    }

    pub(super) fn sky_layout(&self) -> &wgpu::BindGroupLayout {
        &self.sky_layout
    }

    pub(super) fn on(&self) -> bool {
        self.passes.is_some()
    }

    /// The sky draw's group 1: the image the last march wrote.
    pub(super) fn sky_group(&self) -> &wgpu::BindGroup {
        self.passes.as_ref().map_or(&self.placeholder, |p| &p.targets.sky[p.current.get()])
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView, size: (u32, u32)) {
        if let Some(p) = &mut self.passes {
            p.targets = Targets::new(device, (&p.march_layout, &self.sky_layout), &p.resources, depth, size);
            p.previous.set(None);
        }
    }

    /// Draws the weather map around `eye` and marches the clouds of `atmosphere` with the world-anchored `frame`
    /// (whose rotation-only clip matrix is `sky_view_projection`), reading the prepass's depth when it is this
    /// frame's (`depth_fresh`).
    pub(super) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        (frame, atmosphere): (&wgpu::BindGroup, &Atmosphere),
        (eye, sky_view_projection): (Vec3, Mat4),
        depth_fresh: bool,
    ) {
        let Some(p) = &self.passes else { return };
        let texel = WEATHER_REACH / WEATHER_SIZE as f32;
        let corner = ((Vec2::new(eye.x, eye.z) - WEATHER_REACH * 0.5) / texel).floor() * texel;
        let previous = p.previous.replace(Some(sky_view_projection));
        let (layer, shape) = layer(atmosphere);
        let params = Params {
            previous: previous.unwrap_or(Mat4::IDENTITY).to_cols_array_2d(),
            state: [
                p.frame.get() as f32,
                f32::from(u8::from(previous.is_some())),
                f32::from(u8::from(depth_fresh)),
                0.0,
            ],
            weather: [corner.x, corner.y, WEATHER_REACH, 0.0],
            layer,
            shape,
        };
        p.frame.set((p.frame.get() + 1) % 64);
        queue.write_buffer(&p.resources.params, 0, bytemuck::bytes_of(&params));
        let current = 1 - p.current.get();
        p.current.set(current);
        let passes = [
            ("cloud weather", &p.resources.weather, &p.weather_pass, &p.params_group),
            ("cloud march", &p.targets.images[current], &p.march, &p.targets.march[current]),
            ("cloud dome", &p.targets.domes[current], &p.dome, &p.targets.dome_march[current]),
        ];
        for (label, view, pipeline, group) in passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.set_bind_group(1, &p.empty, &[]);
            pass.set_bind_group(2, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

impl Passes {
    fn new(
        context: &Context<'_>,
        queue: &wgpu::Queue,
        frame_layout: &wgpu::BindGroupLayout,
        sky_layout: &wgpu::BindGroupLayout,
        (depth, size): (&wgpu::TextureView, (u32, u32)),
    ) -> Passes {
        let device = context.device;
        let [shape, detail] = [(SHAPE_SIZE, 0), (DETAIL_SIZE, 1)].map(|(side, kind)| noise(device, queue, side, kind));
        let resources = Resources {
            shape,
            detail,
            weather: texture(device, WEATHER_FORMAT, (WEATHER_SIZE, WEATHER_SIZE)),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("clouds"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("clouds"),
                size: size_of::<Params>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let volume = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D3,
            multisampled: false,
        };
        let march_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud march"),
            entries: &[
                texture_entry(1, depth_binding(context.samples)),
                texture_entry(5, volume),
                texture_entry(6, volume),
                texture_entry(7, TEXTURE),
                texture_entry(8, TEXTURE),
                texture_entry(9, uniform),
                texture_entry(10, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud weather"),
            entries: &[texture_entry(9, uniform)],
        });
        let empty_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("clouds"), entries: &[] });
        let pipeline = |group: &wgpu::BindGroupLayout, entry: &str, format: wgpu::TextureFormat| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry),
                bind_group_layouts: &[frame_layout, &empty_layout, group],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
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
                            ("MASK_SAMPLES", f64::from(context.samples)),
                            ("SKY_LUT", f64::from(u8::from(context.sky_lut))),
                        ],
                        ..Default::default()
                    },
                    targets: &[Some(format.into())],
                }),
                multiview: None,
                cache: None,
            })
        };
        let march = pipeline(&march_layout, "fs_cloud_march", FORMAT);
        let dome = pipeline(&march_layout, "fs_cloud_dome", FORMAT);
        let weather_pass = pipeline(&params_layout, "fs_cloud_weather", WEATHER_FORMAT);
        let params_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud weather"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry { binding: 9, resource: resources.params.as_entire_binding() }],
        });
        let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clouds"),
            layout: &empty_layout,
            entries: &[],
        });
        let targets = Targets::new(device, (&march_layout, sky_layout), &resources, depth, size);
        Passes {
            march_layout,
            march,
            dome,
            weather_pass,
            resources,
            params_group,
            empty,
            targets,
            frame: Cell::new(0),
            current: Cell::new(0),
            previous: Cell::new(None),
        }
    }
}

impl Targets {
    fn new(
        device: &wgpu::Device,
        (march_layout, sky_layout): (&wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
        r: &Resources,
        depth: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> Targets {
        let half = (width.div_ceil(2), height.div_ceil(2));
        let images = [(); 2].map(|_| texture(device, FORMAT, half));
        let domes = [(); 2].map(|_| texture(device, FORMAT, (DOME_SIZE, DOME_SIZE)));
        let group = |history: &wgpu::TextureView| {
            let view = wgpu::BindingResource::TextureView;
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cloud march"),
                layout: march_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 1, resource: view(depth) },
                    wgpu::BindGroupEntry { binding: 5, resource: view(&r.shape) },
                    wgpu::BindGroupEntry { binding: 6, resource: view(&r.detail) },
                    wgpu::BindGroupEntry { binding: 7, resource: view(&r.weather) },
                    wgpu::BindGroupEntry { binding: 8, resource: view(history) },
                    wgpu::BindGroupEntry { binding: 9, resource: r.params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&r.sampler) },
                ],
            })
        };
        let march = [0, 1].map(|k| group(&images[1 - k]));
        let dome_march = [0, 1].map(|k| group(&domes[1 - k]));
        let sky = [0, 1].map(|k| sky_group(device, sky_layout, (&images[k], &domes[k])));
        Targets { images, domes, march, dome_march, sky }
    }
}

fn sky_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    (image, dome): (&wgpu::TextureView, &wgpu::TextureView),
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud image"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(image) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(dome) },
        ],
    })
}

fn texture(device: &wgpu::Device, format: wgpu::TextureFormat, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("clouds"),
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

/// A tiling noise texture `side` texels across (`cs_cloud_noise`): the shape's (`kind` 0) or the detail's (1).
fn noise(device: &wgpu::Device, queue: &wgpu::Queue, side: u32, kind: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cloud noise"),
        size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: side },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cloud noise"),
        source: wgpu::ShaderSource::Wgsl(NOISE.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cloud noise"),
        layout: None,
        module: &module,
        entry_point: Some("cs_cloud_noise"),
        compilation_options: Default::default(),
        cache: None,
    });
    let params = wgpu::util::DeviceExt::create_buffer_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("cloud noise"),
            contents: bytemuck::cast_slice(&[side, kind, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        },
    );
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud noise"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(side.div_ceil(4), side.div_ceil(4), side.div_ceil(4));
    }
    queue.submit([encoder.finish()]);
    view
}
