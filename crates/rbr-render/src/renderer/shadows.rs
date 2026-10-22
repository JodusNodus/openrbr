//! The car's sun and contact shadow maps, the sun's cascades' casters and the headlights' shadow.

use super::meshes::{Bound, GpuDraw, GpuMesh, MeshBuffers};
use super::uniforms::LampUniform;
use super::{Renderer, DEPTH_FORMAT};
use crate::camera::Frustum;
use crate::stage::{Anchor, Layer};
use crate::sun::{Cascades, SunShadow};
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
/// The width of one step of the shadow's 4x4 filter, metres: a soft edge about 15 cm wide.
pub(super) const SHADOW_PENUMBRA: f32 = 0.04;
/// The car's contact shadow (ours, [`Enhance::contact`](super::Enhance::contact), in place of the game's shadow decal,
/// `crate::car_shadow`): a second layer of the map, the same draws seen from straight above. The ground the car is
/// over, within `GROUND_REACH` metres below it, darkens by `Car_Deep_Shadow_Alpha` (1 where the section has none, the
/// game's default) times `GROUND_SCALE`, half as much from the near to the far reach. Filtered 6x6 with `GROUND_BLUR`
/// metre steps, a soft blob about the car's footprint.
pub(super) const GROUND_REACH: [f32; 2] = [0.6, 1.4];
pub(super) const GROUND_SCALE: f32 = 0.4;
pub(super) const GROUND_BLUR: f32 = 0.08;

/// How much the sun's shadow darkens the stage (`sun_shade` in `shader.wgsl`): about the baked light maps' shadows.
pub(super) const SUN_SHADE: f32 = 0.45;
/// Under an overcast sky (`UseFog`) the cascades hold the light from overhead instead ([`Renderer::shadow_light`]):
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
        let casting = self.meshes.iter().filter(|mesh| mesh.draws.iter().any(|draw| draw.casts_shadow));
        match casting.clone().all(|mesh| mesh.bounds.is_some()) {
            true => self.car_bounds(),
            false => Some((Vec3::splat(f32::MIN), Vec3::splat(f32::MAX))),
        }
    }

    /// The low and high corners of the meshes with a casting draw (the car), or `None` without a car.
    pub(super) fn car_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.meshes
            .iter()
            .filter(|mesh| mesh.draws.iter().any(|draw| draw.casts_shadow))
            .filter_map(|mesh| mesh.bounds)
            .map(|(center, half)| (center - half, center + half))
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    }

    /// The direction the light the cascades hold travels: the sun's, or under an overcast sky (`UseFog`) the sky's
    /// from overhead, leaning a little the sun's way, for a soft shadow under trees and things.
    pub(super) fn shadow_light(&self) -> Vec3 {
        let sun = self.atmosphere.sun_direction;
        if self.atmosphere.use_fog {
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

impl Casting<'_> {
    /// Draws the sun's cascades ([`Renderer::cascades`]): the world-anchored stage draws that write depth and do not
    /// blend, at the level of detail the view from `eye` shows, the characters among them, and not the car (its own
    /// shadow covers it), and their far layer; and the headlights' map through `lamp` into its own layer.
    pub(super) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cascades: Option<&Cascades>,
        lamp: Option<Mat4>,
        eye: Vec3,
    ) {
        for (index, &matrix) in cascades.iter().flat_map(|cascades| cascades.matrices.iter().enumerate()) {
            self.draw_layer(encoder, index, matrix, eye);
        }
        if let Some(cascades) = cascades {
            self.draw_layer(encoder, crate::sun::FAR, cascades.far, eye);
        }
        if let Some(matrix) = lamp {
            self.draw_layer(encoder, crate::sun::LAMP, matrix, eye);
        }
    }

    /// Clears layer `index` of the sun's map and draws into it, through `matrix`, the world-anchored stage draws that
    /// write depth and do not blend, at the level of detail seen from `eye`, the characters' among them, and not the
    /// car's.
    fn draw_layer(&self, encoder: &mut wgpu::CommandEncoder, index: usize, matrix: Mat4, eye: Vec3) {
        let frustum = Frustum::from_view_projection(matrix);
        let mut casters = Vec::new();
        for mesh in
            self.meshes.iter().filter(|mesh| mesh.visible && mesh.anchor == Anchor::World && mesh.in_car.is_none())
        {
            if mesh.layer == Layer::Sky
                || mesh.bounds.is_some_and(|(centre, half)| !frustum.intersects_box(centre, half))
            {
                continue;
            }
            let distance = mesh.bounds.map_or(0.0, |(centre, _)| centre.distance(eye));
            casters.extend(
                mesh.draws
                    .iter()
                    .filter(|draw| {
                        draw.lod.shows(distance)
                            && draw.pipeline.depth_write
                            && !draw.pipeline.blend
                            && (draw.bind.shade.is_none() || !mesh.car)
                            && !draw.bind.water
                    })
                    .map(|draw| (mesh, draw)),
            );
        }
        // Depth only, so the order does not matter: the uncut draws first, with one bind group (they read nothing from
        // theirs but the mode's zero cut-out flags) and no fragment stage, then the cut-out ones by bind group.
        let cut = |draw: &GpuDraw| draw.bind.alpha_ref.is_some() || draw.bind.coverage;
        casters.sort_by(|(_, a), (_, b)| {
            cut(a).cmp(&cut(b)).then_with(|| match cut(a) {
                true => a.group.cmp(&b.group),
                false => std::cmp::Ordering::Equal,
            })
        });
        self.sun.draw_cascade(self.queue, encoder, index, matrix, |pass, [uncut_pipeline, cut_pipeline]| {
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
                bound.draw(pass, mesh, draw);
            }
        });
    }
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
