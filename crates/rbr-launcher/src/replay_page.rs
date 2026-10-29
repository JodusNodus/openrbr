//! The player's saved drives: browse headers, watch a drive, or export it through the viewer.

use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{ComboBox, RichText};
use rbr_recording::{realtime, replay};

use crate::pages::{section, WARN};
use crate::Form;

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    modified: SystemTime,
    title: String,
    problem: Option<String>,
}

impl Entry {
    fn read(path: PathBuf) -> Self {
        let modified = path.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        let header = (|| {
            let mut input = BufReader::new(File::open(&path).map_err(|e| e.to_string())?);
            let args = replay::read_args(&mut input)?;
            let mut magic = [0; 4];
            input.read_exact(&mut magic).map_err(|_| "No drive frames saved yet")?;
            if magic != realtime::LOG_MAGIC {
                return Err("Invalid replay frame log".into());
            }
            let mut first = [0; realtime::LOGGED_BYTES];
            input.read_exact(&mut first).map_err(|_| "No complete drive frames saved yet")?;
            if !args.iter().any(|arg| arg == "--live") {
                return Err("Missing the saved drive command".into());
            }
            Ok::<_, String>(args)
        })();
        let fallback = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let title = header.as_ref().ok().and_then(|args| {
            let viewer = &args[..args.iter().position(|a| a == "--live").unwrap_or(args.len())];
            viewer.windows(2).find(|w| w[0] == "--title").map(|w| w[1].clone())
        });
        Self { path, modified, title: title.unwrap_or(fallback), problem: header.err() }
    }
}

pub(super) struct Job {
    video: Option<PathBuf>,
    log: PathBuf,
}

/// Cached directory contents and the page's current selection. Re-read on entering, Refresh and child exit.
pub struct Replays {
    entries: Vec<Entry>,
    selected: Option<PathBuf>,
    directory: Option<PathBuf>,
    query: String,
    output: String,
    size: (u32, u32),
    fps: u32,
    view: &'static str,
    message: String,
    pub(super) job: Option<Job>,
}

impl Default for Replays {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected: None,
            directory: None,
            query: String::new(),
            output: String::new(),
            size: (1920, 1080),
            fps: 60,
            view: "cockpit",
            message: String::new(),
            job: None,
        }
    }
}

impl Replays {
    pub fn refresh(&mut self, home: &Path) {
        let directory = home.join("replays");
        self.directory = Some(directory.clone());
        self.entries = match fs::read_dir(&directory) {
            Ok(files) => files
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "rrp"))
                .map(Entry::read)
                .collect(),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.message = format!("Cannot read saved drives: {e}");
                }
                Vec::new()
            }
        };
        self.entries.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| b.path.cmp(&a.path)));
        if !self.entries.iter().any(|e| Some(&e.path) == self.selected.as_ref()) {
            self.select(self.entries.first().map(|e| e.path.clone()), home);
        }
    }

    fn select(&mut self, path: Option<PathBuf>, home: &Path) {
        self.output = path
            .as_ref()
            .map(|p| {
                home.join("videos").join(p.file_name().unwrap_or_default()).with_extension("mp4").display().to_string()
            })
            .unwrap_or_default();
        self.selected = path;
    }

    /// Called by the common child poll instead of advancing a championship for a replay's exit.
    pub fn finished(&mut self, success: bool) {
        if let Some(job) = self.job.take() {
            self.message = match (success, job.video) {
                (true, Some(path)) => format!("Video saved to {}", path.display()),
                (true, None) => "Replay closed".into(),
                (false, _) => format!(
                    "Replay failed. {}\nLog: {}",
                    log_tail(&job.log).lines().last().unwrap_or("See the log for details."),
                    job.log.display()
                ),
            };
        }
    }
}

