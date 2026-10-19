//! Temporal anti-aliasing (`taa.wgsl`): a renderer addition behind [`crate::renderer::Enhance`], not the game's look.
//! The renderer draws each frame with [`Taa::jitter`] added to its projection, into [`Taa::input`]; [`Taa::draw`]
//! blends it with the reprojected history and writes the result into the next history and the HDR post pass's scene.
//!
//! Drawn at a render scale below 1 ([`crate::renderer::Renderer::set_render_scale`]) it upsamples (`fs_taau`): the
//! input is at the lower resolution, and the history and the result are at the target's, so the jittered frames fill
//! in the pixels between their samples over time. A pass at the frame's size first works out each frame pixel's
//! clipping box and where its nearest depth is (`fs_taau_box`, into [`BOX_FORMAT`] targets), which `fs_taau` reads.
//!
//! With the wind ([`crate::renderer::Enhance::wind`]) the swaying plants draw their motion into [`Taa::velocity`]
//! (`fs_velocity` in `shader.wgsl`), which the pass adds where it is set; everything else reprojects from the depth.

use crate::post::SCENE_FORMAT;
use crate::renderer::{depth_binding, depth_source, texture_entry, TEXTURE};
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4};

pub(crate) const SHADER: &str = include_str!("taa.wgsl");
/// The plants' motion in the wind ([`Taa::velocity`]): x and y in metres in stage space (z follows x along the wind),
/// zero where none. Two channels: the pass clears and stores the whole frame.
pub(crate) const VELOCITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// Upsampling, the clipping box per frame pixel: its low corner, and its high corner and where the nearest depth is
/// (`fs_taau_box`).
const BOX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The pass's shader for frames of `samples`: the depth and the motion multisampled or not.
pub(crate) fn source(samples: u32) -> String {
    match samples {
        1 => depth_source(SHADER, 1).replace("texture_multisampled_2d<f32>", "texture_2d<f32>"),
        _ => SHADER.to_owned(),
    }
}
/// Jitter positions before the sequence repeats, at full resolution; upsampling, as many per target pixel.
const JITTERS: u32 = 8;

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    reprojection: [[f32; 4]; 4],
    car_reprojection: [[f32; 4]; 4],
    to_car: [[f32; 4]; 4],
    car_low: [f32; 4],
    car_high: [f32; 4],
    params: [f32; 4],
    /// Upsampling: the frame's and the target's widths and heights.
    sizes: [f32; 4],
    /// The last frame's view-projection without its jitter, and the w row of this frame's clip space to stage space
    /// (for the plants' motion, `VELOCITY`).
    previous: [[f32; 4]; 4],
    unproject_w: [f32; 4],
}

/// The car body this frame: its model space to stage space and its bounds in model space.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Car {
    pub(crate) model_to_stage: Mat4,
    pub(crate) low: Vec3,
    pub(crate) high: Vec3,
}

pub(crate) struct Taa {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    /// `fs_taau_box` and `fs_taau`, for frames drawn smaller than the target, and the layout of the boxes' group.
    boxes: wgpu::RenderPipeline,
    upsample: wgpu::RenderPipeline,
    box_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    /// The size frames are drawn at, and the target's (the history's).
    size: (u32, u32),
    output: (u32, u32),
    /// The frame drawn at `size`, which the pass reads.
    input: wgpu::TextureView,
    /// Two histories: each frame reads one and writes the other.
    histories: [wgpu::Texture; 2],
    /// Reading the frame and history `i`.
    groups: [wgpu::BindGroup; 2],
    /// Upsampling, the boxes at `size` (else 1 texel, unused) and the group that reads them.
    box_targets: [wgpu::TextureView; 2],
    box_group: wgpu::BindGroup,
    /// With the wind, the plants' motion this frame at `size` ([`Taa::velocity`]); else 1 texel.
    velocity: wgpu::TextureView,
    wind: bool,
    samples: u32,
    /// The time the last frame was drawn at ([`Taa::time`]).
    time: Option<f32>,
    /// The history the next frame reads.
    read: usize,
    /// The last frame's matrix without its jitter, `None` before the first frame or after a resize.
    previous: Option<Mat4>,
    /// The car this frame ([`Taa::set_car`]) and its model-to-stage matrix the last frame.
    car: Option<Car>,
    previous_car: Option<Mat4>,
    frame: u32,
}

