//! Renderer for Richard Burns Rally stages (milestone 1: one stage, free-fly viewer).
//!
//! Stage data is read only through `rbr-formats` (see [`stage`]). Conventions for the camera and
//! the vertex data live in [`coords`]. [`camera`] holds the free-fly camera and the frustum used to
//! cull blocks. [`renderer`] draws a loaded stage with wgpu. The `viewer` binary ties them together.
//!
//! Notes: `notes/formats.md` "Shaders and vertex layouts" (render states, texture roles), `notes/formats.md` "Stage geometry".

pub mod atmosphere;
pub mod bindings;
mod bounce;
pub mod camera;
pub mod camera_prefs;
pub mod car;
pub mod car_shadow;
pub mod characters;
pub mod console;
pub mod coords;
pub mod crack_decal;
pub mod damage;
pub mod damage_parts;
pub mod decals;
pub mod devices;
pub mod dials;
pub mod dirt;
mod effects;
pub mod fences;
pub mod ffb;
mod foliage;
pub mod game_camera;
pub mod ghost;
pub mod glass;
mod height_fog;
pub mod hud;
pub mod icon;
mod light_shafts;
pub mod lights;
pub mod live;
pub mod mirrors;
pub mod particles;
pub mod perf_log;
pub mod playback;
mod post;
pub mod present;
mod profile;
mod puddles;
pub mod relight;
pub mod renderer;
#[cfg(test)]
mod shader_check;
pub mod shifter;
pub mod skidmarks;
pub mod skidmarks_pass;
mod sky_occlusion;
mod skylight;
pub mod sound;
pub mod stage;
pub mod stage_picture;
pub mod stage_probe;
pub mod start_board;
mod sun;
mod taa;
pub mod tyre_marks;
pub mod views;
pub mod visibility;
pub mod wheel_rotation;
