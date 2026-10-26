//! The car's reflection cube and the screen-space reflections of the water and the wet roads.

use super::frame::{mark, span, View};
use super::meshes::{Bound, GpuDraw};
use super::pipelines::{texture_entry, vertex_layout, TEXTURE};
use super::textures::depth_view;
use super::{depth_binding, Renderer, CLEAR_COLOR, DEPTH_FORMAT};
use crate::camera::{self, Frustum};
use crate::coords::{self, FRONT_FACE, NEAR};
use crate::profile::Profiler;
use glam::camera::lh::view;
use glam::{Mat4, Vec3};

/// The car's reflection (renderer choice standing in for the game's reflection cube, `notes/formats.md` "Shaders and vertex layouts"): the stage without
/// the car, drawn into a cube map `REFLECTION_SIZE` texels square at the car's centre. All six faces are drawn on the
/// first frame, then one face per frame.
/// The mip chain's downsampling shader.
pub(crate) const BLIT: &str = include_str!("../blit.wgsl");
pub(super) const REFLECTION_SIZE: u32 = 128;
/// The reflection cube leaves out the meshes without a visibility cell wholly beyond this, metres (ours; it takes the
/// stage's cells by [`CUBE_REACH`](crate::visibility::CUBE_REACH), and the surroundings cube reaches the main view's
/// [`CELL_REACH`](crate::visibility::CELL_REACH)): at 128 texels a 90 degree face shows a
/// 10 m thing 500 m away as one texel, and on a long stage (RSF 129, 6972 meshes) the far draws cost a face 2-3 ms.
pub(super) const REFLECTION_REACH: f32 = 500.0;
/// The reflection cube's mip levels, down to 1 texel (blurred surroundings for the car's light, `shader.wgsl`).
pub(super) const REFLECTION_MIPS: u32 = REFLECTION_SIZE.ilog2() + 1;