fn log_tail(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else { return String::new() };
    let len = file.metadata().map_or(0, |m| m.len());
    let _ = file.seek(SeekFrom::Start(len.saturating_sub(4096)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn age(modified: SystemTime) -> String {
    let seconds = modified.elapsed().unwrap_or_default().as_secs();
    match seconds {
        0..60 => "Just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86400),
    }
}

/// Viewer options precede its saved live command, so replay expansion can override presentation settings.
fn viewer_args(path: &Path, view: &str, video: Option<(&Path, (u32, u32), u32)>) -> Vec<String> {
    let mut args = vec!["--replay".into(), path.display().to_string(), "--view".into(), view.into()];
    if let Some((out, (width, height), fps)) = video {
        args.extend([
            "--video".into(),
            out.display().to_string(),
            "--size".into(),
            format!("{width}x{height}"),
            "--video-fps".into(),
            fps.to_string(),
        ]);
    }
    args
}

impl Form {
    pub fn replays_page(&mut self, ui: &mut egui::Ui) {
        let home = self.here();
        if self.replays.directory.as_ref() != Some(&home.join("replays")) {
            self.replays.refresh(&home);
        }
        section(ui, "Past drives");
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut self.replays.query);
            if ui.button("Refresh").clicked() {
                self.replays.refresh(&home);
            }
            ui.weak(format!("{} saved drives", self.replays.entries.len()));
        });
        ui.add_space(8.0);
        if self.replays.entries.is_empty() {
            ui.label("Your drives are saved here automatically after you start driving.");
            ui.weak(home.join("replays").display().to_string());
            ui.label(&self.replays.message);
            return;
        }
        let mut chosen = None;
        let mut watch = false;
        let mut export = false;
        ui.columns(2, |columns| {
            let query = self.replays.query.to_lowercase();
            egui::ScrollArea::vertical().id_salt("replays-list").show(&mut columns[0], |ui| {
                let mut matches = 0;
                for entry in &self.replays.entries {
                    if !entry.title.to_lowercase().contains(&query)
                        && !entry.path.to_string_lossy().to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    matches += 1;
                    let selected = self.replays.selected.as_ref() == Some(&entry.path);
                    let label = format!("{}\n{}", entry.title, age(entry.modified));
                    if ui
                        .add(
                            egui::Button::selectable(selected, label)
                                .wrap()
                                .min_size(egui::vec2(ui.available_width(), 56.0)),
                        )
                        .clicked()
                    {
                        chosen = Some(entry.path.clone());
                    }
                }
                if matches == 0 {
                    ui.label("No saved drives match this search.");
                }
            });
            if let Some(path) = &chosen {
                self.replays.select(Some(path.clone()), &home);
            }
            let ui = &mut columns[1];
            let selected =
                self.replays.entries.iter().find(|e| Some(&e.path) == self.replays.selected.as_ref()).cloned();
            let Some(entry) = selected else { return };
            egui::ScrollArea::vertical().id_salt("replay-details").show(ui, |ui| {
                ui.label(RichText::new(&entry.title).size(18.0).strong());
                ui.weak(entry.path.file_name().unwrap_or_default().to_string_lossy());
                if let Some(problem) = &entry.problem {
                    ui.colored_label(WARN, problem);
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Camera");
                    ComboBox::from_id_salt("replay-view").selected_text(self.replays.view).show_ui(ui, |ui| {
                        for view in ["cockpit", "bonnet", "chase"] {
                            ui.selectable_value(&mut self.replays.view, view, view);
                        }
                    });
                });
                let ready = self.child.is_none() && entry.problem.is_none() && self.readiness().is_none();
                watch = ui.add_enabled(ready, egui::Button::new("Watch replay")).clicked();
                ui.weak("Space pauses · R restarts · Esc closes");
                section(ui, "Export to video");
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt("replay-size").selected_text(format!("{}p", self.replays.size.1)).show_ui(
                        ui,
                        |ui| {
                            for size in [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)] {
                                ui.selectable_value(&mut self.replays.size, size, format!("{}p", size.1));
                            }
                        },
                    );
                    ComboBox::from_id_salt("replay-fps").selected_text(format!("{} fps", self.replays.fps)).show_ui(
                        ui,
                        |ui| {
                            for fps in [30, 60] {
                                ui.selectable_value(&mut self.replays.fps, fps, format!("{fps} fps"));
                            }
                        },
                    );
                });
                ui.label("Save MP4 to");
                ui.add(egui::TextEdit::singleline(&mut self.replays.output).desired_width(f32::INFINITY));
                let output = Path::new(self.replays.output.trim());
                let exists = output.exists();
                let mp4 = output.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"));
                export = ui.add_enabled(ready && !exists && mp4, egui::Button::new("Export to video")).clicked();
                if exists {
                    ui.colored_label(WARN, "That file exists. Choose a new filename.");
                } else if !mp4 {
                    ui.weak("Choose a filename ending in .mp4");
                }
                ui.weak("Includes game audio. Requires ffmpeg on your PATH.");
                ui.add_space(12.0);
                if let Some(job) = &self.replays.job {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if job.video.is_some() { "Exporting video…" } else { "Watching replay…" });
                    });
                    if job.video.is_some() {
                        if let Some(progress) = log_tail(&job.log).lines().rev().find(|line| line.starts_with("video:"))
                        {
                            ui.label(progress);
                        }
                    }
                }
                ui.label(&self.replays.message);
                ui.add_space(10.0);
                ui.weak("Replays use the game files, profile and setup from when you drove.");
            });
        });
        if watch || export {
            self.start_replay(export);
        }
    }

    fn start_replay(&mut self, export: bool) {
        let Some(path) = self.replays.selected.clone() else { return };
        let video = export.then(|| PathBuf::from(self.replays.output.trim()));
        let result = (|| {
            if let Some(video) = &video {
                if video.exists() {
                    return Err("Choose a new filename; that video already exists".into());
                }
                if let Some(parent) = video.parent().filter(|p| !p.as_os_str().is_empty()) {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            let logs = self.logs_dir();
            fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let log = logs.join(format!("replay-{stamp}.log"));
            let stderr = File::create(&log).map_err(|e| e.to_string())?;
            let args = viewer_args(
                &path,
                self.replays.view,
                video.as_deref().map(|p| (p, self.replays.size, self.replays.fps)),
            );
            let mut command = Command::new(&self.paths.viewer);
            command.args(args).envs(self.child_env()).stdout(Stdio::null()).stderr(stderr);
            if self.paths.patch.is_none() {
                command.env_remove("RBR_PATCH");
            }
            let child = command.spawn().map_err(|e| format!("Cannot start replay: {e}"))?;
            Ok::<_, String>((child, log))
        })();
        match result {
            Ok((child, log)) => {
                self.child = Some(child);
                self.status = if export { "Exporting replay video" } else { "Watching replay" }.into();
                self.replays.message.clear();
                self.replays.job = Some(Job { video, log });
            }
            Err(e) => self.replays.message = e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsing_keeps_bad_files_visible_and_preserves_the_selection() {
        let home = std::env::temp_dir().join(format!("rerbr-replay-browser-{}", std::process::id()));
        let dir = home.join("replays");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("123-stage.rrp");
        let mut bytes =
            replay::header(&["track-54_E", "--title", "Stage · Car", "--live", "standalone"].map(str::to_owned));
        bytes.extend_from_slice(&realtime::LOG_MAGIC);
        bytes.extend_from_slice(&realtime::Logged::unchecked([0; rbr_recording::CONTROL_BYTES], 1).to_bytes());
        fs::write(&path, bytes).unwrap();
        fs::write(dir.join("broken.rrp"), b"broken").unwrap();
        fs::write(dir.join("unrelated.txt"), b"keep").unwrap();
        let mut browser = Replays::default();
        browser.refresh(&home);
        assert_eq!(browser.entries.len(), 2);
        let entry = browser.entries.iter().find(|e| e.path == path).unwrap();
        assert_eq!(entry.title, "Stage · Car");
        assert!(entry.problem.is_none());
        assert!(browser.entries.iter().any(|e| e.problem.is_some()));
        browser.select(Some(path.clone()), &home);
        browser.output = "my-edit.mp4".into();
        browser.refresh(&home);
        assert_eq!(browser.selected, Some(path));
        assert_eq!(browser.output, "my-edit.mp4");
        fs::remove_dir_all(home).unwrap();
    }
}
