//! The stage's ground decals blended over the ground: a renderer addition behind
//! [`crate::renderer::Enhance::decals`].
//!
//! The game's factory gives object items (flags `0x15`) a blend state: alpha blend `SRCALPHA` / `INVSRCALPHA`, alpha
//! test `> 0` (`HH_TM2_12E0_Factory` 0x100512e0). The capture at the default quality shows them only cut at 60/255,
//! opaque, as the stage draws them ([`OBJECT_ALPHA_REF`]); what selects the blend is not traced. A decal whose alpha
//! never nears opaque loses most of itself to that cut and the rest turns solid: the RSF skid marks (black, alpha at
//! most 170) draw as hard black shapes. So the draws of soft textures lying on the ground blend instead, after the
//! rest of the stage. Over the retail stages and the RSF samples these are the skid marks of 129, 369 and 370 and the
//! sand patches (`sablepng`) of 129; no retail draw.
//! Upright soft cut-outs (wire fences, phone lines, smoke cards; at most 221/255) keep the cut: blended, the wire
//! mesh fades to nothing, while its cut edges become sample coverage. Trees and bushes reach 255.

use crate::foliage::test_texels;
use crate::stage::{Draw, Layer, Mesh, Stage, TextureRef, OBJECT_ALPHA_REF};
use glam::Vec3;
use std::collections::HashMap;

/// A texture is soft when none of its texels' alpha reaches this.
const SOFT: u8 = 230;
/// A draw lies on the ground when its triangles' area-weighted normal is at least this level (cosine to the vertical,
/// within 32 degrees): the skid marks on 369's climbing start are at 0.89, a pole's arm on it (`PHONE055`) at 0.76,
/// upright cut-outs near 0.
const LEVEL: f32 = 0.85;

/// Moves the soft ground decals' draws to blended copies of their meshes at the end of the stage's meshes (the stage
/// draws in order, so they land on the ground drawn before them), leaving the originals without them so mesh indices
/// hold. Returns how many draws moved.
pub fn blend_ground_decals(stage: &mut Stage) -> usize {
    let mut soft = HashMap::new();
    let mut is_soft = |stage: &Stage, texture: TextureRef| {
        *soft.entry(texture).or_insert_with(|| {
            test_texels(stage, texture).is_some_and(|(_, rgba)| rgba.chunks_exact(4).all(|texel| texel[3] < SOFT))
        })
    };
    let mut blended = Vec::new();
    for index in 0..stage.meshes.len() {
        let mesh = &stage.meshes[index];
        if mesh.layer != Layer::Solid {
            continue;
        }
        let (moved, kept): (Vec<_>, Vec<_>) = mesh.draws.iter().cloned().partition(|draw| {
            let state = &draw.state;
            state.alpha_ref == Some(OBJECT_ALPHA_REF)
                && !state.blend
                && state.textures[1].is_none()
                && state.textures[0].is_some_and(|texture| is_soft(stage, texture))
                && level(mesh, draw)
        });
        if moved.is_empty() {
            continue;
        }
        let draws = moved
            .into_iter()
            .map(|mut draw| {
                draw.state.blend = true;
                draw.state.alpha_ref = Some(0);
                draw.state.depth_write = false;
                draw
            })
            .collect();
        blended.push(Mesh { layer: Layer::Blended, draws, ..mesh.clone() });
        stage.meshes[index].draws = kept;
    }
    let moved = blended.iter().map(|mesh| mesh.draws.len()).sum();
    stage.meshes.extend(blended);
    moved
}

/// Whether `draw`'s triangles lie level, either side up (object items are double-sided).
fn level(mesh: &Mesh, draw: &Draw) -> bool {
    let position = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
    let (mut up, mut area) = (0.0, 0.0);
    for triangle in mesh.indices[draw.indices.clone()].chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(position);
        let normal = (b - a).cross(c - a);
        up += normal.y.abs();
        area += normal.length();
    }
    area > 0.0 && up >= LEVEL * area
}