impl Taa {
    /// For frames of `size` with the `depth` of `samples`, drawn into [`Taa::input`] and resolved at `output` pixels;
    /// with the plants' motion when the `wind` sways them.
    pub(crate) fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        output: (u32, u32),
        depth: &wgpu::TextureView,
        samples: u32,
        wind: bool,
    ) -> Taa {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa"),
            entries: &[
                texture_entry(0, TEXTURE),
                texture_entry(1, TEXTURE),
                texture_entry(2, depth_binding(samples)),
                texture_entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                texture_entry(
                    4,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                texture_entry(
                    5,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: samples > 1,
                    },
                ),
            ],
        });
        let box_entry = |binding| {
            let ty = wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            };
            texture_entry(binding, ty)
        };
        let box_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa boxes"),
            entries: &[box_entry(0), box_entry(1)],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa"),
            source: wgpu::ShaderSource::Wgsl(source(samples).into()),
        });
        let pipeline_layout = |groups: &[&wgpu::BindGroupLayout]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("taa"),
                bind_group_layouts: groups,
                push_constant_ranges: &[],
            })
        };
        let (single, with_boxes) = (pipeline_layout(&[&layout]), pipeline_layout(&[&layout, &box_layout]));
        let make = |entry, layout, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(layout),
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
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("VELOCITY", f64::from(u8::from(wind)))],
                        ..Default::default()
                    },
                    targets,
                }),
                multiview: None,
                cache: None,
            })
        };
        // Into the history and the scene.
        let outputs = [Some(SCENE_FORMAT.into()), Some(SCENE_FORMAT.into())];
        let pipeline = make("fs_taa", &single, &outputs);
        let boxes = make("fs_taau_box", &single, &[Some(BOX_FORMAT.into()), Some(BOX_FORMAT.into())]);
        let upsample = make("fs_taau", &with_boxes, &outputs);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("taa"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("taa"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let Targets { input, histories, groups, box_targets, box_group, velocity } =
            targets(device, [&layout, &box_layout], &sampler, &uniform, [size, output], depth, (samples, wind));
        Taa {
            layout,
            pipeline,
            boxes,
            upsample,
            box_layout,
            sampler,
            uniform,
            size,
            output,
            input,
            histories,
            groups,
            box_targets,
            box_group,
            velocity,
            wind,
            samples,
            time: None,
            read: 0,
            previous: None,
            car: None,
            previous_car: None,
            frame: 0,
        }
    }

    /// New targets for frames of `size` resolved at `output` pixels; the history starts over.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        output: (u32, u32),
        depth: &wgpu::TextureView,
    ) {
        let motion = (self.samples, self.wind);
        let layouts = [&self.layout, &self.box_layout];
        Targets {
            input: self.input,
            histories: self.histories,
            groups: self.groups,
            box_targets: self.box_targets,
            box_group: self.box_group,
            velocity: self.velocity,
        } = targets(device, layouts, &self.sampler, &self.uniform, [size, output], depth, motion);
        (self.size, self.output) = (size, output);
        self.previous = None;
    }

    /// With the wind, the target the plants draw their motion into this frame, cleared first (`fs_velocity`).
    pub(crate) fn velocity(&self) -> Option<&wgpu::TextureView> {
        self.wind.then_some(&self.velocity)
    }

    /// The time the last frame was drawn at, for the plants' motion since (`Frame::wind` in `shader.wgsl`).
    pub(crate) fn time(&self) -> Option<f32> {
        self.time
    }

    /// Frames drawn so far (the jitter's index).
    pub(crate) fn frame(&self) -> u32 {
        self.frame
    }

    /// The target the frame resolves into.
    pub(crate) fn input(&self) -> &wgpu::TextureView {
        &self.input
    }

    /// The car this frame, whose pixels reproject with its motion rather than as still stage (else the car seen from
    /// the chase or the cockpit, which moves with the camera, smears).
    pub(crate) fn set_car(&mut self, car: Option<Car>) {
        self.car = car;
    }

    /// The matrix that moves this frame's clip space by its jitter: a Halton (2, 3) point within the pixel.
    pub(crate) fn jitter(&self) -> Mat4 {
        let offset = self.offset();
        let (width, height) = self.size;
        Mat4::from_translation(Vec3::new(2.0 * offset.x / width as f32, -2.0 * offset.y / height as f32, 0.0))
    }

    /// This frame's jitter in the frame's pixels, -0.5 to 0.5.
    fn offset(&self) -> Vec2 {
        let halton = |mut i: u32, base: u32| {
            let (mut f, mut r) = (1.0, 0.0);
            while i > 0 {
                f /= base as f32;
                r += f * (i % base) as f32;
                i /= base;
            }
            r
        };
        // Upsampling, the target pixels each frame pixel covers each get as many positions.
        let ratio = (self.output.0 * self.output.1) as f32 / (self.size.0 * self.size.1).max(1) as f32;
        let jitters = match self.size == self.output {
            true => JITTERS,
            false => (JITTERS as f32 * ratio).ceil() as u32,
        };
        let i = self.frame % jitters + 1;
        Vec2::new(halton(i, 2) - 0.5, halton(i, 3) - 0.5)
    }

    /// Blends the frame in [`Taa::input`] (drawn with `view_projection` moved by [`Taa::jitter`], at `time`) with the
    /// history, and writes the result into `scene`. Reprojection uses the matrices without their jitter, so a still view reads
    /// the history texel for texel rather than resampling it every frame.
    pub(crate) fn draw(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::Texture,
        view_projection: Mat4,
        time: f32,
    ) {
        // In double precision, so that a still view reprojects by exactly the identity.
        let previous = self.previous.unwrap_or(view_projection).as_dmat4();
        let unproject = view_projection.as_dmat4().inverse();
        let reprojection = previous * unproject;
        // The car's pixels: back to its model space now, and out through where the car and the view were.
        let (car_reprojection, to_car, low, high) = match self.car {
            Some(car) => {
                let to_car = car.model_to_stage.as_dmat4().inverse() * unproject;
                let before = self.previous_car.unwrap_or(car.model_to_stage).as_dmat4();
                (previous * before * to_car, to_car, car.low.extend(1.0), car.high.extend(0.0))
            }
            None => (reprojection, unproject, Vec4::ONE, Vec4::ZERO),
        };
        let offset = self.offset();
        let uniform = Uniform {
            reprojection: reprojection.as_mat4().to_cols_array_2d(),
            car_reprojection: car_reprojection.as_mat4().to_cols_array_2d(),
            to_car: to_car.as_mat4().to_cols_array_2d(),
            car_low: low.to_array(),
            car_high: high.to_array(),
            params: [f32::from(u8::from(self.previous.is_some())), 0.0, offset.x, offset.y],
            sizes: [self.size.0, self.size.1, self.output.0, self.output.1].map(|side| side as f32),
            previous: previous.as_mat4().to_cols_array_2d(),
            unproject_w: unproject.row(3).as_vec4().to_array(),
        };
        queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
        let write = 1 - self.read;
        self.encode(encoder, [&self.histories[write], scene]);
        self.read = write;
        self.previous = Some(view_projection);
        self.previous_car = self.car.map(|car| car.model_to_stage);
        self.time = Some(time);
        self.frame = self.frame.wrapping_add(1);
    }

    /// The passes that resolve the frame into `outputs` (the history written and the scene): the boxes first when
    /// upsampling.
    fn encode(&self, encoder: &mut wgpu::CommandEncoder, outputs: [&wgpu::Texture; 2]) {
        let upsampling = self.size != self.output;
        let pass = |encoder: &mut wgpu::CommandEncoder, label, views: [&wgpu::TextureView; 2]| {
            let ops = wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store };
            let attachments = views.map(|view| {
                Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_bind_group(0, &self.groups[self.read], &[]);
            pass.forget_lifetime()
        };
        if upsampling {
            let mut pass = pass(encoder, "taa boxes", self.box_targets.each_ref());
            pass.set_pipeline(&self.boxes);
            pass.draw(0..3, 0..1);
        }
        let views = outputs.map(|texture| texture.create_view(&Default::default()));
        let mut pass = pass(encoder, "taa", views.each_ref());
        pass.set_pipeline(if upsampling { &self.upsample } else { &self.pipeline });
        if upsampling {
            pass.set_bind_group(1, &self.box_group, &[]);
        }
        pass.draw(0..3, 0..1);
    }
}

