//! Downloading the texture upscaler from the Graphics page ("Download upscaler…") into the player's folder
//! (`rbr_render::upscale::DIR`), where the viewer's `--upscale-textures` finds it: `realesrgan-ncnn-vulkan` (MIT) from
//! Real-ESRGAN's v0.2.5.0 release for this platform, and Real-ESRGAN's General WDN v3 model (BSD-3) in ncnn form from
//! Upscayl's custom models at a fixed commit. Each download is checked against its SHA-256 before anything is kept;
//! the files are written only once all have downloaded, and the cache beside them is left as it is.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};

use crate::gvrc;
use crate::job::{Counter, Job, Progress, Temp};
use crate::pages::{GOOD, WARN};
use crate::Form;
use rbr_formats::rbz;
use rbr_render::upscale::{self, BINARY, MODEL};

type Fail = String;

/// A release zip: its URL, SHA-256, and the files kept from it.
struct Release {
    url: &'static str,
    sha256: &'static str,
    files: &'static [&'static str],
}

const RELEASE_URL: &str = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/";

/// This platform's release, if Real-ESRGAN builds one for it (none for ARM Linux or Windows).
const RELEASE: Option<Release> = if cfg!(target_os = "macos") {
    // A universal binary, x86_64 and arm64.
    Some(Release {
        url: "realesrgan-ncnn-vulkan-20220424-macos.zip",
        sha256: "e0ad05580abfeb25f8d8fb55aaf7bedf552c375b5b4d9bd3c8d59764d2cc333a",
        files: &[BINARY],
    })
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some(Release {
        url: "realesrgan-ncnn-vulkan-20220424-ubuntu.zip",
        sha256: "e5aa6eb131234b87c0c51f82b89390f5e3e642b7b70f2b9bbe95b6a285a40c96",
        files: &[BINARY],
    })
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some(Release {
        url: "realesrgan-ncnn-vulkan-20220424-windows.zip",
        sha256: "abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d",
        files: &[BINARY, "vcomp140.dll"],
    })
} else {
    None
};

const MODEL_URL: &str =
    "https://raw.githubusercontent.com/upscayl/custom-models/500e6a0e8d60b1bf89c685391ef464f3b909b786/models/";

/// The model's files, by extension, with their SHA-256.
const MODEL_FILES: [(&str, &str); 2] = [
    ("param", "22174924330297357434ad21ed0af7f4b820008d2a502b492754d130d4142714"),
    ("bin", "74eae91a6230cbe5b931be6c49141faaff0034976933d4da9accb77c329c8a78"),
];

/// Whether this platform has an upscaler to download.
pub fn available() -> bool {
    RELEASE.is_some()
}

/// Checks the file `path` against `sha256`.
fn check(path: &Path, sha256: &str) -> Result<(), Fail> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let bytes = fs::read(path).map_err(|e| format!("{name}: {e}"))?;
    let found = format!("{:x}", Sha256::digest(&bytes));
    (found == sha256).then_some(()).ok_or_else(|| format!("{name} is not the expected file (SHA-256 {found})"))
}

