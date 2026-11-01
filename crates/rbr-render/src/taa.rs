//! Temporal anti-aliasing ([`AntiAliasing::Taa`](crate::renderer::AntiAliasing::Taa); ours, after gp4's), the
//! renderer's default.
//!
//! The main view's projection is shifted every frame by a sub-pixel offset from the Halton (2, 3) sequence
//! ([`jitter`]: 8 phases at full size, 16 below it), so successive frames sample different points inside each pixel;
//! the mirrors, the reflection cubes and the sun's cascades keep the unjittered view. The frame is drawn into
//! [`Taa::raw`]; the resolve (`taa.wgsl`) blends it into a history of the previous ones, carried by each pixel's
//! motion: the camera's (the point from the depth, seen by the previous frame's unjittered view-projection) or, inside
//! the car's box, the car's (its previous pose, [`Renderer::set_car_body`](crate::renderer::Renderer::set_car_body)),
//! and inside that the turn since the last frame of the steering wheel or of a wiper ([`Room`]).
//! The history is clipped to the current neighbourhood's colour so that what was uncovered or changed does not ghost.
//! The result goes to the post pass's scene and is kept as the next frame's history.
//!
//! The history starts again (the new frame alone) after a resize, on a switch between the in-car and outside views, when
//! the eye jumps further than [`CUT`] in a frame (a camera change, a restart), or on
//! [`Renderer::reset_history`](crate::renderer::Renderer::reset_history). A frame drawn again of the same view (a
//! pause, a screenshot's frames) keeps gathering samples.

use std::f32::consts::TAU;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::post::SCENE_FORMAT;
use crate::renderer::{depth_binding, texture_entry, TEXTURE};

pub(crate) const SHADER: &str = include_str!("taa.wgsl");
/// The new frame's base share of the history.
const BLEND: f32 = 0.1;
/// The clip box's half size, in standard deviations of the neighbourhood.
const CLIP_SIGMAS: f32 = 1.25;
/// How far the eye may move in a frame before the history is dropped, metres (200 km/h at 10 frames a second is 5.6).
const CUT: f32 = 8.0;
/// How far the car's box reaches past its body parts' bounds, metres.
const CAR_MARGIN: f32 = 0.1;
/// A turning part's bands of distance from its axis ([`Room`]; `BANDS` in `taa.wgsl`).
const BANDS: usize = 8;
/// How far a turning part's room reaches past its meshes, metres: out from the axis (and a wiper's, about it) and
/// along it.
const ROOM_MARGIN: (f32, f32) = (0.01, 0.015);
/// The turning parts the resolve carries at most (`ROOMS` in `taa.wgsl`): the steering wheel and the wipers.
const ROOMS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_to_prev: [f32; 16],
    car_to_local: [f32; 16],
    car_to_prev: [f32; 16],
    car_low: [f32; 4],
    car_high: [f32; 4],
    car_rect: [f32; 4],
    rooms: [RoomUniform; ROOMS],
    size: [f32; 4],
    params: [f32; 4],
}

/// A turning part's room in the resolve's uniform (`Room` in `taa.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RoomUniform {
    /// The car's model space to the room's frame with the part at rest, and the part's turn since the last frame
    /// undone, in the car's model space.
    to_rest: [f32; 16],
    turn: [f32; 16],
    /// x the radius, y 1 = it turned.
    size: [f32; 4],
    bands: [[f32; 4]; BANDS],
}

/// The room a part of the car turns in (ours), for the motion of what it covers: car model space to the frame of the
/// axis it turns about (its node: the axis through the origin along `z`), its radius, and per band of distance from
/// the axis, the depth along it and the angles about it that its meshes fill at rest. The steering wheel fills every
/// angle (`characters::incar::Crew::wheel_turn`: its turn spins the room on itself); a wiper, its arm's and blade's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Room {
    to_node: Mat4,
    radius: f32,
    /// Per band: the depth's low and high, the angle's low and high.
    bands: [[f32; 4]; BANDS],
    around: bool,
}

