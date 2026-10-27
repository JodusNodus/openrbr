//! Volumetric sunlight in the air (`light_shafts.wgsl`; ours, a renderer addition under
//! [`crate::renderer::Enhance::sky`] with the HDR pass): a ray march per texel of the quarter-size map, from the eye
//! along the view ray to the scene's depth (`MARCH_REACH` metres at most, the whole way where only sky is drawn),
//! sampling the sun's shadow cascades (`air_sun_seen` in `light_shafts.wgsl`), so the sun shines through the canopy and the
//! cloud gaps in shafts looking toward and across it, in haze, fog and dust. The post pass's composite adds the map to
//! the scene ([`crate::post::Post::shafts`]). At night the map holds instead the car's lamps' light scattered by the
//! air ([`Beams`]), the headlights' beams in the fog.

use crate::atmosphere::Atmosphere;
use crate::post::SCENE_FORMAT;
use crate::renderer::{LampUniform, MAX_LAMPS};
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{Mat4, Vec3};

pub(crate) const SHADER: &str = concat!(include_str!("light_shafts.wgsl"), include_str!("lamps.wgsl"));
/// How strongly the march's in-scatter adds to the scene: the sun's colour times the strength below times this (ours;
/// kept subtle at a clear midday, strong at a low sun, in fog and in dust).
const MARCH_GAIN: f32 = 0.25;
/// The sun's true elevation, degrees, over which the march fades in after sunrise.
const RISEN: [f32; 2] = [0.0, 4.0];

/// The air's scattering for the lamps' beams outside `UseFog`, as a share of the fog's for the section's range (ours: a
/// faint haze).
const CLEAR_AIR: f32 = 0.2;
/// The same under `UseFog` in a rain section, whose fog stands for the rain's spray and mist (ours: drops are
/// millimetres and scatter far less than fog's, heavy rain still seeing about 2 km).
const RAIN_AIR: f32 = 0.1;

/// The dust grid's cells across, down and along (ours): 64 x 32 x 64 over about 80 x 40 x 80 m, 1.25 m a cell.
const DUST_CELLS: [usize; 3] = [64, 32, 64];
/// The dust grid's cell, metres.
const DUST_CELL: f32 = 1.25;
/// The most density a billboard leaves in a cell it covers, per its alpha (ours; the shader's `DUST_GAIN` scales it
/// into per-metre density).
const DUST_PER_ALPHA: f32 = 1.0;
/// Billboards smaller than this radius, metres, hold no dust (sparks and stones).
const DUST_MIN_RADIUS: f32 = 0.15;

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    sun: [f32; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    march_dir: [f32; 4],
    march_colour: [f32; 4],
    march_fog: [f32; 4],
    march_splits: [f32; 4],
    march_matrices: [[[f32; 4]; 4]; 3],
    march_grid: [f32; 4],
    lamps: [LampUniform; MAX_LAMPS],
    lamp_matrix: [[f32; 4]; 4],
}

/// The car's lamps in the air ([`LightShafts::draw`], `fs_beams`): the frame's clip space to stage space, the eye, the
/// air's scattering per metre ([`air`]), the lamps at the share they light the stage, and the headlights' shadow map
/// (`Mat4::ZERO` with no map: beyond the lamps' reach, or the shadows off, the beams shine unshadowed).
pub(crate) struct Beams {
    pub(crate) inverse_view_projection: Mat4,
    pub(crate) eye: Vec3,
    pub(crate) air: f32,
    pub(crate) lamps: [LampUniform; MAX_LAMPS],
    pub(crate) lamp: Mat4,
}

/// The sun's march ([`LightShafts::draw`], `fs_march`): the unjittered main view's clip space to stage space, the eye,
/// the sunlight's travel and colour (pre-exposed linear), how strongly it shows, the height fog's uniform
/// ([`crate::height_fog::uniform`]), the base air's density per metre ([`air`]), the cascades (`None`: no shade) and
/// the frame's index for the jitter.
pub(crate) struct Sun {
    pub(crate) inverse_view_projection: Mat4,
    pub(crate) eye: Vec3,
    pub(crate) direction: Vec3,
    pub(crate) colour: Vec3,
    pub(crate) strength: f32,
    pub(crate) height_fog: [f32; 4],
    pub(crate) base_air: f32,
    pub(crate) matrices: [Mat4; 3],
    pub(crate) splits: [f32; 3],
    pub(crate) shade: f32,
    pub(crate) frame: u32,
}

