//! The HDR post pass (`post.wgsl`): a renderer addition, not the game's look. The
//! renderer resolves the frame into [`Post::scene`] ([`SCENE_FORMAT`], linear colour); [`Post::draw`] builds the
//! bloom chain from it, meters it, and writes the exposed, tone-curved frame into the caller's target.

use crate::renderer::{depth_binding, texture_entry, TEXTURE};
use crate::skylight::NIGHT_PRE_EXPOSURE;

/// The float format of the scene and every target drawn before the post pass.
pub(crate) const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Bloom chain levels below the frame (`LEVELS` in `post.wgsl`).
const LEVELS: u32 = 6;
/// The composite's sharpening under TAA, whose resolve softens texture detail (Laplacian variance of a forest still: 4x
/// MSAA 422, TAA 201, TAA sharpened 355).
const TAA_SHARPEN: f32 = 1.0;

pub(crate) const SHADER: &str = include_str!("post.wgsl");

pub(crate) struct Post {
    source_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// Bins the scene's light (`cs_histogram`), then meters the bins into [`Post::exposure`] (`cs_meter`).
    meter: [wgpu::ComputePipeline; 2],
    /// The exposure the composite applies, kept from frame to frame.
    exposure: wgpu::Buffer,
    /// `cs_histogram`'s bins, which `cs_meter` empties.
    histogram: wgpu::Buffer,
    /// [`Meter`] for `cs_meter`.
    meter_settings: wgpu::Buffer,
    /// The time of the last frame metered ([`Post::draw`]).
    metered: Option<f32>,
    /// The composite's sharpening strength (0: none).
    sharpen: f32,
    /// The resolved frame.
    pub(crate) scene: wgpu::TextureView,
    targets: Targets,
    /// Whether the composite adds [`Post::shafts`] ([`crate::light_shafts`]).
    shafts: bool,
}

/// What depends on the frame's size: the bloom levels and the bind groups that read them.
struct Targets {
    /// Bloom levels, the largest (half the frame) first.
    levels: Vec<wgpu::TextureView>,
    /// Reading the frame, then each level, for the next pass.
    sources: Vec<wgpu::BindGroup>,
    composite: wgpu::BindGroup,
    /// The light shafts the composite adds (linear light, a quarter of the frame's size; 1x1 without them).
    shafts: wgpu::TextureView,
    /// `cs_histogram`'s and `cs_meter`'s: the frame and its sampler (none for `cs_meter`), then the depth, the
    /// exposure, the settings and the bins.
    meter: [[wgpu::BindGroup; 2]; 2],
}

/// The metering's grid, in `cs_histogram`'s workgroups (`METER_GRID` in `post.wgsl` over 16), and its bins (`BINS`).
const METER_GROUPS: (u32, u32) = (8, 4);
const BINS: u64 = 128;

/// `cs_meter`'s and `fs_composite`'s settings (`Meter` in `post.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Meter {
    step: f32,
    snap: f32,
    cabin_depth: f32,
    pre_exposure: f32,
    exposure: f32,
    saturation: f32,
    sharpen: f32,
    pad: f32,
}

/// What the frame's conditions do to the post pass ([`conditions`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Conditions {
    /// The renderer's units per cd/m2, by which the exposure takes the frame's light back to luminance: the sky's
    /// tables' ([`DayLight`](crate::skylight::DayLight)), else [`DAY_PRE_EXPOSURE`] or [`NIGHT_PRE_EXPOSURE`].
    pub pre_exposure: f32,
    /// The metered exposure's scale and the saturation's.
    pub exposure: f32,
    pub saturation: f32,
}

/// How much darker the frame is drawn in heavy rain or snow, and how much more saturated under a heavy cloud (ours,
/// as the Assetto Corsa filters grade the weather: Photorealistic lowers its exposure target by a fifth of the
/// weather's badness, Comfy raises the saturation with the overcast). The stages' rain and snow sections fog to a pale
/// grey that drew brighter and greyer than the dry sections.
const WET_EXPOSURE: f32 = 0.2;
const OVERCAST_SATURATION: f32 = 0.15;

