//! Drawing a frame: the passes in order, from the shadow maps to the post pass.

use super::meshes::{Bound, GpuMesh};
use super::reflection::{self, REFLECTION_REACH};
use super::shadows::Casting;
use super::textures::{color_view, depth_view};
use super::uniforms::frame_index;
use super::{Renderer, CLEAR_COLOR};
use crate::camera::{self, Frustum};
use crate::coords::DEPTH_CLEAR;
use crate::depth_copy::DepthCopy;
use crate::profile::Profiler;
use crate::stage::Lod;
use crate::sun;
use crate::visibility::CELL_REACH;
use glam::Mat4;

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
        let size = match (&self.post, self.render_scale) {
            (Some(_), scale) if scale < 1.0 => {
                let side = |side: u32| ((side as f32 * scale).round() as u32).max(1);
                (side(output.0), side(output.1))
            }
            _ => output,
        };
        if (size, output) != (self.depth_size, self.output_size) {
            (self.depth_size, self.output_size) = (size, output);
            self.depth_view = depth_view(&self.device, size, self.samples);
            // With one sample the passes draw into their targets directly ([`Renderer::draw_scene`]).
            let color_size = if self.samples > 1 { size } else { (1, 1) };
            self.color_view = color_view(&self.device, self.scene_format, color_size, self.samples);
            if let Some(copy) = &mut self.depth_copy {
                copy.resize(&self.device, size, &self.depth_view);
            }
            let copy = self.depth_copy.as_ref().map(DepthCopy::view);
            if let Some(ssr) = &mut self.screen_reflection {
                ssr.resize(&self.device, self.scene_format, size, &self.depth_view, copy);
            }
            if let Some(post) = &mut self.post {
                post.resize(&self.device, size, &self.depth_view);
                if let Some(shafts) = &mut self.light_shafts {
                    shafts.resize(&self.device, &post.scene, size, &self.depth_view);
                }
            }
            if let Some(effects) = &mut self.effects {
                effects.resize(&self.device, &self.depth_view);
            }
        }
        self.cascades =
            (self.enhance.sun && !self.atmosphere.night).then(|| sun::cascades(view_projection, self.shadow_light()));
        // Only at night: by day the sun's cascades hold layer 0.
        self.lamp_matrix = self.sun.as_ref().filter(|_| self.atmosphere.night).and_then(|_| self.lamp_shadow_matrix());
        let eye = camera::eye(view_projection);
        // The reflection cube's faces draw none of these.
        if let Some(particles) = &self.particles {
            particles.prepare(&self.queue, view_projection, &self.atmosphere);
        }
        if let Some(glass) = &self.glass {
            glass.prepare(&self.queue, view_projection, size, &self.atmosphere);
        }
        if let Some(effects) = &mut self.effects {
            let cascades = self.cascades.as_ref();
            effects.update(&self.queue, view_projection, self.time, size, &self.atmosphere, cascades);
        }
        // Without the reflections the glass reads the resolved frame from its own target ([`Renderer::draw_frame`]).
        if let (Some(pass), None) = (&mut self.glass, &self.screen_reflection) {
            if pass.active() {
                pass.scene(&self.device, self.scene_format, size);
            }
        }
        let mut profiler = self.profiler.take();
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
        let device = &self.device;
        let stats = std::thread::scope(|scope| {
            let shadows = match (casting, profiler.is_some()) {
                (Some(casting), false) => Some(scope.spawn(move || {
                    let mut encoder = device.create_command_encoder(&Default::default());
                    casting.draw(&mut encoder, cascades.as_ref(), lamp, eye);
                    encoder.finish()
                })),
                (Some(casting), true) => {
                    let t = mark(&mut profiler, &mut scene_encoder);
                    casting.draw(&mut scene_encoder, cascades.as_ref(), lamp, eye);
                    span(&mut profiler, &mut scene_encoder, "sun cascades and lamp shadow", t);
                    None
                }
                (None, _) => None,
            };
            self.update_skylight(&mut profiler);
            self.update_reflection(&mut profiler);
            // The mirrors' passes read this frame's sun cascades.
            let shadows = match shadows {
                Some(thread) if self.mirrors_due() => {
                    self.queue.submit([thread.join().expect("the shadow maps' thread")]);
                    None
                }
                shadows => shadows,
            };
            self.draw_mirrors(&mut profiler, [view_projection, sky_view_projection]);
            let stats =
                self.draw_frame(&mut scene_encoder, &mut profiler, target, view_projection, sky_view_projection);
            let scene = scene_encoder.finish();
            let shadows = shadows.map(|thread| thread.join().expect("the shadow maps' thread"));
            self.queue.submit(shadows.into_iter().chain([scene]));
            stats
        });
        let aspect = output.0 as f32 / output.1 as f32;
        if let (Some(shafts), Some(post)) = (&self.light_shafts, &self.post) {
            let sun = self
                .skylight_on()
                .then(|| crate::light_shafts::sun_in_view(sky_view_projection, &self.atmosphere, aspect));
            let t = mark(&mut profiler, encoder);
            shafts.draw(&self.queue, encoder, post.shafts(), sun.flatten(), aspect);
            span(&mut profiler, encoder, "light shafts", t);
        }
        let cabin_depth = if self.outside_view { 0.0 } else { crate::post::cabin_depth(view_projection) };
        if let Some(post) = &mut self.post {
            let t = mark(&mut profiler, encoder);
            post.draw(&self.queue, encoder, target, self.time, cabin_depth);
            span(&mut profiler, encoder, "post (bloom, tone)", t);
        }
        self.profiler = profiler;
        stats
    }

    /// Draws the frame of `view_projection` into the post pass's scene, or `target` without one: the car's shadow maps,
    /// the stage and its reflections.
    fn draw_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profiler: &mut Option<Profiler>,
        target: &wgpu::TextureView,
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

        span(profiler, encoder, "car shadow", t);

        // Under HDR the frame resolves into the post pass's scene, which then writes `target`.
        let t = mark(profiler, encoder);
        let resolved = self.post.as_ref().map_or(target, |post| &post.scene);
        let glass = self.glass.as_ref().is_some_and(|g| g.active());
        // The effects' draws over the kept frame, if any this frame: else the scene resolves at once, the same frame
        // without storing and reloading the multisampled colour.
        let effects = self
            .effects
            .as_ref()
            .is_some_and(|effects| self.particles.as_ref().is_some_and(|p| !p.is_empty()) || effects.precipitating());
        let stats = match &self.screen_reflection {
            // The glass reads the resolved frame: resolve into its own, then draw it over the kept frame. The effects
            // read the depth, so they go over the kept frame too.
            None if glass || effects => {
                let scene = match glass {
                    true => self.glass.as_ref().and_then(|g| g.own_scene()).expect("made above"),
                    false => resolved,
                };
                let stats = self.draw_scene(
                    encoder,
                    [&self.color_view, &self.depth_view, scene],
                    view_projection,
                    View::Main,
                    [true; 2],
                );
                span(profiler, encoder, "scene", t);
                let t = mark(profiler, encoder);
                self.draw_over(encoder, resolved, scene);
                span(profiler, encoder, "particles, glass, rain", t);
                stats
            }
            None => {
                let stats = self.draw_scene(
                    encoder,
                    [&self.color_view, &self.depth_view, resolved],
                    view_projection,
                    View::Main,
                    [false; 2],
                );
                span(profiler, encoder, "scene", t);
                stats
            }
            Some(ssr) => {
                let levels = [&self.color_view, &self.depth_view, &ssr.levels[0]];
                let stats = self.draw_scene(encoder, levels, view_projection, View::Main, [true; 2]);
                span(profiler, encoder, "scene", t);
                let t = mark(profiler, encoder);
                if let Some(copy) = &self.depth_copy {
                    copy.draw(encoder);
                }
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
        });
        self.draw_effects(&mut pass);
        if let Some(glass) = &self.glass {
            glass.draw(&self.device, &mut pass, scene);
        }
    }

    /// Draws the rain and the particles into `pass`, which reads the depth: lit and soft with the effects
    /// ([`Enhance::particles`](super::Enhance::particles)), else the particles as the game's.
    pub(super) fn draw_effects(&self, pass: &mut wgpu::RenderPass<'_>) {
        match &self.effects {
            Some(effects) => {
                effects.draw_rain(pass);
                if let Some(particles) = &self.particles {
                    particles.draw_soft(pass, effects.group());
                }
            }
            None => {
                if let Some(particles) = &self.particles {
                    particles.draw(pass);
                }
            }
        }
    }

    /// Draws the sky and the meshes `view` shows of `view_projection` into the multisampled `color` and `depth` of
    /// `[color, depth, target]`, resolved into `target` (with one sample, into `target` and `depth`). `keep` stores
    /// the multisampled colour and depth after the pass (the reflections pass reads both).
    pub(super) fn draw_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        [color, depth, target]: [&wgpu::TextureView; 3],
        view_projection: Mat4,
        view: View,
        keep: [bool; 2],
    ) -> FrameStats {
        let [store, depth_store] = keep.map(|keep| if keep { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard });
        let mut stats = FrameStats::default();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stage"),
            color_attachments: &[Some(match self.samples {
                1 => wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(CLEAR_COLOR), store: wgpu::StoreOp::Store },
                },
                _ => wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(CLEAR_COLOR), store },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(DEPTH_CLEAR), store: depth_store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.sky);
        pass.set_bind_group(0, &self.frames[1].bind_group, &[]);
        pass.draw(0..3, 0..1);
        let shown = self.shown(view_projection, view, &mut stats);
        // The main view lays the alpha-tested draws' depth first ([`DrawPass`]), within [`PREPASS_REACH`]. Depth only, so
        // in any order: by pipeline and bind group, for fewer switches.
        let laid = |distance: f32| view == View::Main && distance <= PREPASS_REACH;
        let mut bound = Bound::new(&self.buffers);
        let mut prepass: Vec<_> = shown
            .iter()
            .filter(|&&(_, distance)| laid(distance))
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
        for &(mesh, distance) in &shown {
            stats.meshes_drawn += 1;
            pass.set_bind_group(0, &self.frames[frame_index(mesh.anchor, mesh.layer)].bind_group, &[]);
            for draw in mesh.draws.iter().filter(|draw| draw.lod.shows(distance)) {
                let pipeline = match &draw.prepassed {
                    Some([_, after]) if laid(distance) => after,
                    _ => &draw.main,
                };
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &draw.group, &[]);
                bound.draw(&mut pass, mesh, draw);
                stats.draws += 1;
                stats.triangles += draw.indices.len() / 3;
            }
        }
        // The marks lie on the ground, so the car's reflection cube shows them too (ours). Each cube face is its own
        // submission, so the marks' view written here is the face's.
        if let Some(marks) = &self.skid_marks {
            marks.draw(&mut pass, &self.queue, view_projection);
        }
        if let Some(shadow) = &self.car_shadow {
            shadow.draw(&mut pass, &self.queue, view_projection);
        }
        // The main view without reflections ends here; with them, the particles go over the reflections. The mirrors
        // draw the game's particles.
        if let (Some(particles), true) = (&self.particles, (view == View::Main && !keep[0]) || view.mirror()) {
            particles.draw(&mut pass);
        }
        stats
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
            if view == View::Reflection && (mesh.interactive.is_some() || mesh.cell_far_from(eye)) {
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
                    View::Surroundings => Some(CELL_REACH),
                    View::Mirror { .. } => Some(super::mirrors::REACH),
                };
                let (distance, radius) = (center.distance(eye), half_extent.length());
                if reach.is_some_and(|reach| distance - radius > reach)
                    || (view == View::Surroundings && reflection::tiny(radius, distance))
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
            let far = (view == View::Surroundings || view.mirror())
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
}

impl Renderer {
    /// The colour attachment of a pass drawn over the kept frame of `draw_scene`, which resolved it into `scene`, and
    /// resolved into `target`. With one sample `draw_scene` drew into `scene` itself: the pass loads `target`, holding
    /// the frame once `scene` is copied into it when they differ (the glass's and the reflections' own scene).
    pub(super) fn over_attachment<'a>(
        &'a self,
        encoder: &mut wgpu::CommandEncoder,
        target: &'a wgpu::TextureView,
        scene: &wgpu::TextureView,
    ) -> wgpu::RenderPassColorAttachment<'a> {
        if self.samples > 1 {
            return wgpu::RenderPassColorAttachment {
                view: &self.color_view,
                depth_slice: None,
                resolve_target: Some(target),
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard },
            };
        }
        if scene != target {
            let size = scene.texture().size();
            encoder.copy_texture_to_texture(scene.texture().as_image_copy(), target.texture().as_image_copy(), size);
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
