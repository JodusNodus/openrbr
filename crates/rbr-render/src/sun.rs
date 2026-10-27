//! Real-time sun shadows on the stage (`sun.wgsl`, sampled by `sun_light` in `shader.wgsl`): a renderer addition
//! behind [`crate::renderer::Enhance`], not the game's look. The stages' baked light maps hold the sun's shadows on
//! the ground chunks only, at one atlas per stage, for the bake's sun; these cascades cover the near [`SPLITS`] metres
//! at full resolution, cast by every opaque and alpha-tested stage draw (so foliage dapples the light) and received by
//! every stage draw. A fourth layer ([`far`]) holds the stage about the eye out to [`FAR_RADIUS`] at about a metre a
//! texel (0.3 ms a frame at 1080p on Semetin's 3.4 million vertices): with it the baked map is not used at all, so the
//! shadows follow any sun.

use crate::renderer::{Detail, Enhance};
use crate::stage::{Mesh, Vertex};
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec3};

pub(crate) const SHADER: &str = include_str!("sun.wgsl");
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Texels across each cascade.
pub(crate) const SIZE: u32 = 2048;
/// The far distance of each cascade from the eye, metres (the first starts at the eye).
pub(crate) const SPLITS: [f32; 3] = [14.0, 45.0, 150.0];
/// How far toward the sun each cascade reaches past its slice, metres, for casters outside the view (trees and
/// banks between the sun and the road).
const BACK: f32 = 250.0;
/// The far layer's half width about the eye, metres (a texel about a metre), and how far toward the sun it reaches
/// for casters (ridges) outside it.
pub(crate) const FAR_RADIUS: f32 = 1000.0;
const FAR_BACK: f32 = 1500.0;
/// How far from the eye the far layer's shadows reach, fading out over the last fifth (`sun_reach`).
pub(crate) const FAR_REACH: f32 = 0.9 * FAR_RADIUS;
/// The far layer's index in the array, the car's headlights' (`Renderer::lamp_shadow_matrix`) and the world's
/// ([`world`]).
pub(crate) const FAR: usize = 3;
pub(crate) const LAMP: usize = 4;
pub(crate) const WORLD: usize = 5;
const LAYERS: usize = 6;

/// What the sun's maps are drawn at: the texels across each layer, and the soft shadows' search and filter in texels
/// at most (`sun_mask.wgsl`). [`Detail::Ultra`] doubles the texels (the soft shadows' reach in metres the same).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quality {
    pub(crate) size: u32,
    pub(crate) search_texels: f32,
    pub(crate) soft_texels: f32,
}

impl Quality {
    /// The maps' quality for `enhance`: [`Detail::Ultra`] counts with the sun's shadows.
    pub(crate) fn of(enhance: &Enhance) -> Quality {
        if enhance.sun && enhance.detail == Detail::Ultra {
            Quality { size: 2 * SIZE, search_texels: 32.0, soft_texels: 8.0 }
        } else {
            Quality { size: SIZE, search_texels: 16.0, soft_texels: 4.0 }
        }
    }
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    matrix: [[f32; 4]; 4],
    drop: [f32; 4],
}

/// How much lower the backdrop's casters sit in the sun's maps and the sky's occlusion for the stage, metres: where
/// the data lays the backdrop over the stage's ground (a sea sheet 120 to 200 m over the stage's tiles at Capo di Feno
/// 369, which it shaded black), it casts nothing on it.
pub(crate) const BACKDROP_DROP: f32 = 200.0;

/// Where the backdrop's casters move in a map of sunlight travelling along `sun`: [`BACKDROP_DROP`] metres lower along
/// the ray, at most [`BACKDROP_DROP`] times 2 along it under a low sun.
pub(crate) fn backdrop_drop(sun: Vec3) -> Vec3 {
    let sun = sun.normalize();
    sun * (BACKDROP_DROP / (-sun.y).max(0.01)).min(2.0 * BACKDROP_DROP)
}

/// The cascades' clip matrices for one view, and the world size of a texel in each; and the far layer's ([`far`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cascades {
    pub(crate) matrices: [Mat4; 3],
    pub(crate) texels: [f32; 3],
    pub(crate) far: Mat4,
    pub(crate) far_texel: f32,
}

pub(crate) struct SunShadow {
    /// The three cascades, the far layer and the headlights' map as an array, for the stage draws to sample.
    pub(crate) view: wgpu::TextureView,
    layers: Vec<wgpu::TextureView>,
    uniforms: Vec<wgpu::Buffer>,
    groups: Vec<wgpu::BindGroup>,
    /// For the draws without a cut-out: no fragment stage. Then for those with one: `fs_sun` drops the cut texels.
    pipelines: [wgpu::RenderPipeline; 2],
}

