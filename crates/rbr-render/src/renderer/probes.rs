//! Reflection probes (ours, with [`Enhance::occlusion`](super::Enhance::occlusion)): cubes of the stage drawn once at
//! load along the driveline, in place of the surroundings cube drawn around the car every few frames. Each pixel that
//! reads the surroundings (the car's light, the wet roads, the water's missed rays, the cabin) blends the two probes
//! nearest it (`surrounding` in `shader.wgsl`), so a road far ahead reflects what stands there.

use super::frame::View;
use super::reflection::face_views;
use super::uniforms::FrameUniform;
use super::Renderer;
use bytemuck::cast_slice;
use glam::{Mat4, Vec2, Vec3};

/// The probes' spacing along the driveline at most, metres; longer stages space them out to [`MOST`].
const SPACING: f32 = 40.0;
/// The most probes: 64 MB at 64 texels with the float format, four times that at 128.
pub(super) const MOST: usize = 256;
/// How far above the driveline a probe sits, metres: about the car's centre.
const HEIGHT: f32 = 1.0;
/// The side of a cell of the grid that names each place's nearest probe, metres.
const CELL: f32 = 32.0;
/// The probes drawn in one submission: a submission waits on the window's frames while the loading screen draws.
const BATCH: usize = 16;

/// The probes' cube array (6 layers each) and the buffer `shader.wgsl` reads as `Probes`: the grid's corner (x, z)
/// and cell side, its columns, rows and the count, the probes' points, then each cell's nearest probe.
pub(super) struct Probes {
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) buffer: wgpu::Buffer,
    points: Vec<Vec3>,
    pub(super) baked: bool,
}

impl Probes {
    /// The probes along `driveline` (stage space) when `on`, as many as the device's array layers hold; else none,
    /// with a placeholder cube. A probe's side is the first halving of the surroundings cube's `cube_side` texels, so
    /// its levels are the cube's from 1.
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        driveline: &[Vec3],
        on: bool,
        cube_side: u32,
    ) -> Probes {
        let most = MOST.min(device.limits().max_texture_array_layers as usize / 6);
        let points = if on { place(driveline, most) } else { Vec::new() };
        let (size, mips) = if points.is_empty() { (1, 1) } else { (cube_side / 2, cube_side.ilog2()) };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("probes"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 6 * points.len().max(1) as u32 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });
        let grid = Grid::of(&points);
        let header = [grid.origin.x, grid.origin.y, CELL, 0.0];
        let counts = [grid.columns, grid.rows, points.len() as u32, 0];
        let mut placed = vec![[0.0f32; 4]; MOST];
        for (slot, point) in placed.iter_mut().zip(&points) {
            *slot = point.extend(1.0).to_array();
        }
        // The struct's size rounds up to its 16-byte alignment.
        let mut cells = grid.cells;
        cells.resize(cells.len().next_multiple_of(4), 0);
        let bytes = [cast_slice(&header), cast_slice(&counts), cast_slice(&placed), cast_slice(&cells)].concat();
        let buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("probes"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            },
        );
        Probes { texture, view, buffer, points, baked: false }
    }
}

impl Renderer {
    /// Whether the stage has probes still to draw ([`Renderer::bake_probes`]).
    pub fn has_probes(&self) -> bool {
        !self.probes.baked && !self.probes.points.is_empty()
    }

    /// Draws the probes along the stage's driveline, once: after the first frame, whose sky tables, world shadow layer
    /// and clouds they show. Returns how many it drew (0 without them or when drawn).
    pub fn bake_probes(&mut self) -> usize {
        if !self.has_probes() {
            return 0;
        }
        let start = std::time::Instant::now();
        let r = &self.surroundings;
        let size = size_of::<FrameUniform>() as u64;
        for (batch, points) in self.probes.points.chunks(BATCH).enumerate() {
            // Each face's frame uniforms, copied into the frames before it is drawn: one submission a batch.
            let faces: Vec<(usize, u32, (Mat4, Mat4))> = (points.iter().enumerate())
                .flat_map(|(k, &at)| (0..6).map(move |face| (batch * BATCH + k, face, face_views(at, face))))
                .collect();
            let uniforms: Vec<FrameUniform> = (faces.iter())
                .flat_map(|&(.., (view_projection, sky))| self.frame_uniforms(view_projection, sky, None, View::Probe))
                .collect();
            let staging = wgpu::util::DeviceExt::create_buffer_init(
                &self.device,
                &wgpu::util::BufferInitDescriptor {
                    label: Some("probe frames"),
                    contents: cast_slice(&uniforms),
                    usage: wgpu::BufferUsages::COPY_SRC,
                },
            );
            let mut encoder = self.device.create_command_encoder(&Default::default());
            for (n, &(index, face, (view_projection, _))) in faces.iter().enumerate() {
                for (k, frame) in self.frames.iter().enumerate() {
                    encoder.copy_buffer_to_buffer(&staging, (3 * n + k) as u64 * size, &frame.buffer, 0, size);
                }
                self.draw_face(&mut encoder, r, View::Probe, face, view_projection);
                for level in 1..r.cube.mip_level_count() {
                    let side = r.size >> level;
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &r.cube,
                            mip_level: level,
                            origin: wgpu::Origin3d { x: 0, y: 0, z: face },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.probes.texture,
                            mip_level: level - 1,
                            origin: wgpu::Origin3d { x: 0, y: 0, z: 6 * index as u32 + face },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                    );
                }
            }
            self.queue.submit([encoder.finish()]);
        }
        if let Err(e) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            eprintln!("probes: {e}");
        }
        self.probes.baked = true;
        // The probes' faces are left in the surroundings cube: all six are due again.
        r.at.set(None);
        let count = self.probes.points.len();
        eprintln!("probes: {count} in {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
        count
    }
}