impl Renderer {
    /// Draws the screen-space reflections ([`ScreenReflection`]) over the frame `draw_scene` kept, and resolves it into
    /// `target`: the water draws, and in wet weather the wet roads, again at half size (`fs_reflect`, which hides what
    /// lies behind the frame's depth), then laid over the frame (`reflection_composite.wgsl`).
    pub(super) fn draw_reflections(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        ssr: &ScreenReflection,
        target: &wgpu::TextureView,
        view_projection: Mat4,
    ) {
        let eye = camera::eye(view_projection);
        let frustum = Frustum::from_view_projection(view_projection);
        let cleared = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen-space reflections"),
            color_attachments: &[cleared(&ssr.traced[0]), cleared(&ssr.traced[1])],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&ssr.pipeline);
        pass.set_bind_group(0, &self.frames[0].bind_group, &[]);
        pass.set_bind_group(2, &ssr.group, &[]);
        let mut bound = Bound::new(&self.buffers);
        for mesh in &self.meshes {
            if mesh.bounds.is_some_and(|(center, half)| !frustum.intersects_box(center, half))
                || mesh.cell_hidden(view_projection)
            {
                continue;
            }
            let distance = mesh.bounds.map_or(0.0, |(center, _)| center.distance(eye));
            for draw in mesh.draws.iter().filter(|draw| draw.lod.shows(distance) && reflects(draw, self.wetness)) {
                pass.set_bind_group(1, &draw.group, &[]);
                bound.draw(&mut pass, mesh, draw);
            }
        }
        drop(pass);
        let attachment = self.over_attachment(encoder, target, &ssr.levels[0]);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen-space reflections' composite"),
            color_attachments: &[Some(attachment)],
            // Read only, so that the effects can also sample it.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&ssr.composite);
        pass.set_bind_group(0, &ssr.composite_group, &[]);
        pass.draw(0..3, 0..1);
        if !self.late_effects() {
            self.draw_effects(&mut pass);
        }
        if let Some(glass) = &self.glass {
            glass.draw(&self.device, &mut pass, &ssr.scene);
        }
    }

    /// Draws the due faces of the reflection cube and of the surroundings cube from the car's centre
    /// ([`REFLECTION_SIZE`]). The reflection cube draws all six until it is complete, then one per frame; the
    /// surroundings cube as [`surroundings_due`] says. Each face goes in its own submission, since the frame uniforms
    /// are rewritten per face; the two cubes share it when they draw the same face, as they share the view. Faces are
    /// drawn into a staging cube and copied into the sampled one, which the frames bind.
    pub(super) fn update_reflection(&self, profiler: &mut Option<Profiler>) {
        // Baked probes stand in for both cubes (`super::probes`), but the car's paint seen from outside by day reflects
        // the surroundings cube (`paint_mirrored` in `shader.wgsl`).
        let paint = self.outside_view && self.day_light().is_some();
        let Some((low, high)) = self.car_bounds().filter(|_| !self.probes.baked || paint) else {
            return;
        };
        let eye = (low + high) * 0.5;
        let s = &self.surroundings;
        let surroundings = surroundings_due(s.skipped.get(), s.at.get(), eye);
        s.skipped.set(if surroundings == 0 { s.skipped.get() + 1 } else { 0 });
        if surroundings > 0 {
            s.at.set(Some(eye));
        }
        // Each cube's due faces run on from the last one it drew.
        let cubes = [
            (
                &self.reflection,
                View::Reflection,
                "reflection cube face",
                match (self.probes.baked, self.reflection.drawn.get()) {
                    (true, _) => 0,
                    (false, 0..6) => 6,
                    (false, _) => 1,
                },
            ),
            (s, View::Surroundings, "surroundings cube face", surroundings),
        ]
        .map(|(r, view, label, count)| (r, view, label, r.drawn.get() % 6, count));
        for face in 0..6 {
            let due = || cubes.iter().filter(|&&(.., first, count)| (face + 6 - first) % 6 < count);
            if due().next().is_none() {
                continue;
            }
            let (view_projection, sky_view_projection) = face_views(eye, face);
            self.write_frames(view_projection, sky_view_projection, None, View::Reflection);
            let mut encoder = self.device.create_command_encoder(&Default::default());
            for &(r, view, label, ..) in due() {
                let t = mark(profiler, &mut encoder);
                self.draw_face(&mut encoder, r, view, face, view_projection);
                span(profiler, &mut encoder, label, t);
                r.drawn.set(r.drawn.get() + 1);
            }
            self.queue.submit([encoder.finish()]);
        }
    }

    /// Draws `view` of `view_projection` into face `face` of `r`'s sampled cube and fills its mip levels.
    pub(super) fn draw_face(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        r: &Reflection,
        view: View,
        face: u32,
        view_projection: Mat4,
    ) {
        self.draw_scene(encoder, [&r.faces[face as usize], &r.depth], view_projection, view, [false; 2]);
        let layer = wgpu::TexelCopyTextureInfo {
            texture: &r.staging,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: face },
            aspect: wgpu::TextureAspect::All,
        };
        encoder.copy_texture_to_texture(
            layer,
            wgpu::TexelCopyTextureInfo { texture: &r.cube, ..layer },
            wgpu::Extent3d { width: REFLECTION_SIZE, height: REFLECTION_SIZE, depth_or_array_layers: 1 },
        );
        r.halver.halve(encoder, &r.mips[face as usize], &r.sources[face as usize]);
    }
}

/// The view and the sky's view (rotation only) of cube face `face` ([`CUBE_FACES`]) from `eye`.
pub(super) fn face_views(eye: Vec3, face: u32) -> (Mat4, Mat4) {
    let (direction, up) = CUBE_FACES[face as usize];
    let projection = coords::reversed(std::f32::consts::FRAC_PI_2, 1.0, NEAR);
    (projection * view::look_to_mat4(eye, direction, up), projection * view::look_to_mat4(Vec3::ZERO, direction, up))
}

