//! The physically based sky (renderer addition behind [`Enhance::sky`](crate::renderer::Enhance::sky), ours): the
//! tables of Hillaire's sky model (`skylight.wgsl`) for the stage's sun and air, read by the stage shader
//! (`sky_lut.wgsl`) for the sky's light, the haze between the eye and the stage, the sunlight's colour, and the sky's
//! irradiance and blurred reflection for the lit draws (image-based light).
//!
//! The air is ours, from the stage's `TrackSettings.ini` section ([`Atmosphere`]): the game's own scattering values
//! (`Rayleigh_Multiplier`, `Mie_Multiplier`, `Turbidity`) drive shaders that are not reproduced, and most are 0.001 or
//! 0. Clear air (Rayleigh, ozone and an aerosol as dense as the section's `crisp` or `hazy` word says) and a ground
//! haze as dense as the section's fog: it hides [`FOG_COVER`] of the stage at `FogEnd` (under a clear sky no nearer
//! than [`CLEAR_REACH`]), as the renderer's earlier aerial perspective did, in the fog's hue. Under an overcast sky ([`Atmosphere::overcast`]) the sky is the cloud
//! deck's, the CIE overcast sky letting through a quarter of the clear sky's light. Night keeps the night sky (`night.wgsl`).
//!
//! The tables also give the day's light on the stage, and the gain that brings it and the sky to the renderer's light
//! ([`DayLight`], `day_light` in `skylight.wgsl`): physical light, pre-exposed to the renderer's range, which the
//! exposure (`post.wgsl`) meters in EV100.
//!
//! The tables are drawn again only when their inputs change ([`SkyLight::update`]).

use crate::atmosphere::Atmosphere;
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::Vec3;
use std::cell::Cell;

/// The eye's height above the planet's surface, km.
const EYE_HEIGHT: f32 = 0.05;
/// How far the aerial table reaches, km (`SKY_AERIAL_REACH`).
const AERIAL_REACH: f32 = 12.0;
const TRANSMITTANCE_SIZE: (u32, u32) = (256, 64);
const MULTISCATTER_SIZE: u32 = 32;
const SKY_VIEW_SIZE: (u32, u32) = (192, 108);
const AERIAL_SIZE: u32 = 32;
/// The sky cube's size and levels, each blurrier (`SKY_CUBE_TOP` is the last level).
const CUBE_SIZE: u32 = 32;
const CUBE_LEVELS: u32 = 6;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const SHADER: &str = include_str!("skylight.wgsl");

/// The share of the stage the ground haze hides at `FogEnd` (before the section's `haze` cap; `Atmosphere::haze`).
pub(crate) const FOG_COVER: f32 = 0.7;
/// Under a clear sky the ground haze hides [`FOG_COVER`] no nearer than this, km, for `crisp`, plain and `hazy`
/// sections: their `FogEnd` (about 1 km) is the game's linear fog, which its scattering shaders replace there, and as
/// a visibility it hid the backdrop's hills in a white wall (ours).
const CLEAR_REACH: [f32; 3] = [7.7, 4.6, 2.3];
/// Air (Rayleigh) scattering at sea level per km, and its scale height (Hillaire 2020).
const RAYLEIGH: [f32; 4] = [5.802e-3, 13.558e-3, 33.1e-3, 8.0];
/// The clear sky's aerosol (Mie): its optical depth for the `crisp`, plain and `hazy` sections (clean to hazy
/// continental air at 550 nm; Hillaire's Earth, 0.005, left the sky's light under half the measured share of the day's,
/// about 15 to 20 percent at noon, and the low sun half again too bright), its scale height, km, and its anisotropy.
const AEROSOL_DEPTH: [f32; 3] = [0.05, 0.1, 0.2];
const AEROSOL_HEIGHT: f32 = 1.2;
const MIE_G: f32 = 0.7;

/// The size of the tables' light (`SkyLight` in `skylight.wgsl`), and of the day's light at its end.
const LIGHT_SIZE: u64 = 23 * 16;
const DAY_SIZE: u64 = 2 * 16;

/// The day's light on the stage (`day_light` in `skylight.wgsl`), renderer units (luminance pre-exposed): the sun's
/// square on, `w` the pre-exposure (renderer units per cd/m2); the sky's above the horizon on up-facing ground, `w` the
/// gain from the tables' units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DayLight {
    pub sun: [f32; 4],
    pub sky: [f32; 4],
}

