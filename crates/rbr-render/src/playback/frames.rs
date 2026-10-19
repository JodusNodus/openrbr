//! The frames a played car is placed by: model to physics body, body to stage, and the tyre nodes of the model.

use crate::coords::physics_to_stage;
use glam::{Mat4, Quat, Vec3, Vec4};
use rbr_formats::data::Data;
use rbr_formats::sgc;
use rbr_recording::pose::CarPose;
use rbr_recording::WHEELS;
use std::fs;
use std::path::PathBuf;

/// Tyre nodes closer than this, m, are at one point: a class the model left stacked ([`tyre_nodes`]).
const STACKED_METRES: f32 = 0.2;
/// The model's lateral axis, the roll axis of the tyres in model space: a positive roll turns the top of the tyre
/// forward (the top `+Y` moves toward the front `-Z`).
pub(super) const WHEEL_ROLL_AXIS: Vec3 = Vec3::NEG_X;

/// The model frame to the physics body frame. It maps front `-Z` to `-Y`, up `+Y` to `+Z` and `+X` to `+X`: a
/// mirror, which the y-z swap of [`physics_to_stage`] undoes (see the module docs).
pub(super) fn model_to_body() -> Mat4 {
    Mat4::from_cols(Vec4::X, Vec4::new(0.0, 0.0, 1.0, 0.0), Vec4::new(0.0, 1.0, 0.0, 0.0), Vec4::W)
}

/// The physics body's frame to stage space at `car`'s pose.
pub fn car_to_stage(car: &CarPose) -> Mat4 {
    physics_to_stage() * physics_pose(car)
}

/// Model space to stage space with the model's root node at `root`'s pose (as LocoMotion poses the ghost car).
pub fn root_to_stage(root: &CarPose) -> Mat4 {
    car_to_stage(root) * model_to_body()
}

/// The car's world pose in the physics frame: its rotation then its position.
pub(super) fn physics_pose(car: &CarPose) -> Mat4 {
    Mat4::from_rotation_translation(
        Quat::from_xyzw(car.orientation[0], car.orientation[1], car.orientation[2], car.orientation[3]),
        Vec3::from(car.position),
    )
}

/// The car's tyre nodes, wheels 0 to 3: the drawn (tarmac) ones, and the wheels' rest places for the body's offset.
/// Those are the first class's (tarmac, gravel, snow) whose four nodes are apart: the game places each wheel node by
/// its animation, so a model may leave a class's at one point (ours; RSF's Fiesta Rally2 and WRC 2019, i20 WRC 2020
/// and Lada VFTS stack the tarmac ones, and the i20's gravel names are swapped, which the offset's mean ignores).
pub(super) fn tyre_nodes(data: &Data, car: &str) -> Result<([Vec3; WHEELS], [Vec3; WHEELS]), String> {
    let path = model_path(data, car)?;
    let model = sgc::parse(&fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e:?}", path.display()))?;
    let chained = sgc::chained_worlds(&model);
    let class = |class: &str| -> Option<[Vec3; WHEELS]> {
        let mut nodes = [None; WHEELS];
        for element in &model {
            let sgc::Element::Mesh(mesh) = element else {
                continue;
            };
            match crate::car::wheel_of(&mesh.node.name) {
                Some((c, k)) if c == class && !mesh.node.name.contains("_tyre_") => {
                    let world = chained.get(&mesh.node.id).copied().unwrap_or(mesh.node.world);
                    nodes[k] = Some(Vec3::new(world[0][3], world[1][3], world[2][3]));
                }
                _ => {}
            }
        }
        nodes.into_iter().collect::<Option<Vec<_>>>()?.try_into().ok()
    };
    let drawn = class("tarmac").ok_or_else(|| format!("{}: not four tarmac tyre nodes", path.display()))?;
    let apart = |n: &[Vec3; WHEELS]| (0..WHEELS).all(|i| (i + 1..WHEELS).all(|j| n[i].distance(n[j]) > STACKED_METRES));
    let rest = ["tarmac", "gravel", "snow"].into_iter().filter_map(class).find(apart).unwrap_or(drawn);
    Ok((drawn, rest))
}

/// The car's `.sgc` model: its `Cars.ini` entry's (an RSF car's through `Data::with_car`), else the one in its `Cars`
/// folder (the folder matches `car` ignoring case).
pub(crate) fn model_path(data: &Data, car: &str) -> Result<PathBuf, String> {
    if let Ok(path) = crate::car::model_file(data, car) {
        return Ok(path);
    }
    let cars = data.install().join("Cars");
    let folder = fs::read_dir(&cars)
        .map_err(|e| format!("{}: {e}", cars.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(car)))
        .ok_or_else(|| format!("no car folder {car}"))?;
    fs::read_dir(&folder)
        .map_err(|e| format!("{}: {e}", folder.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|p| p.extension().is_some_and(|e| e == "sgc"))
        .ok_or_else(|| format!("no .sgc in {}", folder.display()))
}
