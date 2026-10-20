//! wgpu drawing of a loaded [`Stage`]. Each draw's render state ([`DrawState`](crate::stage::DrawState)) maps to a
//! pipeline (culling, blending, depth) and a bind group (its textures, address modes and shader mode). Meshes are
//! uploaded once, and meshes outside the view frustum are skipped.  Textures are BC1 (`DXT1`), BC2 (`DXT3`) or BC3
//! (`DXT5`, community car models) as stored, so the device needs `TEXTURE_COMPRESSION_BC`. Colours are not converted to
//! sRGB, as the D3D9 fixed output did not.

use crate::atmosphere::Atmosphere;
use crate::depth_copy::DepthCopy;
use crate::dials;
use crate::effects::Effects;
use crate::foliage;
use crate::glass;
use crate::lights::Lamp;
use crate::particles;
use crate::post::{Post, SCENE_FORMAT};
use crate::profile::Profiler;
use crate::puddles;
use crate::skylight::{self, SkyLight};
use crate::stage::{Anchor, Draw, Image, Layer, Stage, TextureRef, Weather};
use crate::start_board::{self, StartBoard};
use crate::sun::{self, Cascades, SunShadow};
use glam::{Mat4, Vec3, Vec4};
use rbr_formats::dds::{self, Format};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

mod enhance;
mod frame;
mod gpu;
mod meshes;
mod mirrors;
mod offscreen;
mod pipelines;
mod reflection;
mod shadows;
mod textures;
mod uniforms;

pub use enhance::{AntiAliasing, Enhance};
pub use frame::FrameStats;
pub use gpu::{instance, request_gpu, Gpu, GpuError};
pub(crate) use meshes::mesh_bounds;
use meshes::{transformed_vertices, GpuDraw, GpuMesh, InteractiveGpu, MeshBuffers, Packing};
pub use offscreen::ReadbackError;
use pipelines::{coverage, layouts, prepassed, reflect_entry, vertex_entry, Context, DrawPass, PipelineKey};
pub(crate) use pipelines::{texture_entry, vertex_layout, SHADER, TEXTURE};
#[cfg(test)]
pub(crate) use reflection::BLIT;
use reflection::{reflects, Reflection, ScreenReflection};
use shadows::shadow_map;
pub(crate) use textures::dds_texture;
use textures::{
    bind_group, bind_key, color_view, depth_view, luminance_texture, solid_texture, Samplers, Views, MISSING, WHITE,
};
use uniforms::{FrameBinding, FrameUniform};
pub(crate) use uniforms::{LampUniform, MAX_LAMPS};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLEAR_COLOR: wgpu::Color = wgpu::Color { r: 0.42, g: 0.58, b: 0.78, a: 1.0 };
/// Multisampling of the colour and depth targets, resolved into the caller's target. A renderer choice beyond the
/// original (its device has no multisampling): it smooths edges. Alpha-tested opaque draws (the object lanes'
/// foliage, fences and spectators) use alpha to coverage, so their cut-out edges are smoothed too; the alpha test
/// still drops the same texels. With [`AntiAliasing::Off`] the frame has one sample instead, and the cut-outs are
/// alpha-tested alone, as in 1.02.
pub(crate) const SAMPLE_COUNT: u32 = 4;

/// A shader's `source` for a depth of `samples`: its multisampled depth bindings single-sampled at 1 (their
/// `textureLoad(depth, pixel, 0)` reads mip 0 then, the same texel).
pub(crate) fn depth_source(source: &str, samples: u32) -> String {
    match samples {
        1 => source.replace("texture_depth_multisampled_2d", "texture_depth_2d"),
        _ => source.to_owned(),
    }
}

/// The binding of the frame's depth with `samples`.
pub(crate) fn depth_binding(samples: u32) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Depth,
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: samples > 1,
    }
}

