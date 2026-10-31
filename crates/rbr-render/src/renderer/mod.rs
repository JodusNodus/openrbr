//! wgpu drawing of a loaded [`Stage`]. Each draw's render state ([`DrawState`](crate::stage::DrawState)) maps to a
//! pipeline (culling, blending, depth) and a bind group (its textures, address modes and shader mode). Meshes are
//! uploaded once, and meshes outside the view frustum are skipped.  Textures are BC1 (`DXT1`), BC2 (`DXT3`) or BC3
//! (`DXT5`, community car models) as stored, so the device needs `TEXTURE_COMPRESSION_BC`. Colours are not converted to
//! sRGB, as the D3D9 fixed output did not.

use crate::atmosphere::Atmosphere;
use crate::dials;
use crate::effects::Effects;
use crate::foliage;
use crate::glass;
use crate::lights::Lamp;
use crate::parallel::in_parallel;
use crate::particles;
use crate::post::{Post, SCENE_FORMAT};
use crate::profile::Profiler;
use crate::puddles;
use crate::skylight::{self, SkyLight};
use crate::stage::{Anchor, Draw, Image, Layer, Lod, Mesh, Stage, TextureRef, Weather};
use crate::start_board::{self, StartBoard, StartLamp};
use crate::sun::{self, Cascades, SunShadow};
use crate::terrain_detail;
use glam::{Mat4, Vec3, Vec4};
use rbr_formats::dds::{self, Format};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

mod bounce;
mod cloud_volume;
mod enhance;
mod frame;
mod gpu;
mod meshes;
mod mirrors;
mod occlusion;
mod offscreen;
mod pipelines;
mod probes;
mod reflection;
mod road_scan;
mod shadows;
mod sun_mask;
mod textures;
mod uniforms;

pub use enhance::{AntiAliasing, Detail, Enhance, Reflections};
pub use frame::FrameStats;
pub use gpu::{instance, request_gpu, Gpu, GpuError};
pub(crate) use meshes::mesh_bounds;
use meshes::{transformed_vertices, GpuDraw, GpuMesh, InteractiveGpu, MeshBuffers, Packing};
pub use offscreen::ReadbackError;
use pipelines::{cutout, layouts, prepassed, reflect_entry, vertex_entry, Context, DrawPass, PipelineKey};
pub(crate) use pipelines::{texture_entry, vertex_layout, SHADER, TEXTURE};
#[cfg(test)]
pub(crate) use reflection::BLIT;
use reflection::{reflects, Reflection, ScreenReflection};
use shadows::shadow_map;
#[cfg(test)]
pub(crate) use shadows::RANGE_SHADER;
use sun_mask::SunMask;
pub(crate) use textures::dds_texture;
use textures::{
    bind_group, bind_key, depth_view, luminance_texture, solid_texture, Glow, Samplers, Views, MISSING, WHITE,
};
use uniforms::{frame_index, FrameBinding, FrameUniform};
pub(crate) use uniforms::{LampUniform, MAX_LAMPS};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLEAR_COLOR: wgpu::Color = wgpu::Color { r: 0.42, g: 0.58, b: 0.78, a: 1.0 };
/// The binding of the frame's depth.
pub(crate) fn depth_binding() -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Depth,
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    }
}