/// The surroundings cube draws one face every this many frames, a full refresh every six times as many (ours: it
/// feeds blurred light and far reflections, which change slowly).
const SURROUNDINGS_EVERY: u32 = 3;
/// How far the cube's centre may move from where the surroundings cube last drew a face before it draws all six
/// again, metres (ours): a seek, a teleport or a cut to another car, never driving between two faces.
const SURROUNDINGS_SNAP: f32 = 50.0;
/// The fewest texels across a mesh's bounds a surroundings cube face draws ([`tiny`], ours).
const TINY_TEXELS: f32 = 2.0;

/// How many faces of the surroundings cube are due with its centre at `eye`, after `skipped` frames without one since
/// its last face, drawn at `at` (`None`: none yet): all six on the first frame and after a jump past
/// [`SURROUNDINGS_SNAP`], else one every [`SURROUNDINGS_EVERY`] frames.
pub(super) fn surroundings_due(skipped: u32, at: Option<Vec3>, eye: Vec3) -> u32 {
    match at {
        Some(at) if at.distance(eye) <= SURROUNDINGS_SNAP => u32::from(skipped + 1 >= SURROUNDINGS_EVERY),
        _ => 6,
    }
}

/// Whether a mesh whose bounds reach `radius` from their centre, `distance` from a cube face's eye, spans under
/// [`TINY_TEXELS`] of a 90 degree face of [`REFLECTION_SIZE`] texels: a face spans 2 at unit depth, the mesh about
/// `2 radius / distance`.
pub(super) fn tiny(radius: f32, distance: f32) -> bool {
    radius * (REFLECTION_SIZE as f32) < TINY_TEXELS * distance
}

/// Whether a draw gets screen-space reflections: water, and the wet roads in wet weather (`wetness` 1).
pub(super) fn reflects(draw: &GpuDraw, wetness: f32) -> bool {
    draw.bind.water || (wetness >= 1.0 && draw.wet_road)
}

/// Screen-space reflections (renderer choice, `fs_reflect` in `shader.wgsl`): the frame is resolved into `scene`
/// first, then the reflecting draws are drawn again at half size with `pipeline`, which reads `scene` and the depth
/// (`group`) and writes what their reflected rays hit on screen and their depth into `traced`; `composite` lays that
/// over the frame (`composite_group`).
pub(super) struct ScreenReflection {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// The resolved frame's levels ([`SCENE_MIPS`]): the frame, then ever blurrier halvings of it, for glossy
    /// reflections.
    pub(super) levels: Vec<wgpu::TextureView>,
    /// All the levels, for the glass's lenses.
    scene: wgpu::TextureView,
    pub(super) sources: Vec<wgpu::BindGroup>,
    pub(super) halver: Halver,
    group: wgpu::BindGroup,
    /// The half-size reflections times their weights, the weights ([`TRACED`]), and the surfaces' depths.
    traced: [wgpu::TextureView; 2],
    composite: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    composite_group: wgpu::BindGroup,
}

/// The resolved frame's mip levels for the screen-space reflections.
const SCENE_MIPS: u32 = 6;
/// The half-size reflections' format, and their depths'.
const TRACED: [wgpu::TextureFormat; 2] = [wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::R32Float];
/// The composite's shader (`reflection_composite.wgsl`).
const COMPOSITE: &str = include_str!("../reflection_composite.wgsl");

