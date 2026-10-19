//! The character animation core: a model's LocoMotion node tree, its `.sga` clips sampled on it, and the skin that
//! follows the nodes. Pure: no GPU, no files. The stage characters use it, and the in-car crew is meant to
//! (rbr-recomp `notes/stage-characters.md`, section 7).
//!
//! What LocoMotion does, and what is reproduced here (rbr-recomp `notes/locomotion.md`, `notes/locomotion-animation.md`):
//!
//! - **Tree.** `CLM_CharacterLoader::LoadNodes` (LM `0x10014c20`) reads the model's kind 18 node table; each node goes
//!   under the earlier node whose id is its parent, `-1` under the root (`CLM_Character::Unk1aae0`).
//! - **Clips.** `AddAnimationToCharacter` gives every node one animation object `"<clip>::<node>::PRS"`
//!   (`CLM_NodeAnimationConnector::VisitPRS`, LM `0x10029230`). With no such controller the object is a fixed
//!   identity (`AddTMObjectOrIdentity`, LM `0x10039550`): the node then sits on its parent.
//! - **Sampling.** `CLM_AnimationControllerPRSSampled::Evaluate` (LM `0x1002d6c0`, read from the Ghidra
//!   decompile): the first sample at or before the first time, the last after the last, else the two samples around
//!   `t` blended linearly element by element, `prev * (1 - f) + next * f`, with `f = 0` when their times are within
//!   1e-4. No slerp, no renormalising.
//! - **World.** `CLM_NodePRS::Update` (LM `0x10029620`) and `CLM_ControllerManager::EvaluateTM` give each node
//!   `local * parent` (D3DX row vectors), so `parent * local` here (glam column vectors). The character's own
//!   placement (`+0x160`, scale, rotation, translation) goes on top ([`placement`]).
//! - **Skin.** `HH_SkinUpload` (HH `0x1007f4c0`) moves each vertex by up to four bone matrices with their weights,
//!   positions with translation, normals without. Which matrices HedgeHog3D hands it is not traced (its frame hook
//!   `0x100a57ef` is unresolved): here bone `i`'s matrix is its node's world times the inverse of the bone's bind
//!   matrix in the skin block. The retail models check out with it: a clip's first frame puts every vertex next to
//!   its bones (`tests` in `characters`), so this is **inferred**, not traced.
//!
//! Matrices from the files are `[R | t]` rows ([`crate::coords::affine`]), the same layout as the car models.

use crate::coords::affine;
use glam::{Mat4, Vec3};
use rbr_formats::{sga, sgs};

/// A model's node tree, in node-table order (every parent comes before its children).
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    /// The parent's index in [`Skeleton::nodes`], `None` under the character root.
    pub parent: Option<usize>,
}

impl Skeleton {
    /// The tree of a model's node table. A parent id that no earlier node has puts the node under the root.
    pub fn new(table: &sgs::Hierarchy) -> Skeleton {
        let mut nodes: Vec<Node> = Vec::with_capacity(table.nodes.len());
        for (i, node) in table.nodes.iter().enumerate() {
            let parent = (node.parent != -1)
                .then(|| table.nodes[..i].iter().rposition(|earlier| earlier.id == node.parent))
                .flatten();
            nodes.push(Node { name: node.name.clone(), parent });
        }
        Skeleton { nodes }
    }

    /// The index of the node named `name`; the last of that name, as `CLM_CharacterManager::AddNode` keeps it.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes.iter().rposition(|n| n.name == name)
    }
}

/// One node's baked samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub times: Vec<f32>,
    pub samples: Vec<Mat4>,
}

impl Track {
    /// The node's matrix at clip time `t`, as `CLM_AnimationControllerPRSSampled::Evaluate`.
    pub fn sample(&self, t: f32) -> Mat4 {
        let (times, samples) = (&self.times, &self.samples);
        let n = times.len().min(samples.len());
        if n == 0 {
            return Mat4::IDENTITY;
        }
        if n == 1 || t <= times[0] {
            return samples[0];
        }
        if t > times[n - 1] {
            return samples[n - 1];
        }
        let i = times[..n].iter().position(|&time| t < time).unwrap_or(n - 1);
        let (t0, t1) = (times[i - 1], times[i]);
        let f = if (t0 - t1).abs() > 1e-4 { (t - t0) / (t1 - t0) } else { 0.0 };
        let (a, b) = (samples[i - 1].to_cols_array(), samples[i].to_cols_array());
        Mat4::from_cols_array(&std::array::from_fn(|k| a[k] * (1.0 - f) + b[k] * f))
    }
}

