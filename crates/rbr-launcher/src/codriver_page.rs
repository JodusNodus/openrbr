//! The Settings page's Co-driver section: the voice and its preview, the note set the voice calls on every stage and
//! what the Drive page's stage gets with it, the corner and distance calls, the call timing, and the owned packs
//! (`codriver_packs`).

use egui::{Color32, ComboBox, DragValue};
use rbr_sound::preview::{self as codriver_preview, Preview};

use crate::codriver_packs::{set_label, NoteSet};
use crate::command::ORIGINAL_VOICE;
use crate::pages::{fields, section, FIELD, GOOD, WARN};
use crate::Form;

/// A call style or range set file's stem: without `.ini`.
fn ini_stem(file: &str) -> &str {
    match file.len().checked_sub(4) {
        Some(n) if file[n..].eq_ignore_ascii_case(".ini") => &file[..n],
        _ => file,
    }
}

/// The stem's base when it is a `-xx` language variant (exactly two ASCII lowercase letters after the last `-`):
/// ReRBR speaks English only, so variants stay out of the dropdowns unless chosen.
fn base_stem(file: &str) -> (&str, Option<&str>) {
    let stem = ini_stem(file);
    match stem.rsplit_once('-') {
        Some((base, lang)) if !base.is_empty() && lang.len() == 2 && lang.bytes().all(|b| b.is_ascii_lowercase()) => {
            (stem, Some(base))
        }
        _ => (stem, None),
    }
}

/// Whether the call style or range set file is offered: `-xx` language variants stay out unless chosen.
pub fn show_call_file(file: &str, current: &str) -> bool {
    base_stem(file).1.is_none() || file.eq_ignore_ascii_case(current)
}

/// A Corner calls file's label: what the player hears for each corner. Unknown stems show as themselves.
pub fn style_label(file: &str) -> String {
    let (stem, base) = base_stem(file);
    match base.unwrap_or(stem).to_ascii_lowercase().as_str() {
        "numeric" => "Numbers, 1 tightest (\"one left\")".into(),
        "numeric-swapped" => "Numbers, 1 tightest (\"left one\")".into(),
        "numeric-descending" => "Numbers, 6 tightest (\"six left\")".into(),
        "numeric-descending-swapped" => "Numbers, 6 tightest (\"left six\")".into(),
        "descriptive" => "Words (\"easy left\", \"hairpin\")".into(),
        "rbr" => "Original RBR calls".into(),
        "rbr-enhanced" => "Original RBR calls, more of them".into(),
        _ => stem.to_owned(),
    }
}

/// A Distance calls file's label. Unknown stems show as themselves.
pub fn range_label(file: &str) -> String {
    let (stem, base) = base_stem(file);
    match base.unwrap_or(stem).to_ascii_lowercase().as_str() {
        "rbr" => "Standard RBR distances".into(),
        "extended" => "Extended, finer distances".into(),
        _ => stem.to_owned(),
    }
}

/// The Co-driver dropdown's choice: off, the original 1.02 voice, or a voice by its source pack (none: the
/// RallySimFans install's plugin).
#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceChoice {
    Off,
    Original,
    Voice { pack: Option<String>, voice: String },
}

/// The Stage notes dropdown's choice: the stages' own notes, or a note set by its source pack (none: the
/// RallySimFans install's MyPacenotes).
#[derive(Debug, Clone, PartialEq, Eq)]
enum NotesChoice {
    Original,
    Set { pack: Option<String>, set: String },
}

impl VoiceChoice {
    /// The dropdown's choice for the saved fields: an unset voice plays its source's default (`default_voice`:
    /// its `PaceNote.ini`'s, else its first voice), so an old settings file without `codriver_voice` still names
    /// the voice heard; with no voice source at all the original voice drives.
    fn of(codriver: bool, pack: Option<&str>, voice: Option<&str>, default_voice: Option<&str>) -> Self {
        match voice.or(default_voice) {
            _ if !codriver => VoiceChoice::Off,
            Some(ORIGINAL_VOICE) | None => VoiceChoice::Original,
            Some(voice) => VoiceChoice::Voice { pack: pack.map(str::to_owned), voice: voice.to_owned() },
        }
    }

