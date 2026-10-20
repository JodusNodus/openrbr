//! The stage's plants, for the sun's shadows and the light through their leaves: renderer additions behind
//! [`crate::renderer::Enhance::sun`], not the game's look. A plant is a connected piece of an alpha-to-coverage draw's
//! triangles whose texture is cut out ([`cut_out`]; rocks, fences and trunks are alpha-to-coverage draws too, with
//! opaque textures). Grass (plants lower than [`GRASS`]) casts no sun shadow: its flat cards' shadows show how flat
//! they are. Cut-out structures (power pylons,
//! wire and wooden fences, gates, lamps, signs) are not plants: their textures are named for them ([`STRUCTURES`]).

use crate::stage::{Draw, Image, Mesh, Stage, TextureRef, Vertex};
use rbr_formats::dds::{self, Format};
use std::collections::HashSet;

/// Plants lower than this, metres, are grass. Plant heights on three stages dip between 1.5 and 2 m, between the
/// tufts and low plants and the bushes and trees.
const GRASS: f32 = 1.5;
/// A texture is cut out when more than this share of its texels is at or below the alpha reference.
const CUT: f32 = 0.05;
/// The texture level tested is the first no wider than this.
const TEST_WIDTH: u32 = 256;
/// Words in the file names of cut-out textures that are not plants (upper case), over the retail stages and the RSF
/// samples' (`FENCE_TILE`, `POWERPYLON01_UPDATE`, `fieldfnc`, `panneausta1` and the like). Their colour does not tell
/// them apart: snowy firs are as grey as a pylon, and wooden fences as brown as autumn leaves.
const STRUCTURES: [&str; 14] = [
    "FENCE",
    "FNC",
    "PYLON",
    "POWERLINE",
    "POLE",
    "GATE",
    "WINDMILL",
    "LAMP",
    "BARRIER",
    "GUARDRAIL",
    "BRIDGE",
    "BANNER",
    "PANNEAU",
    "CROIX",
];

/// Whether the texture of `file_name` (a path or a name) shows a cut-out structure rather than a plant.
fn structure(file_name: &str) -> bool {
    let name = file_name.rsplit(['\\', '/']).next().unwrap_or(file_name).to_ascii_uppercase();
    let stem = name.split('.').next().unwrap_or(&name);
    stem == "NET" || STRUCTURES.iter().any(|word| stem.contains(word))
}

/// The textures of the draws `foliage` picks that are cut out at the draw's alpha reference, but for structures.
pub(crate) fn cut_out(stage: &Stage, foliage: impl Fn(&Draw) -> bool) -> HashSet<TextureRef> {
    let mut tested = HashSet::new();
    let mut cut = HashSet::new();
    for draw in stage.meshes.iter().flat_map(|mesh| &mesh.draws).filter(|draw| foliage(draw)) {
        let (Some(texture), Some(alpha_ref)) = (draw.state.textures[0], draw.state.alpha_ref) else { continue };
        if !tested.insert(texture) || stage.texture_names.get(&texture).is_some_and(|name| structure(&name.file_name)) {
            continue;
        }
        let Some(rgba) = test_texels(stage, texture) else { continue };
        let below = rgba.chunks_exact(4).filter(|texel| texel[3] <= alpha_ref).count();
        if below as f32 > CUT * (rgba.len() / 4) as f32 {
            cut.insert(texture);
        }
    }
    cut
}

/// The RGBA texels of `texture`'s first level no wider than [`TEST_WIDTH`], when it is a stage `.dds`.
pub(crate) fn test_texels(stage: &Stage, texture: TextureRef) -> Option<Vec<u8>> {
    let Some(Some(Image::Dds(bytes))) = stage.textures.get(&texture).map(|t| &t.image) else { return None };
    let dds = dds::parse(bytes).ok()?;
    let levels = dds.levels();
    let level = levels.iter().find(|level| level.width <= TEST_WIDTH).or(levels.last())?;
    let bc = match dds.format {
        Format::Dxt1 => texpresso::Format::Bc1,
        Format::Dxt3 => texpresso::Format::Bc2,
        Format::Dxt5 => texpresso::Format::Bc3,
        Format::Rgba8 => return Some(level.data.to_vec()),
    };
    let (width, height) = (level.width.max(4) as usize, level.height.max(4) as usize);
    let mut rgba = vec![0; width * height * 4];
    bc.decompress(level.data, width, height, &mut rgba);
    Some(rgba)
}

/// `vertices` (of `mesh`) with 1 in `specular[1]` on each grass vertex and 1 in `specular[2]` on each plant vertex (the
/// light through its leaves, `sun_light` in `shader.wgsl`), which plant draws do not use (they have no specular map). `plant` picks the draws that hold plants.
pub(crate) fn with_plants(mut vertices: Vec<Vertex>, mesh: &Mesh, plant: impl Fn(&Draw) -> bool) -> Vec<Vertex> {
    let mut parent: Vec<usize> = (0..vertices.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut on = vec![false; vertices.len()];
    for draw in mesh.draws.iter().filter(|draw| plant(draw)) {
        for triangle in mesh.indices[draw.indices.clone()].chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| triangle[k] as usize);
            if [a, b, c].iter().any(|&i| i >= vertices.len()) {
                continue;
            }
            for i in [a, b, c] {
                on[i] = true;
            }
            let (ra, rb, rc) = (root(&mut parent, a), root(&mut parent, b), root(&mut parent, c));
            parent[rb] = ra;
            parent[rc] = ra;
        }
    }
    // Each plant's lowest and highest point.
    let mut span = vec![(f32::MAX, f32::MIN); vertices.len()];
    for i in (0..vertices.len()).filter(|&i| on[i]) {
        let r = root(&mut parent, i);
        let y = vertices[i].position[1];
        span[r] = (span[r].0.min(y), span[r].1.max(y));
    }
    for i in (0..vertices.len()).filter(|&i| on[i]) {
        let (low, high) = span[root(&mut parent, i)];
        let height = high - low;
        vertices[i].specular[1] = f32::from(u8::from(height < GRASS));
        vertices[i].specular[2] = 1.0;
    }
    vertices
}

#[cfg(test)]
mod tests {
    use super::structure;

    #[test]
    fn structures_by_name() {
        for name in ["FENCE_TILE.tga", "POWERPYLON01_UPDATE.tga", "MAPS\\X\\fieldfnc.dds", "Net.dds", "panneausta1.dds"]
        {
            assert!(structure(name), "{name}");
        }
        for name in ["ObjectTexturePlateTransparent.dds", "HO_LD_DECIDUOUS_WOOD_1.tga", "genet2.dds", "Cabinet.dds"] {
            assert!(!structure(name), "{name}");
        }
    }
}
