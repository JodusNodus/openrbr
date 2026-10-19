//! The Drive page: the quick race's car and stage, conditions, race options, the previews and Start.

use std::path::Path;

use egui::{Color32, ComboBox, Key, RichText};
use rbr_formats::rsf::ghosts::Kind as GhostKind;
use rbr_gameplay::race_options::{DamageLevel, RaceOptions};
use rbr_gameplay::weather::{self, Preset, Weather};
use rbr_render::glass::model::Switch;

use crate::catalog::{self, TyreChoice};
use crate::ghosts::{self, GhostChoice};
use crate::pages::{fields, section, ACCENT, DRIVE_FIELDS, FIELD, WARN};
use crate::settings::{clock_text, or_folder};
use crate::{picker, Form};

/// The start times the Drive page offers (`viewer --clock`), minutes past midnight: every half hour from 05:00 to 22:30.
/// None is the stage's own light, as the game draws it.
pub fn start_times() -> impl Iterator<Item = u32> {
    (10..46).map(|half_hours| half_hours * 30)
}

/// A start time on map `map` as the Drive page shows it: the time and where the sun is on that rally's date
/// (`rbr_render::relight::solar`).
pub fn start_time_label(map: u32, start: Option<u32>) -> String {
    let Some(minutes) = start else { return "The stage's own light".into() };
    let (latitude, day) = rbr_render::relight::rally_of(map);
    let hour = minutes as f32 / 60.0;
    let elevation = rbr_render::relight::solar(latitude, day, hour).0;
    let sun = match elevation {
        e if e < -6.0 => "night".to_owned(),
        e if e < 0.0 => (if hour < 12.0 { "dawn" } else { "dusk" }).to_owned(),
        e => format!("sun {e:.0}° up"),
    };
    format!("{} — {sun}", clock_text(minutes))
}

