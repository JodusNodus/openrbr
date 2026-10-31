//! The player's saved drives: browse headers, watch a drive, or export it through the viewer.

use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::RichText;
use rbr_recording::{realtime, replay, splits::Splits};

use crate::settings::{Clouds, Precipitation};

use crate::pages::{combo, section, Choice, WARN};
use crate::Form;

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    modified: SystemTime,
    title: String,
    conditions: String,
    start_time: Option<String>,
    result: Option<Splits>,
    problem: Option<String>,
}

impl Entry {
    fn read(path: PathBuf) -> Self {
        let modified =
            recorded_time(&path).unwrap_or_else(|| path.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH));
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
        let (conditions, start_time) = header.as_ref().map(|args| conditions(args)).unwrap_or_default();
        let result = header
            .as_ref()
            .ok()
            .and_then(|_| replay::read_result(&path).ok())
            .filter(|r| r.race_time.is_finite() && r.race_time >= 0.0);
        Self { path, modified, title: title.unwrap_or(fallback), conditions, start_time, result, problem: header.err() }
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

/// Launcher replay names use epoch nanoseconds, or seconds for older files. Copying a drive preserves this date.
fn recorded_time(path: &Path) -> Option<SystemTime> {
    let name = path.file_stem()?.to_str()?;
    let stamp = name.split_once('-')?.0.parse::<u64>().ok()?;
    UNIX_EPOCH.checked_add(if stamp >= 1_000_000_000_000 {
        Duration::from_nanos(stamp)
    } else {
        Duration::from_secs(stamp)
    })
}

fn recorded_at(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{} · {:02}:{:02} UTC", age(time), seconds / 3600 % 24, seconds / 60 % 60)
}

fn result_text(result: Option<&Splits>) -> String {
    let Some(result) = result else { return "Time unavailable".into() };
    let time = rbr_render::hud::clock_label(f64::from(result.race_time));
    if result.retired {
        format!("DNF at {time}")
    } else if result.finished {
        time
    } else {
        format!("Stopped at {time}")
    }
}

/// Conditions come from the saved command, with the viewer's overrides kept apart from the physics options.
fn conditions(args: &[String]) -> (String, Option<String>) {
    let at = args.iter().position(|a| a == "--live").unwrap_or(args.len());
    let viewer = &args[..at];
    let command = args.get(at + 1..).unwrap_or_default();
    let value =
        |args: &[String], flag: &str| args.windows(2).rev().find(|pair| pair[0] == flag).map(|pair| pair[1].clone());
    let mut parts = Vec::new();
    if let Some(road) = value(viewer, "--weather").or_else(|| value(command, "--wetness")) {
        parts.push(match road.as_str() {
            "dry" => "Dry road".into(),
            "damp" => "Damp road".into(),
            "wet" => "Wet road".into(),
            _ => road,
        });
    }
    if let Some(clouds) = value(viewer, "--clouds").and_then(|v| Clouds::parse(&v)) {
        parts.push(clouds.label().into());
    }
    if let Some(rain) = value(viewer, "--precipitation").and_then(|v| Precipitation::parse(&v)) {
        parts.push(if rain == Precipitation::None {
            "No precipitation".into()
        } else {
            format!("{} precipitation", rain.label())
        });
    }
    if let Some(fog) = value(viewer, "--fog").filter(|v| v != "off") {
        parts.push(format!("{fog} fog"));
    }
    let start = value(viewer, "--clock").or_else(|| value(command, "--start-time"));
    (if parts.is_empty() { "Stage defaults".into() } else { parts.join(" · ") }, start)
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
                        && !entry.conditions.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    matches += 1;
                    let selected = self.replays.selected.as_ref() == Some(&entry.path);
                    let start = entry.start_time.as_ref().map(|t| format!(" · {t} start")).unwrap_or_default();
                    let label = format!(
                        "{}\n{} · {}\n{}{start}",
                        entry.title,
                        result_text(entry.result.as_ref()),
                        entry.conditions,
                        recorded_at(entry.modified)
                    );
                    if ui
                        .add(
                            egui::Button::selectable(selected, label)
                                .wrap()
                                .min_size(egui::vec2(ui.available_width(), 78.0)),
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
                ui.add_space(8.0);
                egui::Grid::new("replay-summary").num_columns(2).show(ui, |ui| {
                    for (label, value) in [
                        ("Stage time", result_text(entry.result.as_ref())),
                        ("Conditions", entry.conditions.clone()),
                        ("Recorded", recorded_at(entry.modified)),
                        ("Stage start", entry.start_time.clone().unwrap_or_else(|| "Stage default".into())),
                    ] {
                        ui.weak(label);
                        ui.label(value);
                        ui.end_row();
                    }
                    if let Some(result) = entry.result.as_ref().filter(|r| r.penalties > 0.0 && r.penalties.is_finite())
                    {
                        ui.weak("Penalties");
                        ui.label(format!("{} included", rbr_render::hud::clock_label(f64::from(result.penalties))));
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Camera");
                    combo(ui, "replay-view", None, &mut self.replays.view, &["cockpit", "bonnet", "chase"], |view| {
                        view
                    });
                });
                let ready = self.child.is_none() && entry.problem.is_none() && self.readiness().is_none();
                watch = ui.add_enabled(ready, egui::Button::new("Watch replay")).clicked();
                ui.weak("Space pauses · R restarts · Esc closes");
                section(ui, "Export to video");
                ui.horizontal(|ui| {
                    combo(
                        ui,
                        "replay-size",
                        None,
                        &mut self.replays.size,
                        &[(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)],
                        |(_, height)| format!("{height}p"),
                    );
                    combo(ui, "replay-fps", None, &mut self.replays.fps, &[30, 60], |fps| {
                        format!("{fps} fps")
                    });
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
            let mut command = self.viewer_command(&args);
            command.stdout(Stdio::null()).stderr(stderr);
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
    fn saved_conditions_distinguish_viewer_weather_from_physics_options() {
        let args = [
            "--weather",
            "wet",
            "--clouds",
            "partly",
            "--precipitation",
            "heavy",
            "--clock",
            "16:00",
            "--live",
            "standalone",
            "--weather",
            "0",
            "--start-time",
            "08:00",
        ]
        .map(str::to_owned);
        assert_eq!(conditions(&args), ("Wet road · Partly cloudy · Heavy precipitation".into(), Some("16:00".into())));
        assert_eq!(conditions(&[]), ("Stage defaults".into(), None));
        let args = ["--live", "standalone", "--wetness", "damp", "--start-time", "08:00"].map(str::to_owned);
        assert_eq!(conditions(&args), ("Damp road".into(), Some("08:00".into())));
    }

    #[test]
    fn stage_times_keep_the_finish_retirement_and_partial_drive_distinct() {
        let mut result = Splits { finished: true, race_time: 389.123, penalties: 20.0, ..Default::default() };
        assert_eq!(result_text(Some(&result)), "06:29.123");
        result.retired = true;
        assert_eq!(result_text(Some(&result)), "DNF at 06:29.123");
        result.finished = false;
        result.retired = false;
        assert_eq!(result_text(Some(&result)), "Stopped at 06:29.123");
        assert_eq!(result_text(None), "Time unavailable");
        assert_eq!(
            recorded_time(Path::new("1793268612-stage.rrp")),
            recorded_time(Path::new("1793268612000000000-stage.rrp"))
        );
        assert!(recorded_time(Path::new("my-drive.rrp")).is_none());
    }

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
        let result = Splits { finished: true, race_time: 389.123, ..Default::default() };
        replay::save_result(&path, &result).unwrap();
        replay::save_result(&dir.join("broken.rrp"), &result).unwrap();
        fs::write(dir.join("unrelated.txt"), b"keep").unwrap();
        let mut browser = Replays::default();
        browser.refresh(&home);
        assert_eq!(browser.entries.len(), 2);
        let entry = browser.entries.iter().find(|e| e.path == path).unwrap();
        assert_eq!(entry.title, "Stage · Car");
        assert_eq!(entry.result, Some(result));
        assert!(browser.entries.iter().find(|e| e.problem.is_some()).unwrap().result.is_none());
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
