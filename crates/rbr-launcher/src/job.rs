//! Work running on a thread with a progress line and Cancel: the disc setup, adding car packs, the co-driver
//! import, and the downloads (`Job`, its `Progress` and its `Counter`, in a `Temp` folder beside the work that is
//! removed however the work ends), and the platform's file dialog.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// A failed or cancelled job's message.
type Fail = String;

/// A job's progress report: the step and the bytes written of all.
pub type Report<'a> = &'a (dyn Fn(&Progress) + Sync);

/// What the work is doing: a step and the bytes written of all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub step: &'static str,
    pub done: u64,
    pub total: u64,
}

impl Progress {
    pub fn line(&self) -> String {
        match self.total {
            0 => format!("{}…", self.step),
            total => format!("{}… {} of {} MB", self.step, self.done >> 20, total >> 20),
        }
    }
}

/// Work running on its thread with a progress line and Cancel.
pub struct Job<T> {
    pub(crate) progress: Arc<Mutex<Progress>>,
    pub(crate) cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<T, Fail>>>,
}

impl<T: Send + 'static> Job<T> {
    /// Runs `work` with its progress report and its cancel flag.
    pub fn start(
        work: impl FnOnce(Report<'_>, &AtomicBool) -> Result<T, Fail> + Send + 'static,
    ) -> Job<T> {
        let progress = Arc::new(Mutex::new(Progress::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (shared, stop) = (progress.clone(), cancel.clone());
        let thread = std::thread::spawn(move || work(&|p| *shared.lock().unwrap() = p.clone(), &stop));
        Job { progress, cancel, thread: Some(thread) }
    }

    /// The work's result, once it has ended.
    fn result(&mut self) -> Option<Result<T, Fail>> {
        if !self.thread.as_ref()?.is_finished() {
            return None;
        }
        Some(self.thread.take()?.join().unwrap_or_else(|_| Err("it stopped unexpectedly".into())))
    }

    /// The spinner, the progress line and Cancel; the result once the work has ended.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Result<T, Fail>> {
        ui.spinner();
        ui.label(self.progress.lock().unwrap().line());
        if ui.button("Cancel").clicked() {
            self.cancel.store(true, Ordering::Relaxed);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(200));
        self.result()
    }
}

impl<T> Drop for Job<T> {
    /// Closing the window cancels the work and waits for it to remove its temp folder.
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A temp folder beside the work, removed however the work ends.
pub struct Temp(pub PathBuf);

impl Temp {
    /// Makes `parent/.<prefix>-<pid>-<nanos>`; removed on drop.
    pub fn create(parent: &Path, prefix: &str) -> Result<Temp, Fail> {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let temp = Temp(parent.join(format!(".{prefix}-{}-{stamp}", std::process::id())));
        fs::create_dir_all(&temp.0).map_err(|e| format!("{}: {e}", temp.0.display()))?;
        Ok(temp)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Counts the bytes written of all, reports them, and stops when cancelled.
pub struct Counter<'a> {
    pub step: &'static str,
    pub done: u64,
    pub total: u64,
    pub report: Report<'a>,
    pub cancel: &'a AtomicBool,
}

impl Counter<'_> {
    pub fn add(&mut self, n: u64) -> Result<(), Fail> {
        self.done += n;
        (self.report)(&Progress { step: self.step, done: self.done, total: self.total });
        match self.cancel.load(Ordering::Relaxed) {
            true => Err("cancelled".into()),
            false => Ok(()),
        }
    }
}

/// The platform's file dialog with `prompt` (`osascript` on macOS, PowerShell on Windows, `zenity` or `kdialog`
/// elsewhere); `None` when it is cancelled or there is none.
pub(crate) fn pick_file(prompt: &str) -> Option<String> {
    dialog(prompt, false)
}

/// The platform's folder dialog with `prompt`; `None` when it is cancelled or there is none.
pub fn pick_folder(prompt: &str) -> Option<String> {
    dialog(prompt, true)
}

/// The platform's dialog: a folder's when `folders`, else a file's.
fn dialog(prompt: &str, folders: bool) -> Option<String> {
    let run = |program: &str, args: &[&str]| {
        let out = rbr_render::console::no_window(&mut Command::new(program)).args(args).output().ok()?;
        let path = String::from_utf8(out.stdout).ok()?.trim().to_owned();
        (out.status.success() && !path.is_empty()).then_some(path)
    };
    if cfg!(target_os = "macos") {
        let choose = match folders {
            true => "choose folder",
            false => "choose file",
        };
        run("osascript", &["-e", &format!("POSIX path of ({choose} with prompt \"{prompt}\")")])
    } else if cfg!(windows) {
        let script = match folders {
            true => "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
                      if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath }",
            false => "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; \
                      if ($d.ShowDialog() -eq 'OK') { $d.FileName }",
        };
        run("powershell", &["-NoProfile", "-Command", script])
    } else if folders {
        run("zenity", &["--file-selection", "--directory", &format!("--title={prompt}")])
            .or_else(|| run("kdialog", &["--getexistingdirectory"]))
    } else {
        run("zenity", &["--file-selection", &format!("--title={prompt}")])
            .or_else(|| run("kdialog", &["--getopenfilename"]))
    }
}