/// The air's scattering per metre (ours): under `UseFog` one over the fog's range, [`RAIN_AIR`] of that in a rain
/// section, else [`CLEAR_AIR`] of it. The march's density floor, and the lamps' beams' air.
pub(crate) fn air(atmosphere: &Atmosphere) -> f32 {
    let fog = 1.0 / (atmosphere.fog_end - atmosphere.fog_start).max(10.0);
    let rain = atmosphere.section.as_deref().is_some_and(|s| s.contains("rain") && !s.contains("norain"));
    match (atmosphere.use_fog, rain) {
        (false, _) => CLEAR_AIR * fog,
        (true, true) => RAIN_AIR * fog,
        (true, false) => fog,
    }
}

/// How strongly the sun marches (0 to 1): by day under a sky that is not overcast ([`Atmosphere::overcast`]), fading in
/// over [`RISEN`] past sunrise. `None` at night, under an overcast sky, or with the sun still down: the lamps' beams
/// draw instead, or the map is cleared.
pub(crate) fn sun_strength(atmosphere: &Atmosphere) -> Option<f32> {
    if atmosphere.night || atmosphere.overcast() {
        return None;
    }
    let elevation = atmosphere.sun_elevation.unwrap_or_else(|| (-atmosphere.sun_direction.y).asin().to_degrees());
    let strength = ((elevation - RISEN[0]) / (RISEN[1] - RISEN[0])).clamp(0.0, 1.0);
    (strength > 0.0).then_some(strength)
}

/// The dust grid's corner (its low x, y and z in stage space) for the `eye`: the grid spans [`DUST_CELLS`] cells of
/// [`DUST_CELL`] about it, snapped to the cell so it does not swim as the eye moves.
pub(crate) fn dust_origin(eye: Vec3) -> Vec3 {
    let half = Vec3::new(DUST_CELLS[0] as f32, DUST_CELLS[1] as f32, DUST_CELLS[2] as f32) * 0.5 * DUST_CELL;
    ((eye - half) / DUST_CELL).floor() * DUST_CELL
}

pub(crate) struct LightShafts {
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sun: wgpu::TextureView,
    sun_sampler: wgpu::Sampler,
    dust: Dust,
    dusty: bool,
    /// The march, the blur and the beams pass.
    pipelines: [wgpu::RenderPipeline; 3],
    targets: Targets,
}

/// The wheel spray's dust about the eye: [`DUST_CELLS`] cells of [`DUST_CELL`] metres, `R32Float`, splatted from
/// the particle billboards each frame ([`LightShafts::set_dust`]).
struct Dust {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    origin: Vec3,
}

