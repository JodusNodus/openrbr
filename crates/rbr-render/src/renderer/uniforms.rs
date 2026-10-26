//! The frame and draw-mode uniforms (`Frame` and `Mode` in `shader.wgsl`), the lamps' (`night.wgsl`), and
//! [`Renderer::write_frames`].

use super::frame::View;
use super::shadows::{
    filter_steps, map_box, GROUND_REACH, GROUND_SCALE, OVERCAST_BLUR, OVERCAST_SHADE, SHADOW_MOST, SHADOW_SCALE,
    SHADOW_SIZE, SUN_SHADE,
};
use super::Renderer;
use crate::camera;
use crate::lights::Lamp;
use crate::stage::{Anchor, Layer};
use crate::sun::{self, Cascades};
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::Mat4;

/// The shader mode uniform (`Mode` in `shader.wgsl`).
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(super) struct ModeUniform {
    pub(super) second_texture: u32,
    pub(super) alpha_test: u32,
    pub(super) alpha_ref: f32,
    pub(super) mip_bias: f32,
    pub(super) coverage: u32,
    /// 1 for a lit draw ([`Shade`](crate::stage::Shade)), 2 for lit glass, 3 for a lit lamp's part.
    pub(super) lit: u32,
    /// 1 for the water surface (`water` in `shader.wgsl`).
    pub(super) water: u32,
    /// 1 for a draw with a specular map (`road_shine` in `shader.wgsl`).
    pub(super) specular: u32,
    /// `Ka`, `Kd`, `CookTorranceKs`, `CookTorranceM`.
    pub(super) light: [f32; 4],
    /// `FresnelMin`, `FresnelMax`, `Kf`.
    pub(super) fresnel: [f32; 4],
    /// `x` 1 for a dial with an emissive mask in `glow_texture` (`DIAL_GLOW` in `shader.wgsl`), `y` 1 when its alpha
    /// counts.
    pub(super) glow: [f32; 4],
    /// `x` a glass pane's share of the mud film (`Shade::grime`).
    pub(super) grime: [f32; 4],
}