/// The post pass's [`Conditions`] for `atmosphere`: the exposure lower by [`WET_EXPOSURE`] times the
/// [precipitation](crate::atmosphere::Atmosphere::precipitation), the saturation higher by [`OVERCAST_SATURATION`]
/// from a light to a heavy cloud ([`cloud_cover`](crate::atmosphere::Atmosphere::cloud_cover) 0.42 to 0.9).
pub(crate) fn conditions(atmosphere: &crate::atmosphere::Atmosphere) -> Conditions {
    let overcast = ((atmosphere.cloud_cover() - 0.42) / (0.9 - 0.42)).clamp(0.0, 1.0);
    Conditions {
        pre_exposure: if atmosphere.night { NIGHT_PRE_EXPOSURE } else { DAY_PRE_EXPOSURE },
        exposure: 1.0 - WET_EXPOSURE * atmosphere.precipitation(),
        saturation: 1.0 + OVERCAST_SATURATION * overcast,
    }
}

/// Without the sky's tables the frame's light stands for physical light at a fixed pre-exposure: by day the open
/// sunlit ground's white (1.93 linear, `OPEN_LIGHT` in `skylight.wgsl`) for 100 000 lux (a white face's luminance is
/// its illuminance over pi), at night [`NIGHT_PRE_EXPOSURE`].
const DAY_PRE_EXPOSURE: f32 = 1.93 * std::f32::consts::PI / 100_000.0;

/// The time between frames past which the exposure is set outright rather than eased, seconds.
const JUMP: f32 = 1.0;
/// How far from the eye the in-car view's cabin reaches, metres (`cs_meter` weighs it less).
pub(crate) const CABIN_REACH: f32 = 2.0;

/// In the in-car view of `view_projection` (reversed depth), the depth of a point [`CABIN_REACH`] ahead of the eye.
pub(crate) fn cabin_depth(view_projection: glam::Mat4) -> f32 {
    // The clip w is the distance along the view, so its row is the view's axis.
    let (z, w) = (view_projection.row(2), view_projection.row(3));
    let eye = crate::camera::eye(view_projection);
    let ahead = (eye + w.truncate().normalize() * CABIN_REACH).extend(1.0);
    z.dot(ahead) / w.dot(ahead)
}