impl SunShadow {
    /// The cascades and the pipeline that draws stage draws (bound with `texture_layout`, the stage's) into them.
    pub(crate) fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout, quality: Quality) -> SunShadow {
        SunShadow::with_layers(device, texture_layout, quality.size, LAYERS)
    }

    /// `layers` depth layers of `size` texels across, drawn as the cascades are (the sky's occlusion,
    /// [`crate::sky_occlusion`]).
    pub(crate) fn with_layers(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        size: u32,
        layers: usize,
    ) -> SunShadow {
        let count = layers;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: count as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layers = (0..count as u32)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun cascade"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniforms: Vec<wgpu::Buffer> = (0..count)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sun cascade"),
                    size: std::mem::size_of::<Uniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let groups = uniforms
            .iter()
            .map(|uniform| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sun cascade"),
                    layout: &layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sun shadow"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow"),
            bind_group_layouts: &[Some(&layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |fragment: Option<&str>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sun shadow"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_sun"),
                    compilation_options: Default::default(),
                    buffers: &[Some(crate::renderer::vertex_layout())],
                },
                // Foliage cards and terrain seen from below both cast.
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.5, clamp: 0.0 },
                }),
                multisample: Default::default(),
                fragment: fragment.map(|entry| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = [pipeline(None), pipeline(Some("fs_sun"))];
        SunShadow { view, layers, uniforms, groups, pipelines }
    }

    /// Clears cascade `index` and draws into it, through `matrix`, the backdrop's draws (instance 1) moved by `drop`:
    /// `draw` gets the pass with the cascade bound and the two pipelines (without and with the cut-out), and sets the
    /// pipeline, the vertex buffers and the stage bind group.
    pub(crate) fn draw_cascade(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        matrix: Mat4,
        drop: Vec3,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>, [&wgpu::RenderPipeline; 2]),
    ) {
        let uniform = Uniform { matrix: matrix.to_cols_array_2d(), drop: drop.extend(0.0).to_array() };
        queue.write_buffer(&self.uniforms[index], 0, bytes_of(&uniform));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sun shadow"),
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.layers[index],
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.groups[index], &[]);
        draw(&mut pass, self.pipelines.each_ref());
    }
}

/// A one-texel cascade array to bind when the sun is off.
pub(crate) fn placeholder(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow (off)"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
}

/// The cascades for the view `view_projection` and sunlight travelling along `sun`. Each covers the bounding sphere
/// of its slice of the view, so its size does not change as the view turns, and its centre moves in whole texels, so
/// shadow edges do not crawl as the eye moves.
pub(crate) fn cascades(view_projection: Mat4, sun: Vec3, quality: Quality) -> Cascades {
    let inverse = view_projection.inverse();
    let eye = crate::camera::eye(view_projection);
    // The four corner rays of the view and its forward axis.
    let corner = |x: f32, y: f32| {
        let near = inverse.project_point3(Vec3::new(x, y, 1.0));
        let far = inverse.project_point3(Vec3::new(x, y, 0.0));
        (far - near).normalize()
    };
    let rays = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0)];
    let forward = (rays[0] + rays[1] + rays[2] + rays[3]).normalize();
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let light = view::look_to_mat4(Vec3::ZERO, sun, up);
    let mut near = 0.0;
    let mut matrices = [Mat4::IDENTITY; 3];
    let mut texels = [0.0; 3];
    for (i, &far) in SPLITS.iter().enumerate() {
        let points: Vec<Vec3> = rays
            .iter()
            .flat_map(|&ray| [near, far].map(|distance| eye + ray * (distance / ray.dot(forward))))
            .collect();
        let centre = points.iter().copied().sum::<Vec3>() / points.len() as f32;
        let radius = points.iter().map(|p| p.distance(centre)).fold(0.0, f32::max).ceil();
        let texel = 2.0 * radius / quality.size as f32;
        let snapped = light.transform_point3(centre);
        let snapped = Vec3::new((snapped.x / texel).floor() * texel, (snapped.y / texel).floor() * texel, snapped.z);
        let centre = light.inverse().transform_point3(snapped);
        matrices[i] = directx::orthographic(-radius, radius, -radius, radius, 0.0, 2.0 * radius + BACK)
            * view::look_to_mat4(centre - sun * (radius + BACK), sun, up);
        texels[i] = texel;
        near = far;
    }
    let (far, far_texel) = self::far(eye, sun, quality.size);
    Cascades { matrices, texels, far, far_texel }
}

