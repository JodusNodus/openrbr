//! Volumetric sunlight in the air (`light_shafts.wgsl`; ours, a renderer addition under
//! [`crate::renderer::Enhance::sky`] with the HDR pass): a ray march per texel of the quarter-size map, from the eye
//! along the view ray to the scene's depth (`MARCH_REACH` metres at most, the whole way where only sky is drawn),
//! sampling the sun's shadow cascades and the volumetric clouds' shadow map (`air_sun_seen` in `light_shafts.wgsl`).
//! The scene's haze and mist already hold the sun's light scattered by sunlit air, so the march takes away the share of
//! the scene's light the air in shadow leaves out: the map is negative, and the post pass's composite adds it to the
//! scene ([`crate::post::Post::shafts`]), so the sun shines through the canopy and the cloud gaps in shafts between
//! darker shaded air. At night the map holds instead the car's lamps' light scattered by the air ([`Beams`]), the
//! headlights' beams in the fog.

use crate::atmosphere::Atmosphere;
use crate::post::SCENE_FORMAT;
use crate::renderer::{LampUniform, MAX_LAMPS};
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{Mat4, Vec3};

pub(crate) const SHADER: &str =
    concat!(include_str!("light_shafts.wgsl"), include_str!("lamps.wgsl"), include_str!("volume_shadow.wgsl"));
/// How much the haze the mist is lit by brightens toward the sun (`haze_colour` in `shader.wgsl`: +0.39 over the
/// horizon's light straight toward it, a share of 0.28), as a multiple of the mist's own sun term (`MIST_SUN` in
/// `sky_lut.wgsl`): with it the sun's share of the fog's light straight toward the sun is 0.36, as the scene's.
const MARCH_LOBE: f32 = 3.7;
/// The sunlit air's glow about the sun, added (ours): the sun's colour times this per metre of optical depth, in a
/// narrow forward lobe (`GLOW_FORWARD` in `light_shafts.wgsl`), through air at least [`GLOW_AIR`] thick per metre, so
/// that the sun shines through the gaps in the canopy in rays while open air away from the sun is left as it is.
const MARCH_GLOW: f32 = 0.06;
const GLOW_AIR: f32 = 1.0 / 3000.0;
/// The share of the sky's light the mist scatters (`MIST_SKY` in `sky_lut.wgsl`): under a clear sky, and as the
/// whole fog.
const MIST_SKY: [f32; 2] = [0.55, 1.0];
/// The sun's true elevation, degrees, over which the march fades in after sunrise.
const RISEN: [f32; 2] = [0.0, 4.0];

