//! The RallySimFans stage folders under `RBR_RSF` (an RSF install, or the samples' copy of its `Maps/` and
//! `rsfdata/cache/`): every folder reads, and every variant of a loadable stage parses. Skipped when `RBR_RSF` is unset.

use rbr_formats::data::Data;
use rbr_formats::{col, dls, fnc, lbs, markers, mat, pss, rsf, stage_files, trk};
use std::path::PathBuf;

fn root() -> Option<PathBuf> {
    std::env::var_os("RBR_RSF").map(PathBuf::from)
}

#[test]
fn every_rsf_stage_folder_reads_and_loadable_ones_parse() {
    let Some(root) = root() else {
        eprintln!("skipping: RBR_RSF not set");
        return;
    };
    let stages = rsf::scan(&root);
    assert!(!stages.is_empty(), "{}: no stage folders", root.display());
    for stage in &stages {
        let stage = stage.as_ref().unwrap_or_else(|(f, e)| panic!("{}: {e}", f.display()));
        eprintln!(
            "{} {:?} variants {:?} surface {:?} weathers {} problem {:?}",
            stage.id,
            stage.name,
            stage.variants,
            stage.surface,
            stage.weathers.len(),
            stage.problem
        );
        // 370 ships an empty `TrackSettings370.ini`, and a copy without its `Maps` folder has no other (the boot then
        // takes the first installed variant).
        let empty = rsf::settings_text(&stage.folder, stage.id).is_none();
        assert!(!stage.weathers.is_empty() || empty, "{}: no TrackSettings weathers", stage.id);
        // The particle file (RSF's own, or the game's when the root has none), plain even on encrypted stages.
        let data = Data::from_env().unwrap_or_else(|| Data::new(root.join("no-game-data"), None));
        if let Some(path) = stage.particle_file(&data, false) {
            let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
            pss::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
        if stage.problem.is_some() {
            continue;
        }
        stage.texture_map().unwrap_or_else(|e| panic!("{}: {e}", stage.id));
        for &letter in &stage.variants {
            let name = stage.stage(letter);
            let read = |ext: &str| std::fs::read(stage_files::file_in(&stage.folder, &name, ext)).unwrap();
            let check = |ext: &str, r: Result<(), String>| r.unwrap_or_else(|e| panic!("{name}.{ext}: {e}"));
            check("lbs", lbs::parse(&read("lbs")).map(drop).map_err(|e| e.to_string()));
            check("col", col::parse(&read("col")).map(drop).map_err(|e| e.to_string()));
            check("trk", trk::parse(&read("trk")).map(drop).map_err(|e| e.to_string()));
            check("dls", markers::parse(&read("dls")).map(drop).map_err(|e| e.to_string()));
            check("dls", dls::parse(&read("dls")).map(drop).map_err(|e| e.to_string()));
            check("fnc", fnc::parse(&read("fnc")).map(drop).map_err(|e| e.to_string()));
            check("mat", mat::parse(&read("mat")).map(drop).map_err(|e| e.to_string()));
        }
    }
}

/// The `PS_*.ini` particle files of the RSF `Maps` folder and its stage folders (RSF ships a particle mod over the
/// retail ones, and stages ship their own, like 450's `PS_Basque`): the ones that do not parse are listed. A stage's
/// own file is checked above; `PS_BTBTarmac.ini` lists more material aliases than its count says (116 for 112), so
/// its later sections read shifted, as they would in the game.
#[test]
fn rsf_particle_files_parse() {
    let Some(root) = root() else {
        eprintln!("skipping: RBR_RSF not set");
        return;
    };
    let maps = stage_files::in_dir(&root, "Maps").expect("Maps");
    let folders = std::fs::read_dir(&maps).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir());
    let (mut files, mut failed) = (0, Vec::new());
    for dir in std::iter::once(maps.clone()).chain(folders) {
        for path in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()) {
            let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("ps_") && name.ends_with(".ini") {
                let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
                if let Err(e) = pss::parse(&text) {
                    failed.push(format!("{name}: {e}"));
                }
                files += 1;
            }
        }
    }
    eprintln!("{files} particle files, {} do not parse: {failed:#?}", failed.len());
    assert!(files > 0);
    assert!(failed.iter().all(|f| f.starts_with("ps_btb")), "{failed:#?}");
}