/// The frame uniform (`Frame` in `shader.wgsl`): the clip matrix and its inverse, the eye, and the stage's haze and
/// sky colours ([`Atmosphere`](crate::atmosphere::Atmosphere)).
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(super) struct FrameUniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    /// Fog colour, and `w` the most it covers (0 for unfogged draws;
    /// [`Atmosphere::haze`](crate::atmosphere::Atmosphere::haze)).
    fog_color: [f32; 4],
    /// Fog start and end, metres, and `z` 1 for the camera-anchored draws (the clouds fade toward the horizon).
    fog_range: [f32; 4],
    /// `y` how far the camera-anchored draws fade into the sky
    /// ([`Atmosphere::cloud_haze`](crate::atmosphere::Atmosphere::cloud_haze)); `x` 1 when the sky's draw lays the
    /// volumetric clouds' image over it, 2 their dome; `z` and `w` unused.
    haze: [f32; 4],
    zenith: [f32; 4],
    /// The direction the sunlight travels, and `w` 1 to draw the sun's glow in the sky.
    sun: [f32; 4],
    /// The lit draws' ambient and sun colours, with the car scales ([`Atmosphere`](crate::atmosphere::Atmosphere)).
    ambient: [f32; 4],
    diffuse: [f32; 4],
    /// Stage space to the car shadow map's clip space ([`SHADOW_SIZE`]).
    shadow_matrix: [[f32; 4]; 4],
    /// `x` how much the car's shadow darkens the stage (from `Car_Shadow_Alpha`, [`SHADOW_SCALE`]), `y` 1 when the map
    /// holds the car (0: no shadow), `z` one filter step in map uv ([`SHADOW_PENUMBRA`]).
    shadow: [f32; 4],
    /// Stage space to the contact shadow layer's clip space ([`GROUND_REACH`]).
    ground_matrix: [[f32; 4]; 4],
    /// `x` how much the contact shadow darkens the stage, `y` one filter step in map uv, `z` and `w` the near and
    /// far reach in map depth.
    ground: [f32; 4],
    /// The uv rectangles (low x and y, high x and y) of the car shadow map and its contact layer outside which their
    /// filters read no car ([`map_box`]).
    shadow_box: [f32; 4],
    ground_box: [f32; 4],
    /// `x` 1 when the reflection cube holds the stage around the car (0: the lit draws reflect the sky), `y` 1 in the
    /// in-car view (the interior takes the game's light).
    reflection: [f32; 4],
    /// `x` how wet the stage is (0 dry, 0.5 damp, 1 wet), for the road shine (`shader.wgsl`); `y` the car's dirt level
    /// ([`Renderer::set_car_dirt`]); `z` how much sun gets into the cabin ([`Renderer::set_cabin_sun`]).
    wetness: [f32; 4],
    /// `x` 1 for [`Enhance::hdr`](super::Enhance::hdr), `y` 1 for [`Enhance::sky`](super::Enhance::sky), `z` 1 for
    /// [`Enhance::water`](super::Enhance::water), `w` 1 for [`Enhance::wet`](super::Enhance::wet).
    enhance: [f32; 4],
    /// Stage space to the sun's cascades' clip spaces ([`SunShadow`](crate::sun::SunShadow)).
    sun_matrices: [[[f32; 4]; 4]; 4],
    /// `x` how much the sun's shadow darkens (0: off), `y` one cascade texel in uv, `z` 1 under an overcast sky, `w` 1
    /// with [`Enhance::sun`](super::Enhance::sun) (the baked light maps are then not used).
    sun_shadow: [f32; 4],
    /// Each cascade's texel in metres, and the far layer's.
    sun_texels: [f32; 4],
    /// Each cascade's far distance from the eye, metres ([`sun::SPLITS`]), and the far layer's reach.
    sun_splits: [f32; 4],
    /// Stage space to the world layer's clip space ([`crate::sun::world`]), and `x` 1 with it, `y` its texel in metres.
    world_matrix: [[f32; 4]; 4],
    world_shadow: [f32; 4],
    /// `x` the time in seconds, `z` how hard rain lands on the wet roads, `y` and `w` where the clouds' field starts
    /// ([`Renderer::wind`]).
    wind: [f32; 4],
    /// `x` 1 at night ([`Atmosphere::night`](crate::atmosphere::Atmosphere::night)), `y` 1 when the headlights' shadow
    /// map is drawn ([`Renderer::lamp_matrix`]), `z` 1 in a mirror's pass (`mirrors.wgsl`), `w` the bake's
    /// [`Relight::open`](crate::relight::Relight::open) (0 without relighting).
    night: [f32; 4],
    lamp_matrix: [[f32; 4]; 4],
    lamps: [LampUniform; MAX_LAMPS],
    /// Toward the bake's sun, and `w` how much vertex light it gave (0: no relight;
    /// [`Atmosphere::relight`](crate::atmosphere::Atmosphere::relight)); the drawn sun's colour and `w` the share of
    /// the bake's ambient that stays.
    relight: [f32; 4],
    relight_light: [f32; 4],
    /// The ambient light's colour, and `w` how much the car's lamps light the stage by day
    /// ([`crate::relight::Relight`]).
    dusk: [f32; 4],
    /// `x` the clouds' cover ([`Atmosphere::cloud_layer`](crate::atmosphere::Atmosphere::cloud_layer)), `y` 1 for
    /// [`Enhance::clouds`](super::Enhance::clouds), `z` and `w` their drift, metres a second
    /// ([`cloud_volume::drift`](super::cloud_volume::drift)).
    clouds: [f32; 4],
    /// The physically based sky (`sky_lut.wgsl`): `x` 1 when its tables are drawn ([`crate::skylight`]), `y` the sine
    /// of the sun's true elevation, `z` 1 in the stage's frame, `w` 1 when the height fog is the whole fog
    /// ([`crate::height_fog::is_volume`]).
    skylight: [f32; 4],
    /// The height fog ([`crate::height_fog::uniform`]): `x` its density at the base, per metre (0: none), `y` its
    /// scale height and `z` the base's height, metres, `w` the density at the eye over the base's.
    height_fog: [f32; 4],
    /// The car body's model space to stage space ([`Renderer::set_car_body`]), for its parts kept in model space
    /// ([`Renderer::set_car_vertices`]).
    car_body: [[f32; 4]; 4],
    /// The moon's light on the stage at night ([`Atmosphere::moonlight`](crate::atmosphere::Atmosphere::moonlight)),
    /// and `w` the night's level ([`Atmosphere::night_level`](crate::atmosphere::Atmosphere::night_level)).
    moon: [f32; 4],
    /// `x` 1 with the computed sky occlusion in the stage vertices ([`Enhance::occlusion`](super::Enhance::occlusion)),
    /// `y` the height over the eye the clouds' shadows are cast from with the volumetric clouds (0: the moving layer's,
    /// [`cloud_volume::shadow_height`](super::cloud_volume::shadow_height)), `z` the car's lowest height (the ground under it, for `ground_bounce`) and `w` 1 with a car.
    occlusion: [f32; 4],
    /// The ground's maps' grid ([`Ground::grid`](super::bounce::Ground)): its corner's x and z, columns and rows; all 0
    /// without them.
    ground_grid: [f32; 4],
    /// The day's light from the sky's tables ([`DayLight`](crate::skylight::DayLight)): the sun's, `w` the
    /// pre-exposure; the sky's, `w` the gain from the tables' units (0 without the tables: night, or no physically
    /// based sky).
    day_sun: [f32; 4],
    day_sky: [f32; 4],
    /// Toward the sun, for the moon's lit side (`moon_shown` in `night.wgsl`; opposite the moon for a full moon), and
    /// `w` 1 when the moon is up ([`Atmosphere::moon`](crate::atmosphere::Atmosphere::moon)).
    moon_disc: [f32; 4],
}