/// Draws one stage into a colour target of `format`.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    enhance: Enhance,
    frames: [FrameBinding; 3],
    meshes: Vec<GpuMesh>,
    /// The meshes with a draw that casts the car's shadow, in order ([`meshes::casters`]).
    casters: Vec<usize>,
    /// The meshes' shared vertex and index buffers ([`Packing`]).
    buffers: Vec<MeshBuffers>,
    /// The GPU mesh of each stage mesh, `None` for a mesh with no draws (not uploaded).
    slots: Vec<Option<usize>>,
    /// Physics handle to the GPU mesh of each supported interactive visibility record.
    interactive_slots: HashMap<u32, usize>,
    /// The grass's tile meshes, filled about the eye ([`Renderer::set_grass`]).
    grass: Option<crate::grass::Ring>,
    atmosphere: Atmosphere,
    /// The ground's light seen from above ([`crate::bounce`], with [`Enhance::lighting`]); the stage's light off itself
    /// is drawn into its vertices at load.
    ground_light: Option<bounce::Ground>,
    /// How wet the stage's weather is ([`FrameUniform::wetness`]).
    wetness: f32,
    /// The stage's ground is snow's ([`Stage::snow_ground`]).
    snow_ground: bool,
    /// The car's dirt ([`Renderer::set_car_dirt`]).
    car_dirt: crate::dirt::CarDirt,
    /// How much sun gets into the cabin ([`Renderer::set_cabin_sun`]) and the car body's model space to stage space
    /// ([`Renderer::set_car_body`]).
    cabin_sun: Option<crate::car::cabin::SunIn>,
    car_body: Option<Mat4>,
    /// The camera is outside the car ([`Renderer::set_outside_view`]).
    outside_view: bool,
    /// The sky gradient, drawn first ([`Atmosphere::zenith`] overhead, the fog colour at the horizon).
    sky: wgpu::RenderPipeline,
    /// The size the frame is drawn at, and the size of the target the post pass writes
    /// ([`Renderer::set_render_scale`]).
    depth_size: (u32, u32),
    output_size: (u32, u32),
    /// The share of the target's width and height the frame is drawn at (1: all of it).
    render_scale: f32,
    depth_view: wgpu::TextureView,
    /// The sun's shadow over the main view, from its depth (with [`Enhance::lighting`]).
    sun_mask: SunMask,
    /// The volumetric clouds over the main view's sky (with [`Enhance::lighting`]).
    cloud_volume: cloud_volume::CloudVolume,
    /// The shadow map's parameters ([`cloud_volume::shadow_params`]) of the main view's frame being drawn: the map is
    /// drawn for the main eye, so the mirrors and the cubes read these rather than their own eye's. The frames before
    /// the main view's read the last frame's.
    shadow_params: std::cell::Cell<[f32; 4]>,
    /// The car's sun and contact shadows ([`SHADOW_SIZE`](shadows::SHADOW_SIZE), [`GROUND_REACH`](shadows::GROUND_REACH)): the map's two
    /// layers, the pipelines that draw into them, and the frame binding those read (the world-anchored frame's buffer
    /// without the map).
    shadow_layers: [wgpu::TextureView; 2],
    shadow_pipelines: [wgpu::RenderPipeline; 2],
    reflection: Reflection,
    /// The stage around the car to the old reach, for its light and the wet roads' and the water's reflections.
    surroundings: Reflection,
    /// The surroundings baked along the driveline in its place ([`Enhance::lighting`]).
    probes: probes::Probes,
    /// The physically based sky's tables ([`Enhance::lighting`]; bound in every frame, drawn only with it).
    skylight: SkyLight,
    /// The low ground the height fog settles on (`None` without [`Enhance::lighting`]).
    fog_ground: Option<crate::height_fog::Ground>,
    shadow_frame: wgpu::BindGroup,
    /// The car's shadow map's depth range per block, redrawn with it.
    shadow_range: shadows::ShadowRange,
    /// The sun's first cascade's depth range per block, redrawn with it (`None` without the cascades).
    sun_range: Option<shadows::ShadowRange>,
    /// `None` when no draw reflects ([`reflects`]).
    screen_reflection: Option<ScreenReflection>,
    post: Post,
    /// `None` unless [`AntiAliasing::Taa`] ([`crate::taa`]).
    taa: Option<crate::taa::Taa>,
    /// `None` unless [`Enhance::lighting`] ([`crate::light_shafts`]).
    light_shafts: Option<crate::light_shafts::LightShafts>,
    /// `None` unless [`Enhance::particles`].
    effects: Option<Effects>,
    /// `None` unless [`Enhance::lighting`].
    sun: Option<SunShadow>,
    /// The sun's cascades for the frame being drawn, `None` when there are none (no sun, or an overcast sky).
    cascades: Option<Cascades>,
    /// The low and high corners of the stage and its backdrop; the world layer's matrix and texel for the frame
    /// ([`sun::world`], `None` without the cascades or under an overcast sky), and the matrix it was last drawn with.
    world_box: Option<(Vec3, Vec3)>,
    world: Option<(Mat4, f32)>,
    world_drawn: Option<Mat4>,
    /// What the cascades' far layer holds ([`sun::FarDrawn`]), drawn again only when it no longer serves the eye.
    far_drawn: Option<sun::FarDrawn>,
    /// The skid marks ([`crate::skidmarks_pass`]), made by the first [`Renderer::set_skid_marks`] or
    /// [`Renderer::set_tyre_marks`].
    skid_marks: Option<crate::skidmarks_pass::SkidPass>,
    /// The game's car shadow decal ([`crate::car_shadow`]), made by the first [`Renderer::set_car_shadow`].
    car_shadow: Option<crate::skidmarks_pass::SkidPass>,
    /// The particle billboards ([`Renderer::set_particles`]), drawn last in the main view.
    particles: Option<particles::gpu::Pass>,
    /// The windscreen's drops ([`Renderer::set_glass`]), drawn after the particles.
    glass: Option<glass::gpu::Pass>,
    /// Seconds since the start ([`Renderer::set_time`]).
    time: f32,
    /// The light shafts' marches drawn (the jitter's frame, [`crate::light_shafts::Sun`]).
    air_frame: u32,
    /// The car's lit lamps at night ([`Renderer::set_lamps`]).
    lamps: [LampUniform; MAX_LAMPS],
    /// The headlights' shadow map's clip matrix for the frame being drawn, `None` without one.
    lamp_matrix: Option<Mat4>,
    /// `None` unless [`Renderer::enable_profiling`].
    profiler: Option<Profiler>,
    /// The start board's clock face ([`Renderer::set_start_clock`]), when the stage shows one.
    start_board: Option<StartBoard>,
    /// The start board's lamps, green then red ([`Renderer::set_start_lamps`]).
    start_lamps: [Vec<StartLamp>; 2],
    /// The dials' textures ([`Image::Dial`]), the draws [`Renderer::draw_dials`] left for the next frame, and their pass
    /// (made by the first).
    dial_targets: BTreeMap<TextureRef, dials::gpu::Target>,
    dial_draws: Vec<dials::gpu::Draw>,
    dial_pass: Option<dials::gpu::Pass>,
    /// The cockpit's mirrors ([`Enhance::mirrors`], `crate::mirrors`).
    mirrors: mirrors::Mirrors,
}