impl DayLight {
    /// The day's light on open faces against noon's, pre-exposed (`day_level` in `shader.wgsl`), in display level: 1
    /// by day, where the pre-exposure holds it, falling in twilight once the pre-exposure stops at the night's. `up` is
    /// the sine of the sun's or the moon's elevation. The fog's colour and the marks take it.
    pub fn level(&self, up: f32) -> f32 {
        let mean = luma(self.sky) + luma(self.sun) * (1.0 + up.abs()) * 0.25;
        (mean / DAY_MEAN).max(0.0).powf(1.0 / 2.2)
    }

    /// How much of the day's light on open ground is the sun's, 0 to 1 (0.9 under a clear noon sun, near 0 under a
    /// closed cloud), faded out as the sun nears the horizon (`up` its elevation's sine, from 0.1 to 0.35): the
    /// share of the exposure's `post::SUN_EXPOSURE` that applies.
    pub fn sun_share(&self, up: f32) -> f32 {
        let up = up.max(0.0);
        let sun = luma(self.sun) * up;
        let t = ((up - 0.1) / 0.25).clamp(0.0, 1.0);
        sun / (sun + luma(self.sky)).max(1e-6) * t * t * (3.0 - 2.0 * t)
    }

    /// The sky's and the direct light's against a sun 37.5 degrees up (`NOON_SKY`, `NOON_SUN`), in display level: the
    /// game's ambient and diffuse take them (the in-car view's light, the water's), so that its sun and sky keep the
    /// stage's proportions, the moon's at night.
    pub fn parts(&self) -> (f32, f32) {
        let part = |light: [f32; 4], noon: f32| (luma(light) / noon).max(0.0).powf(1.0 / 2.2);
        (part(self.sky, NOON_SKY), part(self.sun, NOON_SUN))
    }
}

fn luma(c: [f32; 4]) -> f32 {
    Vec3::new(c[0], c[1], c[2]).dot(LUMA)
}

/// The pre-exposed mean light of `DayLight::level` by day (`DAY_MEAN` in `shader.wgsl`): the open ground's white,
/// `OPEN_LIGHT` (1.93), times noon's mean over open share (`NOON_MEAN` over `NOON_OPEN`, `skylight.wgsl`).
const DAY_MEAN: f32 = 1.93 * 0.45 / 0.75;
/// The sky's and the sun's light, pre-exposed, for a sun 37.5 degrees up: the open ground's 1.93 with the sky's 20.1
/// percent (measured on 45_O), the sun's square on.
const NOON_SKY: f32 = 1.93 * 0.201;
const NOON_SUN: f32 = 1.93 * 0.799 / 0.609;

/// The sky tables' inputs (`Sky` in `skylight.wgsl`).
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct SkyUniform {
    to_sun: [f32; 4],
    rayleigh: [f32; 4],
    mie: [f32; 4],
    haze: [f32; 4],
    ground: [f32; 4],
    shape: [f32; 4],
    level: [f32; 4],
    moon: [f32; 4],
    night_sky: [f32; 4],
}

impl SkyUniform {
    /// The inputs of the transmittance and multiple scattering tables (the air and the ground, not the sun).
    fn medium(&self) -> [[f32; 4]; 4] {
        [self.rayleigh, self.mie, self.haze, self.ground]
    }

    /// Whether the sun is within a tenth of a degree of `other`'s, the levels within half a percent and the rest the
    /// same.
    fn close_to(&self, other: &SkyUniform) -> bool {
        let near = |a: f32, b: f32, tolerance: f32| (a - b).abs() <= tolerance * a.abs().max(b.abs()).max(1e-6);
        let sun = Vec3::from_slice(&self.to_sun[..3]).dot(Vec3::from_slice(&other.to_sun[..3]));
        sun >= 0.1f32.to_radians().cos()
            && self.medium() == other.medium()
            && near(self.shape[2], other.shape[2], 0.05)
            && [self.shape[0], self.shape[1], self.shape[3], self.level[1]]
                == [other.shape[0], other.shape[1], other.shape[3], other.level[1]]
            && near(self.moon[1], other.moon[1], 0.005)
    }
}