impl Post {
    /// The post pass into targets of `format`, for frames of `size` and the frame's `depth`; with
    /// `shafts`, the composite adds [`Post::shafts`]; with `sharpen`, it sharpens the scene by [`TAA_SHARPEN`] (the
    /// temporal resolve's blur).
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        depth: &wgpu::TextureView,
        shafts: bool,
        sharpen: bool,
    ) -> Post {
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post source"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post bloom"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(
                    1,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                texture_entry(2, TEXTURE),
                texture_entry(3, depth_binding()),
                texture_entry(
                    4,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let shafts_on: &[(&str, f64)] = &[("SHAFTS", 1.0)];
        let pipeline = |entry: &str, layouts: &[&wgpu::BindGroupLayout], target: wgpu::ColorTargetState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(entry),
                    bind_group_layouts: &layouts.iter().copied().map(Some).collect::<Vec<_>>(),
                    immediate_size: 0,
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
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: if shafts && entry == "fs_composite" { shafts_on } else { &[] },
                        ..Default::default()
                    },
                    targets: &[Some(target)],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let down = pipeline("fs_down", &[&source_layout], SCENE_FORMAT.into());
        // Each level adds the blurred level below it.
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let up = pipeline(
            "fs_up",
            &[&source_layout],
            wgpu::ColorTargetState {
                format: SCENE_FORMAT,
                blend: Some(wgpu::BlendState { color: add, alpha: add }),
                write_mask: wgpu::ColorWrites::ALL,
            },
        );
        let composite = pipeline("fs_composite", &[&source_layout, &composite_layout], format.into());
        let meter = ["cs_histogram", "cs_meter"].map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        });
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
        };
        let exposure = buffer("exposure", 16, wgpu::BufferUsages::STORAGE);
        let histogram = buffer("histogram", BINS * 4, wgpu::BufferUsages::STORAGE);
        let meter_settings =
            buffer("meter", size_of::<Meter>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        let (scene, targets) = targets(
            device,
            [&source_layout, &composite_layout],
            &sampler,
            [&exposure, &meter_settings, &histogram],
            &meter,
            depth,
            size,
            shafts,
        );
        Post {
            source_layout,
            composite_layout,
            sampler,
            down,
            up,
            composite,
            meter,
            exposure,
            histogram,
            meter_settings,
            metered: None,
            sharpen: if sharpen { TAA_SHARPEN } else { 0.0 },
            scene,
            targets,
            shafts,
        }
    }

    /// For frames drawn at `size` (the composite scales them up to its target when drawn at a lower resolution), and
    /// the frame's `depth`.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, size: (u32, u32), depth: &wgpu::TextureView) {
        (self.scene, self.targets) = targets(
            device,
            [&self.source_layout, &self.composite_layout],
            &self.sampler,
            [&self.exposure, &self.meter_settings, &self.histogram],
            &self.meter,
            depth,
            size,
            self.shafts,
        );
    }

    /// The light shafts the composite adds ([`crate::light_shafts::LightShafts::draw`] draws them).
    pub(crate) fn shafts(&self) -> &wgpu::TextureView {
        &self.targets.shafts
    }

    /// Writes the frame in [`Post::scene`] into `target`, its exposure eased from the last frame's to `time`
    /// (seconds); `cabin_depth` in the in-car view ([`cabin_depth`]), else 0; graded by the frame's `conditions`.
    pub(crate) fn draw(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        time: f32,
        cabin_depth: f32,
        conditions: Conditions,
    ) {
        let step = self.metered.map_or(-1.0, |last| time - last);
        self.metered = Some(time);
        let snap = !(0.0..=JUMP).contains(&step);
        let settings = Meter {
            step: step.max(0.0),
            snap: f32::from(u8::from(snap)),
            cabin_depth,
            pre_exposure: conditions.pre_exposure,
            exposure: conditions.exposure,
            saturation: conditions.saturation,
            sharpen: self.sharpen,
            pad: 0.0,
        };
        queue.write_buffer(&self.meter_settings, 0, bytemuck::bytes_of(&settings));
        let t = &self.targets;
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    view: &wgpu::TextureView,
                    load,
                    pipeline: &wgpu::RenderPipeline,
                    groups: &[&wgpu::BindGroup]| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            for (index, group) in groups.iter().enumerate() {
                pass.set_bind_group(index as u32, *group, &[]);
            }
            pass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        {
            let mut pass =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("meter"), ..Default::default() });
            for ((pipeline, groups), size) in self.meter.iter().zip(&t.meter).zip([METER_GROUPS, (1, 1)]) {
                pass.set_pipeline(pipeline);
                for (index, group) in groups.iter().enumerate() {
                    pass.set_bind_group(index as u32, group, &[]);
                }
                pass.dispatch_workgroups(size.0, size.1, 1);
            }
        }
        for (level, source) in t.levels.iter().zip(&t.sources) {
            pass(encoder, level, clear, &self.down, &[source]);
        }
        for index in (1..t.levels.len()).rev() {
            pass(encoder, &t.levels[index - 1], wgpu::LoadOp::Load, &self.up, &[&t.sources[index + 1]]);
        }
        pass(encoder, target, clear, &self.composite, &[&t.sources[0], &t.composite]);
    }
}

