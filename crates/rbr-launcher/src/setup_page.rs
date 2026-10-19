//! The Setup page: the car's setup values grouped by area, within the car options' ranges and by the game's steps
//! (`rbr_gameplay::setup`), with named setups saved in the game's `Setups/*.lsp` format.
//!
//! Setups are saved in `Physics/<car>/Setups/` in the player's folder (the game's `%sSetups/` under its own folder) and
//! also listed from the install's `Physics/<car>/Setups/`, where the game saves them. Start writes the values to
//! `setup-current.lsp` in the player's folder and passes it to `rbr-standalone --setup`, unless they are the default.
//!
//! The game's three save slots per car (`CSetupSaveSlotButtonUI`) are the files `SavedGames/<car>slot<n>_<name>.lsp`
//! (`rbr_formats::saves::setup_slots`) in the player's folder, so the game reads a slot saved here and the other way
//! round. The championship offers the stage's default, the current setup or a slot before each stage
//! (`CSeasonChooseSetupUI`, [`SetupPage::choose`]).

use std::path::{Path, PathBuf};

use egui::{ComboBox, DragValue, Ui};
use rbr_formats::data::Data;
use rbr_formats::saves::setup_slots::SLOTS;
use rbr_formats::saves::SaveDir;
use rbr_gameplay::setup::{Area, CarSetup, Range, SeasonSetup, Setup, Slot, Spec, CAR_FOLDERS, MAP_POINTS, WHEELS};

use crate::catalog::Car;
use crate::pages::WARN;

/// The defaults and ranges of the NGP car pack at `path` on `surface`.
fn pack_setup(path: &Path, surface: &str) -> Result<CarSetup, String> {
    let pack = rbr_formats::rsf::Pack::open(path)?;
    let text = |name: String| pack.read(&name).map(|b| String::from_utf8_lossy(&b).into_owned());
    CarSetup::parse(&text(format!("setups/d_{surface}.lsp"))?, &text(format!("r_{surface}.lsp"))?)
}

/// The page's state for one car and surface.
pub struct SetupPage {
    /// The car (`[CarNN]`) and surface the setup is for, and the car's name.
    key: Option<(u32, &'static str)>,
    car_name: String,
    car: Result<CarSetup, String>,
    pub setup: Setup,
    /// The loaded or saved setup's name; empty for the default.
    pub name: String,
    area: Area,
    map: usize,
    status: String,
}

impl SetupPage {
    pub fn new(name: String) -> SetupPage {
        SetupPage {
            key: None,
            car_name: String::new(),
            car: Err(String::new()),
            setup: Setup::default(),
            name,
            area: Area::Tyres,
            map: 0,
            status: String::new(),
        }
    }

    /// Loads the car's defaults and ranges when the car or surface changes, then the named setup over them. An RSF
    /// car's come from its pack (`setups/d_<surface>.lsp`, `r_<surface>.lsp`), as the game reads them once RSF has
    /// copied the pack into the slot; its named setups and game slots are the slot's. A retail slot's come from NGP's
    /// `physics.rbz` (`archive`), which has the NGP car installed in it.
    pub fn select(&mut self, data: &Data, here: &Path, car: &Car, archive: &Path, surface: &'static str) {
        if self.key == Some((car.index, surface)) && self.car_name == car.name {
            return;
        }
        self.key = Some((car.index, surface));
        self.car_name = car.name.clone();
        self.car = match car.rsf.as_ref().map(|r| &r.pack) {
            Some(Ok(pack)) => pack_setup(pack, surface),
            Some(Err(e)) => Err(e.clone()),
            None => CarSetup::load_in(archive, car.index as usize, surface),
        };
        self.setup = self.car.as_ref().map_or_else(|_| Setup::default(), |c| c.default);
        if !self.name.is_empty() {
            let name = std::mem::take(&mut self.name);
            self.load(data, here, &name);
        }
    }

    /// Shows `area` (`--screenshot`).
    pub fn show_area(&mut self, area: Area) {
        self.area = area;
    }

    /// The setup file to start with: the current values, unless they are the default (none then).
    pub fn current_file(&self, here: &Path) -> Result<Option<PathBuf>, String> {
        let car = self.car.as_ref().map_err(|e| e.clone())?;
        if self.setup == car.default {
            return Ok(None);
        }
        let path = here.join("setup-current.lsp");
        std::fs::write(&path, self.setup.to_text()).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Some(path))
    }

    /// The setup's text as a setup file holds it (the championship's save keeps it, `PTSC`).
    pub fn current_text(&self) -> String {
        self.setup.to_text()
    }

