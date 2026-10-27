//! Drawing a frame: the passes in order, from the shadow maps to the post pass.

use super::meshes::{Bound, GpuMesh};
use super::reflection::{self, REFLECTION_REACH};
use super::shadows::Casting;
use super::textures::depth_view;
use super::uniforms::frame_index;
use super::{Renderer, CLEAR_COLOR};
use crate::atmosphere::Atmosphere;
use crate::camera::{self, Frustum};
use crate::coords::DEPTH_CLEAR;
use crate::perf_log;
use crate::post::SCENE_FORMAT;
use crate::profile::Profiler;
use crate::stage::{Anchor, Layer, Lod};
use crate::sun;
use crate::visibility::CELL_REACH;
use glam::{Mat4, Vec3};

/// How far from the eye (to a mesh's box centre) the main view lays the alpha-tested draws' depth first
/// ([`DrawPass`](super::pipelines::DrawPass)): the near foliage covers most of the frame, and the far draws would cost
/// more to lay than they save.
const PREPASS_REACH: f32 = 100.0;

/// Counts for one frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub meshes_drawn: usize,
    pub meshes_culled: usize,
    pub draws: usize,
    pub triangles: usize,
}

impl Renderer {
    /// Draws the stage into `target` (`size` pixels). World-anchored meshes use `view_projection`;
    /// camera-anchored ones use `sky_view_projection` (the same projection with the view's rotation only).
    /// The caller submits `encoder`.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        view_projection: Mat4,
        sky_view_projection: Mat4,
    ) -> FrameStats {
        let output = size;
        let size = match self.render_scale {
            scale if scale < 1.0 => {
                let side = |side: u32| ((side as f32 * scale).round() as u32).max(1);
                (side(output.0), side(output.1))
            }
            _ => output,
        };
        if (size, output) != (self.depth_size, self.output_size) {
            (self.depth_size, self.output_size) = (size, output);
            self.depth_view = depth_view(&self.device, size);
            if let Some(ssr) = &mut self.screen_reflection {
                ssr.resize(&self.device, SCENE_FORMAT, size, &self.depth_view);
            }
            self.post.resize(&self.device, size, &self.depth_view);
            if let Some(shafts) = &mut self.light_shafts {
                shafts.resize(&self.device, &self.post.scene, size, &self.depth_view);
            }
            if let Some(effects) = &mut self.effects {
                effects.resize(&self.device, &self.depth_view);
            }
            self.sun_mask.resize(&self.device, &self.depth_view, size);
            self.cloud_volume.resize(&self.device, &self.depth_view, size);
            if let Some(taa) = &mut self.taa {
                taa.resize(&self.device, size, size != output, &self.depth_view);
            }
        }
        // Under the temporal anti-aliasing the main view's draws are jittered; the sun's cascades, the mirrors and
        // the light shafts keep the view.
        let shift = self.taa.as_mut().map_or(Mat4::IDENTITY, crate::taa::Taa::begin);
        let drawn = [shift * view_projection, shift * sky_view_projection];
        // At night the moon casts them (`sun_direction` is the moon's then).
        self.cascades =
            self.enhance.lighting.then(|| sun::cascades(view_projection, self.shadow_light(), self.sun_quality()));
        // The world's layer holds the sun's light alone, drawn again only when that changes.
        self.world = self
            .cascades
            .and(self.world_box)
            .filter(|_| !self.sunless())
            .map(|b| sun::world(b, self.shadow_light(), self.sun_quality().size));
        let world = self.world.map(|(matrix, _)| matrix).filter(|&matrix| self.world_drawn != Some(matrix));
        if world.is_some() {
            self.world_drawn = world;
        }
        let lamps_light = self.lamps_light();
        self.lamp_matrix = self.sun.as_ref().filter(|_| lamps_light).and_then(|_| self.lamp_shadow_matrix());
        let eye = camera::eye(view_projection);
        // The sky's tables first: the game's light follows their day light (`game_light`).
        let mut profiler = self.profiler.take();
        self.update_skylight(&mut profiler);
        // The reflection cube's faces draw none of these.
        let (lit, level) = self.game_light();
        // The effects drawn after the temporal anti-aliasing's resolve keep the view.
        let effects_view = if self.late_effects() { view_projection } else { drawn[0] };
        if let Some(particles) = &self.particles {
            particles.prepare(&self.queue, effects_view, &lit);
        }
        if let Some(glass) = &self.glass {
            glass.prepare(&self.queue, drawn[0], size, &lit);
        }
        if let Some(effects) = &mut self.effects {
            let cascades = self.cascades.as_ref();
            effects.set_inside(!self.outside_view);
            // Without the shadows the lamps light the dust and the snow unshadowed, as before.
            let lamp = self.lamp_matrix.filter(|_| self.enhance.lighting);
            effects.update(&self.queue, effects_view, self.time, size, (&lit, level), cascades, lamp);
        }
        // Without the reflections the glass reads the resolved frame from its own target ([`Renderer::draw_frame`]).
        if let (Some(pass), None) = (&mut self.glass, &self.screen_reflection) {
            if pass.active() {
                pass.scene(&self.device, SCENE_FORMAT, size);
            }
        }
        // What a mark itself costs, to take from the stages' times.
        let t = mark(&mut profiler, encoder);
        span(&mut profiler, encoder, "(nothing)", t);
        // The frame up to the post pass goes in its own command buffer, submitted here after the sun's maps'. Those
        // depend on nothing else in the frame, so unless profiling another thread records and encodes them meanwhile
        // (wgpu encodes a command buffer when it is finished).
        let mut scene_encoder = self.device.create_command_encoder(&Default::default());
        if !self.dial_draws.is_empty() {
            let t = mark(&mut profiler, &mut scene_encoder);
            let pass = self.dial_pass.get_or_insert_with(|| crate::dials::gpu::Pass::new(&self.device));
            pass.draw(&self.device, &self.queue, &mut scene_encoder, &self.dial_targets, &self.dial_draws);
            self.dial_draws.clear();
            span(&mut profiler, &mut scene_encoder, "dials", t);
        }
        let casting = self.sun.as_ref().map(|sun| Casting {
            meshes: &self.meshes,
            buffers: &self.buffers,
            sun,
            queue: &self.queue,
        });
        let (cascades, lamp) = (self.cascades, self.lamp_matrix);
        let drop = sun::backdrop_drop(self.shadow_light());
        let range = self.sun_range.as_ref().filter(|_| cascades.is_some());
        let device = &self.device;
        let stats = std::thread::scope(|scope| {
            let shadows = match (casting, profiler.as_ref().is_some_and(Profiler::blocking)) {
                (Some(casting), false) => {
                    let stamps = profiler.as_mut().and_then(Profiler::shadow_stamps);
                    Some(scope.spawn(move || {
                        perf_log::name_thread("shadow maps");
                        let _span = perf_log::span("sun cascades and lamp shadow");
                        let mut encoder = device.create_command_encoder(&Default::default());
                        stamps.iter().for_each(|set| encoder.write_timestamp(set, 0));
                        casting.draw(&mut encoder, cascades.as_ref(), lamp, world, eye, drop);
                        range.iter().for_each(|range| range.draw(&mut encoder));
                        stamps.iter().for_each(|set| encoder.write_timestamp(set, 1));
                        encoder.finish()
                    }))
                }
                (Some(casting), true) => {
                    let t = mark(&mut profiler, &mut scene_encoder);
                    casting.draw(&mut scene_encoder, cascades.as_ref(), lamp, world, eye, drop);
                    range.iter().for_each(|range| range.draw(&mut scene_encoder));
                    span(&mut profiler, &mut scene_encoder, "sun cascades and lamp shadow", t);
                    None
                }
                (None, _) => None,
            };
            self.update_reflection(&mut profiler);
            // The mirrors' passes read this frame's sun cascades.
            let shadows = match shadows {
                Some(thread) if self.mirrors_due() => {
                    let _span = perf_log::span("wait for shadow maps");
                    self.queue.submit([thread.join().expect("the shadow maps' thread")]);
                    None
                }
                shadows => shadows,
            };
            self.draw_mirrors(&mut profiler, [view_projection, sky_view_projection]);
            let stats = self.draw_frame(&mut scene_encoder, &mut profiler, drawn[0], drawn[1]);
            let mut span = perf_log::span("finish scene commands");
            let scene = scene_encoder.finish();
            span.next("wait for shadow maps");
            let shadows = shadows.map(|thread| thread.join().expect("the shadow maps' thread"));
            span.next("submit scene");
            self.queue.submit(shadows.into_iter().chain([scene]));
            stats
        });
        let car = self.taa.as_ref().and_then(|_| self.car_body.zip(self.car_model_box()));
        if let Some(taa) = &mut self.taa {
            let frame = crate::taa::Frame { view_projection, car, outside: self.outside_view };
            taa.prepare(&self.queue, &frame);
            let t = mark(&mut profiler, encoder);
            taa.resolve(encoder, &self.post.scene);
            span(&mut profiler, encoder, "taa", t);
            if self.late_effects() {
                let t = mark(&mut profiler, encoder);
                self.draw_late_effects(encoder, &self.post.scene);
                span(&mut profiler, encoder, "particles, rain", t);
            }
        }
        // The march takes the unjittered view (the temporal anti-aliasing resolved above does not smooth it;
        // its own jitter and blur do), the sun's colour from the sky's tables, and its cascades.
        let sun = self.skylight_on().then(|| self.air_sun(view_projection)).flatten();
        if let Some(shafts) = &self.light_shafts {
            // Without the sun's march, the lamps' light in the air (at night and at dusk).
            let lamps = self.air_lamps();
            let lit = lamps.iter().any(|l| l.colour[..3].iter().any(|&c| c > 0.0));
            let beams = (sun.is_none() && lit).then(|| {
                let eye = camera::eye(view_projection);
                // The air is at least as thick as the height fog drawn around the eye (its density there).
                let mist = crate::height_fog::uniform(&self.atmosphere, self.fog_ground.as_ref(), eye);
                // Without the shadows the beams shine unshadowed, as before.
                let lamp = self.lamp_matrix.filter(|_| self.enhance.lighting).unwrap_or(Mat4::ZERO);
                crate::light_shafts::Beams {
                    inverse_view_projection: view_projection.inverse(),
                    eye,
                    air: crate::light_shafts::air(&self.atmosphere).max(mist[0] * mist[3]),
                    lamps,
                    lamp,
                }
            });
            let t = mark(&mut profiler, encoder);
            shafts.draw(&self.queue, encoder, self.post.shafts(), sun.as_ref(), beams.as_ref());
            span(&mut profiler, encoder, "light shafts", t);
        }
        let cabin_depth = if self.outside_view { 0.0 } else { crate::post::cabin_depth(view_projection) };
        let mut conditions = crate::post::conditions(&self.atmosphere);
        // With the sky's tables the stage's light is physical, pre-exposed by theirs.
        if let Some(day) = self.day_light() {
            conditions.pre_exposure = day.sun[3];
            // A sunlit day under our sun lighting is exposed `SUN_EXPOSURE` stops down.
            if self.enhance.lighting {
                let share = day.sun_share(-self.atmosphere.sun_direction.y);
                conditions.exposure *= (-crate::post::SUN_EXPOSURE * share).exp2();
            }
        }
        let t = mark(&mut profiler, encoder);
        self.post.draw(&self.queue, encoder, target, self.time, cabin_depth, conditions);
        span(&mut profiler, encoder, "post (bloom, tone)", t);
        self.profiler = profiler;
        stats
    }

    /// Draws the frame of `view_projection` into the post pass's scene: the car's shadow maps, the stage and its
    /// reflections.
    fn draw_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profiler: &mut Option<Profiler>,
        view_projection: Mat4,
        sky_view_projection: Mat4,
    ) -> FrameStats {
        let shadow_matrices = self.shadow_matrices();
        self.write_frames(view_projection, sky_view_projection, shadow_matrices, View::Main);

        let t = mark(profiler, encoder);
        // The contact layer only under `Enhance::contact`.
        let layers = 1 + usize::from(self.enhance.contact);
        for (layer, pipeline) in self.shadow_layers.iter().zip(&self.shadow_pipelines).take(layers) {
            if shadow_matrices.is_none() {
                break;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("car shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.shadow_frame, &[]);
            let mut bound = Bound::new(&self.buffers);
            for mesh in &self.meshes {
                for draw in mesh.draws.iter().filter(|draw| draw.casts_shadow) {
                    bound.draw(&mut pass, mesh, draw);
                }
            }
        }
        if let Some([shadow_matrix, _]) = shadow_matrices {
            self.shadow_range.update(&self.queue, encoder, shadow_matrix);
        }

        span(profiler, encoder, "car shadow", t);

        // The frame is drawn into the post pass's scene, which then writes the target.
        let t = mark(profiler, encoder);
        // Under the temporal anti-aliasing into its own target, which it resolves into the post pass's.
        let resolved = self.taa.as_ref().map_or(&self.post.scene, crate::taa::Taa::raw);
        let glass = self.glass.as_ref().is_some_and(|g| g.active());
        // The effects' draws over the kept frame, if any this frame.
        let effects = !self.late_effects()
            && self.effects.as_ref().is_some_and(|effects| {
                self.particles.as_ref().is_some_and(|p| !p.is_empty()) || effects.precipitating()
            });
        let stats = match &self.screen_reflection {
            // The glass reads the resolved frame: resolve into its own, then draw it over the kept frame. The effects
            // read the depth, so they go over the kept frame too.
            None if glass || effects => {
                let scene = match glass {
                    true => self.glass.as_ref().and_then(|g| g.own_scene()).expect("made above"),
                    false => resolved,
                };
                let stats = self.draw_scene(encoder, [scene, &self.depth_view], view_projection, View::Main, [true; 2]);
                span(profiler, encoder, "scene", t);
                let t = mark(profiler, encoder);
                self.draw_over(encoder, resolved, scene);
                span(profiler, encoder, "particles, glass, rain", t);
                stats
            }
            // The post pass reads the depth (the metering, the light shafts and the lamps' beams).
            None => {
                let stats =
                    self.draw_scene(encoder, [resolved, &self.depth_view], view_projection, View::Main, [false, true]);
                span(profiler, encoder, "scene", t);
                stats
            }
            Some(ssr) => {
                let levels = [&ssr.levels[0], &self.depth_view];
                let stats = self.draw_scene(encoder, levels, view_projection, View::Main, [true; 2]);
                span(profiler, encoder, "scene", t);
                let t = mark(profiler, encoder);
                ssr.halver.halve(encoder, &ssr.levels, &ssr.sources);
                self.draw_reflections(encoder, ssr, resolved, view_projection);
                span(profiler, encoder, "screen reflections", t);
                stats
            }
        };
        stats
    }

    /// Draws the particles and the glass over the frame `draw_scene` kept and resolves it into `target`; the glass
    /// reads the resolved frame `scene`.
    fn draw_over(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, scene: &wgpu::TextureView) {
        let attachment = self.over_attachment(encoder, target, scene);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glass"),
            color_attachments: &[Some(attachment)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !self.late_effects() {
            self.draw_effects(&mut pass);
        }
        if let Some(glass) = &self.glass {
            glass.draw(&self.device, &mut pass, scene);
        }
    }

    /// Whether the rain and the particles go over the temporal anti-aliasing's resolve rather than into the frame it
    /// gathers: they write no depth, so the history cannot carry them (they would smear and fade).
    pub(super) fn late_effects(&self) -> bool {
        self.taa.is_some() && self.effects.is_some()
    }

    /// Draws the rain and the particles over the resolved `scene`, in the unjittered view.
    fn draw_late_effects(&self, encoder: &mut wgpu::CommandEncoder, scene: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("effects over the resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw_effects(&mut pass);
    }

    /// Draws the rain and the particles into `pass`, which reads the depth: lit and soft with the effects
    /// ([`Enhance::particles`](super::Enhance::particles)), else the particles as the game's.
    pub(super) fn draw_effects(&self, pass: &mut wgpu::RenderPass<'_>) {
        match &self.effects {
            Some(effects) => {
                effects.draw_rain(pass);
                if let Some(particles) = &self.particles {
                    particles.draw_soft(pass, effects.group(), self.lamps_light());
                }
            }
            None => {
                if let Some(particles) = &self.particles {
                    particles.draw(pass);
                }
            }
        }
    }

    /// Draws the sky and the meshes `view` shows of `view_projection` into `[target, depth]`. `keep` is whether
    /// passes follow that draw over the frame (the particles, then, are theirs), and whether the depth is stored after
    /// the pass (the reflections, the effects and the post pass read it).
    pub(super) fn draw_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        [target, depth]: [&wgpu::TextureView; 2],
        view_projection: Mat4,
        view: View,
        keep: [bool; 2],
    ) -> FrameStats {
        let depth_store = if keep[1] { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard };
        // The temporal anti-aliasing's mask starts clear (`pipelines::alpha_writes`).
        let colour_clear = match view == View::Main && self.taa.is_some() {
            true => wgpu::Color { a: 0.0, ..CLEAR_COLOR },
            false => CLEAR_COLOR,
        };
        let mut stats = FrameStats::default();
        // With the sun's mask the main view's prepass is a pass of its own, whose depth the mask is worked out from
        // before the rest is drawn over it.
        let masked = view == View::Main && self.sun_mask.on();
        let cloud_volume = self.cloud_volume_shown();
        let faces_hidden = self.cloud_faces_hidden();
        // The clouds march before the stage is drawn; with the mask after the prepass, whose depth they skip.
        let draw_clouds = |encoder: &mut wgpu::CommandEncoder, depth_fresh: bool| {
            if view == View::Main && cloud_volume {
                let eye = camera::eye(view_projection);
                let aim = (eye, view_projection * Mat4::from_translation(eye));
                let frame = (&self.frames[0].bind_group, &self.atmosphere);
                self.cloud_volume.draw(&self.queue, encoder, frame, aim, depth_fresh);
            }
        };
        if !masked {
            draw_clouds(encoder, false);
        }
        let frame = wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(colour_clear), store: wgpu::StoreOp::Store },
        };
        let clear = wgpu::LoadOp::Clear(DEPTH_CLEAR);
        let mut pass = match masked {
            true => {
                let ops = wgpu::Operations { load: clear, store: wgpu::StoreOp::Store };
                Self::stage_pass(encoder, self.sun_mask.normals(), depth, ops)
            }
            false => {
                Self::stage_pass(encoder, frame.clone(), depth, wgpu::Operations { load: clear, store: depth_store })
            }
        };
        let shown = self.shown(view_projection, view, &mut stats);
        // The sky lies behind everything, so it is drawn after the opaque meshes, where they left the clear depth (its
        // pipelines' `Equal` test at the viewport's depth 0): the frame is the same as with the sky first, but the
        // pixels they cover are not shaded. It goes in before the first mesh it could show through, one that blends or
        // writes no depth.
        let (sky, shown): (Vec<_>, Vec<_>) = shown
            .into_iter()
            .filter(|(mesh, _)| !(faces_hidden && mesh.anchor == Anchor::Camera))
            .partition(|(mesh, _)| mesh.layer == Layer::Sky);
        let behind = shown
            .iter()
            .position(|(mesh, _)| mesh.draws.iter().any(|draw| draw.pipeline.blend || !draw.pipeline.depth_write))
            .unwrap_or(shown.len());
        // The main view lays the alpha-tested draws' depth first ([`DrawPass`]), within [`PREPASS_REACH`]; with the
        // sun's mask all the world's opaque draws (the camera-anchored ones are in another frame). Depth only, so in
        // any order: by pipeline and bind group, for fewer switches.
        let laid = |mesh: &GpuMesh, distance: f32| {
            view == View::Main && if masked { mesh.anchor == Anchor::World } else { distance <= PREPASS_REACH }
        };
        let mut bound = Bound::new(&self.buffers);
        pass.set_bind_group(2, self.sun_mask.group(), &[]);
        let mut prepass: Vec<_> = shown
            .iter()
            .filter(|&&(mesh, distance)| laid(mesh, distance))
            .flat_map(|&(mesh, distance)| {
                let draws = mesh.draws.iter().filter(move |draw| draw.lod.shows(distance));
                draws.filter_map(move |draw| Some((mesh, draw, &draw.prepassed.as_ref()?[0])))
            })
            .collect();
        prepass.sort_by(|(_, a, a_pipeline), (_, b, b_pipeline)| (a_pipeline, &a.group).cmp(&(b_pipeline, &b.group)));
        for (mesh, draw, pipeline) in prepass {
            pass.set_bind_group(0, &self.frames[frame_index(mesh.anchor, mesh.layer)].bind_group, &[]);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, &draw.group, &[]);
            bound.draw(&mut pass, mesh, draw);
        }
        if masked {
            drop(pass);
            self.sun_mask.draw(encoder, &self.frames[0].bind_group);
            draw_clouds(encoder, true);
            pass = Self::stage_pass(
                encoder,
                frame,
                depth,
                wgpu::Operations { load: wgpu::LoadOp::Load, store: depth_store },
            );
            pass.set_bind_group(2, self.sun_mask.group(), &[]);
            bound = Bound::new(&self.buffers);
        }
        // The lit opaque draws (the car's, the characters') go in before the shaded stage draws, whose depth test
        // (`Equal` to the prepass) then fails under them: the ground beneath the car is not shaded to be painted over.
        let laid_first = view == View::Main;
        let opaque_lit = |draw: &super::meshes::GpuDraw| {
            draw.bind.shade.is_some() && !draw.pipeline.blend && draw.pipeline.depth_write
        };
        let lamps_light = self.lamps_light() && view != View::Probe;
        let inside = self.in_car(view);
        let mut draw_meshes = |pass: &mut wgpu::RenderPass<'_>, meshes: &[(&GpuMesh, f32)], lit_first: bool| {
            for &(mesh, distance) in meshes {
                stats.meshes_drawn += usize::from(!lit_first);
                pass.set_bind_group(0, &self.frames[frame_index(mesh.anchor, mesh.layer)].bind_group, &[]);
                for draw in mesh.draws.iter().filter(|draw| draw.lod.shows(distance)) {
                    if (laid_first && opaque_lit(draw)) != lit_first {
                        continue;
                    }
                    let after_prepass = draw.prepassed.is_some() && laid(mesh, distance);
                    let pipeline = match (&draw.views, &draw.lamps, &draw.prepassed) {
                        (Some(views), _, _) => &views[usize::from(inside)],
                        (_, Some([_, after]), _) if lamps_light && after_prepass => after,
                        (_, Some([main, _]), _) if lamps_light => main,
                        (_, _, Some([_, after])) if after_prepass => after,
                        _ => &draw.main,
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(1, &draw.group, &[]);
                    bound.draw(pass, mesh, draw);
                    stats.draws += 1;
                    stats.triangles += draw.indices.len() / 3;
                }
            }
        };
        if laid_first {
            let lit: Vec<_> = shown.iter().copied().filter(|(mesh, _)| mesh.draws.iter().any(opaque_lit)).collect();
            draw_meshes(&mut pass, &lit, true);
        }
        draw_meshes(&mut pass, &shown[..behind], false);
        let size = depth.texture().size();
        let (width, height) = (size.width as f32, size.height as f32);
        pass.set_viewport(0.0, 0.0, width, height, 0.0, 0.0);
        pass.set_pipeline(&self.sky);
        pass.set_bind_group(0, &self.frames[1].bind_group, &[]);
        pass.set_bind_group(1, self.cloud_volume.sky_group(), &[]);
        pass.draw(0..3, 0..1);
        draw_meshes(&mut pass, &sky, false);
        pass.set_viewport(0.0, 0.0, width, height, 0.0, 1.0);
        draw_meshes(&mut pass, &shown[behind..], false);
        // The marks lie on the ground, so the car's reflection cube shows them too (ours). Each cube face is its own
        // submission, so the marks' view written here is the face's.
        let (light, lamps) = (self.mark_light(), self.air_lamps());
        if let Some(marks) = &self.skid_marks {
            marks.draw(&mut pass, &self.queue, view_projection, light, lamps);
        }
        if let Some(shadow) = &self.car_shadow {
            shadow.draw(&mut pass, &self.queue, view_projection, light, lamps);
        }
        // The main view without reflections ends here; with them, the particles go over the reflections. The mirrors
        // draw the game's particles.
        if let (Some(particles), true) =
            (&self.particles, (view == View::Main && !keep[0] && !self.late_effects()) || view.mirror())
        {
            particles.draw(&mut pass);
        }
        stats
    }

    /// A stage pass into `colour` and `depth` with the depth's `ops`.
    fn stage_pass<'e>(
        encoder: &'e mut wgpu::CommandEncoder,
        colour: wgpu::RenderPassColorAttachment<'_>,
        depth: &wgpu::TextureView,
        ops: wgpu::Operations<f32>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stage"),
            color_attachments: &[Some(colour)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(ops),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// The atmosphere with the game's light (its ambient, diffuse and fog colours, display level) following the day's
    /// physical light (`DayLight::level`, `DayLight::parts`), and the level: as is without the sky's tables.
    pub(super) fn game_light(&self) -> (Atmosphere, f32) {
        let a = &self.atmosphere;
        let Some(day) = self.day_light() else {
            return (a.clone(), 1.0);
        };
        let level = day.level(a.sun_direction.y);
        let (sky, sun) = day.parts();
        let lit = Atmosphere {
            ambient: a.ambient * sky,
            diffuse: a.diffuse * sun,
            fog_color: a.fog_color * level,
            ..a.clone()
        };
        (lit, level)
    }

    /// The sun's march through the air ([`crate::light_shafts::Sun`]): with the sky's tables by day under a sky
    /// that is not overcast, the sun's colour from them and its cascades (`None` without them: no shade). `None`
    /// where the sun does not march (night, overcast, still down): the lamps' beams draw instead, or nothing.
    fn air_sun(&mut self, view_projection: Mat4) -> Option<crate::light_shafts::Sun> {
        let strength = crate::light_shafts::sun_strength(&self.atmosphere)?;
        let day = self.day_light()?;
        let eye = camera::eye(view_projection);
        let (matrices, splits, shade) =
            self.cascades.as_ref().map_or(([Mat4::IDENTITY; 3], [f32::INFINITY; 3], 0.0), |c| {
                (c.matrices, [crate::sun::SPLITS[0], crate::sun::SPLITS[1], crate::sun::SPLITS[2]], 1.0)
            });
        let frame = self.air_frame;
        self.air_frame = self.air_frame.wrapping_add(1);
        Some(crate::light_shafts::Sun {
            inverse_view_projection: view_projection.inverse(),
            eye,
            direction: self.atmosphere.sun_direction,
            colour: Vec3::new(day.sun[0], day.sun[1], day.sun[2]),
            strength,
            height_fog: crate::height_fog::uniform(&self.atmosphere, self.fog_ground.as_ref(), eye),
            fog_volume: crate::height_fog::is_volume(&self.atmosphere),
            base_air: crate::light_shafts::air(&self.atmosphere),
            matrices,
            splits,
            shade,
            clouds: self.shadow_params.get(),
            frame,
        })
    }

    /// When the lamps light the stage: at night and at dusk.
    fn lamps_light(&self) -> bool {
        self.atmosphere.night || self.atmosphere.relight.is_some_and(|r| r.lamps > 0.01)
    }

    /// The light on the skid marks and the car's shadow decal, whose colours are the game's for the lit ground (ours):
    /// at night the moon's (`Atmosphere::moonlight`), at dusk the fading day's.
    fn mark_light(&self) -> Vec3 {
        if let Some(day) = self.day_light() {
            return Vec3::splat(day.level(self.atmosphere.sun_direction.y));
        }
        if self.atmosphere.night {
            return self.atmosphere.moonlight();
        }
        self.atmosphere
            .relight
            .map_or(Vec3::ONE, |r| (r.tint * r.ambient).lerp(Vec3::ONE, r.sun.max_element().min(1.0)))
    }

    /// The meshes `view` shows of `view_projection`, each with its distance for the level of detail: the distance to the
    /// box centre (`Lod`); a mesh with no bounds has no levels.
    fn shown(&self, view_projection: Mat4, view: View, stats: &mut FrameStats) -> Vec<(&GpuMesh, f32)> {
        let with_car = view.with_car();
        let eye = camera::eye(view_projection);
        let frustum = Frustum::from_view_projection(view_projection);
        let mut shown = Vec::new();
        for mesh in self.meshes.iter().filter(|mesh| mesh.shown_in(view)) {
            if !with_car && mesh.draws.iter().any(|draw| draw.bind.shade.is_some()) {
                continue;
            }
            // The reflection cube draws what lies in the cells near its centre, and no interactive props, as the
            // game's (`visibility::near_cube`); the meshes without a cell keep the surroundings cube's reach (ours).
            if (view == View::Reflection && mesh.cell_far_from(eye))
                || (matches!(view, View::Reflection | View::Probe) && mesh.interactive.is_some())
            {
                stats.meshes_culled += 1;
                continue;
            }
            if let Some((center, half_extent)) = mesh.bounds {
                if !frustum.intersects_box(center, half_extent) || (with_car && mesh.cell_hidden(view_projection)) {
                    stats.meshes_culled += 1;
                    continue;
                }
                let reach = match view {
                    View::Main => None,
                    View::Reflection => mesh.cell.is_none().then_some(REFLECTION_REACH),
                    View::Surroundings | View::Probe => Some(CELL_REACH),
                    View::Mirror { .. } => Some(super::mirrors::REACH),
                };
                let (distance, radius) = (center.distance(eye), half_extent.length());
                if reach.is_some_and(|reach| distance - radius > reach)
                    || (view.surroundings() && reflection::tiny(radius, distance))
                {
                    stats.meshes_culled += 1;
                    continue;
                }
            }
            let distance = mesh.bounds.map_or(0.0, |(center, _)| center.distance(eye));
            // The surroundings cube takes the far ground of the blocks around it (ours: it is only read blurred or
            // far), as if past every switch distance, where only the far and the always-drawn lists show
            // (`Lod::shows`). The block under the car keeps its near ground, which lights the car from below.
            // So do the mirrors.
            let far = (view.surroundings() || view.mirror())
                && mesh.bounds.is_some_and(|(_, half_extent)| distance > half_extent.length())
                && mesh.draws.iter().any(|draw| draw.lod == Lod::Far);
            shown.push((mesh, if far { f32::INFINITY } else { distance }));
        }
        shown
    }
}