/// The most lamps that light the stage at night ([`Renderer::set_lamps`]).
pub(crate) const MAX_LAMPS: usize = 8;

/// A lamp (`Lamp` in `lamps.wgsl`): position and `w` 1 when lit, the way it points and `w` 1 for a headlight's dipped
/// beam, 2 its main beam, the car's up, and its colour times its intensity.
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct LampUniform {
    pub(super) position: [f32; 4],
    pub(super) forward: [f32; 4],
    pub(super) up: [f32; 4],
    pub(super) colour: [f32; 4],
}

impl LampUniform {
    pub(super) fn of(lamp: &Lamp) -> LampUniform {
        LampUniform {
            position: lamp.position.extend(1.0).to_array(),
            forward: lamp.forward.extend(f32::from(u8::from(lamp.beam) + u8::from(lamp.main))).to_array(),
            up: lamp.up.extend(0.0).to_array(),
            colour: lamp.colour.extend(0.0).to_array(),
        }
    }
}

/// The frame uniform and its bind group: world-anchored stage draws, camera-anchored ones (the clouds), and the
/// world-anchored sky faces (the superbowl backdrop, hazed over
/// [`Atmosphere::superbowl_fog`](crate::atmosphere::Atmosphere::superbowl_fog)).
pub(super) struct FrameBinding {
    pub(super) buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
}

/// The index of a mesh's [`FrameBinding`] in [`Renderer::frames`].
pub(super) fn frame_index(anchor: Anchor, layer: Layer) -> usize {
    match (anchor, layer) {
        (Anchor::Camera, _) => 1,
        (Anchor::World, Layer::Sky) => 2,
        (Anchor::World, _) => 0,
    }
}

impl Renderer {
    /// Writes the three frame uniforms for `view`: the world-anchored draws, the camera-anchored ones and the
    /// backdrop. `shadow_matrices` are the car shadow map's (`None`: no shadow); the main view and the mirrors let the
    /// lit draws read the reflection cube once it holds the stage.
    pub(super) fn write_frames(
        &self,
        view_projection: Mat4,
        sky_view_projection: Mat4,
        shadow_matrices: Option<[Mat4; 2]>,
        view: View,
    ) {
        let uniforms = self.frame_uniforms(view_projection, sky_view_projection, shadow_matrices, view);
        for (frame, uniform) in self.frames.iter().zip(&uniforms) {
            self.queue.write_buffer(&frame.buffer, 0, bytes_of(uniform));
        }
    }