/// Draws one stage into a colour target of `format`.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    /// The format drawn in before the post pass: [`SCENE_FORMAT`] under [`Enhance::hdr`], else `format`.
    scene_format: wgpu::TextureFormat,
    enhance: Enhance,
    /// The colour and depth targets' samples ([`AntiAliasing::samples`]).
    samples: u32,
    frames: [FrameBinding; 3],
    meshes: Vec<GpuMesh>,
    /// The meshes' shared vertex and index buffers ([`Packing`]).
    buffers: Vec<MeshBuffers>,
    /// The GPU mesh of each stage mesh, `None` for a mesh with no draws (not uploaded).
    slots: Vec<Option<usize>>,
    /// Physics handle to the GPU mesh of each supported interactive visibility record.
    interactive_slots: HashMap<u32, usize>,
    atmosphere: Atmosphere,
    /// How wet the stage's weather is ([`FrameUniform::wetness`]).
    wetness: f32,
    /// The car's dirt level, 0 to 0.9 ([`Renderer::set_car_dirt`]).
    car_dirt: f32,
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
    /// The share of the target's width and height the frame is drawn at under [`Enhance::hdr`] (1: all of it).
    render_scale: f32,
    depth_view: wgpu::TextureView,
    /// The depth for the passes that read it per pixel (`None` without them).
    depth_copy: Option<DepthCopy>,
    /// The multisampled colour target, resolved into the caller's.
    color_view: wgpu::TextureView,
    /// The car's sun and contact shadows ([`SHADOW_SIZE`], [`GROUND_REACH`](shadows::GROUND_REACH)): the map's two
    /// layers, the pipelines that draw into them, and the frame binding those read (the world-anchored frame's buffer
    /// without the map).
    shadow_layers: [wgpu::TextureView; 2],
    shadow_pipelines: [wgpu::RenderPipeline; 2],
    reflection: Reflection,
    /// The stage around the car to the old reach, for its light and the wet roads' and the water's reflections.
    surroundings: Reflection,
    /// The physically based sky's tables ([`Enhance::sky`]; bound in every frame, drawn only with it).
    skylight: SkyLight,
    /// The low ground the height fog settles on (`None` without [`Enhance::sky`]).
    fog_ground: Option<crate::height_fog::Ground>,
    shadow_frame: wgpu::BindGroup,
    /// `None` when no draw reflects ([`reflects`]).
    screen_reflection: Option<ScreenReflection>,
    /// `None` unless [`Enhance::hdr`].
    post: Option<Post>,
    /// `None` unless [`Enhance::sky`] with [`Enhance::hdr`] ([`crate::light_shafts`]).
    light_shafts: Option<crate::light_shafts::LightShafts>,
    /// `None` unless [`Enhance::particles`].
    effects: Option<Effects>,
    /// `None` unless [`Enhance::sun`].
    sun: Option<SunShadow>,
    /// The sun's cascades for the frame being drawn, `None` when there are none (no sun, or an overcast sky).
    cascades: Option<Cascades>,
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
    /// The car's lit lamps at night ([`Renderer::set_lamps`]).
    lamps: [LampUniform; MAX_LAMPS],
    /// The headlights' shadow map's clip matrix for the frame being drawn, `None` without one.
    lamp_matrix: Option<Mat4>,
    /// `None` unless [`Renderer::enable_profiling`].
    profiler: Option<Profiler>,
    /// The start board's clock face ([`Renderer::set_start_clock`]), when the stage shows one.
    start_board: Option<StartBoard>,
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
        let device = gpu.device.clone();
        let queue = gpu.queue.clone();
        let output_format = format;
        let format = if enhance.hdr { SCENE_FORMAT } else { output_format };

        let [frame_layout, shadow_layout, texture_layout] = layouts(&device);
        let (shadow_view, shadow_layers, shadow_sampler) = shadow_map(&device);

        // A stage drawn at a time of day (`relight`, `Atmosphere::night`) needs the normals the baked draws do not
        // store, and at night the headlights' shadows use the sun's first cascade.
        let timed = stage.atmosphere.relight.is_some() || stage.atmosphere.night;
        let sun = (enhance.sun || timed).then(|| SunShadow::new(&device, &texture_layout));
        let sun_view = sun.as_ref().map_or_else(|| sun::placeholder(&device), |sun| sun.view.clone());
        let samples = enhance.anti_aliasing.samples();
        let reflection = Reflection::new(&device, format, samples);
        let surroundings = Reflection::new(&device, format, samples);
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
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mirrors = mirrors::Mirrors::new(&device, format, enhance.mirrors);
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
        let mut views = Views {
            loaded: stage
                .textures
                .iter()
                .filter_map(|(&key, texture)| {
                    let texture = match texture.image.as_ref()? {
                        Image::Dds(bytes) => {
                            let dds = dds::parse(bytes).ok()?;
                            dds_texture(&device, &queue, &dds)
                        }
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
                                glows.insert(key, (view, mask.format != dds::Format::Dxt1));
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
        };
        let start_board = stage.start_board().and_then(|texture| {
            let Some(Image::Dds(bytes)) = stage.textures.get(&texture)?.image.as_ref() else { return None };
            let placeholder = start_board::Rgba::decode(&dds::parse(bytes).ok()?);
            let digits =
                stage.start_digits.as_deref().and_then(|b| dds::parse(b).ok()).map(|d| start_board::Rgba::decode(&d));
            let board = StartBoard::new(&device, &queue, placeholder, digits);
            views.loaded.insert(texture, board.view());
            Some(board)
        });

        let mut bind_groups = HashMap::new();
        let mut pipelines = HashMap::new();
        let mut meshes = Vec::new();
        let mut packing = Packing::default();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage"),
            source: wgpu::ShaderSource::Wgsl(depth_source(SHADER, samples).into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage"),
            bind_group_layouts: &[&frame_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let context = Context {
            device: &device,
            format,
            shader: &shader,
            layout: &layout,
            vertex: vertex_entry(enhance),
            water_plus: enhance.water || enhance.wet,
            cabin: enhance.cabin,
            sky_lut: enhance.sky,
            samples,
        };
        // A draw of a mesh whose indices start at `first_index` in the shared buffer, with its pipelines and bind group.
        let mut gpu_draw = |draw: &Draw, first_index: u32, casts_shadow: bool, mirror: bool| {
            let pipeline = PipelineKey::of(&draw.state, mirror);
            let bind = bind_key(&draw.state, &views);
            let mut pipeline_in = |pass: DrawPass| {
                let key = pipeline.in_pass(pass);
                pipelines.entry(key).or_insert_with(|| context.pipeline(key)).clone()
            };
            let main = pipeline_in(DrawPass::Main);
            let prepassed =
                prepassed(&draw.state).then(|| [DrawPass::DepthPrepass, DrawPass::AfterPrepass].map(&mut pipeline_in));
            let group = bind_groups
                .entry(bind)
                .or_insert_with(|| bind_group(&device, &texture_layout, bind, &views, &samplers))
                .clone();
            GpuDraw {
                pipeline,
                main,
                prepassed,
                group,
                casts_shadow,
                bind,
                lod: draw.lod,
                indices: first_index + draw.indices.start as u32..first_index + draw.indices.end as u32,
            }
        };

        let mut slots = Vec::with_capacity(stage.meshes.len());
        let cut_out = if enhance.sun { foliage::cut_out(stage, |draw| coverage(&draw.state)) } else { HashMap::new() };
        let plant = |draw: &Draw| cut_out.get(&draw.state.textures[0]?).filter(|_| coverage(&draw.state));
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
        for mesh in solid.iter().map(Some).chain([None]).chain(blended.iter().map(Some)) {
            let Some(mesh) = mesh else {
                for instance in &stage.interactive {
                    let mesh = instance.mesh.as_ref();
                    if mesh.draws.is_empty() {
                        continue;
                    }
                    let placement = Mat4::from_cols_array(&instance.placement);
                    let vertices = transformed_vertices(&mesh.vertices, placement);
                    let bounds = mesh_bounds(&vertices);
                    let (slot, first_index) = packing.add(&device, &vertices, &mesh.indices);
                    let draws = mesh.draws.iter().map(|draw| gpu_draw(draw, first_index, false, false)).collect();
                    interactive_slots.insert(instance.handle, meshes.len());
                    meshes.push(GpuMesh {
                        bounds: bounds.map(|b| (b.center, b.half_extent)),
                        cell: instance.cell.map(|b| (b.center, b.half_extent)),
                        visible: true,
                        outside: None,
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
                    });
                }
                continue;
            };
            if mesh.draws.is_empty() {
                slots.push(None);
                continue;
            }
            slots.push(Some(meshes.len()));
            // The sun's light needs the normals the baked stage draws do not store, the sun's shadows the plants.
            let (slot, first_index) = if enhance.sun || enhance.wet || timed {
                let mut vertices = sun::with_normals(mesh);
                if mesh.anchor == Anchor::World {
                    vertices = foliage::with_plants(vertices, mesh, plant);
                    if let Some(ground) = &ground {
                        vertices = ground.on_foliage(vertices, mesh);
                    }
                    if let Some(heights) = &heights {
                        vertices = puddles::with_lowness(vertices, mesh, heights);
                    }
                }
                packing.add(&device, &vertices, &mesh.indices)
            } else {
                packing.add(&device, &mesh.vertices, &mesh.indices)
            };
            // A mirror's glass (`mirrors::mark`): a lit draw with a marked corner.
            let mirror = |draw: &Draw| {
                enhance.mirrors != crate::mirrors::Rate::Off
                    && draw.state.shade.is_some()
                    && mesh.indices[draw.indices.clone()].iter().any(|&i| mesh.vertices[i as usize].specular[0] >= 1.0)
            };
            let draws = mesh
                .draws
                .iter()
                .map(|draw| {
                    let casts_shadow = draw.state.shade.is_some() && draw.state.depth_write;
                    gpu_draw(draw, first_index, casts_shadow, mirror(draw))
                })
                .collect();
            meshes.push(GpuMesh {
                bounds: mesh.bounds.map(|b| (b.center, b.half_extent)),
                cell: mesh.cell.map(|b| (b.center, b.half_extent)),
                visible: true,
                outside: None,
                anchor: mesh.anchor,
                layer: mesh.layer,
                slot,
                draws,
                interactive: None,
                in_car: None,
                car: false,
            });
        }

        let buffers = packing.finish(&device);
        let sky = context.sky_pipeline(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[&frame_layout],
            push_constant_ranges: &[],
        }));
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("car shadow"),
            bind_group_layouts: &[&shadow_layout],
            push_constant_ranges: &[],
        });
        let shadow_pipelines =
            ["vs_shadow", "vs_ground"].map(|entry| context.shadow_pipeline(&shadow_pipeline_layout, entry));
        let depth_size = (1, 1);
        let depth_view = depth_view(&device, depth_size, samples);
        let color_view = color_view(&device, format, depth_size, samples);
        let wetness = match stage.selection.weather {
            Weather::Dry => 0.0,
            Weather::Damp => 0.5,
            Weather::Wet => 1.0,
        };
        let draws = || meshes.iter().flat_map(|mesh| &mesh.draws);
        let reflecting = draws().any(|draw| reflects(&draw.bind, wetness));
        // The wet roads' reflections read the depth enough to pay for copying it; the water's alone do not.
        let wet_roads = draws().any(|draw| !draw.bind.water && reflects(&draw.bind, wetness));
        let depth_copy = wet_roads.then(|| DepthCopy::new(&device, depth_size, &depth_view, samples));
        let screen_reflection = reflecting.then(|| {
            ScreenReflection::new(
                &device,
                (&shader, vertex_entry(enhance), reflect_entry(enhance)),
                [&frame_layout, &texture_layout],
                format,
                (depth_size, samples),
                &depth_view,
                depth_copy.as_ref().map(DepthCopy::view),
            )
        });
        let post =
            enhance.hdr.then(|| Post::new(&device, output_format, depth_size, &depth_view, samples, enhance.sky));
        let light_shafts = post
            .as_ref()
            .filter(|_| enhance.sky)
            .map(|post| crate::light_shafts::LightShafts::new(&device, &post.scene, depth_size, &depth_view, samples));
        let effects = enhance.particles.then(|| Effects::new(&device, format, &depth_view, &sun_view, samples));
        Renderer {
            device,
            queue,
            format: output_format,
            scene_format: format,
            enhance,
            samples,
            post,
            light_shafts,
            effects,
            sun,
            cascades: None,
            time: 0.0,
            lamps: [LampUniform::default(); MAX_LAMPS],
            lamp_matrix: None,
            profiler: None,
            start_board,
            dial_targets,
            dial_draws: Vec::new(),
            dial_pass: None,
            buffers,
            frames,
            meshes,
            slots,
            interactive_slots,
            atmosphere: stage.atmosphere.clone(),
            wetness,
            car_dirt: 0.0,
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
            depth_copy,
            color_view,
            shadow_layers,
            shadow_pipelines,
            shadow_frame,
            reflection,
            surroundings,
            skylight,
            fog_ground: enhance.sky.then(|| crate::height_fog::Ground::of(&stage.meshes)).flatten(),
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
            particles::gpu::Pass::new(
                &self.device,
                &self.queue,
                self.scene_format,
                self.samples,
                DEPTH_FORMAT,
                sprites,
                effects,
            )
        });
        pass.set(&self.device, &self.queue, billboards);
        pass.set_waves(&self.device, &self.queue, waves);
    }

    /// The windscreen's drops of the next frames (the pass is made on the first call).
    pub fn set_glass(&mut self, drops: &[glass::gpu::Drop]) {
        let pass = self
            .glass
            .get_or_insert_with(|| glass::gpu::Pass::new(&self.device, self.scene_format, self.samples, DEPTH_FORMAT));
        pass.set(&self.device, &self.queue, drops);
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
            crate::skidmarks_pass::SkidPass::decal(&self.device, &self.queue, self.scene_format, self.samples, &image)
        });
        shadow.set_corners(&self.device, &self.queue, &corners);
    }

    /// Sets the skid marks to draw from `quads`, with `texture` (`skidmarks.dds`) on the first call.
    pub fn set_skid_marks(&mut self, texture: &dds::Dds, quads: &[crate::skidmarks::Quad]) {
        let marks = self.skid_marks.get_or_insert_with(|| {
            crate::skidmarks_pass::SkidPass::new(&self.device, &self.queue, self.scene_format, self.samples, texture)
        });
        marks.set_quads(&self.device, &self.queue, quads);
    }

    /// Sets our tyre marks to draw from `quads` (`Enhance::marks`, in place of the skid marks).
    pub fn set_tyre_marks(&mut self, quads: &[crate::tyre_marks::Quad]) {
        let marks = self.skid_marks.get_or_insert_with(|| {
            crate::skidmarks_pass::SkidPass::tyres(&self.device, &self.queue, self.scene_format, self.samples)
        });
        marks.set_tyre_quads(&self.device, &self.queue, quads);
    }

    /// Starts measuring each offscreen frame's stages ([`Renderer::profile`]), slowing the frames.
    pub fn enable_profiling(&mut self) {
        self.profiler = Some(Profiler::new(&self.device, &self.queue));
    }

    /// Each measured stage's median milliseconds per frame, since [`Renderer::enable_profiling`] (offscreen frames).
    pub fn profile(&self) -> Vec<(&'static str, f64)> {
        self.profiler.as_ref().map_or_else(Vec::new, Profiler::medians)
    }

    /// Draws the frame at `scale` of the target's width and height (at most 1, the default), which the post pass
    /// scales up: a cheaper frame for high refresh rates at high resolutions. Only under [`Enhance::hdr`].
    pub fn set_render_scale(&mut self, scale: f32) {
        self.render_scale = scale.clamp(0.25, 1.0);
    }

    /// Sets the car's dirt level, which `CarPaint` faces blend their dirt map in by (`crate::dirt`).
    pub fn set_car_dirt(&mut self, level: f32) {
        self.car_dirt = level;
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

    /// The light and haze from here on (a stage's clock, `relight::Clock`). The renderer's resources for a time of
    /// day are made with it only when the stage's atmosphere at [`Renderer::new`] had a relight or was night.
    pub fn set_atmosphere(&mut self, atmosphere: Atmosphere) {
        self.atmosphere = atmosphere;
    }

    /// Whether the frame reads the physically based sky's tables ([`Enhance::sky`], by day).
    fn skylight_on(&self) -> bool {
        self.enhance.sky && !self.atmosphere.night
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
        [self.time, 0.0, landing, 0.0]
    }
}