/// What [`Renderer::draw_scene`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    /// The view: the car, and the stage cells within the view's reach (`visibility::beyond_reach`).
    Main,
    /// A face of the car's reflection cube: without the car's lit meshes and the interactive props, the stage cells
    /// near the car (`visibility::near_cube`) and the meshes without a cell within [`REFLECTION_REACH`].
    Reflection,
    /// A face of the surroundings cube: without the car's lit meshes and the meshes under a texel or two
    /// (`reflection::tiny`), everything within the main view's [`CELL_REACH`], with the far ground but under the car.
    Surroundings,
    /// A face of a reflection probe (`super::probes`): as the surroundings cube's, without the interactive props
    /// (which move), the car's lamps, and the sun's cascades (which follow the eye).
    Probe,
    /// A mirror's pass (`super::mirrors`): the car's interior set for the centre mirror or its exterior for a door
    /// mirror (`side`), and the stage to [`REACH`](super::mirrors::REACH) with the far ground but under the car.
    Mirror { side: bool },
}

impl View {
    /// Whether the view draws the car's lit meshes (and the other lit ones: the ghost, the characters).
    pub(super) fn with_car(self) -> bool {
        matches!(self, View::Main | View::Mirror { .. })
    }

    pub(super) fn mirror(self) -> bool {
        matches!(self, View::Mirror { .. })
    }