/// The far layer's clip matrix and texel (metres) for the eye at `eye` and light travelling along `sun`: a square
/// [`FAR_RADIUS`] about the eye, its centre in whole texels.
fn far(eye: Vec3, sun: Vec3, size: u32) -> (Mat4, f32) {
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let light = view::look_to_mat4(Vec3::ZERO, sun, up);
    let texel = 2.0 * FAR_RADIUS / size as f32;
    let snapped = light.transform_point3(eye);
    let snapped = Vec3::new((snapped.x / texel).floor() * texel, (snapped.y / texel).floor() * texel, snapped.z);
    let centre = light.inverse().transform_point3(snapped);
    let matrix =
        directx::orthographic(-FAR_RADIUS, FAR_RADIUS, -FAR_RADIUS, FAR_RADIUS, 0.0, 2.0 * FAR_RADIUS + FAR_BACK)
            * view::look_to_mat4(centre - sun * (FAR_RADIUS + FAR_BACK), sun, up);
    (matrix, texel)
}

/// The world layer's clip matrix and texel (metres, the wider side's) for light travelling along `sun` over the box
/// `low` to `high`, the stage and its backdrop: the box's outline seen from the sun. It holds the shade of what is far
/// off (ridges, the backdrop's mountains), drawn once for each light (`world_lit` in `shader.wgsl`).
pub(crate) fn world((low, high): (Vec3, Vec3), sun: Vec3, size: u32) -> (Mat4, f32) {
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let light = view::look_to_mat4(Vec3::ZERO, sun, up);
    let (mut near, mut far) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for corner in 0..8 {
        let pick = |bit: u32, low: f32, high: f32| if corner & bit == 0 { low } else { high };
        let point =
            light.transform_point3(Vec3::new(pick(1, low.x, high.x), pick(2, low.y, high.y), pick(4, low.z, high.z)));
        (near, far) = (near.min(point), far.max(point));
    }
    let matrix = directx::orthographic(near.x, far.x, near.y, far.y, 0.0, far.z - near.z)
        * Mat4::from_translation(Vec3::Z * -near.z)
        * light;
    (matrix, (far.x - near.x).max(far.y - near.y) / size as f32)
}

/// `mesh`'s vertices with smooth normals (area-weighted over the triangles that share each vertex) wherever the
/// stored normal is zero (the stage draws whose light is baked), for the sun's light and shadow offsets.
pub(crate) fn with_normals(mesh: &Mesh) -> Vec<Vertex> {
    let mut vertices = mesh.vertices.clone();
    let missing: Vec<bool> = vertices.iter().map(|v| v.normal == [0.0; 3]).collect();
    if !missing.contains(&true) {
        return vertices;
    }
    // The opaque draws' vertices at one position share a normal (the meshes split them at texture seams), so the
    // light does not break along the seams. Not the alpha-tested ones: a two-sided card's normals would cancel.
    let mut solid = vec![false; vertices.len()];
    for draw in mesh.draws.iter().filter(|d| d.state.alpha_ref.is_none()) {
        for &i in &mesh.indices[draw.indices.clone()] {
            solid[i as usize] = true;
        }
    }
    let mut welded = std::collections::HashMap::new();
    let slot: Vec<usize> = vertices
        .iter()
        .zip(&solid)
        .enumerate()
        .map(|(i, (v, &solid))| {
            let key = if solid { (v.position.map(|c| (c * 100.0).round() as i32), 0) } else { ([0; 3], i + 1) };
            let next = welded.len();
            *welded.entry(key).or_insert(next)
        })
        .collect();
    let mut sums = vec![Vec3::ZERO; welded.len()];
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|k| triangle[k] as usize);
        let (Some(pa), Some(pb), Some(pc)) = (vertices.get(a), vertices.get(b), vertices.get(c)) else {
            continue;
        };
        let [pa, pb, pc] = [pa, pb, pc].map(|v| Vec3::from(v.position));
        // Twice the area along the face normal.
        let normal = (pb - pa).cross(pc - pa);
        for k in [a, b, c] {
            sums[slot[k]] += normal;
        }
    }
    for ((vertex, slot), missing) in vertices.iter_mut().zip(slot).zip(missing) {
        let sum = sums[slot];
        if missing {
            // Left-handed with clockwise front faces (`coords::FRONT_FACE`): the cross product points out of the front.
            vertex.normal = sum.normalize_or_zero().to_array();
        }
    }
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backdrop_drops_along_the_ray_to_its_drop_in_height() {
        let drop = backdrop_drop(Vec3::new(0.3, -0.8, 0.2));
        assert!((-drop.y - BACKDROP_DROP).abs() < 1e-3);
        // A low sun's reach stops short of the drop.
        assert!((backdrop_drop(Vec3::new(1.0, -0.05, 0.0)).length() - 2.0 * BACKDROP_DROP).abs() < 1e-3);
    }
}