/// A clip bound to a skeleton: one optional track per node and the clip's range (the `.sga` kind 20 record).
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Clip time where playback starts (`CLM_Character::Unk19f90` sets the clock offset from the range).
    pub start: f32,
    /// Clip time where it ends: the time left (`CLM_Character::Unk10`) is `end - t`.
    pub end: f32,
    /// Per [`Skeleton::nodes`] index; `None` is a fixed identity.
    pub tracks: Vec<Option<Track>>,
    /// Bindings that name no node of the skeleton, or use a controller other than sampled PRS (none in the
    /// stage-character clips).
    pub unbound: usize,
}

impl Clip {
    /// Binds an `.sga`'s `<node>::PRS` sampled controllers to `skeleton`'s nodes.
    pub fn bind(skeleton: &Skeleton, elements: &[sga::Element]) -> Clip {
        let mut tracks = vec![None; skeleton.nodes.len()];
        let (mut start, mut end, mut unbound) = (0.0, 0.0, 0);
        for element in elements {
            match element {
                sga::Element::Range(range) => (start, end) = (range.start, range.end),
                sga::Element::Animation(animation) => {
                    let node = animation.name.strip_suffix("::PRS").and_then(|name| skeleton.find(name));
                    match (node, &animation.controller) {
                        (Some(node), sga::Controller::SampledPrs(prs)) => {
                            tracks[node] = Some(Track {
                                times: prs.times.clone(),
                                samples: prs.samples.iter().map(|&m| affine(m)).collect(),
                            })
                        }
                        _ => unbound += 1,
                    }
                }
            }
        }
        Clip { start, end, tracks, unbound }
    }

    /// Every node's matrix in the character's frame at clip time `t`, by [`Skeleton::nodes`] index.
    pub fn pose(&self, skeleton: &Skeleton, t: f32) -> Vec<Mat4> {
        let mut world: Vec<Mat4> = Vec::with_capacity(skeleton.nodes.len());
        for (i, node) in skeleton.nodes.iter().enumerate() {
            let local = self.tracks.get(i).and_then(Option::as_ref).map_or(Mat4::IDENTITY, |track| track.sample(t));
            let parent = node.parent.map_or(Mat4::IDENTITY, |p| world[p]);
            world.push(parent * local);
        }
        world
    }
}

/// A character's placement: `CLM_Character` `+0x160 = scale * rotation * translation` (row vectors), from the item's
/// position, its Euler angles in degrees applied X, then Y, then Z (`Unk187c0`, `deg * 0.01745`) and the script's
/// uniform `scale`.
pub fn placement(position: Vec3, degrees: Vec3, scale: f32) -> Mat4 {
    let radians = degrees * 0.01745;
    Mat4::from_translation(position)
        * Mat4::from_rotation_z(radians.z)
        * Mat4::from_rotation_y(radians.y)
        * Mat4::from_rotation_x(radians.x)
        * Mat4::from_scale(Vec3::splat(scale))
}

/// The skin of one mesh bound to a skeleton: per skin bone its node and inverse bind matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub nodes: Vec<Option<usize>>,
    pub inverse_bind: Vec<Mat4>,
    /// Skin bones whose name no node has (they stay at their bind pose).
    pub missing: usize,
}

impl Skin {
    pub fn bind(skeleton: &Skeleton, skin: &sgs::Skin) -> Skin {
        let nodes: Vec<Option<usize>> = skin.bones.iter().map(|b| skeleton.find(&b.name)).collect();
        let missing = nodes.iter().filter(|n| n.is_none()).count();
        let inverse_bind = skin.bones.iter().map(|b| affine(b.matrix).inverse()).collect();
        Skin { nodes, inverse_bind, missing }
    }