/// The moonlight (ours): from the clock's light from `relight::MOON_FROM` (`relight::Daylight::toward`), a full
/// moon's [`MOON_LUX`] times its phase's share (`Atmosphere::moon`, none below the horizon) above the air in [`MOON_COLOUR`] (sunlight the moon reflects, a little
/// redder), through the air by Kasten and Young's air mass and [`NIGHT_EXTINCTION`]; the sky it lights on up-facing
/// ground, [`MOON_SKY`] of the moon's square-on light (the day's tables give the sky 0.15 to 0.18 of the sun's for a sun
/// 10 to 38 degrees up, measured on 45_O), in the day sky's blue ([`NIGHT_SKY_HUE`]); and the night sky's own light,
/// [`AIRGLOW_LUX`] (airglow and stars). The sky's light goes into the tables (`moonlit` in `skylight.wgsl`); cloud
/// takes the moon's as it takes the sun's (the clouds' shadows, the overcast's share). Lux, square on for the moon and
/// on up-facing ground for the sky; none of the moon while the sun is up.
const MOON_LUX: f32 = 0.27;
const MOON_COLOUR: Vec3 = Vec3::new(1.05, 0.99, 0.9);
/// The visible extinction's optical depth of a clear sky (air 0.1, aerosol 0.1, ozone 0.03 at 550 nm).
const NIGHT_EXTINCTION: f32 = 0.25;
const MOON_SKY: f32 = 0.17;
const NIGHT_SKY_HUE: Vec3 = Vec3::new(0.8, 1.0, 1.4);
const AIRGLOW_LUX: f32 = 0.002;

fn moon_light(atmosphere: &Atmosphere) -> (Vec3, Vec3) {
    let sun_up = atmosphere.sun_elevation.is_none_or(|e| e >= crate::relight::MOON_FROM);
    let up = (-atmosphere.sun_direction.y).max(0.0);
    let moon = if sun_up || up <= 0.0 {
        0.0
    } else {
        let zenith = 90.0 - up.asin().to_degrees();
        let air_mass = 1.0 / (zenith.to_radians().cos() + 0.50572 * (96.07995 - zenith).powf(-1.6364));
        MOON_LUX * atmosphere.moon.map_or(1.0, |m| m.light) * (-NIGHT_EXTINCTION * air_mass).exp()
    };
    let sky = (MOON_SKY * moon + AIRGLOW_LUX) * NIGHT_SKY_HUE / NIGHT_SKY_HUE.dot(LUMA);
    (MOON_COLOUR / MOON_COLOUR.dot(LUMA) * moon, sky)
}

/// The renderer's units per cd/m2 at night, and the most the day's pre-exposure reaches (`NIGHT_PRE_EXPOSURE` in
/// `skylight.wgsl`): the moonlit night's earlier level (the open ground's white, `MOONLIGHT` times `NIGHT_ALBEDO` in
/// `night.wgsl`, 0.004 linear, for a full moon's 0.25 lux).
pub(crate) const NIGHT_PRE_EXPOSURE: f32 = 0.004 * std::f32::consts::PI / 0.25;

const LUMA: Vec3 = Vec3::new(0.2126, 0.7152, 0.0722);

