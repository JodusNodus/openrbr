//! The car's sun and contact shadow maps, the sun's cascades' casters and the headlights' shadow.

use super::meshes::{Bound, GpuDraw, GpuMesh, MeshBuffers};
use super::uniforms::LampUniform;
use super::{Renderer, DEPTH_FORMAT};
use crate::camera::Frustum;
use crate::stage::{Anchor, Layer, Lod};
use crate::sun::{self, Cascades, SunShadow};
use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec3, Vec4};

/// The car's sun shadow (renderer choice; the game's car shadow uses the `Plane_*` meshes and is not traced): a depth
/// map of the car's opaque lit draws seen along `SunDir`, `SHADOW_SIZE` texels square over the car's bounds plus
/// `SHADOW_MARGIN` metres. The stage in it darkens by `Car_Shadow_Alpha` times `SHADOW_SCALE`, at most `SHADOW_MOST`
/// (the data's 0.05 to 0.24 barely show on the dark roads; overcast sections have 0, so no sun shadow).
/// The shader fades it where the baked vertex light is already dark.
/// The car's own faces in it lose their sun term.
pub(super) const SHADOW_SIZE: u32 = 2048;
const SHADOW_MARGIN: f32 = 0.5;
pub(super) const SHADOW_SCALE: f32 = 1.75;
pub(super) const SHADOW_MOST: f32 = 0.45;
/// The reach of the shadow's filter each side, metres, over 1.5: the widest penumbra (`car_shadow`), and the range's
/// reach.
pub(super) const SHADOW_PENUMBRA: f32 = 0.04;
/// The car's contact shadow (ours, [`Enhance::contact`](super::Enhance::contact), in place of the game's shadow decal,
/// `crate::car_shadow`): a second layer of the map, the same draws seen from straight above. The ground the car is
/// over, within `GROUND_REACH` metres below it, darkens by `Car_Deep_Shadow_Alpha` (1 where the section has none, the
/// game's default) times `GROUND_SCALE`, half as much from the near to the far reach. Filtered 6x6 with `GROUND_BLUR`
/// metre steps, a soft blob about the car's footprint.
pub(super) const GROUND_REACH: [f32; 2] = [0.6, 1.4];
pub(super) const GROUND_SCALE: f32 = 0.7;
/// The contact shadow's darkening in the in-car view, where the road it falls on is a sliver past the bonnet: the game's
/// look is kept there.
pub(super) const GROUND_SCALE_COCKPIT: f32 = 0.4;
pub(super) const GROUND_BLUR: f32 = 0.08;

/// How much the sun's shadow darkens the stage (`sun_light` in `shader.wgsl`): about the baked light maps' shadows.
pub(super) const SUN_SHADE: f32 = 0.45;
/// Under an overcast sky ([`Renderer::sunless`]) the cascades hold the light from overhead instead ([`Renderer::shadow_light`]):
/// how much it darkens, and its filter step in texels, wide for a soft shadow under trees and things.
pub(super) const OVERCAST_SHADE: f32 = 0.35;
pub(super) const OVERCAST_BLUR: f32 = 6.0;

/// The headlights' shadow map (at night, [`Atmosphere::night`](crate::atmosphere::Atmosphere::night), drawn into the
/// sun's first cascade): its field of view across and up, radians, and its near and far planes, metres.
const LAMP_SHADOW_FOV: [f32; 2] = [2.0, 1.0];
const LAMP_SHADOW_RANGE: [f32; 2] = [0.3, 160.0];

impl Renderer {
    /// The clip matrices of the car shadow map's layers: orthographic views along the sun and straight down over the
    /// bounds of the meshes with a casting draw, or `None` when nothing casts (a stage without a car).
    pub(super) fn shadow_matrices(&self) -> Option<[Mat4; 2]> {
        let (low, high) = self.car_bounds()?;
        let center = (low + high) * 0.5;
        let radius = (high - low).length() * 0.5 + SHADOW_MARGIN;
        let sun = self.atmosphere.sun_direction;
        let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let along = |direction: Vec3, up: Vec3, depth: f32| {
            directx::orthographic(-radius, radius, -radius, radius, 0.0, depth)
                * view::look_to_mat4(center - direction * radius, direction, up)
        };
        Some([along(sun, up, 2.0 * radius), along(Vec3::NEG_Y, Vec3::Z, 2.0 * radius + GROUND_REACH[1])])
    }