    /// Puts the setup of `CSeasonChooseSetupUI`'s row `choice` on the page.
    pub fn choose(&mut self, here: &Path, choice: SeasonSetup) -> Result<(), String> {
        let (Ok(car), Some((index, _))) = (&self.car, self.key) else {
            return Err("no setup for this car".into());
        };
        let slot = match choice {
            SeasonSetup::Slot(n) => SaveDir::new(here).load_setup_slot(index, n).map_err(|e| e.to_string())?,
            _ => None,
        };
        self.setup = choice.setup(&car.default, &self.setup, slot.as_ref().map(|(_, t)| t.as_str()))?;
        match (choice, slot) {
            (SeasonSetup::Default, _) => self.name.clear(),
            (_, Some((name, _))) => self.name = name,
            _ => {}
        }
        Ok(())
    }

    /// Puts a setup file's text over the default on the page, unnamed (a season's saved setup).
    pub fn load_text(&mut self, text: &str) -> Result<(), String> {
        let car = self.car.as_ref().map_err(|e| e.clone())?;
        self.setup = Setup::from_text(&car.default, text)?;
        self.name.clear();
        Ok(())
    }

    /// The car's game slots: each one's name, `None` when empty.
    pub fn slots(&self, here: &Path) -> Vec<Option<String>> {
        let empty = vec![None; SLOTS as usize];
        self.key.map_or(empty.clone(), |(car, _)| SaveDir::new(here).setup_slots(car).unwrap_or(empty))
    }

    /// The game's save slots: load one, or save the current values into one under the name field's name.
    fn slots_ui(&mut self, ui: &mut Ui, here: &Path) {
        let Some((car, _)) = self.key else {
            return;
        };
        let slots = self.slots(here);
        ui.horizontal(|ui| {
            ui.label("Game slots");
            for (n, slot) in slots.iter().enumerate() {
                let n = n as u32;
                ui.separator();
                ui.label(format!("{}: {}", n + 1, slot.as_deref().unwrap_or("empty")));
                if slot.is_some() && ui.small_button("Load").clicked() {
                    self.status = match self.choose(here, SeasonSetup::Slot(n)) {
                        Ok(()) => format!("loaded slot {}", n + 1),
                        Err(e) => e,
                    };
                }
                if ui.small_button("Save").on_hover_text("Save the values here under the name field's name").clicked() {
                    self.status = match valid_name(&self.name) {
                        Ok(name) => match SaveDir::new(here).save_setup_slot(car, n, name, &self.setup.to_text()) {
                            Ok(()) => format!("saved {name} to slot {}", n + 1),
                            Err(e) => format!("slot {}: {e}", n + 1),
                        },
                        Err(e) => e,
                    };
                }
            }
        });
    }

