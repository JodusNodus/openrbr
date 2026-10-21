//! A co-driver mod of the RSF install (`RBR_RSF`'s `Plugins/Pacenote`, `PaceNote.ini`'s voice) calls a stage: the
//! MyPacenotes notes of Tanner, driven through at 25 m/s, and the stage's own `.dls` keys (`RBR_DATA`). Skipped
//! without them.

use std::path::PathBuf;

use rbr_formats::data::Data;
use rbr_formats::rsf::{codriver, pacenotes};
use rbr_sound::custom_codriver::{stage_notes, Choice, CustomCoDriver, DEFAULT_RANGES, DEFAULT_STYLE};
use rbr_sound::voice::{Command, Speech};

/// The calls of a drive from 0 to `end` m at 25 m/s, a 7 ms step at a time, as sample names (1.02 words as `#id`).
fn drive(custom: &mut CustomCoDriver, end: f32) -> Vec<String> {
    let (mut x, mut said) = (0.0, Vec::new());
    while x < end {
        for command in custom.step(x, 25.0, || 0) {
            said.push(match command {
                Command::Speech(Speech::Sample(k)) => custom.sample_name(k).unwrap_or("?").to_owned(),
                Command::Speech(Speech::Word(w)) => format!("#{w:x}"),
                other => panic!("{other:?}"),
            });
        }
        x += 25.0 * 0.007;
    }
    said
}

#[test]
fn the_installed_voice_calls_mypacenotes_and_dls_notes() {
    let Some(root) = std::env::var_os("RBR_RSF").map(PathBuf::from) else {
        eprintln!("RBR_RSF is not set; skipped");
        return;
    };
    let Some(plugin) = codriver::plugin_dir(&root) else {
        eprintln!("{}: no Plugins/Pacenote; skipped", root.display());
        return;
    };
    let voice = codriver::settings(&plugin).voice.expect("PaceNote.ini names a voice");
    let notes = pacenotes::my_pacenotes_dir(&root).and_then(|dir| pacenotes::my_pacenotes(&dir, &["Tanner"]));
    let choice =
        Choice { plugin, voice, style: DEFAULT_STYLE.into(), ranges: DEFAULT_RANGES.into(), notes: notes.clone() };
    if notes.is_some() {
        let mut custom = CustomCoDriver::load(&choice, Vec::new(), 4.0).unwrap();
        let said = drive(&mut custom, 9000.0);
        eprintln!("{} calls: {}", said.len(), said[..said.len().min(60)].join(" "));
        assert!(said.len() > 100 && !said.iter().any(|s| s == "?"));
    }
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set; the .dls notes skipped");
        return;
    };
    let dls = stage_notes(&data, "track-54_O").unwrap();
    let mut custom = CustomCoDriver::load(&Choice { notes: None, ..choice }, dls, 4.0).unwrap();
    let said = drive(&mut custom, 9000.0);
    eprintln!("{} calls on the .dls notes: {}", said.len(), said[..said.len().min(60)].join(" "));
    assert!(said.len() > 100);
}