    /// [`Renderer::car_bounds`] when they hold everything the car's shadow maps draw (every casting mesh has bounds),
    /// else the whole stage.
    pub(super) fn car_box(&self) -> Option<(Vec3, Vec3)> {
        match self.casting().all(|mesh| mesh.bounds.is_some()) {
            true => self.car_bounds(),
            false => Some((Vec3::splat(f32::MIN), Vec3::splat(f32::MAX))),
        }
    }

    /// The low and high corners of the meshes with a casting draw (the car), or `None` without a car.
    pub(super) fn car_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.casting()
            .filter_map(|mesh| mesh.bounds)
            .map(|(center, half)| (center - half, center + half))
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    }

    /// The sun's maps' quality ([`sun::Quality`]).
    pub(super) fn sun_quality(&self) -> sun::Quality {
        sun::Quality::of(&self.enhance)
    }

    /// The direction the light the cascades hold travels: the sun's, or under an overcast sky ([`Renderer::sunless`]) the sky's
    /// from overhead, leaning a little the sun's way, for a soft shadow under trees and things.
    pub(super) fn shadow_light(&self) -> Vec3 {
        let sun = self.atmosphere.sun_direction;
        if self.sunless() {
            (Vec3::NEG_Y + 0.35 * sun).normalize()
        } else {
            sun
        }
    }

    /// The headlights' shadow map's clip matrix: a perspective view from between the lit headlights along the car, or
    /// `None` when none is lit.
    pub(super) fn lamp_shadow_matrix(&self) -> Option<Mat4> {
        let beams: Vec<&LampUniform> =
            self.lamps.iter().filter(|l| l.position[3] != 0.0 && l.forward[3] != 0.0).collect();
        if beams.is_empty() {
            return None;
        }
        let mean = |f: fn(&LampUniform) -> [f32; 4]| {
            beams.iter().map(|&l| Vec4::from(f(l)).truncate()).sum::<Vec3>() / beams.len() as f32
        };
        let (centre, forward, up) = (mean(|l| l.position), mean(|l| l.forward), mean(|l| l.up));
        let [across, high] = LAMP_SHADOW_FOV;
        let aspect = (across * 0.5).tan() / (high * 0.5).tan();
        let projection = directx::perspective(high, aspect, LAMP_SHADOW_RANGE[0], LAMP_SHADOW_RANGE[1]);
        Some(projection * view::look_to_mat4(centre, forward.normalize(), up.normalize()))
    }
}

/// What the sun's map draws from ([`Casting::draw_layer`]): borrowed from the renderer apart from the rest of it, so
/// that it can be recorded on another thread.
pub(super) struct Casting<'a> {
    pub(super) meshes: &'a [GpuMesh],
    pub(super) buffers: &'a [MeshBuffers],
    pub(super) sun: &'a SunShadow,
    pub(super) queue: &'a wgpu::Queue,
}

/// The sun's map's layers a frame draws ([`Casting::draw`]).
pub(super) struct Layers {
    /// The cascades ([`Renderer::cascades`]).
    pub(super) cascades: Option<Cascades>,
    /// The far layer, when it is due ([`crate::sun::FarDrawn`]).
    pub(super) far: Option<Mat4>,
    /// The headlights' map.
    pub(super) lamp: Option<Mat4>,
    /// The world's, when the light has changed ([`crate::sun::world`]).
    pub(super) world: Option<Mat4>,
    /// Where the level of detail is seen from.
    pub(super) eye: Vec3,
    /// How far the backdrop's casters in the sun's maps move ([`crate::sun::backdrop_drop`]).
    pub(super) drop: Vec3,
}