impl Room {
    /// The room of meshes whose model-space `points` turn about `node`, all `around` it or at the angles they fill;
    /// `None` without any.
    pub(crate) fn new(node: Mat4, points: impl IntoIterator<Item = Vec3>, around: bool) -> Option<Room> {
        let mut to_node = node.inverse();
        let mut points: Vec<Vec3> = points.into_iter().map(|p| to_node.transform_point3(p)).collect();
        if !around {
            // Angles from the points' mean direction, so that theirs do not wrap.
            let mean = points.iter().map(|p| p.truncate()).sum::<Vec2>();
            let turn = Mat4::from_rotation_z(-mean.y.atan2(mean.x));
            to_node = turn * to_node;
            points.iter_mut().for_each(|p| *p = turn.transform_point3(*p));
        }
        let radius = points.iter().map(|p| p.truncate().length()).reduce(f32::max)? + ROOM_MARGIN.0;
        let band = |p: Vec3| ((p.truncate().length() / radius * BANDS as f32) as usize).min(BANDS - 1);
        let mut found = [None; BANDS];
        for p in points {
            let at = [p.z, p.z, p.y.atan2(p.x), p.y.atan2(p.x)];
            let slot = &mut found[band(p)];
            *slot = Some(slot.map_or(at, |b| union(b, at)));
        }
        // A band without vertices (crossed by a spoke's or a blade's long faces, or the hub's middle) takes the room of
        // those each side of it.
        let bands = std::array::from_fn(|i| {
            let inner = found[..i].iter().rev().flatten().take(1);
            let outer = found[i + 1..].iter().flatten().take(1);
            let [low, high, from, to] = found[i].or(inner.chain(outer).copied().reduce(union)).unwrap_or_default();
            let spread = ROOM_MARGIN.0 / (radius * i as f32 / BANDS as f32).max(ROOM_MARGIN.0 * 4.0);
            let (from, to) = if around { (-TAU, TAU) } else { (from - spread, to + spread) };
            [low - ROOM_MARGIN.1, high + ROOM_MARGIN.1, from, to]
        });
        Some(Room { to_node, radius, bands, around })
    }
}

/// Element `i` (from 1) of the Halton sequence of base `b`, in 0..1.
fn halton(mut i: u32, b: u32) -> f32 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= b as f32;
        r += f * (i % b) as f32;
        i /= b;
    }
    r
}

/// The jitter of frame `n` in pixels (x right, y down, -0.5..0.5): Halton (2, 3), 8 phases at full size, 16 when the
/// frame is drawn below the output's size (an output pixel then spans more than one of the frame's).
pub(crate) fn jitter(n: u32, upscaling: bool) -> Vec2 {
    let phases = if upscaling { 16 } else { 8 };
    let i = n % phases + 1;
    Vec2::new(halton(i, 2) - 0.5, halton(i, 3) - 0.5)
}

/// The clip space shift that moves what a projection draws by `jitter` pixels of a `width` x `height` target (left
/// multiplied onto the view-projection).
pub(crate) fn shift(jitter: Vec2, (width, height): (u32, u32)) -> Mat4 {
    let x = 2.0 * jitter.x / width.max(1) as f32;
    let y = -2.0 * jitter.y / height.max(1) as f32;
    Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::new(x, y, 0.0, 1.0))
}

/// The rectangle (uv, y down: min x, min y, max x, max y) the box `low`..`high` covers on the screen under `to_clip`,
/// widened a little; the whole screen when it reaches behind the eye, `None` off the screen.
fn screen_rect(to_clip: Mat4, (low, high): (Vec3, Vec3)) -> Option<[f32; 4]> {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for corner in 0..8 {
        let pick = |bit: u32, l: f32, h: f32| if corner & bit == 0 { l } else { h };
        let p = Vec3::new(pick(1, low.x, high.x), pick(2, low.y, high.y), pick(4, low.z, high.z));
        let c = to_clip * p.extend(1.0);
        if c.w < 0.05 {
            return Some([0.0, 0.0, 1.0, 1.0]);
        }
        let (u, v) = (c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
        r = [r[0].min(u), r[1].min(v), r[2].max(u), r[3].max(v)];
    }
    let pad = 0.01;
    let r = [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad];
    (r[2] >= 0.0 && r[3] >= 0.0 && r[0] <= 1.0 && r[1] <= 1.0).then_some(r)
}

/// The depth and angle ranges holding both.
fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0].min(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].max(b[3])]
}