/// The frame and the bloom levels for `size`, and their bind groups; the light shafts' map with `shafts`.
#[allow(clippy::too_many_arguments)]
fn targets(
    device: &wgpu::Device,
    [source_layout, composite_layout]: [&wgpu::BindGroupLayout; 2],
    sampler: &wgpu::Sampler,
    [exposure, meter_settings, histogram]: [&wgpu::Buffer; 3],
    [binning, metering]: &[wgpu::ComputePipeline; 2],
    depth: &wgpu::TextureView,
    (width, height): (u32, u32),
    shafts: bool,
) -> (wgpu::TextureView, Targets) {
    let texture = |label, width: u32, height: u32| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };
    let scene = texture("hdr scene", width, height).create_view(&Default::default());
    let levels: Vec<_> = (1..=LEVELS)
        .map(|level| texture("bloom", width >> level, height >> level).create_view(&Default::default()))
        .collect();
    let source = |view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post source"),
            layout: source_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    };
    let sources = std::iter::once(&scene).chain(&levels).map(source).collect();
    let group = |pipeline: &wgpu::ComputePipeline, index, entries: &[wgpu::BindGroupEntry]| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("meter"),
            layout: &pipeline.get_bind_group_layout(index),
            entries,
        })
    };
    let settings = wgpu::BindGroupEntry { binding: 4, resource: meter_settings.as_entire_binding() };
    let bins = wgpu::BindGroupEntry { binding: 5, resource: histogram.as_entire_binding() };
    let meter = [
        [
            group(
                binning,
                0,
                &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&scene) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            ),
            group(
                binning,
                1,
                &[
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
                    settings.clone(),
                    bins.clone(),
                ],
            ),
        ],
        [
            group(metering, 0, &[]),
            group(
                metering,
                1,
                &[wgpu::BindGroupEntry { binding: 3, resource: exposure.as_entire_binding() }, settings, bins],
            ),
        ],
    ];
    let shafts = match shafts {
        true => texture("light shafts", width.div_ceil(4), height.div_ceil(4)),
        false => texture("no light shafts", 1, 1),
    }
    .create_view(&Default::default());
    let composite = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post bloom"),
        layout: composite_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&levels[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: exposure.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&shafts) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(depth) },
            wgpu::BindGroupEntry { binding: 4, resource: meter_settings.as_entire_binding() },
        ],
    });
    (scene, Targets { levels, sources, composite, shafts, meter })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn heavy_rain_draws_darker_and_a_heavy_cloud_more_saturated() {
        use crate::atmosphere::Atmosphere;
        let of = |section: &str| conditions(&Atmosphere { section: Some(section.into()), ..Atmosphere::default() });
        let clear = of("54E_crisp_clear");
        assert_eq!(clear, Conditions { pre_exposure: DAY_PRE_EXPOSURE, exposure: 1.0, saturation: 1.0 });
        let heavy = of("54O_E_heavyrain_heavycloud");
        assert_eq!((heavy.exposure, heavy.saturation), (1.0 - WET_EXPOSURE, 1.0 + OVERCAST_SATURATION));
        assert_eq!(of("54O_E_lightrain_heavycloud").exposure, 1.0 - WET_EXPOSURE / 2.0);
        // The fog sections are not darkened, nor the dry overcast.
        assert_eq!(of("54O_E_lightfog_heavycloud").exposure, 1.0);
        assert_eq!(of("54O_E_norain_lightcloud").exposure, 1.0);
    }

    #[test]
    fn the_cabin_reaches_two_metres_ahead_whatever_the_look() {
        let eye = Vec3::new(10.0, 2.0, -5.0);
        let forward = Vec3::new(0.3, -0.2, 1.0).normalize();
        let view_projection =
            crate::coords::perspective(1.2, 16.0 / 9.0, 0.1) * crate::coords::view_matrix(eye, forward);
        let depth = |p: Vec3| {
            let clip = view_projection * p.extend(1.0);
            clip.z / clip.w
        };
        let at = cabin_depth(view_projection);
        assert!((at - depth(eye + forward * CABIN_REACH)).abs() < 1e-5);
        // Reversed: the cabin is nearer, so deeper.
        assert!(depth(eye + forward * 1.0) > at && depth(eye + forward * 5.0) < at);
    }
}