/// Every RBRHUD gauge config (`rsf::gauges`) under `Plugins/RBRHUD/gauges` reads, with its atlas; the sprites and fonts
/// its elements name that the atlas lacks are listed (an element without its sprite is not drawn).
#[test]
fn rsf_gauge_configs_read() {
    let Some(gauges) = root().map(|r| r.join(rsf::gauges::GAUGES)).filter(|g| g.is_dir()) else {
        eprintln!("skipping: RBR_RSF has no {}", rsf::gauges::GAUGES);
        return;
    };
    let dirs = |dir: &std::path::Path| {
        let mut dirs: Vec<PathBuf> =
            std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        dirs
    };
    let (mut configs, mut references, mut missing) = (0, 0, Vec::new());
    for car in dirs(&gauges) {
        for variant in dirs(&car).into_iter().filter(|d| !d.ends_with("textures")) {
            for path in std::fs::read_dir(&variant).unwrap().filter_map(|e| e.ok()).map(|e| e.path()) {
                let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
                if !(name.starts_with("config") && name.ends_with(".ini")) {
                    continue;
                }
                let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
                let config =
                    rsf::gauges::Config::parse(&text).unwrap_or_else(|| panic!("{}: no [main]", path.display()));
                let atlas_path = rbr_formats::cars::resolve(&car, &config.atlas)
                    .unwrap_or_else(|| panic!("{}: no atlas {}", path.display(), config.atlas));
                let atlas = rsf::gauges::Atlas::parse(&String::from_utf8_lossy(&std::fs::read(&atlas_path).unwrap()));
                assert!(rbr_formats::cars::resolve(&car, &atlas.texture).is_some(), "{}", atlas_path.display());
                for element in &config.elements {
                    let found = match &element.kind {
                        rsf::gauges::Kind::Number(number) => atlas.font(&number.font).is_some(),
                        rsf::gauges::Kind::Other(_) => continue,
                        _ => element.texture.as_deref().is_none_or(|t| atlas.sprite(t).is_some()),
                    };
                    references += 1;
                    if !found {
                        missing.push(format!("{}: {}", path.display(), element.name));
                    }
                }
                configs += 1;
            }
        }
    }
    eprintln!(
        "{configs} gauge configs, {references} sprite references, {} not in the atlas: {missing:#?}",
        missing.len()
    );
    assert!(configs > 0);
    assert!(missing.len() * 50 < references, "{missing:#?}");
}

/// The Pacenote plugin of the RSF install: every style with every range set loads, and every MyPacenotes note file
/// parses, with the files its notes name present. Skipped without the plugin's folder.
#[test]
fn pacenote_plugin_styles_and_note_files_read() {
    use rsf::{codriver, pacenotes};
    let Some(root) = root() else {
        eprintln!("skipping: RBR_RSF not set");
        return;
    };
    let Some(dir) = codriver::plugin_dir(&root) else {
        eprintln!("skipping: {} has no Plugins/Pacenote", root.display());
        return;
    };
    let (styles, ranges) = (codriver::styles(&dir), codriver::range_sets(&dir));
    assert!(!styles.is_empty() && !ranges.is_empty(), "{}: no styles or range sets", dir.display());
    for style in &styles {
        for set in &ranges {
            let calls = codriver::load(&dir, style, set).unwrap_or_else(|e| panic!("{style} {set}: {e}"));
            assert!(!calls.notes.is_empty() && !calls.ranges.is_empty(), "{style} {set}: no calls");
        }
    }
    let voices = codriver::voices(&dir);
    eprintln!("{} styles, {} range sets, voices {voices:?}", styles.len(), ranges.len());
    if let Some(voice) = codriver::settings(&dir).voice {
        assert!(codriver::voice_dir(&dir, &voice).is_some(), "PaceNote.ini voice {voice} is not installed");
    }
    let Some(notes_dir) = pacenotes::my_pacenotes_dir(&root) else {
        eprintln!("no MyPacenotes folder");
        return;
    };
    let mut files = 0;
    for stage in std::fs::read_dir(&notes_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir())
    {
        for path in std::fs::read_dir(&stage).unwrap().filter_map(|e| e.ok()).map(|e| e.path()) {
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
                continue;
            }
            let notes = pacenotes::read(&path).unwrap_or_else(|e| panic!("{e}"));
            for call in [pacenotes::id::PACENOTES_FILE, pacenotes::id::RANGES_FILE] {
                let named = pacenotes::named_file(&notes, call);
                let known = if call == pacenotes::id::PACENOTES_FILE { &styles } else { &ranges };
                if let Some(name) = named.filter(|n| !known.iter().any(|k| k.eq_ignore_ascii_case(n))) {
                    eprintln!("{}: names {name}, not installed", path.display());
                }
            }
            files += 1;
        }
        let name = stage.file_name().unwrap().to_string_lossy().into_owned();
        assert!(pacenotes::my_pacenotes(&notes_dir, &[&name]).is_some(), "{name}: no note file chosen");
    }
    eprintln!("{files} note files");
}