/// The frame the resolve drew last.
struct Last {
    view_projection: Mat4,
    car: Option<Mat4>,
    /// The turning parts and their turns.
    rooms: Vec<(Room, Mat4)>,
    outside: bool,
}

/// What [`Taa::prepare`] takes of a frame.
pub(crate) struct Frame<'a> {
    /// The main view's unjittered view-projection.
    pub(crate) view_projection: Mat4,
    /// The car body's model space to stage space, and its parts' box in model space.
    pub(crate) car: Option<(Mat4, (Vec3, Vec3))>,
    /// The turning parts drawn (the steering wheel, the wipers) and their turns in the car's model space.
    pub(crate) rooms: &'a [(Room, Mat4)],
    /// The camera is outside the car.
    pub(crate) outside: bool,
}

pub(crate) struct Taa {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// The frame drawn this frame, jittered.
    raw: wgpu::TextureView,
    /// The resolved frames: the one [`Taa::current`] holds the latest.
    history: [wgpu::TextureView; 2],
    /// Reading history `i` (and the frame and its depth).
    groups: [wgpu::BindGroup; 2],
    current: usize,
    /// Frames begun (the jitter's phase).
    frame: u32,
    /// The frame's size and the output's.
    size: (u32, u32),
    upscaling: bool,
    last: Option<Last>,
    /// `RBR_TAA_DEBUG=motion` or `mask`: show the motion or the car glass's mask instead of the frame (0: the frame).
    debug: f32,
}