    /// Whether the view is a face of the surroundings cube or of a probe.
    fn surroundings(self) -> bool {
        matches!(self, View::Surroundings | View::Probe)
    }
}

impl Renderer {
    /// Whether `view` sees the car from inside: the frame's `reflection.y` ([`Renderer::write_frames`]) and the lit
    /// draws' pipeline (`INSIDE` in `shader.wgsl`). A door mirror sees the car from outside.
    pub(super) fn in_car(&self, view: View) -> bool {
        !self.outside_view && view != View::Mirror { side: true }
    }

    /// The colour attachment of a pass drawn over the kept frame, which `draw_scene` drew into `scene`: it loads
    /// `target`, holding the frame once `scene` is copied into it when they differ (the glass's and the reflections'
    /// own scene).
    pub(super) fn over_attachment<'a>(
        &'a self,
        encoder: &mut wgpu::CommandEncoder,
        target: &'a wgpu::TextureView,
        scene: &wgpu::TextureView,
    ) -> wgpu::RenderPassColorAttachment<'a> {
        if scene != target {
            encoder.copy_texture_to_texture(
                scene.texture().as_image_copy(),
                target.texture().as_image_copy(),
                scene.texture().size(),
            );
        }
        wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
        }
    }
}

/// A timestamp for [`span`] when profiling ([`Renderer::enable_profiling`]).
pub(super) fn mark(profiler: &mut Option<Profiler>, encoder: &mut wgpu::CommandEncoder) -> usize {
    profiler.as_mut().map_or(0, |p| p.mark(encoder))
}

/// Ends the profiled span `label` that started at `start`.
pub(super) fn span(
    profiler: &mut Option<Profiler>,
    encoder: &mut wgpu::CommandEncoder,
    label: &'static str,
    start: usize,
) {
    if let Some(p) = profiler {
        p.span(encoder, label, start);
    }
}