    /// The dropdown's text: the voice and its source.
    fn label(&self) -> String {
        match self {
            VoiceChoice::Off => "Off".into(),
            VoiceChoice::Original => "Original (1.02)".into(),
            VoiceChoice::Voice { pack: None, voice } => format!("{voice} (RallySimFans install)"),
            VoiceChoice::Voice { pack: Some(pack), voice } => format!("{voice} ({pack})"),
        }
    }
}

impl NotesChoice {
    /// The dropdown's text: the set and its source.
    fn label(&self) -> String {
        match self {
            NotesChoice::Original => "The stage's own notes".into(),
            NotesChoice::Set { pack: Some(pack), set } if set.is_empty() => pack.clone(),
            NotesChoice::Set { pack, set } => {
                let source = pack.as_deref().unwrap_or("RallySimFans install");
                format!("{} ({source})", set_label(pack.as_deref(), set))
            }
        }
    }
}

/// A note set's entry in the Stage notes dropdown: its name and how many stages it covers.
fn set_entry(pack: Option<&str>, set: &NoteSet) -> String {
    let stages = match set.stages {
        1 => "1 stage".to_owned(),
        n => format!("{n} stages"),
    };
    format!("{} · {stages}", set_label(pack, &set.name))
}

impl Form {
    /// The Co-driver dropdown's choice from the fields, resolving an unset voice to its source's default.
    fn voice_choice(&self) -> VoiceChoice {
        let default = self.codriver_mods.as_ref().and_then(|mods| mods.voice(None));
        VoiceChoice::of(
            self.codriver,
            self.codriver_pack.as_deref(),
            self.codriver_voice.as_deref(),
            default.as_deref(),
        )
    }

    /// The Stage notes dropdown's choice from the fields: the set that plays.
    fn notes_choice(&self) -> NotesChoice {
        match self.active_set() {
            Some(set) => NotesChoice::Set { pack: self.notes_pack.clone(), set: set.name.clone() },
            None => NotesChoice::Original,
        }
    }