impl Taa {
    /// The resolve for frames of `size` (drawn below the output's size with `upscaling`) and their `depth`.
    pub(crate) fn new(device: &wgpu::Device, size: (u32, u32), upscaling: bool, depth: &wgpu::TextureView) -> Taa {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                texture_entry(1, TEXTURE),
                texture_entry(2, depth_binding()),
                texture_entry(3, TEXTURE),
                texture_entry(4, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let target = Some(wgpu::ColorTargetState::from(SCENE_FORMAT));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("taa"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("taa"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_taa"),
                compilation_options: Default::default(),
                targets: &[target.clone(), target],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("taa"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("taa history"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (raw, history, groups) = targets(device, &layout, &uniform, &sampler, depth, size);
        Taa {
            layout,
            pipeline,
            uniform,
            sampler,
            raw,
            history,
            groups,
            current: 0,
            frame: 0,
            size,
            upscaling,
            last: None,
            debug: match std::env::var("RBR_TAA_DEBUG").as_deref() {
                Ok("motion") => 1.0,
                Ok("mask") => 2.0,
                _ => 0.0,
            },
        }
    }

    /// For frames of `size` (below the output's with `upscaling`) and their `depth`; the history starts again.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        upscaling: bool,
        depth: &wgpu::TextureView,
    ) {
        (self.raw, self.history, self.groups) =
            targets(device, &self.layout, &self.uniform, &self.sampler, depth, size);
        (self.size, self.upscaling, self.last) = (size, upscaling, None);
    }

    /// The target the frame is drawn into.
    pub(crate) fn raw(&self) -> &wgpu::TextureView {
        &self.raw
    }

    /// Starts the history again with the next frame.
    pub(crate) fn reset(&mut self) {
        self.last = None;
    }

    /// Begins a frame: the shift its main view's view-projection takes ([`shift`]).
    pub(crate) fn begin(&mut self) -> Mat4 {
        self.frame = self.frame.wrapping_add(1);
        shift(jitter(self.frame, self.upscaling), self.size)
    }

    /// Writes the resolve's uniform for `frame`.
    pub(crate) fn prepare(&mut self, queue: &wgpu::Queue, frame: &Frame) {
        let eye = crate::camera::eye(frame.view_projection);
        let last = self.last.take().filter(|last| {
            last.outside == frame.outside && (crate::camera::eye(last.view_projection) - eye).length() <= CUT
        });
        let current = frame.view_projection.as_dmat4();
        let inverse = current.inverse();
        let previous = last.as_ref().map_or(current, |last| last.view_projection.as_dmat4());
        let mut uniform = Uniform::zeroed();
        uniform.view_to_prev = (previous * inverse).as_mat4().to_cols_array();
        let moved = (frame.car.zip(last.as_ref().and_then(|last| last.car))).and_then(|((body, (low, high)), was)| {
            let (low, high) = (low - Vec3::splat(CAR_MARGIN), high + Vec3::splat(CAR_MARGIN));
            screen_rect(frame.view_projection * body, (low, high)).map(|rect| (body, was, low, high, rect))
        });
        if let Some((body, was, low, high, rect)) = moved {
            let car_to_prev = previous * was.as_dmat4();
            uniform.car_to_local = (body.as_dmat4().inverse() * inverse).as_mat4().to_cols_array();
            uniform.car_to_prev = car_to_prev.as_mat4().to_cols_array();
            uniform.car_low = low.extend(1.0).to_array();
            uniform.car_high = high.extend(0.0).to_array();
            uniform.car_rect = rect;
            // A part that kept its turn moves with the car (a still's frames stay as they were).
            for ((room, now), slot) in frame.rooms.iter().zip(&mut uniform.rooms) {
                let then = last.as_ref().and_then(|last| last.rooms.iter().find(|(r, _)| r == room));
                let Some(&(_, then)) = then.filter(|(_, then)| then != now) else { continue };
                let undo = now.as_dmat4().inverse();
                // The steering wheel's room is the same at any turn.
                let to_rest = if room.around { room.to_node } else { (room.to_node.as_dmat4() * undo).as_mat4() };
                *slot = RoomUniform {
                    to_rest: to_rest.to_cols_array(),
                    turn: (then.as_dmat4() * undo).as_mat4().to_cols_array(),
                    size: [room.radius, 1.0, 0.0, 0.0],
                    bands: room.bands,
                };
            }
        }
        let (width, height) = (self.size.0 as f32, self.size.1 as f32);
        uniform.size = [width, height, 1.0 / width, 1.0 / height];
        let reset = f32::from(u8::from(last.is_none()));
        uniform.params = [BLEND, CLIP_SIGMAS, reset, self.debug];
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.last = Some(Last {
            view_projection: frame.view_projection,
            car: frame.car.map(|(body, _)| body),
            rooms: frame.rooms.to_vec(),
            outside: frame.outside,
        });
    }

    /// Resolves the frame in [`Taa::raw`] into `scene` and the next history.
    pub(crate) fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &wgpu::TextureView) {
        let write = 1 - self.current;
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("taa"),
            color_attachments: &[attachment(scene), attachment(&self.history[write])],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.groups[self.current], &[]);
        pass.draw(0..3, 0..1);
        self.current = write;
    }
}

/// The frame's target, the two histories and the bind groups reading each, for frames of `size`.
fn targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    depth: &wgpu::TextureView,
    (width, height): (u32, u32),
) -> (wgpu::TextureView, [wgpu::TextureView; 2], [wgpu::BindGroup; 2]) {
    let texture = |label| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                // The reflections pass copies into the frame (as into the post pass's scene).
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let raw = texture("taa frame");
    let history = [texture("taa history"), texture("taa history")];
    let groups = [0, 1].map(|read| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&raw) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&history[read]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    });
    (raw, history, groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn jitter_covers_the_pixel_evenly() {
        let points: Vec<Vec2> = (1..=8).map(|n| jitter(n, false)).collect();
        assert!(points.iter().all(|p| p.x.abs() <= 0.5 && p.y.abs() <= 0.5));
        let mean = points.iter().copied().sum::<Vec2>() / 8.0;
        assert!(mean.length() < 0.1, "{mean}");
        assert_eq!(jitter(3, false), jitter(11, false));
        assert_ne!(jitter(3, true), jitter(11, true));
    }

    #[test]
    fn the_shift_moves_a_point_by_the_jitter_in_pixels() {
        let view_projection = crate::coords::reversed(1.0, 1.5, 0.1);
        let p = Vec4::new(0.3, -0.2, 5.0, 1.0);
        let (width, height) = (300, 200);
        let pixel =
            |c: Vec4| Vec2::new((c.x / c.w * 0.5 + 0.5) * width as f32, (0.5 - c.y / c.w * 0.5) * height as f32);
        let moved = shift(Vec2::new(0.5, 0.25), (width, height)) * view_projection * p;
        let d = pixel(moved) - pixel(view_projection * p);
        assert!((d - Vec2::new(0.5, 0.25)).length() < 1e-3, "{d}");
    }

    #[test]
    fn the_wheel_fills_its_bands_and_the_gaps_between() {
        // Tilted back and off to the left; a rim at 0.18 m, 0.05 m up the column, spokes from a hub at -0.02 m with no
        // vertices between, and the column down to -0.5 m.
        let node = Mat4::from_translation(Vec3::new(-0.35, 0.6, -1.5)) * Mat4::from_rotation_x(-0.3);
        let ring = |r: f32, z: f32| (0..16).map(move |k| Vec3::from((Vec2::from_angle(k as f32 * 0.4) * r, z)));
        let column = [0.0, -0.25, -0.5].map(|z| Vec3::new(0.01, 0.0, z));
        let points = ring(0.18, 0.05).chain(ring(0.17, 0.04)).chain(ring(0.03, -0.02)).chain(column);
        let wheel = Room::new(node, points.map(|p| node.transform_point3(p)), true).unwrap();
        assert!((wheel.radius - 0.19).abs() < 1e-4, "{}", wheel.radius);
        let near = |d: [f32; 4], [low, high]: [f32; 2]| {
            (d[0] - (low - ROOM_MARGIN.1)).abs() < 1e-4 && (d[1] - (high + ROOM_MARGIN.1)).abs() < 1e-4
        };
        assert!(near(wheel.bands[0], [-0.5, 0.0]), "{:?}", wheel.bands[0]);
        assert!(near(wheel.bands[1], [-0.02, -0.02]), "{:?}", wheel.bands[1]);
        for band in 2..7 {
            assert!(near(wheel.bands[band], [-0.02, 0.05]), "{band}: {:?}", wheel.bands[band]);
        }
        assert!(near(wheel.bands[7], [0.04, 0.05]), "{:?}", wheel.bands[7]);
        assert!(wheel.bands.iter().all(|b| b[2] < -PI && b[3] > PI), "all round");
        assert_eq!(Room::new(node, [], true), None);
    }

    #[test]
    fn a_wiper_s_room_is_its_blade_s_angles() {
        // A blade parked along +y from a pivot, 0.1 to 0.5 m out, 2 cm wide and 3 cm off the glass, its axis tilted.
        let node = Mat4::from_translation(Vec3::new(0.3, 0.9, -1.0)) * Mat4::from_rotation_x(0.9);
        let blade = (0..=20).flat_map(|k| {
            let r = 0.1 + 0.02 * k as f32;
            [Vec3::new(-0.01, r, 0.03), Vec3::new(0.01, r, 0.03)]
        });
        let room = Room::new(node, blade.map(|p| node.transform_point3(p)), false).unwrap();
        assert!((room.radius - (0.5f32.hypot(0.01) + ROOM_MARGIN.0)).abs() < 1e-4, "{}", room.radius);
        // Its angles about the blade's direction: 2 cm wide at the tip, and a margin.
        let tip = room.bands[BANDS - 1];
        assert!(tip[2] < -0.02 && tip[2] > -0.1 && tip[3] > 0.02 && tip[3] < 0.1, "{tip:?}");
        assert!((tip[0] - (0.03 - ROOM_MARGIN.1)).abs() < 1e-4 && (tip[1] - (0.03 + ROOM_MARGIN.1)).abs() < 1e-4);
        // The band inside its reach takes its neighbour's room.
        assert_eq!(room.bands[0][..2], room.bands[1][..2]);
    }

    #[test]
    fn the_shift_keeps_the_eye() {
        let view_projection =
            crate::coords::reversed(1.0, 1.5, 0.1) * Mat4::from_translation(Vec3::new(-3.0, -1.0, 2.0));
        let eye = crate::camera::eye(shift(Vec2::new(0.4, -0.3), (640, 360)) * view_projection);
        assert!((eye - Vec3::new(3.0, 1.0, -2.0)).length() < 1e-3, "{eye}");
    }
}