/// What [`targets`] makes.
struct Targets {
    input: wgpu::TextureView,
    histories: [wgpu::Texture; 2],
    groups: [wgpu::BindGroup; 2],
    box_targets: [wgpu::TextureView; 2],
    box_group: wgpu::BindGroup,
    velocity: wgpu::TextureView,
}

/// For frames of `size` into a target of `output`: the frame's own target, the two histories, the bind groups that
/// read the frame and each, the upsampling's boxes at `size` (else 1 texel, unused) and their group, and the plants'
/// motion of `samples`, at `size` with the `wind` (else 1 texel, unread).
fn targets(
    device: &wgpu::Device,
    [layout, box_layout]: [&wgpu::BindGroupLayout; 2],
    sampler: &wgpu::Sampler,
    uniform: &wgpu::Buffer,
    [size, output]: [(u32, u32); 2],
    depth: &wgpu::TextureView,
    (samples, wind): (u32, bool),
) -> Targets {
    let texture = |label, (width, height): (u32, u32), usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | usage,
            view_formats: &[],
        })
    };
    let input = texture("taa input", size, wgpu::TextureUsages::COPY_DST).create_view(&Default::default());
    let histories = [(); 2].map(|_| texture("taa history", output, wgpu::TextureUsages::empty()));
    let (width, height) = if wind { size } else { (1, 1) };
    let velocity = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("taa velocity"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: VELOCITY_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let velocity = velocity.create_view(&Default::default());
    let (width, height) = if size != output { size } else { (1, 1) };
    let box_targets = [(); 2].map(|_| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("taa box"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: BOX_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        texture.create_view(&Default::default())
    });
    let box_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("taa boxes"),
        layout: box_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&box_targets[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&box_targets[1]) },
        ],
    });
    let groups = [0, 1].map(|i| {
        let history = histories[i].create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&input) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&history) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&velocity) },
            ],
        })
    });
    Targets { input, histories, groups, box_targets, box_group, velocity }
}