/// The sky for `atmosphere`, by day and by night (the tables then hold what is left of the twilight, and the moon's
/// light, [`moon_light`]).
pub(crate) fn sky_uniform(atmosphere: &Atmosphere) -> Option<SkyUniform> {
    let to_sun = -atmosphere.sun_direction;
    let elevation = atmosphere.sun_elevation.unwrap_or_else(|| to_sun.y.clamp(-1.0, 1.0).asin().to_degrees());
    // From `MOON_FROM` the clock's light is the moon's: the sun is where the clock puts it, else opposite the moon.
    let moon = elevation < crate::relight::MOON_FROM && atmosphere.sun_elevation.is_some();
    let flat = Vec3::new(to_sun.x, 0.0, to_sun.z).normalize_or(Vec3::X) * if moon { -1.0 } else { 1.0 };
    let to_sun = atmosphere
        .moon
        .map_or(flat * elevation.to_radians().cos() + Vec3::Y * elevation.to_radians().sin(), |m| m.to_sun);
    let words = atmosphere.section.as_deref().unwrap_or("").to_ascii_lowercase();
    let (aerosol, haze_height, clear) = if words.contains("crisp") {
        (AEROSOL_DEPTH[0], 0.15, CLEAR_REACH[0])
    } else if words.contains("hazy") {
        (AEROSOL_DEPTH[2], 0.3, CLEAR_REACH[2])
    } else {
        (AEROSOL_DEPTH[1], 0.2, CLEAR_REACH[1])
    };
    // The ground haze: what the fog hides at `FogEnd` along the ground (at least `CLEAR_REACH` under a clear sky, or
    // with our fog off), in the fog's hue.
    let fogged = atmosphere.use_fog && atmosphere.fog != Some(crate::atmosphere::Fog::Off);
    let fog_end = (atmosphere.fog_end * 0.001).max(if fogged { 0.05 } else { clear });
    let along = (-(1.0 - FOG_COVER).ln() / fog_end).clamp(0.0, 8.0);
    let fog = atmosphere.fog_color.max(Vec3::splat(1e-3)).powf(2.2);
    let hue = fog / luminance(fog);
    let haze = hue * along * (EYE_HEIGHT / haze_height).exp();
    let snow = words.contains("snow") || section_map(&words).is_some_and(|map| (10..20).contains(&map));
    let albedo = if snow { 0.6 } else { 0.2 };
    let (moon, night_sky) = moon_light(atmosphere);
    Some(SkyUniform {
        to_sun: to_sun.extend(0.0).to_array(),
        rayleigh: RAYLEIGH,
        mie: Vec3::splat(aerosol / AEROSOL_HEIGHT).extend(AEROSOL_HEIGHT).to_array(),
        haze: haze.extend(haze_height).to_array(),
        ground: Vec3::splat(albedo).extend(EYE_HEIGHT).to_array(),
        // After sunset the sky's light falls about tenfold per 2.5 degrees: the gain keeps it in half floats.
        shape: [
            MIE_G,
            AERIAL_REACH,
            10f32.powf((-elevation).clamp(0.0, -crate::relight::PHYSICAL_NIGHT) / 2.5),
            f32::from(u8::from(atmosphere.overcast())),
        ],
        level: [0.0, atmosphere.cloud_cover(), 0.0, 0.0],
        moon: moon
            .extend(if moon.max_element() > 0.0 { (-atmosphere.sun_direction.y).max(0.0) } else { 0.0 })
            .to_array(),
        night_sky: night_sky.extend(0.0).to_array(),
    })
}

fn luminance(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(1e-6)
}