    /// The bone matrices for a pose ([`Clip::pose`]) placed by `placement`.
    pub fn palette(&self, world: &[Mat4], placement: Mat4) -> Vec<Mat4> {
        self.nodes
            .iter()
            .zip(&self.inverse_bind)
            .map(|(node, inverse)| match node.and_then(|n| world.get(n)) {
                Some(&node) => placement * node * *inverse,
                None => placement * inverse.inverse(),
            })
            .collect()
    }
}

/// The matrix that places a mesh's unskinned vertices: its own node's pose in the character, then `placement`. A
/// mesh whose name no node has is placed by `placement` alone.
pub fn rigid(node: Option<usize>, world: &[Mat4], placement: Mat4) -> Mat4 {
    placement * node.and_then(|n| world.get(n)).copied().unwrap_or(Mat4::IDENTITY)
}

/// One vertex skinned by `palette`, as `HH_SkinUpload`: the weighted sum of the bone matrices applied to the position
/// (with translation) and to the normal (without; not renormalised there, normalised here for the lighting). A vertex
/// whose valid bones carry no weight (`sgs::SkinWeight::bones` with no bone, or weight 0) follows `rigid` instead, as
/// an unskinned vertex would. Without it the sum is the origin, and the vertex stretches its triangles there (ours:
/// retail `richard.sgs` and `robert.sgs` have four such vertices each, `rbr-formats` `real_data`).
pub fn skin_vertex(
    palette: &[Mat4],
    rigid: Mat4,
    weight: &sgs::SkinWeight,
    position: Vec3,
    normal: Vec3,
) -> (Vec3, Vec3) {
    let (mut p, mut n, mut total) = (Vec3::ZERO, Vec3::ZERO, 0.0);
    for (bone, w) in weight.bones() {
        let Some(&m) = bone.and_then(|bone| palette.get(bone)) else { continue };
        p += m.transform_point3(position) * w;
        n += m.transform_vector3(normal) * w;
        total += w;
    }
    if total == 0.0 {
        return (rigid.transform_point3(position), rigid.transform_vector3(normal).normalize_or_zero());
    }
    (p, n.normalize_or_zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(t: [f32; 3]) -> [[f32; 4]; 3] {
        [[1.0, 0.0, 0.0, t[0]], [0.0, 1.0, 0.0, t[1]], [0.0, 0.0, 1.0, t[2]]]
    }

    fn table(nodes: &[(&str, i32, i32)]) -> sgs::Hierarchy {
        sgs::Hierarchy {
            nodes: nodes
                .iter()
                .map(|&(name, id, parent)| sgs::HierarchyNode {
                    node_type: 47,
                    name: name.to_owned(),
                    id,
                    parent,
                    world: rows([0.0; 3]),
                    sub: 2,
                })
                .collect(),
        }
    }

    #[test]
    fn tree_takes_earlier_parents_only() {
        let skeleton = Skeleton::new(&table(&[("root", 1, -1), ("a", 2, 1), ("b", 3, 2), ("late", 4, 9), ("c", 9, 1)]));
        let parents: Vec<_> = skeleton.nodes.iter().map(|n| n.parent).collect();
        assert_eq!(parents, [None, Some(0), Some(1), None, Some(0)]);
    }

    #[test]
    fn sampling_is_lm_linear_with_held_ends() {
        let track = Track {
            times: vec![0.0, 1.0, 1.00001, 2.0],
            samples: [0.0, 10.0, 20.0, 30.0].map(|x| Mat4::from_translation(Vec3::new(x, 0.0, 0.0))).to_vec(),
        };
        let x = |t: f32| track.sample(t).w_axis.x;
        assert_eq!(x(-1.0), 0.0);
        assert_eq!(x(0.0), 0.0);
        assert!((x(0.25) - 2.5).abs() < 1e-6);
        assert_eq!(x(3.0), 30.0);
        // Two samples closer than 1e-4 in time: the earlier is taken whole.
        assert_eq!(x(1.000005), 10.0);
        assert!((x(1.5) - 25.0).abs() < 1e-3);
    }

    #[test]
    fn pose_chains_parents_and_unbound_nodes_are_identity() {
        let skeleton = Skeleton::new(&table(&[("root", 1, -1), ("arm", 2, 1), ("hand", 3, 2)]));
        let animation = |name: &str, x: f32| {
            sga::Element::Animation(sga::Animation {
                name: format!("{name}::PRS"),
                controller: sga::Controller::SampledPrs(sga::SampledPrs {
                    times: vec![0.0],
                    samples: vec![rows([x, 0.0, 0.0])],
                }),
            })
        };
        let elements = vec![
            sga::Element::Range(sga::NamedRange { start: 0.0, end: 2.0 }),
            animation("root", 1.0),
            animation("arm", 2.0),
            animation("missing", 9.0),
        ];
        let clip = Clip::bind(&skeleton, &elements);
        assert_eq!((clip.start, clip.end, clip.unbound), (0.0, 2.0, 1));
        let world = clip.pose(&skeleton, 0.5);
        let xs: Vec<f32> = world.iter().map(|m| m.w_axis.x).collect();
        assert_eq!(xs, [1.0, 3.0, 3.0]);
    }

    #[test]
    fn skinning_blends_bones_and_bind_pose_is_rest() {
        let skeleton = Skeleton::new(&table(&[("a", 1, -1), ("b", 2, -1)]));
        let skin = sgs::Skin {
            head: rows([0.0; 3]),
            bones: vec![
                sgs::SkinBone { name: "a".into(), matrix: rows([1.0, 0.0, 0.0]) },
                sgs::SkinBone { name: "b".into(), matrix: rows([0.0, 1.0, 0.0]) },
            ],
            weight_count: 1,
            weights: vec![],
        };
        let skin = Skin::bind(&skeleton, &skin);
        let weight = sgs::SkinWeight { weights: [0.5, 0.5, 0.0, 0.0], offsets: [0, 0x40, sgs::NO_BONE, sgs::NO_BONE] };
        let rest = [Mat4::from_translation(Vec3::X), Mat4::from_translation(Vec3::Y)];
        let palette = skin.palette(&rest, Mat4::IDENTITY);
        let (p, n) = skin_vertex(&palette, Mat4::IDENTITY, &weight, Vec3::new(0.0, 0.0, 5.0), Vec3::Z);
        assert!((p - Vec3::new(0.0, 0.0, 5.0)).length() < 1e-6 && (n - Vec3::Z).length() < 1e-6);
        let moved = [Mat4::from_translation(Vec3::X * 3.0), Mat4::from_translation(Vec3::Y)];
        let (p, _) = skin_vertex(&skin.palette(&moved, Mat4::IDENTITY), Mat4::IDENTITY, &weight, Vec3::ZERO, Vec3::Z);
        assert!((p - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn vertex_without_weight_follows_its_node_rigidly() {
        let palette = [Mat4::from_translation(Vec3::X * 9.0)];
        let world = [Mat4::from_translation(Vec3::Y * 2.0), Mat4::from_translation(Vec3::Z * 7.0)];
        let matrix = rigid(Some(0), &world, Mat4::from_translation(Vec3::X));
        // Slot 0 has no bone and weight 0: the sum would be the origin.
        let unbound =
            sgs::SkinWeight { weights: [0.0; 4], offsets: [sgs::NO_BONE, sgs::NO_BONE, sgs::NO_BONE, sgs::NO_BONE] };
        let (p, n) = skin_vertex(&palette, matrix, &unbound, Vec3::new(0.0, 0.0, 5.0), Vec3::Z);
        assert!((p - Vec3::new(1.0, 2.0, 5.0)).length() < 1e-6 && (n - Vec3::Z).length() < 1e-6, "{p}");
        // A valid bone with weight 0 is no weight either.
        let zero = sgs::SkinWeight { weights: [0.0; 4], offsets: [0x40, sgs::NO_BONE, sgs::NO_BONE, sgs::NO_BONE] };
        let (p, _) = skin_vertex(&palette, matrix, &zero, Vec3::ZERO, Vec3::Z);
        assert!((p - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-6, "{p}");
        // A mesh with no node in the skeleton is placed by the placement alone.
        assert_eq!(rigid(None, &world, Mat4::from_translation(Vec3::X)), Mat4::from_translation(Vec3::X));
    }

    #[test]
    fn placement_scales_then_rotates_then_moves() {
        let m = placement(Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 90.0, 0.0), 0.001);
        let p = m.transform_point3(Vec3::new(1000.0, 0.0, 0.0));
        assert!((p - Vec3::new(10.0, 0.0, -1.0)).length() < 1e-3, "{p}");
    }
}