impl Casting<'_> {
    /// Draws `layers`: into the sun's cascades and far and world layers, the world-anchored stage draws that write
    /// depth and do not blend, at the level of detail the view from the eye shows, the characters among them, and not
    /// the car (its own shadow covers it); the same into the headlights' map. Calls `around` with the encoder and each
    /// layer's label before (`true`) and after (`false`) it (`--profile` times them).
    pub(super) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        layers: &Layers,
        mut around: impl FnMut(&mut wgpu::CommandEncoder, &'static str, bool),
    ) {
        const CASCADES: [&str; 3] = ["sun cascade 1", "sun cascade 2", "sun cascade 3"];
        let &Layers { cascades, far, lamp, world, eye, drop } = layers;
        let mut drawn = Vec::new();
        if let Some(cascades) = cascades {
            drawn.extend(cascades.matrices.iter().enumerate().map(|(i, &m)| (CASCADES[i], i, m, Some(eye), drop)));
        }
        drawn.extend(far.map(|m| ("sun far layer", crate::sun::FAR, m, Some(eye), drop)));
        drawn.extend(lamp.map(|m| ("lamp shadow", crate::sun::LAMP, m, Some(eye), Vec3::ZERO)));
        drawn.extend(world.map(|m| ("sun world layer", crate::sun::WORLD, m, None, drop)));
        for (label, index, matrix, eye, drop) in drawn {
            around(encoder, label, true);
            self.draw_layer(encoder, index, matrix, eye, drop);
            around(encoder, label, false);
        }
    }

    /// Clears layer `index` of the sun's map and draws into it, through `matrix`, the world-anchored stage draws that
    /// write depth (or, the backdrop's, are opaque) and do not blend, at the level of detail seen from `eye` (the
    /// nearest without one), the characters' among them, and not the car's; the backdrop's moved by `drop`.
    pub(super) fn draw_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        matrix: Mat4,
        eye: Option<Vec3>,
        drop: Vec3,
    ) {
        self.draw_layer_of(encoder, index, matrix, eye, drop, |_| true);
    }

    /// [`Casting::draw_layer`] over the meshes `keep` names alone.
    pub(super) fn draw_layer_of(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        matrix: Mat4,
        eye: Option<Vec3>,
        drop: Vec3,
        keep: impl Fn(&GpuMesh) -> bool,
    ) {
        let frustum = Frustum::from_view_projection(matrix);
        let mut casters = Vec::new();
        for mesh in self.meshes.iter().filter(|mesh| {
            mesh.visible && mesh.anchor == Anchor::World && mesh.in_car.is_none() && !mesh.sky_dome && keep(mesh)
        }) {
            if mesh.bounds.is_some_and(|(centre, half)| !frustum.intersects_box(centre, half)) {
                continue;
            }
            // The backdrop's casters stay on the map's near and far planes (`vs_sun`): only its sides cull them.
            if mesh.backdrop_bounds.is_some_and(|(centre, half)| beside(matrix, centre + drop, half)) {
                continue;
            }
            let distance = mesh.bounds.zip(eye).map_or(0.0, |((centre, _), eye)| centre.distance(eye));
            casters.extend(
                mesh.draws
                    .iter()
                    .filter(|draw| {
                        // The grass's tiles (`Lod::Close`) cast nothing (`vs_sun`): left out, not drawn off the map.
                        draw.lod.shows(distance)
                            && draw.lod != Lod::Close
                            && (draw.pipeline.depth_write || mesh.layer == Layer::Sky)
                            && !draw.pipeline.blend
                            && (draw.bind.shade.is_none() || !mesh.car)
                            && !draw.bind.water
                    })
                    .map(|draw| (mesh, draw)),
            );
        }
        // Depth only, so the order does not matter: the uncut draws first, with one bind group (they read nothing from
        // theirs but the mode's zero cut-out flags) and no fragment stage, then the cut-out ones by bind group.
        let cut = |draw: &GpuDraw| draw.bind.alpha_ref.is_some() || draw.bind.cutout;
        casters.sort_by(|(_, a), (_, b)| {
            cut(a).cmp(&cut(b)).then_with(|| match cut(a) {
                true => a.group.cmp(&b.group),
                false => std::cmp::Ordering::Equal,
            })
        });
        self.sun.draw_cascade(self.queue, encoder, index, matrix, drop, |pass, [uncut_pipeline, cut_pipeline]| {
            let mut bound = Bound::new(self.buffers);
            // Whether the pipeline set is the cut-out one, and whether an uncut draw's group is bound.
            let (mut cut_set, mut uncut_bound) = (None, false);
            for (mesh, draw) in casters {
                let cuts = cut(draw);
                if cut_set != Some(cuts) {
                    pass.set_pipeline(if cuts { cut_pipeline } else { uncut_pipeline });
                    cut_set = Some(cuts);
                }
                if cuts || !uncut_bound {
                    pass.set_bind_group(1, &draw.group, &[]);
                    uncut_bound = !cuts;
                }
                bound.draw_instance(pass, mesh, draw, u32::from(mesh.layer == Layer::Sky));
            }
        });
    }
}