/// The map number a section's name starts with (`54` in `54e_crisp_clear`).
fn section_map(words: &str) -> Option<u32> {
    let digits: String = words.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The sky tables on the GPU and the passes that fill them.
pub(crate) struct SkyLight {
    uniform: wgpu::Buffer,
    passes: [Pass; 6],
    cube: [Pass; CUBE_LEVELS as usize],
    /// The inputs the tables were last drawn for.
    drawn: Cell<Option<SkyUniform>>,
    /// The day's light from the tables last drawn (`day_light` in `skylight.wgsl`), read back once they are.
    day: Cell<Option<DayLight>>,
    readback: wgpu::Buffer,
    /// What the stage shader binds (`sky_lut.wgsl`).
    pub(crate) sky_view: wgpu::TextureView,
    pub(crate) aerial: wgpu::TextureView,
    pub(crate) cube_view: wgpu::TextureView,
    pub(crate) light: wgpu::Buffer,
}

/// One compute pass: its pipeline, bind groups and workgroup counts.
struct Pass {
    pipeline: wgpu::ComputePipeline,
    groups: Vec<wgpu::BindGroup>,
    workgroups: (u32, u32, u32),
}

impl SkyLight {
    pub(crate) fn new(device: &wgpu::Device) -> SkyLight {
        let texture = |label, (width, height): (u32, u32), layers: u32, dimension, levels: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: layers },
                mip_level_count: levels,
                sample_count: 1,
                dimension,
                format: FORMAT,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let d2 = wgpu::TextureDimension::D2;
        let transmittance = texture("sky transmittance", TRANSMITTANCE_SIZE, 1, d2, 1);
        let multiscatter = texture("sky multiple scattering", (MULTISCATTER_SIZE, MULTISCATTER_SIZE), 1, d2, 1);
        let sky_view = texture("sky view", SKY_VIEW_SIZE, 1, d2, 1);
        let shown = texture("sky view shown", SKY_VIEW_SIZE, 1, d2, 1);
        let aerial = texture("sky aerial", (AERIAL_SIZE, AERIAL_SIZE), AERIAL_SIZE, wgpu::TextureDimension::D3, 1);
        let cube = texture("sky cube", (CUBE_SIZE, CUBE_SIZE), 6, d2, CUBE_LEVELS);
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky"),
            size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Nine harmonics, the scale, the sunlight, the nine of the sky alone, the white balance and the day's light
        // (`SkyLight` in `skylight.wgsl`).
        let light = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky light"),
            size: LIGHT_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("day light"),
            size: DAY_SIZE,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky tables"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky tables"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let views = [&transmittance, &multiscatter, &sky_view].map(view);
        let aerial_view = view(&aerial);
        let shown_view = view(&shown);
        // Each pass's bindings in group 0 (`skylight.wgsl`), as its entry point uses them.
        let resource = |binding: u32| match binding {
            0 => uniform.as_entire_binding(),
            1 => wgpu::BindingResource::Sampler(&sampler),
            2..=4 => wgpu::BindingResource::TextureView(&views[binding as usize - 2]),
            5..=7 => wgpu::BindingResource::TextureView(&views[binding as usize - 5]),
            8 => wgpu::BindingResource::TextureView(&aerial_view),
            9 => light.as_entire_binding(),
            _ => wgpu::BindingResource::TextureView(&shown_view),
        };
        let pass = |entry: &str,
                    bindings: &[u32],
                    workgroups: (u32, u32, u32),
                    extra: Option<&[wgpu::BindGroupEntry]>| {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            let entries: Vec<_> =
                bindings.iter().map(|&binding| wgpu::BindGroupEntry { binding, resource: resource(binding) }).collect();
            let mut groups = vec![device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(entry),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            })];
            if let Some(extra) = extra {
                groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(entry),
                    layout: &pipeline.get_bind_group_layout(1),
                    entries: extra,
                }));
            }
            Pass { pipeline, groups, workgroups }
        };
        let groups = |(width, height): (u32, u32)| (width.div_ceil(8), height.div_ceil(8), 1);
        let passes = [
            pass("cs_transmittance", &[0, 5], groups(TRANSMITTANCE_SIZE), None),
            pass("cs_multiscatter", &[0, 1, 2, 6], groups((MULTISCATTER_SIZE, MULTISCATTER_SIZE)), None),
            pass("cs_skyview", &[0, 1, 2, 3, 7], groups(SKY_VIEW_SIZE), None),
            // The light before the aerial table, which holds the haze's colour at its scale.
            pass("cs_light", &[0, 1, 2, 4, 9], (1, 1, 1), None),
            pass("cs_shown", &[0, 4, 9, 10], groups(SKY_VIEW_SIZE), None),
            pass("cs_aerial", &[0, 1, 2, 3, 8, 9], groups((AERIAL_SIZE, AERIAL_SIZE)), None),
        ];
        let cube_passes = std::array::from_fn(|level| {
            let level = level as u32;
            let size = CUBE_SIZE >> level;
            let blur = level as f32 / (CUBE_LEVELS - 1) as f32;
            let level_uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sky cube level"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: true,
            });
            level_uniform.slice(..).get_mapped_range_mut().expect("mapped at creation").copy_from_slice(bytes_of(&[
                blur,
                size as f32,
                0.0,
                0.0,
            ]));
            level_uniform.unmap();
            let target = cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let extra = [
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&target) },
                wgpu::BindGroupEntry { binding: 1, resource: level_uniform.as_entire_binding() },
            ];
            let (x, y, _) = groups((size, size));
            pass("cs_cube", &[0, 1, 4, 9], (x, y, 6), Some(&extra))
        });
        SkyLight {
            uniform,
            passes,
            cube: cube_passes,
            drawn: Cell::new(None),
            day: Cell::new(None),
            readback,
            sky_view: shown_view,
            aerial: aerial_view,
            cube_view: cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            }),
            light,
        }
    }

    /// Whether the tables hold `sky` already, or a sky too close to it to tell apart (a clock's sun moves a tenth of a
    /// degree in about 25 s).
    pub(crate) fn holds(&self, sky: &SkyUniform) -> bool {
        self.drawn.get().is_some_and(|drawn| drawn.close_to(sky))
    }

    /// Draws the tables for `sky` into `encoder`; the air's tables only when the air changed.
    pub(crate) fn update(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, sky: &SkyUniform) {
        let drawn = self.drawn.get();
        queue.write_buffer(&self.uniform, 0, bytes_of(sky));
        let air = drawn.is_none_or(|d| d.medium() != sky.medium());
        let mut pass = encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("sky tables"), timestamp_writes: None });
        let first = if air { 0 } else { 2 };
        for p in self.passes[first..].iter().chain(&self.cube) {
            pass.set_pipeline(&p.pipeline);
            for (index, group) in p.groups.iter().enumerate() {
                pass.set_bind_group(index as u32, group, &[]);
            }
            pass.dispatch_workgroups(p.workgroups.0, p.workgroups.1, p.workgroups.2);
        }
        drop(pass);
        encoder.copy_buffer_to_buffer(&self.light, LIGHT_SIZE - DAY_SIZE, &self.readback, 0, DAY_SIZE);
        self.drawn.set(Some(*sky));
        self.day.set(None);
    }

    /// The day's light of the tables last drawn, read back (waiting for the GPU) the first time after they are.
    pub(crate) fn day(&self, device: &wgpu::Device) -> Option<DayLight> {
        self.drawn.get()?;
        if self.day.get().is_none() {
            let slice = self.readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
            let v: [f32; 8] = bytemuck::pod_read_unaligned(&slice.get_mapped_range().ok()?);
            self.readback.unmap();
            self.day.set(Some(DayLight { sun: v[..4].try_into().unwrap(), sky: v[4..].try_into().unwrap() }));
        }
        self.day.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_haze_follows_the_fog() {
        let clear = Atmosphere { section: Some("54E_crisp_clear".into()), fog_end: 975.0, ..Atmosphere::default() };
        let far = Atmosphere { fog_end: 80_000.0, ..clear.clone() };
        let near = Atmosphere { fog_end: 300.0, ..clear.clone() };
        let [haze, far_haze, near_haze] = [&clear, &far, &near].map(|a| sky_uniform(a).unwrap().haze);
        // Under a clear sky the fog's end sets the haze only past `CLEAR_REACH`; its hue is the fog's (blue for the
        // clear sections' fog).
        assert!(haze[1] > 3.0 * far_haze[1]);
        assert_eq!(near_haze, haze);
        assert!(far_haze[2] > far_haze[0]);
        // `crisp` keeps a thinner haze layer than `hazy`.
        let hazy = Atmosphere { section: Some("54E_hazy_clear".into()), ..clear.clone() };
        assert!(sky_uniform(&hazy).unwrap().haze[3] > far_haze[3]);
    }

    #[test]
    fn the_sun_keeps_its_true_elevation_and_the_gain_follows_dusk() {
        let mut a = Atmosphere::default();
        let day = sky_uniform(&a).unwrap();
        assert!((Vec3::from_slice(&day.to_sun[..3]) + a.sun_direction).length() < 1e-5);
        assert_eq!(day.shape[2], 1.0);
        a.sun_elevation = Some(-5.0);
        let dusk = sky_uniform(&a).unwrap();
        assert!((dusk.to_sun[1] - (-5f32).to_radians().sin()).abs() < 1e-6);
        assert!((dusk.shape[2] - 100.0).abs() < 1e-3);
        // The air is the same, so only the sun's tables are drawn again.
        assert_eq!(day.medium(), dusk.medium());
    }

    #[test]
    fn a_moving_clock_redraws_every_tenth_of_a_degree() {
        let sky = |elevation: f32| {
            sky_uniform(&Atmosphere { sun_elevation: Some(elevation), ..Atmosphere::default() }).unwrap()
        };
        assert!(sky(20.0).close_to(&sky(20.05)));
        assert!(!sky(20.0).close_to(&sky(20.2)));
        let cloudy = Atmosphere { section: Some("54E_crisp_partcloud".into()), ..Atmosphere::default() };
        assert!(!sky(20.0).close_to(&sky_uniform(&cloudy).unwrap()));
    }

    #[test]
    fn snow_stages_have_a_bright_ground() {
        let snow = Atmosphere { section: Some("10O_N_heavysnow_heavycloud".into()), ..Atmosphere::default() };
        let gravel = Atmosphere { section: Some("54E_crisp_clear".into()), ..Atmosphere::default() };
        assert!(sky_uniform(&snow).unwrap().ground[0] > sky_uniform(&gravel).unwrap().ground[0]);
    }
}