    /// The three frame uniforms [`Renderer::write_frames`] writes.
    pub(super) fn frame_uniforms(
        &self,
        view_projection: Mat4,
        sky_view_projection: Mat4,
        shadow_matrices: Option<[Mat4; 2]>,
        view: View,
    ) -> [FrameUniform; 3] {
        let reflect = view.with_car();
        let eye = camera::eye(view_projection);
        // With the physical light the game's light follows the day's (`Renderer::game_light`).
        let (lit, _) = self.game_light();
        let a = &lit;
        let (drift, _) = super::cloud_volume::drift(a);
        let day = self.day_light();
        // The haze covers the world-anchored draws only; the camera-anchored clouds are the sky.
        let stage_fog = [a.fog_start, a.fog_end];
        // Under fog with the physically based haze the backdrop takes the stage's range, so that it fades out behind it.
        let superbowl_fog = if self.enhance.sky && a.use_fog { stage_fog } else { a.superbowl_fog() };
        // The shadow falls on the world-anchored draws only, not on the clouds or the backdrop.
        let [shadow_matrix, ground_matrix] = shadow_matrices.unwrap_or_default();
        let (shadow, ground) = shadow_matrices.map_or(([0.0; 4], [0.0; 4]), |_| {
            // Both views have the same width; the ground view's depth scale is its z axis.
            let [shadow_step, ground_step] = filter_steps(shadow_matrix);
            let depth = ground_matrix.y_axis.z.abs();
            (
                [(a.car_shadow * SHADOW_SCALE).min(SHADOW_MOST), 1.0, shadow_step, 0.0],
                [
                    if self.enhance.contact { a.car_deep_shadow * GROUND_SCALE } else { 0.0 },
                    ground_step,
                    GROUND_REACH[0] * depth,
                    GROUND_REACH[1] * depth,
                ],
            )
        });
        let boxes = shadow_matrices.zip(self.car_box()).map_or([[0.0; 4]; 2], |(matrices, bounds)| {
            [(matrices[0], 1.5 * shadow[2]), (matrices[1], 2.5 * ground[1])]
                .map(|(matrix, reach)| map_box(matrix, bounds, reach + 2.0 / SHADOW_SIZE as f32))
        });
        let reflection = f32::from(u8::from(reflect && self.reflection.drawn.get() >= 6));
        let probes = f32::from(u8::from(reflect && self.probes.baked));
        // A probe's faces take the world's shadow layer alone: the far layer's reach a millimetre (`sun_reach` 0 past it).
        let splits = if view == View::Probe {
            [0.0, 0.0, 0.0, 1e-3]
        } else {
            [sun::SPLITS[0], sun::SPLITS[1], sun::SPLITS[2], sun::FAR_REACH]
        };
        let cascades = self.cascades.unwrap_or(Cascades {
            matrices: [Mat4::IDENTITY; 3],
            texels: [0.0; 3],
            far: Mat4::IDENTITY,
            far_texel: 0.0,
        });
        let overcast = self.sunless();
        debug_assert_eq!(
            super::pipelines::Conditions::of(&self.atmosphere, self.skylight_on(), self.wetness),
            self.conditions,
            "the stage's pipelines are built for the weather at load"
        );
        let sun_strength = match (self.cascades.is_some(), overcast) {
            (false, _) => 0.0,
            (true, false) => SUN_SHADE,
            (true, true) => OVERCAST_SHADE,
        };
        let sun_step = if overcast { OVERCAST_BLUR } else { 1.0 } / sun::SIZE as f32;
        // The camera-anchored frame marks its draws (the clouds) to fade toward the horizon and into the sky.
        let skylight = self.skylight_on();
        let true_sun = a.sun_elevation.map_or(-a.sun_direction.y, |e| e.to_radians().sin());
        let height_fog = crate::height_fog::uniform(a, self.fog_ground.as_ref(), eye);
        let volume = f32::from(u8::from(crate::height_fog::is_volume(a)));
        let cabin_sun = (self.cabin_sun.as_ref().zip(self.car_body))
            .map_or(0.0, |(sun_in, body)| sun_in.at(body.inverse().transform_vector3(-a.sun_direction)));
        // The backdrop's depth in the physically based sky's haze is scaled as the game's scattering scales it.
        let depth_scale = a.superbowl_scale.clamp(0.0, 1.0);
        // The sky's draw in the main view lays the volumetric clouds' image over the sky, the other views their dome once
        // it is drawn (the cubes' first faces come before the first march: their sky was black).
        let cloud_volume = self.cloud_volume_shown();
        let cloud_image = if !cloud_volume {
            0.0
        } else if view == View::Main {
            1.0
        } else if self.cloud_volume.marched() {
            2.0
        } else {
            0.0
        };
        let cloud_height = if cloud_volume { super::cloud_volume::shadow_height(a) } else { 0.0 };
        let mut uniforms = [FrameUniform::zeroed(); 3];
        for (index, (slot, (matrix, haze, [start, end], (shadow, ground), anchored, depth))) in uniforms
            .iter_mut()
            .zip([
                (view_projection, a.haze(), stage_fog, (shadow, ground), 0.0, 1.0),
                (sky_view_projection, 0.0, stage_fog, ([0.0; 4], [0.0; 4]), 1.0, 1.0),
                (view_projection, a.haze(), superbowl_fog, ([0.0; 4], [0.0; 4]), 0.0, depth_scale),
            ])
            .enumerate()
        {
            let uniform = FrameUniform {
                view_projection: matrix.to_cols_array_2d(),
                inverse_view_projection: matrix.inverse().to_cols_array_2d(),
                eye: eye.extend(1.0).to_array(),
                fog_color: a.fog_color.extend(haze).to_array(),
                fog_range: [start, end, anchored, depth],
                haze: [if index == 1 { cloud_image } else { 0.0 }, a.cloud_haze(), 0.0, 0.0],
                zenith: a.zenith().extend(1.0).to_array(),
                sun: a.sun_direction.extend(f32::from(u8::from(!overcast))).to_array(),
                ambient: (a.ambient * a.car_ambient).extend(1.0).to_array(),
                diffuse: (a.diffuse * a.car_diffuse).extend(1.0).to_array(),
                shadow_matrix: shadow_matrix.to_cols_array_2d(),
                shadow,
                ground_matrix: ground_matrix.to_cols_array_2d(),
                ground,
                shadow_box: boxes[0],
                ground_box: boxes[1],
                reflection: [reflection, f32::from(u8::from(self.in_car(view))), probes, 0.0],
                wetness: [self.wetness, self.car_dirt, cabin_sun, 0.0],
                enhance: [self.enhance.hdr, self.enhance.sky, self.enhance.water, self.enhance.wet]
                    .map(|on| f32::from(u8::from(on))),
                sun_matrices: [cascades.matrices[0], cascades.matrices[1], cascades.matrices[2], cascades.far]
                    .map(|m| m.to_cols_array_2d()),
                sun_shadow: [
                    sun_strength,
                    sun_step,
                    f32::from(u8::from(overcast)),
                    f32::from(u8::from(self.enhance.sun)),
                ],
                sun_texels: [cascades.texels[0], cascades.texels[1], cascades.texels[2], cascades.far_texel],
                sun_splits: splits,
                world_matrix: self.world.map_or(Mat4::IDENTITY, |(matrix, _)| matrix).to_cols_array_2d(),
                world_shadow: self.world.map_or([0.0; 4], |(_, texel)| [1.0, texel, 0.0, 0.0]),
                wind: self.wind(),
                night: [
                    f32::from(u8::from(self.atmosphere.night)),
                    f32::from(u8::from(self.lamp_matrix.is_some() && view != View::Probe)),
                    f32::from(u8::from(view.mirror())),
                    a.relight.map_or(0.0, |r| r.open),
                ],
                lamp_matrix: self.lamp_matrix.unwrap_or_default().to_cols_array_2d(),
                lamps: self.lamps,
                relight: a.relight.map_or([0.0; 4], |r| r.bake_sun.extend(r.strength).to_array()),
                relight_light: a.relight.map_or([1.0; 4], |r| r.sun.extend(r.ambient).to_array()),
                dusk: a.relight.map_or([1.0, 1.0, 1.0, 0.0], |r| r.tint.extend(r.lamps).to_array()),
                clouds: [a.cloud_layer(), f32::from(u8::from(self.enhance.clouds)), drift[0], drift[1]],
                skylight: [f32::from(u8::from(skylight)), true_sun, f32::from(u8::from(index == 0)), volume],
                height_fog,
                car_body: self.car_body.unwrap_or_default().to_cols_array_2d(),
                moon: a.moonlight().extend(a.night_level()).to_array(),
                occlusion: [
                    f32::from(u8::from(self.enhance.occlusion)),
                    cloud_height,
                    self.car_bounds().map_or(0.0, |(low, _)| low.y),
                    f32::from(u8::from(self.car_bounds().is_some())),
                ],
                ground_grid: self.ground_light.as_ref().map_or([0.0; 4], |g| g.grid),
                day_sun: day.map_or([0.0; 4], |d| d.sun),
                day_sky: day.map_or([0.0; 4], |d| d.sky),
                moon_disc: a
                    .moon
                    .map_or(a.sun_direction.extend(1.0), |m| m.to_sun.extend(f32::from(u8::from(m.light > 0.0))))
                    .to_array(),
            };
            *slot = uniform;
        }
        uniforms
    }
}