/// Downloads the upscaler into the folder `dir`.
pub fn download(dir: &Path, report: &(dyn Fn(&Progress) + Sync), cancel: &AtomicBool) -> Result<(), Fail> {
    let release = RELEASE.ok_or("Real-ESRGAN has no upscaler build for this platform")?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let temp = Temp::create(dir, "downloading")?;
    let agent = gvrc::agent();
    let zip_url = format!("{RELEASE_URL}{}", release.url);
    let model_urls = MODEL_FILES.map(|(ext, _)| format!("{MODEL_URL}{MODEL}.{ext}"));
    let total = std::iter::once(&zip_url).chain(&model_urls).map(|url| gvrc::size_of(&agent, url)).sum();
    let mut count = Counter { step: "Downloading the upscaler", done: 0, total, report, cancel };
    count.add(0)?;

    let zip_path = temp.0.join(release.url);
    gvrc::fetch(&agent, &zip_url, &zip_path, &mut count)?;
    check(&zip_path, release.sha256)?;
    let mut zip = rbz::open(&zip_path).map_err(|e| format!("{}: {e}", release.url))?;
    let staged = temp.0.join("files");
    fs::create_dir_all(staged.join("models")).map_err(|e| e.to_string())?;
    for name in release.files {
        let mut bytes = Vec::new();
        rbz::by_name(&mut zip, name)
            .and_then(|mut file| Ok(file.read_to_end(&mut bytes)?))
            .map_err(|e| format!("{name} in {}: {e}", release.url))?;
        fs::write(staged.join(name), bytes).map_err(|e| format!("{name}: {e}"))?;
    }
    for (url, (ext, sha256)) in model_urls.iter().zip(MODEL_FILES) {
        let path = staged.join("models").join(format!("{MODEL}.{ext}"));
        gvrc::fetch(&agent, url, &path, &mut count)?;
        check(&path, sha256)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staged.join(BINARY), fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }

    report(&Progress { step: "Installing the upscaler", ..Progress::default() });
    fs::create_dir_all(dir.join("models")).map_err(|e| e.to_string())?;
    let models = MODEL_FILES.map(|(ext, _)| format!("models/{MODEL}.{ext}"));
    for name in release.files.iter().copied().chain(models.iter().map(String::as_str)) {
        fs::rename(staged.join(name), dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// The download's confirmation, job and result line.
#[derive(Default)]
pub struct Download {
    confirm: bool,
    job: Option<Job<()>>,
    status: Option<Result<String, String>>,
}

impl Form {
    /// The upscaler's folder in the player's folder.
    pub fn upscaler_dir(&self) -> std::path::PathBuf {
        self.here().join(upscale::DIR)
    }

    /// "Download upscaler…", its confirmation, progress and result line, while the upscaler is not there.
    pub fn upscaler_download_ui(&mut self, ui: &mut egui::Ui) {
        let dir = self.upscaler_dir();
        let download = &mut self.upscaler_download;
        if download.job.is_none() && !download.confirm && ui.button("Download upscaler…").clicked() {
            (download.confirm, download.status) = (true, None);
        }
        if download.confirm {
            ui.label(format!(
                "Download the texture upscaler (about 50 MB) into {}? It is Real-ESRGAN's realesrgan-ncnn-vulkan (MIT \
                 licence, github.com/xinntao/Real-ESRGAN) and its General WDN v3 model (BSD-3 licence), from \
                 github.com/upscayl/custom-models.",
                dir.display()
            ));
            ui.horizontal(|ui| {
                if ui.button("Download").clicked() {
                    download.confirm = false;
                    let to = dir.clone();
                    download.job = Some(Job::start(move |report, cancel| self::download(&to, report, cancel)));
                }
                if ui.button("Cancel").clicked() {
                    download.confirm = false;
                }
            });
        }
        if let Some(job) = &mut download.job {
            if let Some(result) = ui.horizontal_wrapped(|ui| job.ui(ui)).inner {
                download.job = None;
                download.status = Some(match result {
                    Err(e) if e.ends_with("cancelled") => Err("Cancelled; nothing was kept.".into()),
                    Err(e) => Err(e),
                    Ok(()) => Ok("The upscaler is installed.".into()),
                });
            }
        }
        match &download.status {
            Some(Ok(text)) => _ = ui.colored_label(GOOD, format!("✔ {text}")),
            Some(Err(text)) => _ = ui.colored_label(WARN, format!("⚠ {text}")),
            None => {}
        }
        if matches!(download.status, Some(Ok(_))) {
            self.upscaler = upscale::Upscaler::find(&dir).is_some();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Downloads the real upscaler (network, about 50 MB): `cargo test -p rbr-launcher upscaler -- --ignored`.
    #[test]
    #[ignore]
    fn the_download_is_found() {
        let dir = std::env::temp_dir().join(format!("rerbr-upscaler-{}", std::process::id()));
        download(&dir, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert!(upscale::Upscaler::find(&dir).is_some());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1 + RELEASE.unwrap().files.len(), "no temp folder left");
        fs::remove_dir_all(&dir).unwrap();
    }
}
