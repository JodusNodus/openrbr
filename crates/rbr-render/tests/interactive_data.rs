//! Per-stage accounting for the retail interactive objects in `(1,6)`.

use rbr_render::stage::{self, Selection};
use std::path::{Path, PathBuf};

fn maps_dir() -> Option<PathBuf> {
    let Ok(data_dir) = std::env::var("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return None;
    };
    Some(Path::new(&data_dir).join("Maps"))
}

fn stage_names(maps: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(maps)
        .expect("Maps directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lbs"))
        .filter_map(|path| path.file_stem()?.to_str().map(str::to_owned))
        .filter(|name| !name.eq_ignore_ascii_case("clouds") && !name.eq_ignore_ascii_case("skyboxes"))
        .collect();
    names.sort();
    names
}

#[test]
fn every_stage_accounts_for_interactive_draw_items() {
    let Some(maps) = maps_dir() else { return };
    let names = stage_names(&maps);
    assert_eq!(names.len(), 86, "retail stage files");

    let (mut stages_with_interactives, mut objects, mut parsed, mut drawn, mut withheld) = (0, 0, 0, 0, 0);
    let (mut instances, mut instances_drawn, mut instances_withheld) = (0, 0, 0);
    for name in &names {
        let stage = stage::load(&maps, name, Selection::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let report = &stage.report;
        assert_eq!(
            report.interactive_draw_items_drawn + report.interactive_draw_items_not_drawn,
            report.interactive_draw_items_parsed,
            "{name}: no parsed interactive draw item may disappear from accounting"
        );
        assert_eq!(
            report.interactive_instances_drawn + report.interactive_instances_withheld,
            report.interactive_instances_parsed,
            "{name}: no visibility instance may disappear from accounting"
        );
        assert_eq!(report.interactive_instances_drawn, stage.interactive.len(), "{name}: drawn instance meshes");
        assert_eq!(report.interactive_withheld_by_class.values().sum::<usize>(), report.interactive_instances_withheld);
        assert_eq!(
            report.interactive_objects_parsed > 0,
            report.interactive_draw_items_parsed > 0,
            "{name}: interactive objects and their draw records"
        );
        if report.interactive_objects_parsed > 0 {
            stages_with_interactives += 1;
        }
        objects += report.interactive_objects_parsed;
        parsed += report.interactive_draw_items_parsed;
        drawn += report.interactive_draw_items_drawn;
        withheld += report.interactive_draw_items_not_drawn;
        instances += report.interactive_instances_parsed;
        instances_drawn += report.interactive_instances_drawn;
        instances_withheld += report.interactive_instances_withheld;
        println!(
            "{name}: {} objects, {}/{} draw records drawn/withheld, {}/{} instances drawn/withheld",
            report.interactive_objects_parsed,
            report.interactive_draw_items_drawn,
            report.interactive_draw_items_not_drawn,
            report.interactive_instances_drawn,
            report.interactive_instances_withheld
        );
    }

    assert_eq!(stages_with_interactives, 76);
    assert_eq!(objects, 1127);
    assert_eq!(parsed, 1992);
    assert_eq!(drawn + withheld, parsed);
    assert_eq!(instances, 13_703);
    assert_eq!(instances_drawn + instances_withheld, instances);
    println!("retail total: {drawn}/{withheld} draw records and {instances_drawn}/{instances_withheld} visibility instances drawn/withheld");
}