impl ScreenReflection {
    /// `shader` with the stage draws' vertex entry point ([`vertex_entry`](super::pipelines::vertex_entry)) and the
    /// fragment entry point ([`reflect_entry`](super::pipelines::reflect_entry)), `rays` a pixel and
    /// `steps` a ray ([`Reflections::rays`](super::Reflections::rays)).
    pub(super) fn new(
        device: &wgpu::Device,
        (shader, vertex, fragment, [rays, steps], write_mask): (
            &wgpu::ShaderModule,
            &str,
            &str,
            [u32; 2],
            wgpu::ColorWrites,
        ),
        [frame_layout, texture_layout]: [&wgpu::BindGroupLayout; 2],
        format: wgpu::TextureFormat,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) -> ScreenReflection {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen-space reflections"),
            entries: &[texture_entry(0, TEXTURE), texture_entry(1, depth_binding())],
        });
        let constants = [("SSR_RAYS", f64::from(rays)), ("SSR_STEPS", f64::from(steps))];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-space reflections"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("screen-space reflections"),
                bind_group_layouts: &[Some(frame_layout), Some(texture_layout), Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vertex),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            primitive: wgpu::PrimitiveState {
                front_face: FRONT_FACE,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment),
                compilation_options: wgpu::PipelineCompilationOptions { constants: &constants, ..Default::default() },
                // The reflecting layers (an RSF road's decals over it) blend over each other as they did over the
                // frame: from transparent, the colour comes out premultiplied by the weight.
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: TRACED[0],
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(TRACED[1].into()),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen-space reflections' composite"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                texture_entry(2, depth_binding()),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen-space reflections' composite"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE.into()),
        });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-space reflections' composite"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("screen-space reflections' composite"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_composite"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            // The pass keeps the frame's depth bound, read only, for the effects drawn after.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_composite"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let halver = Halver::new(device, format);
        let sized = Self::targets(device, [&layout, &composite_layout], format, size, depth);
        ScreenReflection {
            pipeline,
            layout,
            sources: halver.sources(device, &sized.levels),
            levels: sized.levels,
            scene: sized.scene,
            halver,
            group: sized.group,
            traced: sized.traced,
            composite,
            composite_layout,
            composite_group: sized.composite_group,
        }
    }

    pub(super) fn resize(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) {
        let sized = Self::targets(device, [&self.layout, &self.composite_layout], format, size, depth);
        self.sources = self.halver.sources(device, &sized.levels);
        (self.levels, self.scene, self.group) = (sized.levels, sized.scene, sized.group);
        (self.traced, self.composite_group) = (sized.traced, sized.composite_group);
    }

    /// The targets for frames of `size`: the resolved frame's levels and the bind group that reads them and `depth`;
    /// the half-size traced targets and the composite's bind group.
    fn targets(
        device: &wgpu::Device,
        [layout, composite_layout]: [&wgpu::BindGroupLayout; 2],
        format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        depth: &wgpu::TextureView,
    ) -> FrameTargets {
        let (width, height) = (width.max(1), height.max(1));
        let mips = SCENE_MIPS.min(width.min(height).ilog2() + 1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("resolved frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            // Copied from ([`Renderer::over_attachment`]).
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let levels = (0..mips)
            .map(|mip| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let scene = texture.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen-space reflections"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&scene) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        });
        let traced = TRACED.map(|format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("screen-space reflections, half size"),
                    size: wgpu::Extent3d {
                        width: width.div_ceil(2),
                        height: height.div_ceil(2),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen-space reflections' composite"),
            layout: composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&traced[0]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&traced[1]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        });
        FrameTargets { levels, scene, group, traced, composite_group }
    }
}

/// What [`ScreenReflection::targets`] makes for a frame's size.
struct FrameTargets {
    levels: Vec<wgpu::TextureView>,
    scene: wgpu::TextureView,
    group: wgpu::BindGroup,
    traced: [wgpu::TextureView; 2],
    composite_group: wgpu::BindGroup,
}

/// The cube faces in the order of their layers (+x, -x, +y, -y, +z, -z): the view direction and up of each, as
/// D3D and wgpu lay out cube maps for a left-handed frame.
const CUBE_FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y),
    (Vec3::NEG_X, Vec3::Y),
    (Vec3::Y, Vec3::NEG_Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];

/// A cube drawn around the car ([`REFLECTION_SIZE`]): the car's reflection cube, which holds the game's cells near
/// the car (`View::Reflection`), or the surroundings cube (ours), which holds the stage to the main view's reach for
/// what reads the car's surroundings rather than its paint: the car's light from them (`surroundings` in
/// `shader.wgsl`), the wet road's reflection and the water's missed rays (`View::Surroundings`). Each holds the
/// sampled cube, the staging cube its faces are drawn into (a view per face), the depth target of a face, and how many faces have been drawn. The sampled cube's mip levels ([`REFLECTION_MIPS`]) are
/// made by halving each level into the next: `mips` are the views of each face's levels, `sources` the bind groups
/// that read each level but the last.
pub(super) struct Reflection {
    pub(super) cube: wgpu::Texture,
    staging: wgpu::Texture,
    faces: [wgpu::TextureView; 6],
    mips: [Vec<wgpu::TextureView>; 6],
    sources: [Vec<wgpu::BindGroup>; 6],
    halver: Halver,
    depth: wgpu::TextureView,
    /// Faces drawn so far ([`Renderer::update_reflection`]).
    pub(super) drawn: std::cell::Cell<u32>,
    /// The surroundings cube's refresh ([`surroundings_due`]): the frames since its last face, and the centre it was
    /// drawn from.
    skipped: std::cell::Cell<u32>,
    pub(super) at: std::cell::Cell<Option<Vec3>>,
}

impl Reflection {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Reflection {
        let cube = |label, usage, mip_level_count| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: REFLECTION_SIZE, height: REFLECTION_SIZE, depth_or_array_layers: 6 },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let staging =
            cube("car reflection faces", wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, 1);
        let sampled = cube(
            "car reflection",
            // Copied from into the probes (`super::probes`).
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            REFLECTION_MIPS,
        );
        let mips = [0, 1, 2, 3, 4, 5].map(|layer| {
            (0..REFLECTION_MIPS)
                .map(|mip| {
                    sampled.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_mip_level: mip,
                        mip_level_count: Some(1),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>()
        });
        let halver = Halver::new(device, format);
        let sources = mips.each_ref().map(|levels| halver.sources(device, levels));
        let faces = [0, 1, 2, 3, 4, 5].map(|layer| {
            staging.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        let size = (REFLECTION_SIZE, REFLECTION_SIZE);
        Reflection {
            cube: sampled,
            staging,
            faces,
            mips,
            sources,
            halver,
            depth: depth_view(device, size),
            drawn: std::cell::Cell::new(0),
            skipped: std::cell::Cell::new(0),
            at: std::cell::Cell::new(None),
        }
    }
}

/// Halves a texture level into the next (`blit.wgsl`), to fill mip chains.
pub(super) struct Halver {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
}

impl Halver {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Halver {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mip"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mip"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mip"),
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mip"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mip"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Halver { layout, sampler, pipeline }
    }

    /// The bind groups that read each of `levels` but the last.
    fn sources(&self, device: &wgpu::Device, levels: &[wgpu::TextureView]) -> Vec<wgpu::BindGroup> {
        levels[..levels.len() - 1]
            .iter()
            .map(|view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mip"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    ],
                })
            })
            .collect()
    }

    /// Fills `levels` after the first, each from the one before (`sources`, from [`Halver::sources`]).
    pub(super) fn halve(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        levels: &[wgpu::TextureView],
        sources: &[wgpu::BindGroup],
    ) {
        for (target, source) in levels.iter().skip(1).zip(sources) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(CLEAR_COLOR), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, source, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surroundings_snap_then_one_face_every_third_frame() {
        let at = Some(Vec3::ZERO);
        assert_eq!(surroundings_due(0, None, Vec3::ZERO), 6);
        assert_eq!(surroundings_due(0, at, Vec3::X), 0);
        assert_eq!(surroundings_due(1, at, Vec3::X), 0);
        assert_eq!(surroundings_due(2, at, Vec3::X), 1);
        // A seek, teleport or cut to another car redraws the whole cube at once.
        assert_eq!(surroundings_due(0, at, Vec3::new(0.0, 0.0, 50.5)), 6);
    }

    #[test]
    fn tiny_under_two_texels() {
        // 2 texels of 128 across 90 degrees: a 1 m sphere (radius) at 64 m.
        assert!(!tiny(1.0, 63.0));
        assert!(tiny(1.0, 65.0));
        assert!(!tiny(10.0, 0.0));
    }
}