    fn folder(&self) -> Option<&'static str> {
        self.key.and_then(|(car, _)| CAR_FOLDERS.get(car as usize).copied())
    }

    /// The saved setups' names: the launcher's and the install's `Setups/*.lsp`.
    fn saved(&self, data: &Data, here: &Path) -> Vec<String> {
        let Some(folder) = self.folder() else {
            return Vec::new();
        };
        let mut names: Vec<String> = [here.join(folder), data.install().join(folder)]
            .iter()
            .filter_map(|dir| std::fs::read_dir(dir.join("Setups")).ok())
            .flatten()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.to_ascii_lowercase().ends_with(".lsp"))
            .map(|n| n[..n.len() - 4].to_owned())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn load(&mut self, data: &Data, here: &Path, name: &str) {
        let (Ok(car), Some(folder)) = (&self.car, self.folder()) else {
            return;
        };
        let file = format!("{name}.lsp");
        let path = [here.join(folder), data.install().join(folder)]
            .into_iter()
            .map(|dir| dir.join("Setups").join(&file))
            .find(|p| p.is_file());
        let result = path
            .ok_or_else(|| format!("{file}: not found"))
            .and_then(|p| std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display())))
            .and_then(|text| Setup::from_text(&car.default, &text));
        match result {
            Ok(setup) => {
                self.setup = setup;
                self.name = name.to_owned();
                self.status = format!("loaded {name}");
            }
            Err(e) => self.status = e,
        }
    }

    fn save(&mut self, here: &Path) {
        let Some(folder) = self.folder() else {
            return;
        };
        let name = match valid_name(&self.name) {
            Ok(name) => name,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let dir = here.join(folder).join("Setups");
        let path = dir.join(format!("{name}.lsp"));
        self.status = match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, self.setup.to_text())) {
            Ok(()) => format!("saved {}", path.display()),
            Err(e) => format!("{}: {e}", path.display()),
        };
    }

    pub fn ui(&mut self, ui: &mut Ui, data: &Data, here: &Path) {
        let car = match &self.car {
            Ok(car) => car.clone(),
            Err(e) => {
                ui.colored_label(WARN, format!("no setup for this car: {e}"));
                return;
            }
        };
        let surface = self.key.map_or("", |(_, s)| s);
        let name = if self.name.is_empty() { "default setup" } else { &self.name };
        let modified = if self.setup == car.default { "" } else { " (changed from the default)" };
        ui.label(format!("{surface} · {name}{modified}"));
        ui.horizontal(|ui| {
            let saved = self.saved(data, here);
            let mut pick = None;
            ComboBox::from_id_salt("setup-load").width(180.0).selected_text("Load…").show_ui(ui, |ui| {
                for name in &saved {
                    if ui.selectable_label(*name == self.name, name).clicked() {
                        pick = Some(name.clone());
                    }
                }
            });
            if let Some(name) = pick {
                self.load(data, here, &name);
            }
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(160.0).hint_text("setup name"));
            if ui.button("Save").clicked() {
                self.save(here);
            }
            if ui.button("Reset to default").clicked() {
                self.setup = car.default;
                self.name.clear();
                self.status = "default values".into();
            }
        });
        self.slots_ui(ui, here);
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(190.0);
                for area in Area::ALL {
                    ui.selectable_value(&mut self.area, area, area.name());
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| ui.vertical(|ui| self.area_ui(ui, &car)));
        });
    }

    fn area_ui(&mut self, ui: &mut Ui, car: &CarSetup) {
        let area = self.area;
        let mut specs: Vec<(Spec, Range)> = Setup::specs().into_iter().zip(car.ranges.iter().copied()).collect();
        specs.retain(|(spec, _)| spec.area == area);
        match area {
            Area::Tyres | Area::Springs | Area::Dampers | Area::Geometry => self.wheels_ui(ui, &specs),
            Area::Differentials => self.differentials_ui(ui, &specs),
            Area::Gears => self.gears_ui(ui, car, &specs),
            Area::RollBars | Area::Brakes => {
                self.rows_ui(ui, &specs);
                if area == Area::Brakes {
                    let (f, r) = (self.setup.front_brake_pressure, self.setup.rear_brake_pressure);
                    ui.label(format!("Brake balance: {:.0}% front", 100.0 * f / (f + r).max(1.0)));
                }
            }
        }
    }

    /// One slider per value, with its range.
    fn rows_ui(&mut self, ui: &mut Ui, specs: &[(Spec, Range)]) {
        egui::Grid::new("setup-rows").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            for (spec, range) in specs.iter().filter(|(s, _)| s.index.is_none()) {
                ui.label(spec.label).on_hover_text(range_text(spec, range));
                self.with_slot(spec, |slot| value_ui(ui, spec, *range, slot, Widget::Slider));
                ui.end_row();
            }
        });
    }

    /// The per-wheel values: one row per value, one column per wheel.
    fn wheels_ui(&mut self, ui: &mut Ui, specs: &[(Spec, Range)]) {
        egui::Grid::new("setup-wheels").num_columns(5).spacing([12.0, 8.0]).striped(true).show(ui, |ui| {
            ui.label("");
            for wheel in ["Left front", "Right front", "Left rear", "Right rear"] {
                ui.strong(wheel);
            }
            ui.end_row();
            let mut labels: Vec<&'static str> = Vec::new();
            for (spec, _) in specs {
                if !labels.contains(&spec.label) {
                    labels.push(spec.label);
                }
            }
            for label in labels {
                let first = specs.iter().find(|(s, _)| s.label == label).map(|(s, r)| range_text(s, r));
                ui.label(label).on_hover_text(first.unwrap_or_default());
                for w in 0..WHEELS.len() {
                    if let Some((spec, range)) = specs.iter().find(|(s, _)| s.label == label && s.index == Some(w)) {
                        self.with_slot(spec, |slot| value_ui(ui, spec, *range, slot, Widget::Field));
                    }
                }
                ui.end_row();
            }
        });
    }

    fn differentials_ui(&mut self, ui: &mut Ui, specs: &[(Spec, Range)]) {
        self.rows_ui(ui, specs);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.strong("Diff map");
            for (m, name) in ["Centre", "Centre (left-foot brake)", "Front", "Rear"].into_iter().enumerate() {
                ui.selectable_value(&mut self.map, m, name);
            }
        });
        let map: Vec<&(Spec, Range)> = specs.iter().filter(|(s, _)| s.index == Some(self.map)).collect();
        egui::Grid::new("setup-map").num_columns(MAP_POINTS + 1).spacing([6.0, 6.0]).show(ui, |ui| {
            ui.label("point");
            for n in 0..MAP_POINTS {
                ui.label(format!("{}", n * 10));
            }
            ui.end_row();
            for (row, suffix) in
                map.chunks(MAP_POINTS).zip(["throttle lock", "brake lock", "speed", "speed lock factor"])
            {
                let unit = row[0].0.unit.suffix;
                ui.label(format!("{suffix} ({unit})")).on_hover_text(range_text(&row[0].0, &row[0].1));
                for (spec, range) in row.iter().map(|p| (&p.0, p.1)) {
                    self.with_slot(spec, |slot| value_ui(ui, spec, range, slot, Widget::Cell));
                }
                ui.end_row();
            }
        });
    }

    fn gears_ui(&mut self, ui: &mut Ui, car: &CarSetup, specs: &[(Spec, Range)]) {
        let Some((_, range)) = specs.first() else {
            return;
        };
        let ratio =
            |id: u32| car.gears.iter().find(|g| g.0 == id).map_or(format!("{id}"), |&(_, p, g)| format!("{p}:{g}"));
        ui.horizontal(|ui| {
            ui.label("Drop gear").on_hover_text("the car options' drop gear rows; the gearbox ratios are fixed");
            let gear = &mut self.setup.drop_gear;
            ui.add_enabled_ui(range.max > range.min, |ui| {
                ComboBox::from_id_salt("drop-gear").selected_text(ratio(*gear)).show_ui(ui, |ui| {
                    for id in range.min as u32..=range.max as u32 {
                        ui.selectable_value(gear, id, ratio(id));
                    }
                });
            });
        });
    }

    /// Calls `f` with the setup's slot of `spec`.
    fn with_slot(&mut self, spec: &Spec, f: impl FnOnce(Slot<'_>)) {
        let mut f = Some(f);
        self.setup.visit(&mut |s, slot| {
            if s.group == spec.group && s.key == spec.key {
                if let Some(f) = f.take() {
                    f(slot);
                }
            }
        });
    }
}

