//! The deformation weights against HedgeHog3D's own, read from a snapshot of the original. Runs only when `RBR_DATA` is
//! set.
//!
//! `vcu-tanner-damage.snap-11827` (step 11827 of `drive-tanner-damage`, COROLLA after its impacts): the damage handler's
//! zones (`*(car+0x9f4)+0x3808`) equal HH's `g_carDamage`, and every `CGeomObjectCar` with `m_deformEnabled` holds the
//! same zones in `m_damage` (`+0x164`), so its `m_weights` (`+0x288`) are `UpdateDeformation`'s for these zones. The
//! rows were read from the snapshot by `/tmp/damage-visuals-scripts/hh_dump.py` (`lead/reports/damage-visuals.md`):
//! per panel its vertex count, its non-zero weights, the sum of its weights' f32 bit patterns, and the sum of
//! `|delta|` over the morph target its holder (`+0x160`) holds (0 without one).

use rbr_formats::data::Data;
use rbr_formats::sgc;
use rbr_render::damage::{self, ZONES};

/// `g_carDamage` in the snapshot: zone, value (f32 bits).
const ZONE_VALUES: [(usize, u32); 12] = [
    (1, 0x3f800000),
    (3, 0x3eda50fb),
    (8, 0x3f072838),
    (10, 0x3ef79eab),
    (11, 0x3f800000),
    (13, 0x3e1bee81),
    (14, 0x3d8ad74b),
    (24, 0x3e9daffa),
    (25, 0x3eaa339a),
    (27, 0x3ec6228d),
    (29, 0x3e58a159),
    (31, 0x3da51df2),
];

/// Per deformable object: name, vertices, non-zero weights, sum of the weights' bits, sum of |delta|.
const HH_PANELS: [(&str, usize, usize, u64, f64); 30] = [
    ("pc_e_window_lb", 13, 0, 0, 0.000112445),
    ("pc_e_window_rb", 13, 0, 0, 0.000125226),
    ("pc_e_window_f", 40, 0, 0, 0.000000000),
    ("pc_e_window_b", 44, 0, 0, 0.017664775),
    ("e_door_l", 131, 9, 10012217799, 0.040770548),
    ("e_window_lf", 10, 0, 0, 0.000503542),
    ("i_door_l", 131, 9, 10012217799, 0.000000000),
    ("pc_e_window_lf", 10, 0, 0, 0.000503542),
    ("e_door_r", 129, 116, 127903564871, 0.046433769),
    ("e_window_rf", 10, 4, 4432581228, 0.000724828),
    ("i_door_r", 129, 116, 127903564871, 0.000000000),
    ("pc_e_window_rf", 10, 0, 0, 0.000724828),
    ("e_wing", 321, 6, 6592605822, 0.024071057),
    ("e_window_lb", 13, 2, 2205258866, 0.000112445),
    ("e_window_rb", 13, 5, 5541443009, 0.000125226),
    ("i_window_lb", 13, 2, 2205258866, 0.000000000),
    ("i_window_rb", 13, 5, 5541443009, 0.000000000),
    ("e_body", 2509, 1216, 1349278166875, 0.677815233),
    ("i_window_f", 40, 12, 13355844631, 0.000000000),
    ("e_window_f", 40, 12, 13355844631, 0.000000000),
    ("e_bonnet", 264, 184, 204099174086, 0.288781241),
    ("e_bonnetvent", 16, 9, 9953468120, 0.008068676),
    ("i_bonnet", 264, 184, 204099174086, 0.288781242),
    ("i_bonnetvent", 16, 0, 0, 0.008068676),
    ("e_bumper_b", 461, 347, 384845184775, 0.239296772),
    ("e_bumper_f", 988, 613, 685055877888, 1.036954582),
    ("e_door_b", 192, 52, 57841048896, 0.040498855),
    ("e_window_b", 44, 5, 5438100813, 0.017664775),
    ("i_door_b", 192, 52, 57841048896, 0.040815034),
    ("i_window_b", 44, 5, 5438100813, 0.000000000),
];

#[test]
fn corolla_weights_equal_hedgehogs_at_snapshot_11827() {
    let Some(data) = Data::from_env() else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let path = data.resolve("Cars\\COROLLA\\corolla.sgc").expect("corolla.sgc");
    let model = sgc::parse(&std::fs::read(path).expect("read")).expect("parse");
    let mut zones = [0.0; ZONES];
    for (zone, value) in ZONE_VALUES {
        zones[zone] = f32::from_bits(value);
    }
    for (name, vertices, nonzero, bits, deltas) in HH_PANELS {
        let mesh = model
            .iter()
            .find_map(|e| match e {
                sgc::Element::Mesh(m) if m.node.name == name => Some(m),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: not in the model"));
        assert!(mesh.damageable, "{name}");
        assert_eq!(mesh.vertices.len(), vertices, "{name}");
        let weights = damage::vertex_weights(&mesh.morph_lists, &zones, vertices);
        assert_eq!(weights.iter().filter(|&&w| w != 0.0).count(), nonzero, "{name}: non-zero weights");
        assert_eq!(weights.iter().map(|w| u64::from(w.to_bits())).sum::<u64>(), bits, "{name}: weight bits");
        let ours: f64 =
            mesh.morph_targets.first().map_or(0.0, |t| t.deltas.iter().flatten().map(|d| f64::from(d.abs())).sum());
        assert!((ours - deltas).abs() <= 1e-6 * deltas.max(1.0), "{name}: |delta| sum {ours} against {deltas}");
    }
}