fn dust_texture(device: &wgpu::Device) -> Dust {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("light shafts dust"),
        size: wgpu::Extent3d {
            width: DUST_CELLS[0] as u32,
            height: DUST_CELLS[1] as u32,
            depth_or_array_layers: DUST_CELLS[2] as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Dust { texture, view, origin: Vec3::ZERO }
}

/// What depends on the frame's size: the march's map, a quarter of the frame's size, and the passes' bind groups.
struct Targets {
    march: wgpu::TextureView,
    groups: [wgpu::BindGroup; 3],
}

impl LightShafts {
    /// The passes over the frame's `depth` (`size` pixels) and the sun's map `sun` (the cascades' layers for the march,
    /// the headlights' layer for the beams; its comparison sampler is made here).
    pub(crate) fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        depth: &wgpu::TextureView,
        sun: &wgpu::TextureView,
    ) -> LightShafts {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light shafts"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sun_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("light shafts sun"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("light shafts"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        // One layout for all three passes (the march reads the depth and the sun, the blur its map, the beams the
        // depth; wgpu derives a pruned layout per entry point from `None`, so each group is built for its own).
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light shafts"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("light shafts"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = ["fs_march", "fs_blur", "fs_beams"].map(|entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
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
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: SCENE_FORMAT,
                        blend: None,
                        // The march and the beams keep their depth in alpha (`shafts_at` in `post.wgsl`); the blur
                        // keeps its map's, averaged with the light.
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let dust = dust_texture(device);
        let targets = targets(
            device,
            &Maps { uniform: &uniform, layout: &layout, sun_sampler: &sun_sampler, dust: &dust.view, depth, sun },
            size,
        );
        LightShafts { uniform, layout, sun: sun.clone(), sun_sampler, dust, dusty: false, pipelines, targets }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, size: (u32, u32), depth: &wgpu::TextureView) {
        self.targets = targets(
            device,
            &Maps {
                uniform: &self.uniform,
                layout: &self.layout,
                sun_sampler: &self.sun_sampler,
                dust: &self.dust.view,
                depth,
                sun: &self.sun,
            },
            size,
        );
    }

    /// Splat the particle `billboards`' dust into the grid about the `eye` for the march to read: each billboard
    /// leaves its alpha's share in the cells its puff covers, so sunlit dust behind trees shows shafts. Only with
    /// [`crate::renderer::Enhance::sky`] and `particles` (the caller gates that); without billboards the grid stays
    /// as it was, cleared on the first empty frame.
    pub(crate) fn set_dust(&mut self, queue: &wgpu::Queue, billboards: &[crate::particles::sim::Billboard], eye: Vec3) {
        let origin = dust_origin(eye);
        self.dust.origin = origin;
        if billboards.is_empty() {
            if self.dusty {
                queue.write_texture(
                    self.dust.texture.as_image_copy(),
                    bytemuck::cast_slice(&vec![0.0f32; DUST_CELLS.iter().product()]),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some((DUST_CELLS[0] * 4) as u32),
                        rows_per_image: Some(DUST_CELLS[1] as u32),
                    },
                    wgpu::Extent3d {
                        width: DUST_CELLS[0] as u32,
                        height: DUST_CELLS[1] as u32,
                        depth_or_array_layers: DUST_CELLS[2] as u32,
                    },
                );
                self.dusty = false;
            }
            return;
        }
        let mut grid = vec![0.0f32; DUST_CELLS.iter().product()];
        let at = |c: usize, r: usize, s: usize| s * DUST_CELLS[0] * DUST_CELLS[1] + r * DUST_CELLS[0] + c;
        for b in billboards {
            let alpha = b.color[3];
            if alpha <= 0.0 {
                continue;
            }
            let centre = Vec3::from(b.centre);
            let radius = Vec3::from(b.axis_u).length().max(Vec3::from(b.axis_v).length());
            if radius < DUST_MIN_RADIUS {
                continue;
            }
            let deposit = alpha * DUST_PER_ALPHA;
            let low = ((centre - radius - origin) / DUST_CELL).floor().as_ivec3();
            let high = ((centre + radius - origin) / DUST_CELL).ceil().as_ivec3();
            for z in low.z.max(0)..=high.z.min(DUST_CELLS[2] as i32 - 1) {
                for y in low.y.max(0)..=high.y.min(DUST_CELLS[1] as i32 - 1) {
                    for x in low.x.max(0)..=high.x.min(DUST_CELLS[0] as i32 - 1) {
                        let cell = origin + (Vec3::new(x as f32, y as f32, z as f32) + 0.5) * DUST_CELL;
                        let d = (cell - centre).length() / radius;
                        if d < 1.0 {
                            let slot = &mut grid[at(x as usize, y as usize, z as usize)];
                            *slot = (*slot + deposit * (1.0 - d)).min(4.0);
                        }
                    }
                }
            }
        }
        queue.write_texture(
            self.dust.texture.as_image_copy(),
            bytemuck::cast_slice(&grid),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((DUST_CELLS[0] * 4) as u32),
                rows_per_image: Some(DUST_CELLS[1] as u32),
            },
            wgpu::Extent3d {
                width: DUST_CELLS[0] as u32,
                height: DUST_CELLS[1] as u32,
                depth_or_array_layers: DUST_CELLS[2] as u32,
            },
        );
        self.dusty = true;
    }

    /// Draws into `shafts` ([`crate::post::Post::shafts`]) the lamps' `beams`, else the `sun`'s march (`None`: none,
    /// the map is cleared) with the sun's colour times its strength and [`MARCH_GAIN`].
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        shafts: &wgpu::TextureView,
        sun: Option<&Sun>,
        beams: Option<&Beams>,
    ) {
        if let Some(beams) = beams {
            let uniform = Uniform {
                sun: [0.0; 4],
                inverse_view_projection: beams.inverse_view_projection.to_cols_array_2d(),
                eye: beams.eye.extend(beams.air).to_array(),
                lamps: beams.lamps,
                lamp_matrix: beams.lamp.to_cols_array_2d(),
                ..Zeroable::zeroed()
            };
            queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
            let mut pass = cleared(encoder, shafts);
            pass.set_pipeline(&self.pipelines[2]);
            pass.set_bind_group(0, &self.targets.groups[2], &[]);
            pass.draw(0..3, 0..1);
            return;
        }
        let Some(sun) = sun else {
            cleared(encoder, shafts);
            return;
        };
        let gain = MARCH_GAIN * sun.strength;
        let colour = (sun.colour * gain).extend(sun.frame as f32).to_array();
        let uniform = Uniform {
            inverse_view_projection: sun.inverse_view_projection.to_cols_array_2d(),
            eye: sun.eye.extend(0.0).to_array(),
            march_dir: sun.direction.extend(0.0).to_array(),
            march_colour: colour,
            march_fog: [sun.height_fog[0], sun.height_fog[1], sun.height_fog[2], sun.base_air],
            march_splits: [sun.splits[0], sun.splits[1], sun.splits[2], sun.shade],
            march_matrices: sun.matrices.map(|m| m.to_cols_array_2d()),
            march_grid: self.dust.origin.extend(DUST_CELL).to_array(),
            ..Zeroable::zeroed()
        };
        queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
        for ((pipeline, group), target) in
            self.pipelines.iter().zip(&self.targets.groups).zip([&self.targets.march, shafts])
        {
            let mut pass = cleared(encoder, target);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// A pass that clears `target` to black and draws into it.
fn cleared<'a>(encoder: &'a mut wgpu::CommandEncoder, target: &wgpu::TextureView) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("light shafts"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    })
}