/// A setup name the page saves under: trimmed, 1 to 27 characters, no slashes.
fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name.len() > 27 {
        return Err("a name of 1 to 27 characters, no slashes".into());
    }
    Ok(name)
}

fn range_text(spec: &Spec, range: &Range) -> String {
    let unit = spec.unit;
    let wrap = if range.wrap { ", wraps" } else { "" };
    format!("{} to {}, step {}{wrap}", unit.show(range.min), unit.show(range.max), unit.show(range.step))
}

/// How a value is edited.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Widget {
    Slider,
    Field,
    /// A field without the unit or the step buttons (the diff map's table).
    Cell,
}

/// A value's widget: − and + take the game's steps; the field or slider takes the steps toward what is set.
fn value_ui(ui: &mut Ui, spec: &Spec, range: Range, slot: Slot<'_>, widget: Widget) {
    let Slot::Real(value) = slot else {
        return;
    };
    let unit = spec.unit;
    let fixed = range.max <= range.min || range.step <= 0.0;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!fixed, |ui| {
            if widget != Widget::Cell && ui.small_button("−").clicked() {
                *value = range.down(*value);
            }
            let mut shown = *value * unit.scale;
            let bounds = range.min * unit.scale..=range.max * unit.scale;
            let suffix = if widget == Widget::Cell { String::new() } else { format!(" {}", unit.suffix) };
            let response = if widget == Widget::Slider {
                ui.add(
                    egui::Slider::new(&mut shown, bounds)
                        .fixed_decimals(unit.decimals)
                        .suffix(suffix)
                        .clamping(egui::SliderClamping::Never),
                )
            } else {
                ui.add(
                    DragValue::new(&mut shown)
                        .range(bounds)
                        .speed(range.step * unit.scale)
                        .fixed_decimals(unit.decimals)
                        .suffix(suffix),
                )
            };
            if response.changed() {
                *value = range.toward(*value, shown / unit.scale);
            }
            if widget != Widget::Cell && ui.small_button("+").clicked() {
                *value = range.up(*value);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every NGP car pack of the RSF install (`RBR_RSF`) with plain setup files gives the page its defaults and ranges
    /// on each surface.
    #[test]
    fn every_rsf_pack_has_its_setup() {
        let Some(rsf) = std::env::var_os("RBR_RSF") else { return };
        let packs = std::fs::read_dir(Path::new(&rsf).join("rsfdata/cars")).unwrap();
        let mut checked = 0;
        for pack in packs.filter_map(|e| Some(e.ok()?.path())) {
            for surface in ["gravel", "tarmac", "snow"] {
                match pack_setup(&pack, surface) {
                    Ok(car) => assert!(!car.ranges.is_empty(), "{}: {surface}: no ranges", pack.display()),
                    Err(e) if e.contains("RSF-encrypted") => continue,
                    Err(e) => panic!("{}: {surface}: {e}", pack.display()),
                }
                checked += 1;
            }
        }
        eprintln!("{checked} pack setups");
        assert!(checked > 0);
    }
}
