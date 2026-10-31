//! Every installed RSF car's steering wheel: `RBR_DATA` and `RBR_RSF`. The interior `i_steeringwheel`
//! meshes the loader draws are marked for the steering to turn, so the wheel of a car built like the Lancia
//! Delta HF 4WD GrpA (rim and column in one mesh, its centre 0.17 m from the hub: found by no centre match)
//! turns with the car, and `viewer --hide-wheel` hides it.

use rbr_formats::data::Data;
use rbr_formats::{rsf, sgc};
use rbr_render::car;

use std::path::PathBuf;

/// The distance the steering wheel's mesh centre had to be within of its node to turn, before meshes were marked by
/// their node name: cars whose wheel centre is further off were not recognised.
const BEFORE_MATCH_METRES: f32 = 0.12;

/// The install (`RBR_DATA`) and the RSF root (`RBR_RSF`); `None` (the test skips) without both.
fn installs() -> Option<(Data, PathBuf)> {
    let (Some(data), Some(rsf)) = (Data::from_env(), std::env::var_os("RBR_RSF")) else {
        eprintln!("skipping: RBR_DATA or RBR_RSF not set");
        return None;
    };
    Some((data, PathBuf::from(rsf)))
}

/// Whether car `name`'s `.sgc` model has an `i_steeringwheel` mesh, and its vertex count.
fn sgc_wheel(data: &Data, file_name: &str) -> Option<usize> {
    let path = data.resolve(file_name)?;
    let bytes = std::fs::read(&path).ok()?;
    let model = sgc::parse(&bytes).ok()?;
    model.iter().find_map(|e| match e {
        sgc::Element::Mesh(m) if m.node.name.eq_ignore_ascii_case("i_steeringwheel") => Some(m.vertices.len()),
        _ => None,
    })
}

#[test]
fn every_installed_rsf_car_with_a_drawn_interior_wheel_turns_it() {
    let Some((data, rsf_root)) = installs() else { return };
    let install = rsf::Install::open(&rsf_root).expect("RSF install");
    let (mut checked, mut skipped, mut without_node) = (0, 0, Vec::new());
    let mut before_missed = Vec::new();
    for rsf_car in &install.cars {
        let Ok(selected) = rsf::select(data.clone(), &rsf_car.pack, rsf::SLOT, Some(&rsf_root)) else {
            skipped += 1;
            continue;
        };
        if selected.model.fallback.is_some() {
            // Its own model folder is not installed: the retail fallback is covered by `crew_data`.
            skipped += 1;
            continue;
        }
        let name = selected.folder().to_owned();
        let Ok(interior) = car::load_interior(&selected.data, &name, glam::Mat4::IDENTITY) else {
            eprintln!("{name}: interior does not load");
            skipped += 1;
            continue;
        };
        let node = car::node_place(&selected.data, &name, "i_steeringwheel").expect("node place");
        let drawn = !interior.report.skipped.iter().any(|(n, _)| n.eq_ignore_ascii_case("i_steeringwheel"));
        let marked: Vec<(usize, f32)> = interior
            .meshes
            .iter()
            .zip(&interior.damage)
            .filter(|(_, effect)| effect.steering)
            .filter_map(|(m, _)| {
                m.bounds.map(|b| (m.vertices.len(), node.map_or(f32::INFINITY, |n| b.center.distance(n))))
            })
            .collect();
        let sgc_verts = sgc_wheel(&selected.data, &selected.model.entry.file_name);
        let min_dist = marked.iter().map(|(_, d)| *d).fold(f32::INFINITY, f32::min);
        if min_dist.is_finite() && min_dist >= BEFORE_MATCH_METRES {
            before_missed.push(name.clone());
        }
        eprintln!(
            "{name}: sgc wheel {sgc_verts:?}, node {}, drawn {drawn}, marked {} (min centre dist {min_dist:.3})",
            node.is_some(),
            marked.len(),
        );
        if node.is_none() {
            without_node.push(name.clone());
            assert!(marked.is_empty(), "{name}: marked wheel without its node");
            continue;
        }
        if sgc_verts.is_some() && drawn {
            assert!(!marked.is_empty(), "{name}: its drawn i_steeringwheel is not recognised as the wheel");
        }
        checked += 1;
    }
    eprintln!("{checked} RSF cars checked, {skipped} skipped, without an i_steeringwheel node: {without_node:?}");
    eprintln!("not recognised by the old centre match (>= {BEFORE_MATCH_METRES} m from the node): {before_missed:?}");
    assert!(checked > 0, "no RSF car with its own model installed");
}