    /// The Co-driver section: the voice and its preview, the note set, what the selected stage gets, the corner and
    /// distance calls (when they change anything), the call timing, and the packs.
    pub fn codriver_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Co-driver");
        fields(ui, "codriver", |ui| {
            self.codriver_voice_row(ui);
            self.codriver_notes_row(ui);
            self.codriver_status_row(ui);
            let custom = matches!(self.voice_choice(), VoiceChoice::Voice { .. });
            if let Some(mods) = self.codriver_mods.as_ref().filter(|_| custom) {
                if self.codriver_styles_differ {
                    let tip = "Which words the voice says for each corner";
                    Self::call_file_row(ui, "Corner calls", tip, &mods.styles, &mut self.codriver_style, style_label);
                }
                if self.codriver_ranges_differ {
                    let tip = "How the voice says the distances between notes";
                    Self::call_file_row(
                        ui,
                        "Distance calls",
                        tip,
                        &mods.ranges,
                        &mut self.codriver_ranges,
                        range_label,
                    );
                }
            }
            let tip = "How many seconds before a note the co-driver calls it, at your speed";
            ui.label("Call ahead").on_hover_text(tip);
            ui.add(DragValue::new(&mut self.marker.callout_distance).range(0.05..=8.0).speed(0.05).suffix(" s"))
                .on_hover_text(tip);
            ui.end_row();

            // The custom co-driver fires on its own lookahead and never reads the threshold: only the original
            // voice skips notes below it.
            if matches!(self.voice_choice(), VoiceChoice::Original) {
                let tip = "The original co-driver skips a note while the speed is below this";
                ui.label("Quiet below").on_hover_text(tip);
                ui.add(DragValue::new(&mut self.marker.marker_threshold).suffix(" km/h")).on_hover_text(tip);
                ui.end_row();
            }

            self.codriver_packs_row(ui);
        });
    }

    /// The Voice row: who calls the pace notes, with a preview of the corner ladder and one distance call. An
    /// unset voice plays its source's default, so an old settings file still names the voice heard.
    fn codriver_voice_row(&mut self, ui: &mut egui::Ui) {
        let tip = "Who calls the pace notes: off (no pace notes at all), the original 1.02 co-driver, or a \
                     voice from the RallySimFans install's Pacenote plugin (only read) or a pack copied into \
                     your files' folder. Whether a pack covers every note of a stage varies by pack.";
        ui.label("Voice").on_hover_text(tip);
        let current = self.voice_choice();
        ui.horizontal(|ui| {
            let mut picked = current.clone();
            ComboBox::from_id_salt("codriver voice").width(FIELD).selected_text(current.label()).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, VoiceChoice::Off, "Off");
                ui.selectable_value(&mut picked, VoiceChoice::Original, "Original (1.02)");
                if !self.rsf_voices.is_empty() {
                    ui.label("RallySimFans install");
                    for voice in &self.rsf_voices {
                        ui.selectable_value(
                            &mut picked,
                            VoiceChoice::Voice { pack: None, voice: voice.clone() },
                            voice,
                        );
                    }
                }
                for pack in self.owned_voices.iter().filter(|p| p.problem.is_none()) {
                    ui.label(&pack.name);
                    for voice in &pack.voices {
                        let choice = VoiceChoice::Voice { pack: Some(pack.name.clone()), voice: voice.clone() };
                        ui.selectable_value(&mut picked, choice, voice);
                    }
                }
            });
            match picked {
                _ if picked == current => {}
                // Off keeps the pack and voice, so turning it back on restores them.
                VoiceChoice::Off => self.codriver = false,
                VoiceChoice::Original => {
                    (self.codriver, self.codriver_voice) = (true, Some(ORIGINAL_VOICE.to_owned()));
                }
                // A changed pack rescans and normalizes in `pages.rs`.
                VoiceChoice::Voice { pack, voice } => {
                    (self.codriver, self.codriver_pack, self.codriver_voice) = (true, pack, Some(voice));
                }
            }
            // The preview beside the dropdown: the corner ladder and one distance call with this voice. Built
            // on a thread, so the UI never stalls.
            let idle = self.child.is_none() && self.codriver_job.is_none();
            let preview = ui
                .add_enabled(idle && !matches!(current, VoiceChoice::Off), egui::Button::new("▶ Preview"))
                .on_hover_text(
                    "Hear the corners, tightest to fastest, and one distance call with this voice, style and \
                     range set",
                );
            if preview.clicked() {
                self.preview_codriver();
            }
        });
        ui.end_row();

        // A chosen source that is gone or broke: the original voice drives instead.
        if let VoiceChoice::Voice { pack, .. } = self.voice_choice() {
            let bad = match pack.as_deref() {
                Some(pack) => match self.owned_voices.iter().find(|p| p.name == pack) {
                    Some(found) => found.problem.clone().map(|p| format!("{pack}: {p}")),
                    None => Some(format!("{pack}: no longer installed")),
                },
                None if !self.rsf_plugin => Some("the RallySimFans install has no Pacenote plugin".to_owned()),
                None => None,
            };
            if let Some(warn) = bad {
                ui.label("");
                ui.colored_label(WARN, format!("⚠ {warn}: the original voice"));
                ui.end_row();
            }
        }
    }

    /// A Corner calls or Distance calls row: the plugin's `files` by `label`, language variants left out unless chosen.
    fn call_file_row(
        ui: &mut egui::Ui,
        name: &str,
        tip: &str,
        files: &[String],
        chosen: &mut String,
        label: fn(&str) -> String,
    ) {
        ui.label(name).on_hover_text(tip);
        ComboBox::from_id_salt(name)
            .selected_text(label(chosen))
            .show_ui(ui, |ui| {
                let shown: Vec<&String> = files.iter().filter(|f| show_call_file(f, chosen)).collect();
                for file in shown {
                    ui.selectable_value(chosen, file.clone(), label(file));
                }
            })
            .response
            .on_hover_text(tip);
        ui.end_row();
    }

    /// The Stage notes row: which note set the custom voice calls on every stage, each with the stages it covers.
    /// Without a custom voice there is nothing to call them with, so the dropdown stays disabled.
    fn codriver_notes_row(&mut self, ui: &mut egui::Ui) {
        let custom = matches!(self.voice_choice(), VoiceChoice::Voice { .. });
        let tip = match custom {
            true => {
                "The notes the co-driver calls on every stage: the stage's own, or a note set from the \
                     RallySimFans install (only read) or a pack in your files' folder. A stage the set has no notes \
                     for keeps its own notes."
            }
            false => {
                "Note sets need a voice from the RallySimFans install or a pack: the original co-driver only \
                      calls the stage's own notes."
            }
        };
        ui.label("Stage notes").on_hover_text(tip);
        let current = self.notes_choice();
        let mut picked = current.clone();
        ui.add_enabled_ui(custom, |ui| {
            ComboBox::from_id_salt("notes set")
                .width(FIELD)
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut picked, NotesChoice::Original, "The stage's own notes");
                    if !self.rsf_note_sets.is_empty() {
                        ui.label("RallySimFans install");
                        for set in &self.rsf_note_sets {
                            let choice = NotesChoice::Set { pack: None, set: set.name.clone() };
                            ui.selectable_value(&mut picked, choice, set_entry(None, set));
                        }
                    }
                    for pack in &self.owned_notes {
                        ui.label(&pack.name);
                        for set in &pack.sets {
                            let choice = NotesChoice::Set { pack: Some(pack.name.clone()), set: set.name.clone() };
                            ui.selectable_value(&mut picked, choice, set_entry(Some(&pack.name), set));
                        }
                    }
                })
                .response
                .on_hover_text(tip);
        });
        match picked {
            _ if picked == current => {}
            NotesChoice::Original => self.my_pacenotes = false,
            NotesChoice::Set { pack, set } => {
                (self.my_pacenotes, self.notes_pack, self.notes_set) = (true, pack, Some(set));
            }
        }
        ui.end_row();
    }

    /// The summary row: what the co-driver says on the Drive page's stage (the note file on hover), refreshed when
    /// its inputs change, never scanned per draw.
    fn codriver_status_row(&mut self, ui: &mut egui::Ui) {
        ui.label("");
        self.refresh_notes_cache();
        let prefix = format!("On {}: ", self.notes_summary.stage);
        ui.vertical(|ui| self.notes_summary_label(ui, &prefix));
        ui.end_row();
    }

    /// The notes summary after `prefix`, in the warning colour when the stage keeps its own notes for want of the
    /// chosen set's; the note file on hover. Takes a `ui` of its own (it sets the width).
    fn notes_summary_label(&self, ui: &mut egui::Ui, prefix: &str) -> egui::Response {
        let summary = &self.notes_summary;
        let text = format!("{prefix}{}", summary.text);
        let text = match summary.fallback {
            true => egui::RichText::new(format!("⚠ {text}")).color(WARN),
            false => egui::RichText::new(text).color(Color32::GRAY),
        };
        // Wrapped at the fields' width: a long line never widens the page's column.
        ui.set_max_width(FIELD);
        let response = ui.add(egui::Label::new(text).wrap());
        match &summary.file {
            Some(file) => response.on_hover_text(file.display().to_string()),
            None => response,
        }
    }

    /// The Packs row: the import buttons and Manage packs, as before.
    fn codriver_packs_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Packs");
        ui.vertical(|ui| {
            self.import_codriver_ui(ui);
            egui::CollapsingHeader::new("Manage packs").id_salt("codriver packs").show(ui, |ui| {
                // No removals while importing or driving.
                let idle = self.codriver_job.is_none() && self.child.is_none();
                let mut remove_voice = None;
                for pack in &self.owned_voices {
                    ui.horizontal(|ui| {
                        match &pack.problem {
                            Some(problem) => ui.colored_label(WARN, format!("{}: {problem}", pack.name)),
                            None => ui.colored_label(
                                GOOD,
                                format!(
                                    "✔ {}: {} voices, {} call styles, {} range sets",
                                    pack.name,
                                    pack.voices.len(),
                                    pack.styles.len(),
                                    pack.ranges.len()
                                ),
                            ),
                        };
                        if ui.add_enabled(idle, egui::Button::new("Remove")).clicked() {
                            remove_voice = Some(pack.name.clone());
                        }
                    });
                }
                if let Some(pack) = remove_voice {
                    self.codriver_status = crate::codriver_packs::remove_voice(&self.here(), &pack).err().map(Err);
                    if self.codriver_pack.as_deref() == Some(&pack) {
                        self.codriver_pack = None;
                    }
                    self.rescan_codrivers();
                }
                let mut remove_notes = None;
                for pack in &self.owned_notes {
                    ui.horizontal(|ui| {
                        ui.colored_label(GOOD, format!("✔ {}: {} note files", pack.name, pack.files));
                        if ui.add_enabled(idle, egui::Button::new("Remove")).clicked() {
                            remove_notes = Some(pack.name.clone());
                        }
                    });
                }
                if let Some(pack) = remove_notes {
                    self.codriver_status = crate::codriver_packs::remove_notes(&self.here(), &pack).err().map(Err);
                    if self.notes_pack.as_deref() == Some(&pack) {
                        (self.notes_pack, self.notes_set) = (None, None);
                    }
                    self.rescan_codrivers();
                }
            });
        });
        ui.end_row();
    }

    /// Plays the corner ladder and one distance call in the chosen voice through the menus' sound, built on its own
    /// thread.
    fn preview_codriver(&mut self) {
        let build: Box<dyn FnOnce() -> Result<Preview, String> + Send> = match self.voice_choice() {
            VoiceChoice::Off => return,
            VoiceChoice::Original => {
                let data = self.data.clone();
                Box::new(move || codriver_preview::original_preview(&data))
            }
            VoiceChoice::Voice { .. } => {
                let Some(mods) = &self.codriver_mods else { return };
                let Some(voice) = mods.voice(self.codriver_voice.as_deref()) else { return };
                let (plugin, style, ranges) =
                    (mods.dir.clone(), self.codriver_style.clone(), self.codriver_ranges.clone());
                Box::new(move || codriver_preview::custom_preview(&plugin, &voice, &style, &ranges))
            }
        };
        let gain = self.codriver_volume;
        if let Some(player) = self.menu_player() {
            player.preview(gain, build);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::settings::{load, Settings};
    use crate::{Start, SETTINGS_FILE};

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }

    /// The co-driver rows drawn offscreen to `RBR_SCREENSHOTS`: a locally owned voice pack and Tanner notes with
    /// no RSF install, and the RSF install's plugin. Needs `RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS` and `RBR_RSF`.
    #[test]
    fn screenshots_of_the_codriver_rows() {
        let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        let (Some(shots), Some(data), Some(patch), Some(ngp), Some(rsf)) =
            (env("RBR_SCREENSHOTS"), env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS"), env("RBR_RSF"))
        else {
            eprintln!("skipping: RBR_SCREENSHOTS, RBR_DATA, RBR_PATCH, RBR_NGP_PHYSICS or RBR_RSF not set");
            return;
        };
        let root = std::env::temp_dir().join(format!("rbr-launcher-codriver-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // A voice download and a notes-only download, as imported.
        let download = root.join("voice");
        write(download.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(download.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(download.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(download.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(download.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        let notes_src = root.join("notes");
        write(
            notes_src.join("Tanner/tanner.ini"),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n",
        );
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        crate::codriver_packs::import(&home, &download, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        crate::codriver_packs::import(&home, &notes_src, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let start_in = |home: &Path| Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: home.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        // Without RSF: the owned packs drive.
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp.clone()),
            rsf_root: Some(String::new()),
            codriver_pack: Some("voice".into()),
            notes_pack: Some("notes".into()),
            ..Settings::default()
        };
        let mut local = load(&start_in(&home), settings).unwrap();
        local.read_only = true;
        assert!(local.codriver_mods.is_some());
        assert!(!local.rsf_plugin);
        // With RSF: the install's plugin drives, as before.
        let settings = Settings { map: Some(54), ngp_physics: Some(ngp), rsf_root: Some(rsf), ..Settings::default() };
        let elsewhere = root.join("rsf-home");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let mut installed = load(&start_in(&elsewhere), settings).unwrap();
        installed.read_only = true;
        assert!(installed.codriver_mods.is_some());
        assert!(installed.rsf_plugin);
        for (name, form) in [("codriver-local.png", &mut local), ("codriver-rsf.png", &mut installed)] {
            // The summary names the stage, the voice and the set; the file rides the tooltip.
            form.refresh_notes_cache();
            let summary = &form.notes_summary;
            assert!(!summary.stage.is_empty() && !summary.fallback, "{summary:?}");
            assert!(summary.text.contains(" calls ") && summary.text.ends_with(")"), "{summary:?}");
            assert!(summary.file.as_ref().is_some_and(|f| f.extension().is_some_and(|e| e == "ini")), "{summary:?}");
            let rgba = crate::capture::draw(|ctx| {
                ctx.root_ui(|root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        form.codriver_section(ui);
                    })
                });
            })
            .unwrap();
            crate::capture::write_png(&Path::new(&shots).join(name), &rgba).unwrap();
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The Co-driver dropdown reads the saved fields: an unset voice names its source's default, and without a
    /// source the original voice drives. The Stage notes dropdown names the set and its source. Needs no game data.
    #[test]
    fn the_dropdowns_read_the_fields() {
        use super::{NotesChoice, VoiceChoice};
        use crate::command::ORIGINAL_VOICE;
        let voice =
            |pack: Option<&str>, voice: &str| VoiceChoice::Voice { pack: pack.map(Into::into), voice: voice.into() };
        assert_eq!(VoiceChoice::of(false, Some("pack"), Some("v"), None), VoiceChoice::Off);
        assert_eq!(VoiceChoice::of(true, Some("pack"), Some(ORIGINAL_VOICE), None), VoiceChoice::Original);
        assert_eq!(VoiceChoice::of(true, None, Some("Janne"), Some("steve")), voice(None, "Janne"));
        assert_eq!(VoiceChoice::of(true, Some("pack"), None, Some("steve")), voice(Some("pack"), "steve"));
        assert_eq!(VoiceChoice::of(true, Some("pack"), None, None), VoiceChoice::Original);
        assert_eq!(voice(None, "Janne").label(), "Janne (RallySimFans install)");
        let set = |pack: Option<&str>, set: &str| NotesChoice::Set { pack: pack.map(Into::into), set: set.into() };
        assert_eq!(set(None, "LuppisV3").label(), "LuppisV3 (RallySimFans install)");
        assert_eq!(set(None, "").label(), "MyPacenotes (RallySimFans install)");
        assert_eq!(set(Some("tanner-notes"), "").label(), "tanner-notes");
        assert_eq!(set(Some("pack"), "Luppis").label(), "Luppis (pack)");
    }

    /// The summary follows the note set, the notes switch, the co-driver switch and the voice: switching one dirties
    /// the cache (as `pages.rs` does), and the next refresh says what the launch passes: never a stale file, and
    /// never a custom file when the viewer would not get `--pacenotes`; a set without the stage says so.
    #[test]
    fn notes_summary_follows_sources_and_voice() {
        use crate::command::ORIGINAL_VOICE;
        let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        let (Some(data), Some(patch), Some(ngp)) = (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS")) else {
            eprintln!("skipping: RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS not set");
            return;
        };
        let root = std::env::temp_dir().join(format!("rbr-launcher-notes-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let download = root.join("voice");
        write(download.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(download.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(download.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(download.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(download.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        let notes_src = root.join("notes");
        let note = "[PACENOTES]\ncount = 2\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n\n\
                    [P1]\ntype = 3\ndistance = 200.0\nflag = 0\n";
        write(notes_src.join("Tanner/tanner.ini"), note);
        write(notes_src.join("Somewhere/Somewhere Fast.ini"), note);
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let no_cancel = std::sync::atomic::AtomicBool::new(false);
        crate::codriver_packs::import(&home, &download, &no_cancel).unwrap();
        crate::codriver_packs::import(&home, &notes_src, &no_cancel).unwrap();
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: home.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp),
            rsf_root: Some(String::new()),
            codriver_pack: Some("voice".into()),
            notes_pack: Some("notes".into()),
            ..Settings::default()
        };
        let mut form = load(&start, settings).unwrap();
        form.read_only = true;
        let ctx = egui::Context::default();
        let summary = |form: &mut crate::Form| {
            form.refresh_notes_cache();
            (form.notes_summary.text.clone(), form.notes_summary.fallback)
        };
        // Unset, the pack's largest set plays: its file for Tanner.
        let calls = |set: &str| format!("english/steve calls {set} (2 notes)");
        assert_eq!(summary(&mut form), (calls("notes"), false));
        assert!(form.notes_summary.file.as_ref().is_some_and(|f| f.ends_with("Tanner/tanner.ini")));
        // Another set without Tanner: the stage keeps its own notes, said as a fallback.
        let before = form.notes_inputs();
        form.notes_set = Some("Fast".into());
        form.invalidate_notes_cache(&ctx, before);
        let own = "english/steve calls the stage's own notes".to_owned();
        assert_eq!(
            summary(&mut form),
            ("Fast has no notes for this stage: english/steve calls the stage's own notes".into(), true)
        );
        assert_eq!(form.notes_summary.file, None);
        // The stage's own notes, chosen.
        let before = form.notes_inputs();
        form.my_pacenotes = false;
        form.invalidate_notes_cache(&ctx, before);
        assert_eq!(summary(&mut form), (own.clone(), false));
        // A note set never claims to play with the co-driver off or the 1.02 voice.
        let before = form.notes_inputs();
        (form.my_pacenotes, form.notes_set, form.codriver) = (true, None, false);
        form.invalidate_notes_cache(&ctx, before);
        assert_eq!(summary(&mut form), ("no co-driver, no notes are called".into(), false));
        let before = form.notes_inputs();
        (form.codriver, form.codriver_voice) = (true, Some(ORIGINAL_VOICE.into()));
        form.invalidate_notes_cache(&ctx, before);
        assert_eq!(summary(&mut form), ("the 1.02 co-driver calls the stage's own notes".into(), false));
        // A missing voice pack (rescans dirty the cache): the original voice drives.
        form.codriver_voice = None;
        form.codriver_mods = None;
        form.cached_notes_dirty = true;
        assert_eq!(summary(&mut form), ("the 1.02 co-driver calls the stage's own notes".into(), false));
        // Unchanged inputs dirty nothing.
        form.cached_notes_dirty = false;
        form.invalidate_notes_cache(&ctx, form.notes_inputs());
        assert!(!form.cached_notes_dirty);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Corner and Distance calls show human labels for the known stems, the bare stem otherwise; `-xx` language
    /// variants stay out of the dropdowns unless chosen. Needs no game data.
    #[test]
    fn corner_and_distance_labels_and_language_variants() {
        use super::{range_label, show_call_file, style_label};
        assert_eq!(style_label("Numeric.ini"), "Numbers, 1 tightest (\"one left\")");
        assert_eq!(style_label("Numeric-Swapped.ini"), "Numbers, 1 tightest (\"left one\")");
        assert_eq!(style_label("Numeric-Descending.ini"), "Numbers, 6 tightest (\"six left\")");
        assert_eq!(style_label("Numeric-Descending-Swapped.ini"), "Numbers, 6 tightest (\"left six\")");
        assert_eq!(style_label("Descriptive.ini"), "Words (\"easy left\", \"hairpin\")");
        assert_eq!(style_label("Rbr.ini"), "Original RBR calls");
        assert_eq!(style_label("Rbr-Enhanced.ini"), "Original RBR calls, more of them");
        assert_eq!(style_label("Mine.ini"), "Mine");
        assert_eq!(range_label("Rbr.ini"), "Standard RBR distances");
        assert_eq!(range_label("Extended.ini"), "Extended, finer distances");
        assert_eq!(range_label("Mine.ini"), "Mine");
        // Variants label as their base stem but stay hidden unless chosen.
        assert_eq!(style_label("Numeric-cz.ini"), style_label("Numeric.ini"));
        assert!(!show_call_file("Numeric-cz.ini", "Numeric.ini"));
        assert!(show_call_file("Numeric-cz.ini", "Numeric-cz.ini"));
        assert!(show_call_file("Numeric.ini", "Numeric.ini"));
        // `Rbr-Enhanced` is no variant: more than two letters after the last dash.
        assert!(show_call_file("Rbr-Enhanced.ini", "Numeric.ini"));
    }

    /// The RSF install's styles and range sets all load the same packages, so they are detected as identical, and
    /// its default voice's preview says every corner and the distance. Needs `RBR_RSF`.
    #[test]
    fn the_rsf_installs_styles_are_identical_and_its_preview_speaks() {
        let Some(rsf) = std::env::var_os("RBR_RSF").map(|v| v.to_string_lossy().into_owned()) else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let Some(mods) = crate::command::CodriverMods::scan(&rsf) else {
            eprintln!("skipping: no Pacenote plugin in RBR_RSF");
            return;
        };
        assert!(!mods.styles.is_empty() && !mods.ranges.is_empty());
        let (styles_differ, ranges_differ) =
            crate::codriver_packs::call_sets_differ(&mods.dir, &mods.styles, &mods.ranges);
        assert!(!styles_differ, "every top-level style points at the same packages");
        assert!(!ranges_differ, "every range set points at the same packages");
        let Some(voice) = mods.voice(None) else {
            eprintln!("skipping: the install names no voice");
            return;
        };
        let preview = rbr_sound::preview::custom_preview(&mods.dir, &voice, &mods.styles[0], &mods.ranges[0]).unwrap();
        assert_eq!(preview.calls.len(), rbr_sound::preview::LADDER.len() + 1, "every corner and the distance");
        assert!(preview.wave.samples.len() > preview.calls.len() * preview.wave.rate as usize / 10);
    }
}
