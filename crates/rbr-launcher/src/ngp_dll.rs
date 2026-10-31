//! The NGP DLL choice: follow the installed plugin until the player chooses an override.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rbr_formats::{cars, data::Data, ngp};

#[derive(Debug, PartialEq, Eq)]
struct File {
    path: PathBuf,
    stamp: Option<(u64, Option<SystemTime>)>,
}

#[derive(Default)]
pub struct Choice {
    /// Empty follows the installed plugin. Only an explicit choice is saved.
    pub path: String,
    checked: RefCell<Option<(File, Option<String>)>>,
}

impl Choice {
    pub fn new(path: String) -> Self {
        Self { path, ..Self::default() }
    }

    pub fn automatic(data: &Data, rsf: &str) -> Option<PathBuf> {
        (!rsf.trim().is_empty())
            .then(|| cars::resolve(Path::new(rsf.trim()), "Plugins/NGP.dll"))
            .flatten()
            .or_else(|| data.resolve("Plugins/NGP.dll"))
    }

    pub fn resolved(&self, data: &Data, rsf: &str) -> Option<PathBuf> {
        match self.path.trim() {
            "" => Self::automatic(data, rsf),
            path => Some(PathBuf::from(path)),
        }
    }

    /// Hash only when the selected file or its metadata changes, rather than on every UI frame.
    pub fn problem(&self, data: &Data, rsf: &str) -> Option<String> {
        let Some(path) = self.resolved(data, rsf) else {
            return Some("Choose NGP.dll on the Settings page; no installed plugin was found.".into());
        };
        let stamp = std::fs::metadata(&path).ok().map(|m| (m.len(), m.modified().ok()));
        let file = File { path, stamp };
        let mut checked = self.checked.borrow_mut();
        if let Some((_, problem)) = checked.as_ref().filter(|(old, _)| *old == file) {
            return problem.clone();
        }
        let problem = std::fs::read(&file.path)
            .map_err(|e| format!("{}: {e}", file.path.display()))
            .and_then(|bytes| ngp::validate(&bytes))
            .err();
        *checked = Some((file, problem.clone()));
        problem
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, data: &Data, rsf: &str, enabled: bool) {
        let automatic = Self::automatic(data, rsf);
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("NGP.dll");
                let hint = automatic.as_ref().map_or("Choose your NGP.dll".into(), |p| p.display().to_string());
                ui.add(egui::TextEdit::singleline(&mut self.path).hint_text(hint).desired_width(420.0));
                if ui.button("Browse…").clicked() {
                    if let Some(path) = crate::job::pick_file("Choose NGP 7.4.778's NGP.dll") {
                        self.path = path;
                    }
                }
                if !self.path.is_empty() && automatic.is_some() && ui.button("Use installed DLL").clicked() {
                    self.path.clear();
                }
            });
        });
        if let Some(problem) = self.problem(data, rsf) {
            ui.colored_label(crate::pages::WARN, problem);
        } else if self.path.trim().is_empty() {
            ui.label("Linked to your installed NGP.dll. The game folder is read only.");
        } else {
            ui.label("Using your selected NGP 7.4.778 DLL.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let root = crate::test_dir(&format!("dll-{name}"));
        std::fs::create_dir_all(root.join("Plugins")).unwrap();
        root
    }

    #[test]
    fn follows_rsf_and_keeps_an_explicit_choice() {
        let game = root("game");
        let rsf = root("rsf");
        std::fs::write(game.join("Plugins/NGP.dll"), b"game").unwrap();
        std::fs::write(rsf.join("Plugins/NGP.dll"), b"rsf").unwrap();
        let data = Data::new(&game, None);
        let mut choice = Choice::default();
        assert_eq!(choice.resolved(&data, rsf.to_str().unwrap()), Some(rsf.join("Plugins/NGP.dll")));
        choice.path = "my-plugin.dll".into();
        assert_eq!(choice.resolved(&data, ""), Some(PathBuf::from("my-plugin.dll")));
        choice.path.clear();
        assert_eq!(choice.resolved(&data, ""), Some(game.join("Plugins/NGP.dll")));
        std::fs::remove_dir_all(game).unwrap();
        std::fs::remove_dir_all(rsf).unwrap();
    }

    #[test]
    fn a_plugin_is_required_and_an_unsupported_one_blocks_start() {
        let game = root("missing");
        let data = Data::new(&game, None);
        let choice = Choice::default();
        assert!(choice.problem(&data, "").unwrap().contains("Choose NGP.dll"));
        std::fs::write(game.join("Plugins/NGP.dll"), b"another version").unwrap();
        assert!(choice.problem(&data, "").unwrap().contains("unsupported NGP.dll"));
        std::fs::remove_dir_all(game).unwrap();
    }

    #[test]
    fn notices_a_selected_file_being_replaced() {
        let data = Data::from_env().unwrap_or_else(|| Data::new("", None));
        let Some(source) = ngp::path(&data) else {
            return;
        };
        let game = root("replaced");
        let target = game.join("Plugins/NGP.dll");
        std::fs::copy(source, &target).unwrap();
        let choice = Choice::new(target.display().to_string());
        assert_eq!(choice.problem(&data, ""), None);
        std::fs::write(&target, b"unsupported").unwrap();
        assert!(choice.problem(&data, "").unwrap().contains("unsupported NGP.dll"));
        std::fs::remove_dir_all(game).unwrap();
    }
}