/// What `targets` reads: the passes' inputs.
struct Maps<'a> {
    uniform: &'a wgpu::Buffer,
    layout: &'a wgpu::BindGroupLayout,
    sun_sampler: &'a wgpu::Sampler,
    dust: &'a wgpu::TextureView,
    depth: &'a wgpu::TextureView,
    sun: &'a wgpu::TextureView,
}

fn targets(device: &wgpu::Device, maps: &Maps<'_>, (width, height): (u32, u32)) -> Targets {
    let (uniform, layout, sun_sampler, dust, depth, sun) =
        (maps.uniform, maps.layout, maps.sun_sampler, maps.dust, maps.depth, maps.sun);
    let march = {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("light shafts"),
                size: wgpu::Extent3d {
                    width: width.div_ceil(4).max(1),
                    height: height.div_ceil(4).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let group = |read: &wgpu::TextureView| {
        // One layout for all three passes; each reads its own map.
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(depth) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(read) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(sun) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(sun_sampler) },
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(dust) },
        ];
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("light shafts"), layout, entries: &entries })
    };
    // The march writes its map while bound, so its unused map binding takes a dummy (the blur and the beams read
    // the march's; a pass may not read the target it draws into).
    let dummy = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("light shafts (unused)"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let groups = [group(&dummy), group(&march), group(&march)];
    Targets { march, groups }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_marches_by_day() {
        let day = Atmosphere { sun_elevation: Some(10.0), ..Default::default() };
        assert_eq!(sun_strength(&day), Some(1.0));
        // Low over the horizon it fades in; at night or under an overcast sky, none.
        let low = Atmosphere { sun_elevation: Some(2.0), ..day.clone() };
        assert!((sun_strength(&low).unwrap() - 0.5).abs() < 1e-4);
        assert!(sun_strength(&Atmosphere { night: true, ..day.clone() }).is_none());
        let overcast = Atmosphere { section: Some("45O_N_lightrain_heavycloud".into()), ..day };
        assert!(sun_strength(&overcast).is_none());
    }

    #[test]
    fn rain_scatters_the_lamps_less_than_fog_of_its_range() {
        let section = |name: &str| Atmosphere {
            section: Some(name.into()),
            use_fog: true,
            fog_start: -100.0,
            fog_end: 300.0,
            ..Default::default()
        };
        assert_eq!(air(&section("45O_N_lightfog_heavycloud")), 1.0 / 400.0);
        assert_eq!(air(&section("45O_N_heavyrain_heavycloud")), RAIN_AIR / 400.0);
        assert_eq!(air(&section("45O_N_norain_lightcloud")), 1.0 / 400.0);
        assert_eq!(air(&Atmosphere { use_fog: false, ..section("45O_N_heavyrain_heavycloud") }), CLEAR_AIR / 400.0);
    }
}