impl Form {
    /// The quick race: the choices on the left, the previews, the drive's summary and Start on the right. `unready`:
    /// the status line says what the install lacks.
    pub fn drive_page(&mut self, ui: &mut egui::Ui, unready: bool) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(DRIVE_FIELDS);
                egui::ScrollArea::vertical().id_salt("drive").show(ui, |ui| self.selectors(ui));
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                // The lists' highlighted entries, while open.
                let map = self.map_picker.previewed(ui.ctx(), self.map);
                let car = self.car_picker.previewed(ui.ctx(), self.car);
                let rsf = or_folder(Some(&self.rsf_scanned), &self.folder.rsf_root);
                if let Some(car) = self.catalog.cars.get(car) {
                    self.previews.ui(ui, &self.catalog.maps[map], car, &rsf);
                }
                ui.add_space(12.0);
                self.start_area(ui, unready);
            });
        });
    }

    /// The drive's summary, Start (or Stop) and what blocks it.
    fn start_area(&mut self, ui: &mut egui::Ui, unready: bool) {
        match self.selection() {
            Ok((stage, weather)) => {
                let wear = catalog::WEARS[weather.wear.min(2) as usize];
                let name =
                    Path::new(&stage.name).file_name().map_or(stage.name.clone(), |n| n.to_string_lossy().into());
                ui.label(format!(
                    "{name} · {} · {} · {wear} wear",
                    weather::name(self.options.weather),
                    weather.mat_words().1
                ))
                .on_hover_text(&stage.name);
            }
            Err(reason) => {
                ui.colored_label(WARN, reason);
            }
        }
        ui.add_space(8.0);
        let blocker = self.blocker();
        ui.horizontal(|ui| {
            let text = RichText::new("Start").size(20.0).strong().color(Color32::WHITE);
            let button = egui::Button::new(text).fill(ACCENT).min_size(egui::vec2(180.0, 44.0));
            let start = ui.add_enabled(blocker.is_none(), button).on_hover_text("Enter starts too");
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if blocker.is_none() && (start.clicked() || enter) {
                self.start();
            }
            if let Some(child) = &mut self.child {
                if ui
                    .add(egui::Button::new(RichText::new("Stop").size(16.0)).min_size(egui::vec2(90.0, 44.0)))
                    .clicked()
                {
                    // Our own child: the viewer, which ends its stepping process with it.
                    let _ = child.kill();
                }
            }
        });
        // The install's problems and the last event (`status`) are on the status line.
        match blocker {
            Some(issue) if self.child.is_none() && !unready => {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(WARN, issue.text);
                    if let Some(fix) = issue.fix {
                        if ui.button(fix.label()).clicked() {
                            self.apply(fix);
                        }
                    }
                });
            }
            Some(_) if unready && self.child.is_none() => {
                ui.colored_label(Color32::GRAY, "The status line below says what is missing.");
            }
            _ => {}
        }
    }

    fn selectors(&mut self, ui: &mut egui::Ui) {
        section(ui, "Car and stage");
        fields(ui, "car and stage", |ui| {
            ui.label("Car");
            let (cars, maps) = self.recent();
            if self.car_picker.ui(ui, &mut self.car, &cars) {
                picker::remember(&mut self.recent_cars, self.catalog.cars[self.car].name.clone());
            }
            ui.end_row();

            ui.label("Stage");
            if self.map_picker.ui(ui, &mut self.map, &maps) {
                picker::remember(&mut self.recent_maps, self.catalog.maps[self.map].number);
            }
            ui.end_row();
        });

        section(ui, "Conditions");
        fields(ui, "conditions", |ui| {
            let tip = "The game's presets: good, bad, or random as the game draws it from the stage's season";
            ui.label("Weather").on_hover_text(tip);
            if let Some((_, weather)) = self.ghost_weather() {
                // A saved ghost brings its own weather (`E18_ConnGhostMemory::Dispatch`).
                ui.label(format!("from the ghost: {}", weather_label(&weather)));
            } else {
                let weather = &mut self.options.weather;
                ComboBox::from_id_salt("weather").width(FIELD).selected_text(weather::name(*weather)).show_ui(
                    ui,
                    |ui| {
                        for preset in [Preset::Good, Preset::Random, Preset::Bad] {
                            ui.selectable_value(weather, preset, weather::name(preset));
                        }
                    },
                );
            }
            ui.end_row();

            let tip = "Draws the stage at this local time on its rally's date: the sun, its light and shadows, and \
                       dusk and night lit by the car's lamps. The stage's own light is the game's";
            ui.label("Time of day").on_hover_text(tip);
            let map = self.catalog.maps[self.map].number;
            ComboBox::from_id_salt("time of day")
                .width(FIELD)
                .selected_text(start_time_label(map, self.start_time))
                .show_ui(ui, |ui| {
                    for start in std::iter::once(None).chain(start_times().map(Some)) {
                        ui.selectable_value(&mut self.start_time, start, start_time_label(map, start));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            ui.label("Tyre wear").on_hover_text("How worn the tyres are at the start");
            if self.options.weather == Preset::Random {
                // The random weather draws the wear.
                ui.label("drawn with the weather");
            } else {
                ComboBox::from_id_salt("wear")
                    .width(FIELD)
                    .selected_text(catalog::WEARS[self.wear.min(2) as usize])
                    .show_ui(ui, |ui| {
                        for (wear, name) in catalog::WEARS.into_iter().enumerate() {
                            ui.selectable_value(&mut self.wear, wear as u32, name);
                        }
                    });
            }
            ui.end_row();

            let default_tyre = self.selection().ok().and_then(|(_, weather)| self.tyre_id(&weather).ok());
            let tyre_label = match self.tyre {
                TyreChoice::Auto => default_tyre
                    .map_or_else(|| "Auto".to_owned(), |tyre| format!("Auto — {}", catalog::tyre_label(tyre))),
                TyreChoice::Fixed(tyre) => catalog::tyre_label(tyre).to_owned(),
            };
            ui.label("Tyre").on_hover_text("Auto takes the game's tyre for the surface and the weather");
            ComboBox::from_id_salt("tyre").width(FIELD).selected_text(tyre_label).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.tyre, TyreChoice::Auto, "Auto (game default)");
                for tyre in 0..=6 {
                    ui.selectable_value(&mut self.tyre, TyreChoice::Fixed(tyre), catalog::tyre_label(tyre));
                }
            });
            ui.end_row();

            let tip =
                "The wiper switch at the start; auto follows the rain and the mud on the glass. I steps it while \
                       driving";
            ui.label("Wipers").on_hover_text(tip);
            ComboBox::from_id_salt("wipers").width(FIELD).selected_text(self.wipers.name()).show_ui(ui, |ui| {
                for switch in Switch::ALL {
                    ui.selectable_value(&mut self.wipers, switch, switch.name());
                }
            });
            ui.end_row();
        });

        section(ui, "Race");
        fields(ui, "race", |ui| {
            self.ghost_row(ui);
            ui.end_row();

            let penalty = self.options.assist_settings().penalty();
            let tip = format!("Off, Safe, Reduced or Realistic. Calling for help costs {penalty} s at this level");
            ui.label("Car damage").on_hover_text(&tip);
            ui.horizontal(|ui| {
                let damage = &mut self.options.damage;
                ComboBox::from_id_salt("damage")
                    .width(160.0)
                    .selected_text(damage_label(*damage))
                    .show_ui(ui, |ui| {
                        for level in DamageLevel::ALL {
                            ui.selectable_value(damage, level, damage_label(level));
                        }
                    })
                    .response
                    .on_hover_text(&tip);
                ui.colored_label(Color32::GRAY, format!("help costs {penalty} s"));
            });
            ui.end_row();

            ui.label("Driver aids");
            let advanced = self.bindings.shifter.advanced;
            let aids = &mut self.options.aids;
            egui::Grid::new("aids").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                // The game's English labels (`bstrings.eng`).
                let clutch = if advanced { "Clutch Help: off with the advanced H-shifter" } else { "Clutch Help" };
                let rows = [
                    ("Automatic gearbox", &mut aids.auto_gears, true, "Shifts up and down for you"),
                    ("Gear Protection", &mut aids.gear_guard, true, "Stops a shift into a gear that would over-rev"),
                    ("Neutral Lock", &mut aids.neutral_lock, true, "Keeps the gearbox from slipping into neutral"),
                    (clutch, &mut aids.clutch_help, !advanced, "Works the clutch for you when you shift"),
                ];
                for (n, (label, value, enabled, tip)) in rows.into_iter().enumerate() {
                    ui.add_enabled(enabled, egui::Checkbox::new(value, label)).on_hover_text(tip);
                    if n % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.end_row();

            ui.label("");
            if ui.small_button("Game defaults").on_hover_text("Car damage Safe and the game's driver aids").clicked() {
                self.options = RaceOptions { weather: self.options.weather, ..RaceOptions::default() };
            }
            ui.end_row();
        });
    }

    /// The Ghost row: RSF's ghosts in an RSF race, else the best one and the saved ones of the map.
    fn ghost_row(&mut self, ui: &mut egui::Ui) {
        let map = self.catalog.maps[self.map].number;
        let records = self.records();
        let map_season = self.catalog.maps[self.map].season(&self.season).into_owned();
        // In an RSF race, RSF's ghosts there are (`ghosts::rsf_ghosts`), each with its time and car.
        let rsf_key = self.selection().ok().and_then(|(_, w)| self.rsf_ghost_key(&w));
        let rsf_ghosts = rsf_key.as_ref().map(|key| ghosts::rsf_ghosts(&records, Path::new(self.rsf().trim()), key));
        let group = rsf_key.as_ref().and_then(|k| k.group.clone());
        let rsf_labels: Vec<(GhostKind, String)> = GhostKind::ALL
            .into_iter()
            .map(|kind| {
                let what = match (kind, &group) {
                    (GhostKind::Overall, _) => "Best overall".to_owned(),
                    (GhostKind::Car, _) => "Best of this car".to_owned(),
                    (GhostKind::Group, Some(group)) => format!("Best of the class ({group})"),
                    (GhostKind::Group, None) => "Best of the class".to_owned(),
                    (GhostKind::Latest, _) => "Latest drive".to_owned(),
                };
                let found = rsf_ghosts.iter().flatten().find(|(k, _, _)| *k == kind);
                let Some((_, _, gch)) = found else { return (kind, format!("{what}: none yet")) };
                let car = self.catalog.cars.iter().find(|c| c.rsf.as_ref().is_some_and(|r| r.id == gch.car));
                let car = car.map_or_else(|| format!("car {}", gch.car), |c| c.name.clone());
                let (minutes, seconds) = ((gch.finish / 60.0) as u32, gch.finish % 60.0);
                (kind, format!("{what}: {minutes}:{seconds:05.2}, {car}"))
            })
            .collect();
        let rsf_label = |kind: GhostKind| rsf_labels.iter().find(|(k, _)| *k == kind).map(|(_, l)| l.clone()).unwrap();
        // A saved ghost with the weather its words bring on this map's season (`Season::ghost_weather`).
        let ghost_label = |g: &GhostChoice| match g {
            GhostChoice::Rsf(kind) => rsf_label(*kind),
            GhostChoice::Best if rsf_ghosts.is_some() => rsf_label(GhostKind::Overall),
            GhostChoice::Off => "Off".to_owned(),
            GhostChoice::Best => "Best (kept per stage)".to_owned(),
            GhostChoice::File(f) => {
                let weather = ghosts::ghost_words(&records, f).and_then(|w| map_season.ghost_weather(map, &w));
                let name = f.trim_end_matches(".ghc");
                weather.map_or_else(|| name.to_owned(), |w| format!("{name} — {}", weather_label(&w)))
            }
        };
        let tip = if rsf_ghosts.is_some() {
            "RSF's ghosts for this stage and road condition: the best overall, of this car, of its class, or your \
             latest drive; a finished drive is kept as RSF does"
        } else {
            "Best: race the fastest finished drive on this stage, replaced when you beat it"
        };
        ui.label("Ghost").on_hover_text(tip);
        ComboBox::from_id_salt("ghost")
            .width(FIELD)
            .selected_text(ghost_label(&self.ghost))
            .show_ui(ui, |ui| {
                let choices: Vec<GhostChoice> = match &rsf_ghosts {
                    Some(found) => [GhostChoice::Off]
                        .into_iter()
                        .chain(found.iter().map(|(kind, _, _)| GhostChoice::Rsf(*kind)))
                        .collect(),
                    None => [GhostChoice::Off, GhostChoice::Best]
                        .into_iter()
                        .chain(ghosts::ghost_files(&self.records(), map).into_iter().map(GhostChoice::File))
                        .collect(),
                };
                for choice in choices {
                    let label = ghost_label(&choice);
                    ui.selectable_value(&mut self.ghost, choice, label);
                }
            })
            .response
            .on_hover_text(tip);
    }
}

pub(crate) fn damage_label(level: DamageLevel) -> &'static str {
    match level {
        DamageLevel::Off => "Off",
        DamageLevel::Safe => "Safe",
        DamageLevel::Reduced => "Reduced",
        DamageLevel::Realistic => "Realistic",
    }
}

/// A race weather in a few words: the stage letter, the surface wetness and the tyre wear.
fn weather_label(weather: &Weather) -> String {
    let (wear, wetness) = weather.mat_words();
    format!("{}, {wetness}, {wear} wear", weather.stage())
}
