//! The cameras' collision probes on every retail stage's `.col`. Runs only when `RBR_DATA` is set.

use glam::Vec3;
use rbr_formats::col;
use rbr_render::stage_probe::StageProbe;
use std::path::Path;

#[test]
fn a_segment_down_through_each_upward_face_hits_it() {
    let Ok(data_dir) = std::env::var("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let maps = Path::new(&data_dir).join("Maps");
    let mut stages = 0;
    for entry in std::fs::read_dir(&maps).expect("Maps") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("col")) {
            continue;
        }
        let file = col::parse(&std::fs::read(&path).expect("read")).expect("parse");
        let probe = StageProbe::new(&file);
        let mut tested = 0;
        for cell in file.cells.iter().step_by(7) {
            for t in cell.tree.triangles.iter().step_by(97) {
                let [a, b, c] = t.vertices.map(|v| Vec3::from(cell.vertices[v as usize]));
                let normal = (c - a).cross(b - a);
                if normal.length() < 1e-3 || normal.normalize().z < 0.5 {
                    continue;
                }
                let centre = (a + b + c) / 3.0;
                let hit = probe.segment(centre + Vec3::Z * 2.0, centre - Vec3::Z * 2.0);
                let hit = hit.unwrap_or_else(|| panic!("{}: no hit at {centre:?}", path.display()));
                // The nearest face from above: this one or one over it.
                assert!(hit.point.z >= centre.z - 1e-3, "{}: {hit:?} below {centre:?}", path.display());
                tested += 1;
            }
        }
        assert!(tested > 0, "{}", path.display());
        stages += 1;
    }
    assert!(stages > 40, "{stages} stages");
}