/// Points along `line` (above it by [`HEIGHT`]), evenly spaced from its start to its end, [`SPACING`] apart or as
/// far as `most` of them need.
fn place(line: &[Vec3], most: usize) -> Vec<Vec3> {
    let lengths: Vec<f32> = line.windows(2).map(|w| w[0].distance(w[1])).collect();
    let length: f32 = lengths.iter().sum();
    if line.is_empty() || most == 0 {
        return Vec::new();
    }
    let count = ((length / SPACING).round() as usize + 1).min(most);
    let step = if count > 1 { length / (count - 1) as f32 } else { 0.0 };
    let (mut segment, mut walked) = (0, 0.0);
    (0..count)
        .map(|k| {
            let along = k as f32 * step;
            while segment + 1 < lengths.len() && walked + lengths[segment] < along {
                walked += lengths[segment];
                segment += 1;
            }
            let (a, b) = (line[segment], line.get(segment + 1).copied().unwrap_or(line[segment]));
            let t = lengths.get(segment).filter(|&&l| l > 0.0).map_or(0.0, |l| ((along - walked) / l).clamp(0.0, 1.0));
            a.lerp(b, t) + Vec3::Y * HEIGHT
        })
        .collect()
}

/// A grid of [`CELL`] metres over the probes' ground plan, a cell's width past them: each cell's nearest probe.
struct Grid {
    /// The corner's x and z.
    origin: Vec2,
    columns: u32,
    rows: u32,
    /// Row by row from the corner.
    cells: Vec<u32>,
}

impl Grid {
    fn of(points: &[Vec3]) -> Grid {
        let plan: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, p.z)).collect();
        let Some((low, high)) = plan
            .iter()
            .fold(None, |b: Option<(Vec2, Vec2)>, &p| Some(b.map_or((p, p), |(low, high)| (low.min(p), high.max(p)))))
        else {
            return Grid { origin: Vec2::ZERO, columns: 1, rows: 1, cells: vec![0] };
        };
        let origin = low - CELL;
        let [columns, rows] = ((high + CELL - origin) / CELL).ceil().as_uvec2().max(glam::UVec2::ONE).to_array();
        let nearest = |at: Vec2| {
            (0..plan.len()).min_by(|&a, &b| at.distance_squared(plan[a]).total_cmp(&at.distance_squared(plan[b])))
        };
        let cells = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| {
                let at = origin + (Vec2::new(column as f32, row as f32) + 0.5) * CELL;
                nearest(at).unwrap_or(0) as u32
            })
            .collect();
        Grid { origin, columns, rows, cells }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_spread_evenly_from_start_to_end() {
        let line = [Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 60.0)];
        let points = place(&line, MOST);
        // 160 m at 40 m: five, from the start to the end, raised.
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], Vec3::new(0.0, HEIGHT, 0.0));
        assert_eq!(points[2], Vec3::new(80.0, HEIGHT, 0.0));
        assert_eq!(points[3], Vec3::new(100.0, HEIGHT, 20.0));
        assert_eq!(points[4], Vec3::new(100.0, HEIGHT, 60.0));
        // A long stage spaces them out to the most.
        let long = place(&[Vec3::ZERO, Vec3::new(20_000.0, 0.0, 0.0)], MOST);
        assert_eq!(long.len(), MOST);
        assert!(place(&[], MOST).is_empty());
        assert_eq!(place(&[Vec3::ZERO], MOST).len(), 1);
    }

    #[test]
    fn each_cell_names_its_nearest_probe() {
        let points = [Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0)];
        let grid = Grid::of(&points);
        assert_eq!(grid.origin, Vec2::splat(-CELL));
        assert_eq!((grid.columns, grid.rows), (6, 2));
        let cell = |x: f32, z: f32| {
            let c = ((Vec2::new(x, z) - grid.origin) / CELL).floor().as_uvec2();
            grid.cells[(c.y * grid.columns + c.x) as usize]
        };
        assert_eq!(cell(10.0, 0.0), 0);
        assert_eq!(cell(90.0, 0.0), 1);
        assert_eq!(Grid::of(&[]).cells, [0]);
    }
}
