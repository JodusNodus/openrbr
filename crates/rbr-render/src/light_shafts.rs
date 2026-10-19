//! Light shafts (`light_shafts.wgsl`; ours, a renderer addition under [`crate::renderer::Enhance::sky`] with the HDR
//! pass): by day, with the sun in or near the view, the sky's light near the sun is blurred along the lines toward it
//! into the map the post pass's composite adds to the scene ([`crate::post::Post::shafts`]), so the sun shines through
//! the canopy and the cloud gaps in shafts.

use crate::atmosphere::Atmosphere;
use crate::post::SCENE_FORMAT;
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{Mat4, Vec2};

pub(crate) const SHADER: &str = include_str!("light_shafts.wgsl");
/// How strongly the shafts add to the scene, at most (ours; kept subtle).
const STRENGTH: f32 = 0.2;
/// How far out of the frame (in frame heights) the sun still casts shafts into it, fading to none there.
const OUT_OF_VIEW: f32 = 0.35;
/// The sun's true elevation, degrees, over which the shafts fade in after sunrise.
const RISEN: [f32; 2] = [0.0, 4.0];

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    sun: [f32; 4],
}

/// The sun's place in the frame of the rotation-only `sky_view_projection` (uv, y down) and how strongly it casts
/// shafts (0 to 1): by day under a clear section, in front of the eye, and in or within [`OUT_OF_VIEW`] of the frame.
pub(crate) fn sun_in_view(sky_view_projection: Mat4, atmosphere: &Atmosphere, aspect: f32) -> Option<(Vec2, f32)> {
    if atmosphere.night || atmosphere.use_fog {
        return None;
    }
    let elevation = atmosphere.sun_elevation.unwrap_or_else(|| (-atmosphere.sun_direction.y).asin().to_degrees());
    let risen = ((elevation - RISEN[0]) / (RISEN[1] - RISEN[0])).clamp(0.0, 1.0);
    let clip = sky_view_projection * (-atmosphere.sun_direction).extend(0.0);
    if risen <= 0.0 || clip.w <= 1e-4 {
        return None;
    }
    let uv = Vec2::new(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5);
    // How far outside the frame, in frame heights.
    let outside = ((uv - uv.clamp(Vec2::ZERO, Vec2::ONE)) * Vec2::new(aspect, 1.0)).length();
    let strength = risen * (1.0 - outside / OUT_OF_VIEW).clamp(0.0, 1.0);
    (strength > 0.0).then_some((uv, strength))
}

pub(crate) struct LightShafts {
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// The mask and the blur pass.
    pipelines: [wgpu::RenderPipeline; 2],
    targets: Targets,
}

/// What depends on the frame's size: the mask, a quarter of the frame's size, and the passes' bind groups.
struct Targets {
    mask: wgpu::TextureView,
    groups: [wgpu::BindGroup; 2],
}

impl LightShafts {
    /// The passes over `scene` (the post pass's, [`SCENE_FORMAT`], `size` pixels) and the frame's `depth`.
    pub(crate) fn new(
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) -> LightShafts {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light shafts"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
        let pipelines = ["fs_mask", "fs_blur"].map(|entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
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
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        });
        let targets = targets(device, &uniform, &sampler, &pipelines, scene, size, depth);
        LightShafts { uniform, sampler, pipelines, targets }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) {
        self.targets = targets(device, &self.uniform, &self.sampler, &self.pipelines, scene, size, depth);
    }

    /// Draws into `shafts` ([`crate::post::Post::shafts`]) the shafts of the sun at `sun` (its uv in the frame and its
    /// strength, [`sun_in_view`]; `None`: none, the map is cleared) in a frame `aspect` times as wide as it is high.
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        shafts: &wgpu::TextureView,
        sun: Option<(Vec2, f32)>,
        aspect: f32,
    ) {
        let Some((sun, strength)) = sun else {
            cleared(encoder, shafts);
            return;
        };
        let uniform = Uniform { sun: [sun.x, sun.y, STRENGTH * strength, aspect] };
        queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
        for ((pipeline, group), target) in
            self.pipelines.iter().zip(&self.targets.groups).zip([&self.targets.mask, shafts])
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
    sampler: &wgpu::Sampler,
    pipelines: &[wgpu::RenderPipeline; 2],
    scene: &wgpu::TextureView,
    (width, height): (u32, u32),
    depth: &wgpu::TextureView,
) -> Targets {
    let mask = {
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
    let view = wgpu::BindingResource::TextureView;
    let group = |pipeline: &wgpu::RenderPipeline, last: Vec<(u32, wgpu::BindingResource)>| {
        let entries: Vec<_> = [(0, uniform.as_entire_binding()), (1, wgpu::BindingResource::Sampler(sampler))]
            .into_iter()
            .chain(last)
            .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource })
            .collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light shafts"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    };
    let groups =
        [group(&pipelines[0], vec![(2, view(scene)), (3, view(depth))]), group(&pipelines[1], vec![(4, view(&mask))])];
    Targets { mask, groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn the_sun_casts_shafts_by_day_in_view() {
        // Looking along +z (left-handed), the sun 10 degrees up ahead.
        let projection = crate::coords::perspective(70f32.to_radians(), 16.0 / 9.0, 0.1);
        let up = 10f32.to_radians();
        let ahead = Vec3::new(0.0, up.sin(), up.cos());
        let day = Atmosphere { sun_direction: -ahead, sun_elevation: Some(10.0), ..Default::default() };
        let (uv, strength) = sun_in_view(projection, &day, 16.0 / 9.0).unwrap();
        assert!((uv.x - 0.5).abs() < 1e-4 && uv.y < 0.5 && strength == 1.0, "{uv} {strength}");
        // Low over the horizon it fades in; behind the eye, at night or under fog, none.
        let low = Atmosphere { sun_elevation: Some(2.0), ..day.clone() };
        assert!((sun_in_view(projection, &low, 16.0 / 9.0).unwrap().1 - 0.5).abs() < 1e-4);
        let behind = Atmosphere { sun_direction: Vec3::new(0.0, -up.sin(), up.cos()), ..day.clone() };
        assert!(sun_in_view(projection, &behind, 16.0 / 9.0).is_none());
        assert!(sun_in_view(projection, &Atmosphere { night: true, ..day.clone() }, 16.0 / 9.0).is_none());
        assert!(sun_in_view(projection, &Atmosphere { use_fog: true, ..day.clone() }, 16.0 / 9.0).is_none());
        // Off to the side, past the frame's edge by more than `OUT_OF_VIEW`, none.
        let side = Atmosphere { sun_direction: -Vec3::new(up.cos(), up.sin(), 0.2), ..day };
        assert!(sun_in_view(projection, &side, 16.0 / 9.0).is_none());
    }
}