/// The air's scattering for the lamps' beams outside `UseFog`, as a share of the fog's for the section's range (ours: a
/// faint haze).
const CLEAR_AIR: f32 = 0.2;
/// The same under `UseFog` in a rain section, whose fog stands for the rain's spray and mist (ours: drops are
/// millimetres and scatter far less than fog's, heavy rain still seeing about 2 km).
const RAIN_AIR: f32 = 0.1;

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    march_dir: [f32; 4],
    march_light: [f32; 4],
    march_glow: [f32; 4],
    march_fog: [f32; 4],
    march_splits: [f32; 4],
    march_matrices: [[[f32; 4]; 4]; 3],
    march_clouds: [f32; 4],
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
/// ([`crate::height_fog::uniform`]) and whether it is the whole fog ([`crate::height_fog::is_volume`]), the base air's
/// density per metre ([`air`]), the cascades (`shade` 0: none), the volumetric clouds' shadow map's parameters
/// (`cloud_volume::shadow_params`; zero without them) and the frame's index for the jitter.
pub(crate) struct Sun {
    pub(crate) inverse_view_projection: Mat4,
    pub(crate) eye: Vec3,
    pub(crate) direction: Vec3,
    pub(crate) colour: Vec3,
    pub(crate) strength: f32,
    pub(crate) height_fog: [f32; 4],
    pub(crate) fog_volume: bool,
    pub(crate) base_air: f32,
    pub(crate) matrices: [Mat4; 3],
    pub(crate) splits: [f32; 3],
    pub(crate) shade: f32,
    pub(crate) clouds: [f32; 4],
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

pub(crate) struct LightShafts {
    uniform: wgpu::Buffer,
    inputs: Inputs,
    /// The march, the blur and the beams pass.
    pipelines: [wgpu::RenderPipeline; 3],
    targets: Targets,
}

/// What the passes read besides the frame and its depth: their layout, the sun's map and the clouds' shadow map, and
/// their samplers.
struct Inputs {
    layout: wgpu::BindGroupLayout,
    sun_map: wgpu::TextureView,
    cloud_map: wgpu::TextureView,
    sun_sampler: wgpu::Sampler,
    /// The frame's and the clouds' shadow map's (linear).
    sampler: wgpu::Sampler,
}

/// What depends on the frame's size: the march's map, a quarter of the frame's size, and the passes' bind groups.
struct Targets {
    march: wgpu::TextureView,
    groups: [wgpu::BindGroup; 3],
}

impl LightShafts {
    /// The passes over `scene` (the post pass's, [`SCENE_FORMAT`], `size` pixels) and the frame's `depth`, the sun's
    /// map `sun_map` (the cascades' layers for the march, the headlights' layer for the beams) and the volumetric
    /// clouds' shadow map `cloud_map` (`CloudVolume::shadow_map`); their samplers are made here.
    pub(crate) fn new(
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: (u32, u32),
        depth: &wgpu::TextureView,
        sun_map: &wgpu::TextureView,
        cloud_map: &wgpu::TextureView,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("light shafts"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("light shafts"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let texture = |binding, sample_type, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension, multisampled: false },
            count: None,
        };
        let sampler_entry = |binding, kind| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(kind),
            count: None,
        };
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        // One layout for all three passes (the march reads the frame, its depth, the sun's map and the clouds', the
        // blur its map, the beams the depth and the sun's map).
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
                texture(2, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2),
                texture(3, wgpu::TextureSampleType::Depth, wgpu::TextureViewDimension::D2),
                texture(4, unfiltered, wgpu::TextureViewDimension::D2),
                texture(5, wgpu::TextureSampleType::Depth, wgpu::TextureViewDimension::D2Array),
                sampler_entry(6, wgpu::SamplerBindingType::Comparison),
                texture(7, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2),
                sampler_entry(8, wgpu::SamplerBindingType::Filtering),
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
        let inputs = Inputs { layout, sun_map: sun_map.clone(), cloud_map: cloud_map.clone(), sun_sampler, sampler };
        let targets = targets(device, &uniform, &inputs, scene, size, depth);
        LightShafts { uniform, inputs, pipelines, targets }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) {
        self.targets = targets(device, &self.uniform, &self.inputs, scene, size, depth);
    }

    /// Draws into `shafts` ([`crate::post::Post::shafts`]) the lamps' `beams`, else the `sun`'s march (`None`: none,
    /// the map is cleared): the scene's light the shaded air leaves out, negative, and the sunlit air's glow about the
    /// sun.
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
        let mist_sky = MIST_SKY[usize::from(sun.fog_volume)];
        let uniform = Uniform {
            inverse_view_projection: sun.inverse_view_projection.to_cols_array_2d(),
            eye: sun.eye.extend(0.0).to_array(),
            march_dir: sun.direction.extend(0.0).to_array(),
            march_light: [sun.strength, mist_sky, MARCH_LOBE, sun.frame as f32],
            march_glow: (sun.colour * MARCH_GLOW * sun.strength).extend(GLOW_AIR).to_array(),
            march_fog: [sun.height_fog[0], sun.height_fog[1], sun.height_fog[2], sun.base_air],
            march_splits: [sun.splits[0], sun.splits[1], sun.splits[2], sun.shade],
            march_matrices: sun.matrices.map(|m| m.to_cols_array_2d()),
            march_clouds: sun.clouds,
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

fn targets(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    inputs: &Inputs,
    scene: &wgpu::TextureView,
    (width, height): (u32, u32),
    depth: &wgpu::TextureView,
) -> Targets {
    let map = |label, (width, height), usage| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let march = map(
        "light shafts",
        (width.div_ceil(4).max(1), height.div_ceil(4).max(1)),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    // The march writes its map while bound, so its unused map binding takes a dummy (the blur and the beams read
    // the march's; a pass may not read the target it draws into).
    let dummy = map("light shafts (unused)", (1, 1), wgpu::TextureUsages::TEXTURE_BINDING);
    let group = |read: &wgpu::TextureView| {
        let view = wgpu::BindingResource::TextureView;
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: view(scene) },
            wgpu::BindGroupEntry { binding: 3, resource: view(depth) },
            wgpu::BindGroupEntry { binding: 4, resource: view(read) },
            wgpu::BindGroupEntry { binding: 5, resource: view(&inputs.sun_map) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&inputs.sun_sampler) },
            wgpu::BindGroupEntry { binding: 7, resource: view(&inputs.cloud_map) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&inputs.sampler) },
        ];
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light shafts"),
            layout: &inputs.layout,
            entries: &entries,
        })
    };
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