/// Whether the box about `centre` with half extents `half` lies wholly to one side of the map `matrix` (past its clip x
/// or y), whatever its depth; never for a box reaching behind a perspective map's eye.
fn beside(matrix: Mat4, centre: Vec3, half: Vec3) -> bool {
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    for k in 0..8 {
        let corner = Vec3::select(glam::BVec3::new(k & 1 == 0, k & 2 == 0, k & 4 == 0), -half, half);
        let clip = matrix * (centre + corner).extend(1.0);
        if clip.w <= 0.0 {
            return false;
        }
        let p = clip.truncate() / clip.w;
        (low, high) = (low.min(p), high.max(p));
    }
    high.x < -1.0 || low.x > 1.0 || high.y < -1.0 || low.y > 1.0
}

/// The car's shadow map ([`SHADOW_SIZE`]): its view, a view of each of its two layers (the sun's and the contact
/// shadow's), and the comparison sampler that reads it.
pub(super) fn shadow_map(device: &wgpu::Device) -> (wgpu::TextureView, [wgpu::TextureView; 2], wgpu::Sampler) {
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("car shadow"),
        size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 2 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let shadow_layers = [0, 1].map(|layer| {
        shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("car shadow"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    (shadow_view, shadow_layers, shadow_sampler)
}

/// The uv rectangle (low x and y, high x and y) of the car's shadow map `matrix` that the box `bounds` covers, widened
/// by `reach`: beyond it a filter reaching `reach` reads only the cleared map, so the shader skips it (the same
/// result).
pub(super) fn map_box(matrix: Mat4, (low, high): (Vec3, Vec3), reach: f32) -> [f32; 4] {
    let mut uv_box = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for corner in 0..8 {
        let pick = |bit: u32, low: f32, high: f32| if corner & bit == 0 { low } else { high };
        let point = Vec3::new(pick(1, low.x, high.x), pick(2, low.y, high.y), pick(4, low.z, high.z));
        let clip = matrix.project_point3(point);
        let uv = [clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5];
        uv_box = [uv_box[0].min(uv[0]), uv_box[1].min(uv[1]), uv_box[2].max(uv[0]), uv_box[3].max(uv[1])];
    }
    [uv_box[0] - reach, uv_box[1] - reach, uv_box[2] + reach, uv_box[3] + reach]
}

/// The steps of the car shadow's filter and of the contact shadow's (`frame.shadow.z`, `frame.ground.y`), in the map's
/// uv, for its view `shadow_matrix` (both layers' views are as wide).
pub(super) fn filter_steps(shadow_matrix: Mat4) -> [f32; 2] {
    let width = 2.0 / shadow_matrix.x_axis.truncate().length();
    [SHADOW_PENUMBRA / width, GROUND_BLUR / width]
}

/// The depth range pass's shader (`shadow_range.wgsl`).
pub(crate) const RANGE_SHADER: &str = include_str!("../shadow_range.wgsl");
/// Texels a side of [`ShadowRange`]'s cells (`RANGE_CELL` in `shadow_range.wgsl` and `shader.wgsl`).
const RANGE_CELL: u32 = 4;

/// A shadow map's least and greatest depth about each [`RANGE_CELL`] texels square of its first `layers` layers
/// (`shadow_range.wgsl`, ours), redrawn with the map: over every texel a filter centred in the cell reads. Where the
/// compare's depth is before all of them every tap passes, past all of them every tap fails, and the filter skips its
/// compares for the same result. The cells' own ranges, then spread along x into `spread` and along y back.
///
/// The car's map ([`ShadowRange::new`]): its 4 x 4 taps 1.5 steps each side (`car_shadow`) or its 6 x 6 2.5 steps
/// (`car_contact`), 16 or up to 108 compares. The sun's first cascade ([`ShadowRange::sun`]): the soft filter's search
/// and up to 9 x 9 taps (`soft_filter`).
pub(super) struct ShadowRange {
    pipelines: [wgpu::ComputePipeline; 3],
    groups: [wgpu::BindGroup; 3],
    reach: wgpu::Buffer,
    layers: u32,
    cells: u32,
    pub(super) view: wgpu::TextureView,
}

impl ShadowRange {
    pub(super) fn new(device: &wgpu::Device, shadow_view: &wgpu::TextureView) -> ShadowRange {
        ShadowRange::with_layers(device, shadow_view, "car shadow range", (SHADOW_SIZE, 2))
    }

    /// The ranges of the sun's first cascade, whose soft filter reads at most `quality.soft_texels` from a pixel's
    /// (`SOFT_TEXELS` in `sun_mask.wgsl`).
    pub(super) fn sun(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sun_view: &wgpu::TextureView,
        quality: sun::Quality,
    ) -> ShadowRange {
        let range = ShadowRange::with_layers(device, sun_view, "sun range", (quality.size, 1));
        // The filter's taps and the next texel (bilinear) and one more each side for the rounding, from anywhere in
        // the cell.
        let cells = ((quality.soft_texels + 2.0) / RANGE_CELL as f32).ceil() as u32 + 1;
        queue.write_buffer(&range.reach, 0, bytemuck::cast_slice(&[cells, 0, 0, 0]));
        range
    }

    fn with_layers(
        device: &wgpu::Device,
        shadow_view: &wgpu::TextureView,
        label: &'static str,
        (size, layers): (u32, u32),
    ) -> ShadowRange {
        let cells = size / RANGE_CELL;
        let [view, spread] = [label, "range spread"].map(|label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: cells, height: cells, depth_or_array_layers: layers },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rg32Float,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                })
        });
        let reach = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(RANGE_SHADER.into()),
        });
        let pipelines = ["cs_cells", "cs_spread_x", "cs_spread_y"].map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        });
        let texture =
            |binding, view| wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(view) };
        let group = |pipeline: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry]| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &pipeline.get_bind_group_layout(0),
                entries,
            })
        };
        let reach_entry = wgpu::BindGroupEntry { binding: 4, resource: reach.as_entire_binding() };
        let groups = [
            group(&pipelines[0], &[texture(0, shadow_view), texture(1, &view)]),
            group(&pipelines[1], &[texture(2, &view), texture(3, &spread), reach_entry.clone()]),
            group(&pipelines[2], &[texture(2, &spread), texture(3, &view), reach_entry]),
        ];
        ShadowRange { pipelines, groups, reach, layers, cells, view }
    }

    /// Redraws the car's ranges from the map as it now is, drawn with view `shadow_matrix`.
    pub(super) fn update(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, shadow_matrix: Mat4) {
        // The cells a reach spans each side: a cell's centre filter reads its taps' texels and the next (bilinear) and
        // one more each side for the rounding, from anywhere in the cell.
        let [shadow, ground] = filter_steps(shadow_matrix);
        let span = |reach: f32| {
            let texels = reach * SHADOW_SIZE as f32 + 2.0;
            if texels.is_finite() {
                (texels / RANGE_CELL as f32).ceil() as u32 + 1
            } else {
                0
            }
        };
        let reach = [span(1.5 * shadow), span(2.5 * ground), 0, 0].map(|cells| cells.min(self.cells));
        queue.write_buffer(&self.reach, 0, bytemuck::cast_slice(&reach));
        self.draw(encoder);
    }

    /// Redraws the ranges from the map as it now is.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("shadow range"), timestamp_writes: None });
        let groups = self.cells.div_ceil(8);
        for (pipeline, group) in self.pipelines.iter().zip(&self.groups) {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(groups, groups, self.layers);
        }
    }
}