impl Renderer {
    /// Uploads `stage` and builds the pipelines and bind groups its draws use, for colour targets of
    /// `format`, with the additions in `enhance`.
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, stage: &Stage, enhance: Enhance) -> Renderer {
        Renderer::with_progress(gpu, format, stage, enhance, &mut |_| {})
    }

    /// [`Renderer::new`], telling `progress` the share of the stage's meshes made so far, 0 to 1: they take most of
    /// the time (3.2 s of 3.3 s on Semetin, M3 Pro).
    pub fn with_progress(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        stage: &Stage,
        enhance: Enhance,
        progress: &mut dyn FnMut(f32),
    ) -> Renderer {
        let device = gpu.device.clone();
        let queue = gpu.queue.clone();
        let output_format = format;
        let format = SCENE_FORMAT;

        let [frame_layout, shadow_layout, texture_layout] = layouts(&device);
        let (shadow_view, shadow_layers, shadow_sampler) = shadow_map(&device);

        // A stage drawn at a time of day (`relight`, `Atmosphere::night`) needs the normals the baked draws do not
        // store, and at night the headlights' shadows use the sun's first cascade.
        let timed = stage.atmosphere.relight.is_some() || stage.atmosphere.night;
        let quality = sun::Quality::of(&enhance);
        let sun = (enhance.lighting || timed).then(|| SunShadow::new(&device, &texture_layout, quality));
        let sun_range = sun.as_ref().map(|sun| shadows::ShadowRange::sun(&device, &queue, &sun.view, quality));
        let sun_view = sun.as_ref().map_or_else(|| sun::placeholder(&device), |sun| sun.view.clone());
        let taa = enhance.anti_aliasing == AntiAliasing::Taa;
        let ultra = enhance.detail == Detail::Ultra;
        let surroundings_side = if ultra { reflection::ULTRA_SIZE } else { reflection::REFLECTION_SIZE };
        let reflection = Reflection::new(&device, format, reflection::REFLECTION_SIZE);
        let surroundings = Reflection::new(&device, format, surroundings_side);
        let probes = probes::Probes::new(&device, format, &stage.driveline, enhance.lighting, surroundings_side);
        let skylight = SkyLight::new(&device);
        let cube_view = |cube: &Reflection| {
            cube.cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        let (reflection_view, surroundings_view) = (cube_view(&reflection), cube_view(&surroundings));
        let reflection_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("car reflection"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let mirrors = mirrors::Mirrors::new(&device, format, enhance.mirrors, enhance.windscreen);
        let ground_light = bounce::Ground::new(&device, stage, enhance.lighting);
        let world_box = (stage.meshes.iter().filter(|m| m.anchor == Anchor::World).flat_map(|m| &m.vertices))
            .map(|v| Vec3::from(v.position))
            .fold(None, |b: Option<(Vec3, Vec3)>, p| Some(b.map_or((p, p), |(low, high)| (low.min(p), high.max(p)))));
        let shadow_range = shadows::ShadowRange::new(&device, &shadow_view);
        // The volumetric clouds' shadow map ([`cloud_volume::shadow_target`]), read by every draw.
        let (cloud_shadow_view, cloud_shadow_sampler) = cloud_volume::shadow_target(&device);
        let (scan_view, scan_sampler) =
            road_scan::load(&device, &queue, enhance.gravel || enhance.asphalt || enhance.snow || enhance.ground);
        let frames = [(); 3].map(|_| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &frame_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&reflection_view) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&reflection_sampler) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&sun_view) },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&skylight.sky_view),
                    },
                    wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&skylight.aerial) },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::TextureView(&skylight.cube_view),
                    },
                    wgpu::BindGroupEntry { binding: 10, resource: skylight.light.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::TextureView(&surroundings_view),
                    },
                    wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(&mirrors.view) },
                    wgpu::BindGroupEntry { binding: 13, resource: mirrors.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&ground_light.view),
                    },
                    wgpu::BindGroupEntry { binding: 15, resource: ground_light.cells.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 16, resource: wgpu::BindingResource::TextureView(&probes.view) },
                    wgpu::BindGroupEntry { binding: 17, resource: probes.buffer.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 18,
                        resource: wgpu::BindingResource::TextureView(&shadow_range.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 19,
                        resource: wgpu::BindingResource::TextureView(&cloud_shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 20,
                        resource: wgpu::BindingResource::Sampler(&cloud_shadow_sampler),
                    },
                    wgpu::BindGroupEntry { binding: 21, resource: wgpu::BindingResource::TextureView(&scan_view) },
                    wgpu::BindGroupEntry { binding: 22, resource: wgpu::BindingResource::Sampler(&scan_sampler) },
                ],
            });
            FrameBinding { buffer, bind_group }
        });
        let shadow_frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow frame"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frames[0].buffer.as_entire_binding() }],
        });

        let samplers = Samplers::new(&device, if enhance.anisotropy { 16 } else { 1 });
        let mut dial_targets = BTreeMap::new();
        let mut glows = BTreeMap::new();
        // `RERBR_NO_TERRAIN_DETAIL` leaves the terrain detail out (A/B timing; `terrain_detail`). Its windows over the
        // machine's threads: one after another they took 0.5 s of the load on 580.
        let detail = if std::env::var_os("RERBR_NO_TERRAIN_DETAIL").is_none() {
            let dds: Vec<_> = (stage.textures.iter())
                .filter_map(|(&key, texture)| match texture.image.as_ref()? {
                    Image::Dds(bytes) => Some((key, bytes)),
                    _ => None,
                })
                .collect();
            let windows =
                in_parallel(&dds, |&(key, bytes)| Some((key, terrain_detail::window(&dds::parse(bytes).ok()?)?)));
            windows.into_iter().flatten().collect()
        } else {
            BTreeMap::new()
        };
        let mut views = Views {
            loaded: stage
                .textures
                .iter()
                .filter_map(|(&key, texture)| {
                    let texture = match texture.image.as_ref()? {
                        Image::Dds(bytes) => dds_texture(&device, &queue, &dds::parse(bytes).ok()?),
                        Image::Solid(texel) => solid_texture(&device, &queue, *texel),
                        Image::Shadow { width, height, pixels } => {
                            luminance_texture(&device, &queue, *width, *height, pixels)
                        }
                        Image::Dial { base, glow } => {
                            let target =
                                dials::gpu::Target::new(&device, dds_texture(&device, &queue, &dds::parse(base).ok()?));
                            if let Some(mask) = glow.as_deref().and_then(|bytes| dds::parse(bytes).ok()) {
                                // DXT1 has no alpha to read (its texels are opaque).
                                let view = dds_texture(&device, &queue, &mask).create_view(&Default::default());
                                glows.insert(key, Glow { view, alpha: mask.format != dds::Format::Dxt1, day: false });
                            }
                            let view = target.view();
                            dial_targets.insert(key, target);
                            return Some((key, view));
                        }
                    };
                    Some((key, texture.create_view(&Default::default())))
                })
                .collect(),
            white: solid_texture(&device, &queue, WHITE).create_view(&Default::default()),
            missing: solid_texture(&device, &queue, MISSING).create_view(&Default::default()),
            glows,
            detail,
        };
        let start_board = stage.start_board().and_then(|texture| {
            let Some(Image::Dds(bytes)) = stage.textures.get(&texture)?.image.as_ref() else { return None };
            let placeholder = start_board::Rgba::decode(&dds::parse(bytes).ok()?);
            let digits =
                stage.start_digits.as_deref().and_then(|b| dds::parse(b).ok()).map(|d| start_board::Rgba::decode(&d));
            let board = StartBoard::new(&device, &queue, placeholder, digits);
            views.loaded.insert(texture, board.face.view());
            views.glows.insert(texture, Glow { view: board.face.glow_view(), alpha: false, day: false });
            Some(board)
        });
        let start_lamps = start_board::LAMPS.map(|name| {
            stage
                .textures_ending(name)
                .filter_map(|texture| {
                    let Some(Image::Dds(bytes)) = stage.textures.get(&texture)?.image.as_ref() else { return None };
                    let lamp = StartLamp::new(&device, &queue, start_board::Rgba::decode(&dds::parse(bytes).ok()?));
                    views.loaded.insert(texture, lamp.face.view());
                    views.glows.insert(texture, Glow { view: lamp.face.glow_view(), alpha: false, day: true });
                    Some(lamp)
                })
                .collect::<Vec<_>>()
        });

        let mut bind_groups = HashMap::new();
        let mut pipelines = HashMap::new();
        let mut meshes = Vec::new();
        let mut packing = Packing::default();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let mask_layout = SunMask::layout(&device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage"),
            bind_group_layouts: &[Some(&frame_layout), Some(&texture_layout), Some(&mask_layout)],
            immediate_size: 0,
        });
        let context = Context {
            device: &device,
            format,
            shader: &shader,
            layout: &layout,
            vertex: vertex_entry(enhance),
            water_plus: enhance.water || enhance.wet,
            cabin: enhance.cabin,
            gravel: enhance.gravel,
            asphalt: enhance.asphalt,
            snow: enhance.snow,
            ground: enhance.ground,
            grass: enhance.grass,
            lighting: enhance.lighting,
            cube_shift: u32::from(ultra),
            taa,
        };
        // A draw of a mesh whose indices start at `first_index` in the shared buffer, with its pipelines and bind group.
        let mut gpu_draw = |draw: &Draw,
                            first_index: u32,
                            casts_shadow: bool,
                            mirror: bool,
                            (anchor, layer): (Anchor, Layer),
                            cabin_set,
                            wet_road: bool| {
            let bind = bind_key(&draw.state, &views);
            let pipeline = PipelineKey::of(&draw.state, mirror, layer == Layer::Sky)
                .drawing(bind.features())
                .in_cabin(cabin_set)
                .framed(frame_index(anchor, layer));
            let viewed = pipeline.takes_views().then(|| {
                [false, true].map(|inside| {
                    let key = pipeline.viewed(inside);
                    pipelines.entry(key).or_insert_with(|| context.pipeline(key)).clone()
                })
            });
            let mut pipeline_in = |pass: DrawPass| {
                let key = pipeline.in_pass(pass);
                pipelines.entry(key).or_insert_with(|| context.pipeline(key)).clone()
            };
            // A lit draw is only drawn with its views' pipelines: its `main` is the outside one, not a third to compile.
            let main = viewed.as_ref().map_or_else(|| pipeline_in(DrawPass::Main), |[outside, _]| outside.clone());
            // The car's lit draws are shaded after the prepass by their outside view's pipeline.
            let prepassed = prepassed(&draw.state, context.lighting).then(|| {
                let lit = pipeline.takes_views();
                [DrawPass::DepthPrepass, DrawPass::AfterPrepass].map(|pass| {
                    let key = if lit && pass == DrawPass::AfterPrepass {
                        pipeline.viewed(false).in_pass(DrawPass::CarMain)
                    } else {
                        pipeline.in_pass(pass)
                    };
                    pipelines.entry(key).or_insert_with(|| context.pipeline(key)).clone()
                })
            });
            let lamps = pipeline.takes_lamps().then(|| {
                [DrawPass::Main, DrawPass::AfterPrepass].map(|pass| {
                    let key = pipeline.with_lamps().in_pass(pass);
                    pipelines.entry(key).or_insert_with(|| context.pipeline(key)).clone()
                })
            });
            let group = bind_groups
                .entry(bind)
                .or_insert_with(|| bind_group(&device, &texture_layout, bind, &views, &samplers))
                .clone();
            GpuDraw {
                pipeline,
                main,
                prepassed,
                lamps,
                views: viewed,
                group,
                casts_shadow,
                bind,
                wet_road,
                lod: draw.lod,
                indices: first_index + draw.indices.start as u32..first_index + draw.indices.end as u32,
            }
        };

        let mut slots = Vec::with_capacity(stage.meshes.len());
        let cut_out =
            if enhance.lighting { foliage::cut_out(stage, |draw| cutout(&draw.state)) } else { HashMap::new() };
        let plant = |draw: &Draw| cut_out.get(&draw.state.textures[0]?).filter(|_| cutout(&draw.state));
        let ground = stage.atmosphere.relight.map(|_| crate::relight::Ground::of(stage));
        // The puddles show in wet weather only (`wet_plus`).
        let heights = (enhance.wet && matches!(stage.selection.weather, Weather::Wet))
            .then(|| puddles::Heights::of(stage.meshes.iter().map(|mesh| (mesh, &mesh.vertices[..]))));
        // The interactive props go after the solid meshes and before the blended ones: the car's glass, which writes
        // no depth, is drawn over them as over the rest of the stage. Drawn after it, a sign behind the car showed
        // through its windows untinted, as if in the cabin.
        let first_blended = stage.meshes.iter().position(|mesh| mesh.layer == Layer::Blended);
        let (solid, blended) = stage.meshes.split_at(first_blended.unwrap_or(stage.meshes.len()));
        let mut interactive_slots = HashMap::with_capacity(stage.interactive.len());
        let over = crate::sky_dome::Over::of(&stage.meshes);
        // A stage mesh's vertices as drawn, when they are not its own: the sun's light needs the normals the baked stage
        // draws do not store, the sun's shadows the plants. The grass's tile meshes (`Lod::Close`) have none to light:
        // `Renderer::stream_grass` gives them their cards' vertices. Their zeroed ones, all in one place, made grouping
        // the plants into trees quadratic: 5 s of the load on 580.
        let lit = enhance.lighting || enhance.wet || timed;
        let prepare = |mesh: &Mesh| {
            let grass = mesh.draws.iter().all(|draw| draw.lod == Lod::Close);
            (lit && !grass).then(|| {
                let mut vertices = sun::with_normals(mesh);
                // The fences carry their own light (`fences::FENCE`).
                if mesh.anchor == Anchor::World && !crate::fences::is_fence(mesh) {
                    if let Some(ground) = &ground {
                        vertices = ground.on_foliage(vertices, mesh);
                    }
                    vertices = foliage::with_plants(vertices, mesh, plant);
                    if let Some(heights) = &heights {
                        vertices = puddles::with_lowness(vertices, mesh, heights);
                    }
                }
                vertices
            })
        };
        // Made over the load's threads 1024 meshes at a time, bounding the vertices held; the uploads stay in order.
        // One after another they took 1.2 s of the load on 580.
        let mut prepared = stage.meshes.chunks(1024).flat_map(|part| in_parallel(part, prepare));
        let meshes_and_props = solid.iter().map(Some).chain([None]).chain(blended.iter().map(Some));
        for (done, mesh) in meshes_and_props.enumerate() {
            progress(done as f32 / (stage.meshes.len() + 1) as f32);
            let Some(mesh) = mesh else {
                for instance in &stage.interactive {
                    let mesh = instance.mesh.as_ref();
                    if mesh.draws.is_empty() {
                        continue;
                    }
                    let placement = Mat4::from_cols_array(&instance.placement);
                    let vertices = transformed_vertices(&mesh.vertices, placement);
                    let bounds = mesh_bounds(&vertices);
                    let (slot, first_index) = packing.add(&device, &vertices, &mesh.indices, false);
                    let draws = mesh
                        .draws
                        .iter()
                        .map(|draw| {
                            gpu_draw(draw, first_index, false, false, (Anchor::World, Layer::Solid), None, false)
                        })
                        .collect();
                    interactive_slots.insert(instance.handle, meshes.len());
                    meshes.push(GpuMesh {
                        bounds: bounds.map(|b| (b.center, b.half_extent)),
                        cell: instance.cell.map(|b| (b.center, b.half_extent)),
                        visible: true,
                        outside: None,
                        reflected: false,
                        anchor: Anchor::World,
                        layer: Layer::Solid,
                        slot,
                        draws,
                        interactive: Some(InteractiveGpu {
                            placement: instance.placement,
                            local_mesh: Arc::clone(&instance.mesh),
                            current_pose: None,
                        }),
                        in_car: None,
                        car: false,
                        sky_dome: false,
                        backdrop_bounds: None,
                    });
                }
                continue;
            };
            let drawn = prepared.next().expect("one per stage mesh");
            // Under the physically based sky a sky dome is left out: the sky is ours (`crate::sky_dome`).
            let sky_dome = over.as_ref().is_some_and(|o| o.roofed_by(mesh));
            if mesh.draws.is_empty() || (sky_dome && enhance.lighting) {
                slots.push(None);
                continue;
            }
            slots.push(Some(meshes.len()));
            let backdrop = mesh.anchor == Anchor::World && mesh.layer == Layer::Sky;
            let drawn = drawn.as_deref().unwrap_or(&mesh.vertices);
            let (slot, first_index) = packing.add(&device, drawn, &mesh.indices, backdrop);
            // A mirror's glass (`mirrors::mark`): a lit draw with a marked corner.
            let mirror = |draw: &Draw| {
                enhance.mirrors != crate::mirrors::Rate::Off
                    && draw.state.shade.is_some()
                    && mesh.indices[draw.indices.clone()].iter().any(|&i| mesh.vertices[i as usize].specular[0] >= 1.0)
            };
            // The ground whose surface is road (`puddles::mark_roads`), with the wet film.
            let wet_road = |draw: &Draw| {
                enhance.wet
                    && draw.state.shade.is_none()
                    && draw.state.mip_bias
                    && mesh.indices[draw.indices.clone()].iter().any(|&i| mesh.vertices[i as usize].wet > 0.0)
            };
            // Whether all of a draw's vertices are the cabin's interior set's (`car::cabin::INTERIOR` and up), or none
            // are: `None` when some are.
            let cabin_set = |draw: &Draw| {
                let indices = &mesh.indices[draw.indices.clone()];
                let interior = |i: &u32| mesh.vertices[*i as usize].specular[1] >= crate::car::cabin::INTERIOR;
                match (indices.iter().all(interior), indices.iter().any(interior)) {
                    (true, _) => Some(true),
                    (_, false) => Some(false),
                    _ => None,
                }
            };
            let draws = mesh
                .draws
                .iter()
                .map(|draw| {
                    let casts_shadow = draw.state.shade.is_some() && draw.state.depth_write;
                    let placed = (mesh.anchor, mesh.layer);
                    gpu_draw(draw, first_index, casts_shadow, mirror(draw), placed, cabin_set(draw), wet_road(draw))
                })
                .collect();
            meshes.push(GpuMesh {
                bounds: mesh.bounds.map(|b| (b.center, b.half_extent)),
                cell: mesh.cell.map(|b| (b.center, b.half_extent)),
                visible: true,
                outside: None,
                reflected: false,
                anchor: mesh.anchor,
                layer: mesh.layer,
                slot,
                draws,
                interactive: None,
                in_car: None,
                car: false,
                sky_dome,
                backdrop_bounds: mesh_bounds(&mesh.vertices)
                    .filter(|_| backdrop && mesh.bounds.is_none())
                    .map(|b| (b.center, b.half_extent)),
            });
        }

        let buffers = packing.finish(&device);
        let ground_light = enhance.lighting.then(|| {
            let start = std::time::Instant::now();
            let tiles = occlusion::bake(&device, &queue, &texture_layout, &meshes, &buffers);
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            eprintln!("sky occlusion: {tiles} tiles in {:.1} s", start.elapsed().as_secs_f32());
            let toward = -stage.atmosphere.sun_direction;
            bounce::bake(&device, &queue, &texture_layout, &meshes, &buffers, &ground_light, toward);
            ground_light
        });
        let depth_size = (1, 1);
        let depth_view = depth_view(&device, depth_size);
        let cloud_volume = cloud_volume::CloudVolume::new(
            &context,
            &queue,
            &frame_layout,
            (&depth_view, depth_size),
            enhance.lighting,
            &frames[0].buffer,
            cloud_shadow_view,
        );
        let sky = context.sky_pipeline(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[Some(&frame_layout), Some(cloud_volume.sky_layout())],
            immediate_size: 0,
        }));
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("car shadow"),
            bind_group_layouts: &[Some(&shadow_layout)],
            immediate_size: 0,
        });
        let shadow_pipelines =
            ["vs_shadow", "vs_ground"].map(|entry| context.shadow_pipeline(&shadow_pipeline_layout, entry));
        let sun_mask = SunMask::new(
            &context,
            (mask_layout, &frame_layout),
            &depth_view,
            sun_range.as_ref().map(|range| &range.view),
            depth_size,
            quality,
        );
        let draws = || meshes.iter().flat_map(|mesh| &mesh.draws);
        let wetness = match stage.selection.weather {
            Weather::Dry => 0.0,
            Weather::Damp => 0.5,
            Weather::Wet => 1.0,
        };
        let rays = enhance.reflections.rays().filter(|_| draws().any(|draw| reflects(draw, wetness)));
        let screen_reflection = rays.map(|rays| {
            ScreenReflection::new(
                &device,
                (&shader, vertex_entry(enhance), reflect_entry(enhance), rays, pipelines::alpha_writes(taa, false)),
                [&frame_layout, &texture_layout],
                format,
                depth_size,
                &depth_view,
                ultra,
            )
        });
        let post = Post::new(&device, output_format, depth_size, &depth_view, enhance.lighting, taa);
        let light_shafts = enhance.lighting.then(|| {
            let clouds = cloud_volume.shadow_map();
            crate::light_shafts::LightShafts::new(&device, &post.scene, depth_size, &depth_view, &sun_view, clouds)
        });
        let effects = enhance.particles.then(|| Effects::new(&device, format, &depth_view, &sun_view));
        let taa = taa.then(|| crate::taa::Taa::new(&device, depth_size, false, &depth_view));
        Renderer {
            device,
            queue,
            format: output_format,
            enhance,
            post,
            taa,
            light_shafts,
            effects,
            sun,
            cascades: None,
            world_box,
            world: None,
            world_drawn: None,
            far_drawn: None,
            time: 0.0,
            air_frame: 0,
            lamps: [LampUniform::default(); MAX_LAMPS],
            lamp_matrix: None,
            profiler: None,
            start_board,
            start_lamps,
            dial_targets,
            dial_draws: Vec::new(),
            dial_pass: None,
            buffers,
            frames,
            casters: meshes::casters(&meshes),
            meshes,
            slots,
            interactive_slots,
            grass: None,
            atmosphere: stage.atmosphere.clone(),
            ground_light,
            wetness,
            snow_ground: stage.snow_ground,
            car_dirt: Default::default(),
            cabin_sun: None,
            car_body: None,
            outside_view: false,
            screen_reflection,
            skid_marks: None,
            car_shadow: None,
            sky,
            depth_size,
            output_size: depth_size,
            render_scale: 1.0,
            depth_view,
            sun_mask,
            cloud_volume,
            shadow_params: std::cell::Cell::new([0.0; 4]),
            shadow_layers,
            shadow_pipelines,
            shadow_frame,
            shadow_range,
            sun_range,
            reflection,
            surroundings,
            probes,
            skylight,
            fog_ground: enhance.lighting.then(|| crate::height_fog::Ground::of(&stage.meshes)).flatten(),
            particles: None,
            glass: None,
            mirrors,
        }
    }

    /// The particle billboards and the water waves' triangles of the next frames, with `sprites` (the pass is made on
    /// the first call).
    pub fn set_particles(
        &mut self,
        sprites: &particles::gpu::Sprites,
        billboards: &[particles::sim::Billboard],
        waves: &[particles::wave::WaveVertex],
    ) {
        let pass = self.particles.get_or_insert_with(|| {
            let effects = self.effects.as_ref().map(|effects| &effects.layout);
            particles::gpu::Pass::new(&self.device, &self.queue, SCENE_FORMAT, DEPTH_FORMAT, sprites, effects)
        });
        pass.set(&self.device, &self.queue, billboards);
        pass.set_waves(&self.device, &self.queue, waves);
    }

    /// The additions drawn.
    pub fn enhance(&self) -> Enhance {
        self.enhance
    }

    /// The windscreen's drops of the next frames (the pass is made on the first call).
    pub fn set_glass(&mut self, drops: &[glass::gpu::Drop], wipers: [[f32; 4]; 4]) {
        let pass = self.glass.get_or_insert_with(|| {
            let sun_map = self.sun.as_ref().map_or_else(|| sun::placeholder(&self.device), |sun| sun.view.clone());
            glass::gpu::Pass::new(&self.device, SCENE_FORMAT, DEPTH_FORMAT, &sun_map)
        });
        pass.set(&self.device, &self.queue, drops);
        pass.set_wipers(wipers);
    }

    /// Lays the car's shadow decal ([`crate::car_shadow::decal`]) on `probe`'s ground for the car's body frame `frame`
    /// (physics axes) and the section's `Car_Deep_Shadow_Alpha`, with `texture` (`car_dpshad2.tga`) on the first call.
    /// Under [`Enhance::contact`] the contact shadow stands in for it.
    pub fn set_car_shadow(
        &mut self,
        texture: &rbr_formats::tga::Image,
        probe: &crate::stage_probe::StageProbe,
        frame: Mat4,
    ) {
        if self.enhance.contact {
            return;
        }
        let corners = crate::car_shadow::decal(probe, frame, self.atmosphere.car_deep_shadow);
        let shadow = self.car_shadow.get_or_insert_with(|| {
            let image = dds::Dds {
                width: texture.width,
                height: texture.height,
                format: Format::Rgba8,
                mip_count: 1,
                data: &texture.rgba,
            };
            crate::skidmarks_pass::SkidPass::decal(&self.device, &self.queue, SCENE_FORMAT, &image)
        });
        shadow.set_corners(&self.device, &self.queue, &corners);
    }

    /// Sets the skid marks to draw from `quads`, with `texture` (`skidmarks.dds`) on the first call.
    pub fn set_skid_marks(&mut self, texture: &dds::Dds, quads: &[crate::skidmarks::Quad]) {
        let marks = self.skid_marks.get_or_insert_with(|| {
            crate::skidmarks_pass::SkidPass::new(&self.device, &self.queue, SCENE_FORMAT, texture)
        });
        marks.set_quads(&self.device, &self.queue, quads);
    }

    /// Sets our tyre marks to draw from `quads` (`Enhance::marks`, in place of the skid marks).
    pub fn set_tyre_marks(&mut self, quads: &[crate::tyre_marks::Quad]) {
        let marks = self
            .skid_marks
            .get_or_insert_with(|| crate::skidmarks_pass::SkidPass::tyres(&self.device, &self.queue, SCENE_FORMAT));
        marks.set_tyre_quads(&self.device, &self.queue, quads);
    }

    /// Starts measuring each offscreen frame's stages ([`Renderer::profile`]), slowing the frames.
    pub fn enable_profiling(&mut self) {
        self.profiler = Some(Profiler::new(&self.device, &self.queue, false));
    }

    /// Starts feeding the performance log ([`crate::perf_log`]) the stages' CPU times and the passes' GPU times, which
    /// [`Renderer::end_frame`] reads back; whether the GPU's are timed here.
    pub fn enable_perf_log(&mut self) -> bool {
        let profiler = Profiler::new(&self.device, &self.queue, true);
        let timed = profiler.gpu_timed();
        self.profiler = Some(profiler);
        timed
    }

    /// Ends a frame the caller submitted ([`Renderer::render`] and the commands after it): reads its GPU times back.
    pub fn end_frame(&mut self) {
        if let Some(profiler) = &mut self.profiler {
            profiler.end_frame();
        }
    }

    /// Each measured stage's median milliseconds per frame, since [`Renderer::enable_profiling`] (offscreen frames).
    pub fn profile(&self) -> Vec<(&'static str, f64)> {
        self.profiler.as_ref().map_or_else(Vec::new, Profiler::medians)
    }

    /// Draws the frame at `scale` of the target's width and height (at most 1, the default), which the post pass
    /// scales up: a cheaper frame for high refresh rates at high resolutions.
    pub fn set_render_scale(&mut self, scale: f32) {
        self.render_scale = scale.clamp(0.25, 1.0);
    }

    /// Sets the car's dirt, which `CarPaint` faces blend their dirt map in by (`crate::dirt`).
    pub fn set_car_dirt(&mut self, dirt: crate::dirt::CarDirt) {
        self.car_dirt = dirt;
    }

    /// Sets whether the camera is outside the car (the reflections' interior flag).
    pub fn set_outside_view(&mut self, outside: bool) {
        self.outside_view = outside;
    }

    /// Draws the dials' `draws` into their textures ([`Image::Dial`]) at the start of the next frame.
    pub fn draw_dials(&mut self, draws: Vec<dials::gpu::Draw>) {
        self.dial_draws.extend(draws);
    }

    /// Sets the time of day the start board's clock shows, s after midnight (`None`: its placeholder face).
    pub fn set_start_clock(&mut self, seconds: Option<f32>) {
        if let Some(board) = &mut self.start_board {
            board.set(&self.queue, seconds);
        }
    }

    /// Lights the start board's lamps by the race clock (`None`: no race, unlit; [`start_board::lamps`]).
    pub fn set_start_lamps(&mut self, race: Option<&crate::live::RaceHud>) {
        for (lamps, lit) in self.start_lamps.iter_mut().zip(start_board::lamps(race)) {
            for lamp in lamps {
                lamp.set(&self.queue, lit);
            }
        }
    }

    /// The light and haze from here on (a stage's clock, `relight::Clock`). The renderer's resources for a time of
    /// day are made with it only when the stage's atmosphere at [`Renderer::new`] had a relight or was night.
    /// Whether the frame reads the physically based sky's tables ([`Enhance::lighting`]).
    fn skylight_on(&self) -> bool {
        self.enhance.lighting
    }

    /// Whether the sun is hidden: under an overcast sky with the sky's tables ([`Atmosphere::overcast`]), else as the
    /// game under `UseFog`.
    pub(super) fn sunless(&self) -> bool {
        if self.skylight_on() {
            self.atmosphere.overcast()
        } else {
            self.atmosphere.use_fog
        }
    }

    /// Draws the sky's tables for the atmosphere when it changed, in a submission of their own, before the reflection
    /// cube's faces read them.
    fn update_skylight(&self, profiler: &mut Option<Profiler>) {
        let Some(sky) = self.skylight_on().then(|| skylight::sky_uniform(&self.atmosphere)).flatten() else {
            return;
        };
        if self.skylight.holds(&sky) {
            return;
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let t = frame::mark(profiler, &mut encoder);
        self.skylight.update(&self.queue, &mut encoder, &sky);
        frame::span(profiler, &mut encoder, "sky tables", t);
        self.queue.submit([encoder.finish()]);
    }

    /// The day's light from the sky's tables ([`skylight::DayLight`]), when the frame reads them.
    fn day_light(&self) -> Option<skylight::DayLight> {
        self.skylight_on().then(|| self.skylight.day(&self.device)).flatten()
    }

    /// Whether the volumetric clouds are drawn in place of the game's cloud faces: with the sky's tables, by day and
    /// night.
    pub(super) fn cloud_volume_shown(&self) -> bool {
        self.cloud_volume.on() && self.day_light().is_some()
    }

    /// Whether the game's cloud faces are left out: under the volumetric clouds, and with a cover chosen over the
    /// section's ([`Atmosphere::clouds`]), whose faces would show the section's sky.
    pub(super) fn cloud_faces_hidden(&self) -> bool {
        self.cloud_volume_shown() || (self.atmosphere.clouds.is_some() && self.skylight_on())
    }

    /// Sets the time in seconds the moving parts of the frame are drawn at (water, rain, clouds, stars).
    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds;
    }

    /// The car's lit lamps, which light the stage at night and at dusk ([`Atmosphere::night`],
    /// [`crate::relight::Relight::lamps`]); the first [`MAX_LAMPS`].
    pub fn set_lamps(&mut self, lamps: &[Lamp]) {
        self.lamps = [LampUniform::default(); MAX_LAMPS];
        for (slot, lamp) in self.lamps.iter_mut().zip(lamps) {
            *slot = LampUniform::of(lamp);
        }
        let air = self.air_lamps();
        if let Some(effects) = &mut self.effects {
            effects.set_lamps(air);
        }
    }

    /// The lamps as they light the air: the dust, the spray, the rain and the fog take them as the stage does, all at
    /// night, the dusk's share at dusk and none by day (the game lights neither; by day the tail lamps lit the dust
    /// behind the car as a red streak).
    fn air_lamps(&self) -> [LampUniform; MAX_LAMPS] {
        let dark = if self.atmosphere.night { 1.0 } else { self.atmosphere.relight.map_or(0.0, |r| r.lamps) };
        self.lamps.map(|l| LampUniform { colour: (Vec4::from(l.colour) * dark).to_array(), ..l })
    }

    /// Starts the temporal anti-aliasing's history again with the next frame ([`crate::taa`]): for a cut it cannot
    /// tell from the view alone.
    pub fn reset_history(&mut self) {
        if let Some(taa) = &mut self.taa {
            taa.reset();
        }
    }

    /// The box of the car's body parts in its model space ([`Renderer::set_car_vertices`]), `None` without any.
    fn car_model_box(&self) -> Option<(Vec3, Vec3)> {
        let boxes = self.meshes.iter().filter_map(|mesh| mesh.in_car);
        boxes
            .map(|(centre, half)| (centre - half, centre + half))
            .reduce(|(l, h), (low, high)| (l.min(low), h.max(high)))
    }

    /// The car body's model space to stage space this frame.
    pub fn set_car_body(&mut self, model_to_stage: Mat4) {
        self.car_body = Some(model_to_stage);
        for mesh in &mut self.meshes {
            if let Some(model) = mesh.in_car {
                mesh.bounds = Some(meshes::placed_box(model_to_stage, model));
            }
        }
    }

    /// How much sun gets into the car's cabin by its direction ([`Enhance::cabin`], `crate::car::cabin::SunIn`), which
    /// the cabin light bounces off its sunlit faces.
    pub fn set_cabin_sun(&mut self, sun_in: Option<crate::car::cabin::SunIn>) {
        self.cabin_sun = sun_in;
    }

    /// The rain or snow ([`Enhance::particles`]; `None` for none).
    pub fn set_precipitation(&mut self, precipitation: Option<particles::Precipitation>) {
        if let Some(effects) = &mut self.effects {
            effects.set_precipitation(precipitation);
        }
    }

    /// The eye's velocity in stage space, metres per second, for the rain's streaks in frames that do not move on in
    /// time ([`Enhance::particles`]; a screenshot). Moving frames measure it.
    pub fn set_eye_velocity(&mut self, velocity: Vec3) {
        if let Some(effects) = &mut self.effects {
            effects.set_velocity(velocity);
        }
    }

    /// The wind uniform ([`FrameUniform::wind`]): x the time, z how hard rain lands on the wet roads
    /// ([`Enhance::particles`]).
    fn wind(&self) -> [f32; 4] {
        let rain = self.effects.as_ref().map_or(0.0, |effects| effects.rain());
        let landing = if self.wetness >= 1.0 { rain } else { 0.0 };
        let (_, start) = cloud_volume::drift(&self.atmosphere);
        [self.time, start[0], landing, start[1]]
    }
}
