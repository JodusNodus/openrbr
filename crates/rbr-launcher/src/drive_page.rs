//! The Drive page: the quick race's car and stage, conditions, race options, the previews and Start.

use std::path::Path;

use egui::{Color32, ComboBox, Key, RichText};
use rbr_formats::rsf::ghosts::Kind as GhostKind;
use rbr_gameplay::race_options::DamageLevel;
use rbr_gameplay::weather::{self, Preset, Weather};
use rbr_render::glass::model::Switch;

use crate::browser::{self, Pick};
use crate::car_browser::{self, Target};
use crate::catalog::{self, TyreChoice};
use crate::ghosts::{self, GhostChoice};
use crate::pages::{fields, section, ACCENT, DRIVE_FIELDS, FIELD, WARN};
use crate::settings::{clock_text, or_folder, Clouds, Fog, Precipitation};
use crate::stage_browser::{self, Action, Collection, StageRow};
use crate::Form;

/// The start times the Drive page offers (`viewer --clock`), minutes past midnight: every half hour from 05:00 to 22:30.
/// None is the bake's sun (the viewer relights the stage under it, at its hour; `relight::Clock::new`).
pub fn start_times() -> impl Iterator<Item = u32> {
    (10..46).map(|half_hours| half_hours * 30)
}

/// The Weather combo's choice: a game preset, or one season row of the map (its section).
#[derive(Debug, Clone, PartialEq, Eq)]
enum WeatherSel {
    Preset(Preset),
    Row(String),
}

/// A Custom weather look: our clouds, rain or snow and fog, and the road's wetness (0 dry, 1 damp, 2 wet).
#[derive(Clone, Copy)]
struct Look {
    name: &'static str,
    /// The name on a snow stage, where it snows.
    snowy: &'static str,
    clouds: Clouds,
    rain: Precipitation,
    fog: Fog,
    wetness: u32,
}

const fn look(names: [&'static str; 2], clouds: Clouds, rain: Precipitation, fog: Fog, wetness: u32) -> Look {
    Look { name: names[0], snowy: names[1], clouds, rain, fog, wetness }
}

impl Look {
    fn name(self, snowy: bool) -> &'static str {
        if snowy {
            self.snowy
        } else {
            self.name
        }
    }
}

/// The Custom weather's looks, the second ([`Form::apply_look`]) the one Custom starts on.
const LOOKS: [Look; 7] = [
    look(["Sunny"; 2], Clouds::Clear, Precipitation::None, Fog::Off, 0),
    look(["Fair"; 2], Clouds::Partly, Precipitation::None, Fog::Off, 0),
    look(["Cloudy"; 2], Clouds::Light, Precipitation::None, Fog::Off, 0),
    look(["Overcast"; 2], Clouds::Heavy, Precipitation::None, Fog::Off, 0),
    look(["Drizzle", "Flurries"], Clouds::Heavy, Precipitation::Light, Fog::Light, 1),
    look(["Rain", "Snow"], Clouds::Heavy, Precipitation::Heavy, Fog::Off, 2),
    look(["Foggy"; 2], Clouds::Light, Precipitation::None, Fog::Heavy, 1),
];

impl Form {
    /// Sets the Custom weather to `look` (the Stage weather's choice is kept for going back to it).
    fn apply_look(&mut self, look: Look) {
        self.clouds = Some(look.clouds);
        self.precipitation = Some(look.rain);
        self.fog = Some(look.fog);
        self.wetness = Some(look.wetness);
    }

    /// The Custom look the clouds, rain, fog and wetness make, `None` for an own mix.
    fn look(&self) -> Option<&'static str> {
        let now = (self.clouds, self.precipitation, self.fog, self.wetness);
        let look = LOOKS.iter().find(|l| now == (Some(l.clouds), Some(l.rain), Some(l.fog), Some(l.wetness)));
        look.map(|l| l.name(self.snowy()))
    }

    /// Whether what falls is snow: a snow stage's (the viewer's emitter, `Precipitation::chosen`).
    fn snowy(&self) -> bool {
        self.catalog.maps[self.map].surface() == 2
    }

    /// For the screenshots: the Custom weather on the Rain look, and with `adjust` its controls open on an own mix.
    pub(crate) fn screenshot_custom_weather(&mut self, adjust: bool) {
        self.apply_look(LOOKS[5]);
        self.weather_adjust = adjust;
        if adjust {
            self.precipitation = Some(Precipitation::Light);
        }
    }

    /// The Custom weather's Fog row.
    fn fog_row(&mut self, ui: &mut egui::Ui) {
        let tip = "Ours, over any weather (needs the Sky addition in Graphics), not the game's: off, light, moderate or heavy";
        ui.label("Fog").on_hover_text(tip);
        let selected_text = self.fog.unwrap_or(Fog::Off).label();
        ComboBox::from_id_salt("fog").width(FIELD).selected_text(selected_text).show_ui(ui, |ui| {
            for fog in [Fog::Off, Fog::Light, Fog::Heavy] {
                ui.selectable_value(&mut self.fog, Some(fog), fog.label());
            }
        });
        ui.end_row();
    }
}

/// A start time on map `map` as the Drive page shows it: the time and where the sun is on that rally's date
/// (`rbr_render::relight::solar`).
pub fn start_time_label(map: u32, start: Option<u32>) -> String {
    let Some(minutes) = start else { return "As baked".into() };
    let (latitude, _, day) = rbr_render::relight::rally_of(map);
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
    /// The quick race: the choices on the left, the previews, the drive's summary and Start on the right.
    pub fn drive_page(&mut self, ui: &mut egui::Ui) {
        if self.stage_browser.open {
            return self.stage_browser_page(ui);
        }
        if self.car_browser.open && self.car_browser.target == Target::Drive {
            return self.car_browser_page(ui);
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(DRIVE_FIELDS);
                egui::ScrollArea::vertical().id_salt("drive").show(ui, |ui| self.selectors(ui));
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                let rsf = or_folder(Some(&self.rsf_scanned), &self.folder.rsf_root);
                if let Some(car) = self.catalog.cars.get(self.car) {
                    self.previews.ui(ui, &self.catalog.maps[self.map], car, &rsf);
                }
                ui.add_space(12.0);
                self.start_area(ui);
            });
        });
    }

    /// The drive's summary, Start (or Stop) and what blocks it.
    fn start_area(&mut self, ui: &mut egui::Ui) {
        match self.selection() {
            Ok((stage, weather)) => {
                let wear = catalog::WEARS[weather.wear.min(2) as usize];
                let map = &self.catalog.maps[self.map];
                let name = map.name();
                ui.label(format!("{name} · {} · {} · {wear} wear", self.weather_name(), weather.mat_words().1))
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
        // What blocks Start, with its fix; the last event (`status`) is on the status line.
        if let Some(issue) = blocker.filter(|_| self.child.is_none()) {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(WARN, issue.text);
                if let Some(fix) = issue.fix {
                    if ui.button(fix.label()).clicked() {
                        self.apply(fix);
                    }
                }
            });
        }
    }

    fn selectors(&mut self, ui: &mut egui::Ui) {
        section(ui, "Car and stage");
        fields(ui, "car and stage", |ui| {
            ui.label("Car");
            if self.catalog.cars.is_empty() {
                ui.vertical(|ui| {
                    ui.colored_label(WARN, "No cars yet: add some below");
                    self.add_cars_ui(ui);
                });
            } else {
                ui.vertical(|ui| self.car_field(ui));
            }
            ui.end_row();

            ui.label("Stage");
            ui.vertical(|ui| self.stage_field(ui));
            ui.end_row();
        });

        section(ui, "Environment");
        fields(ui, "environment", |ui| {
            // A saved ghost brings its own weather (`E18_ConnGhostMemory::Dispatch`): the wetness and the wear
            // follow it, while the time of day and our sky stay editable.
            let ghost = self.ghost_weather();
            let tip = "Stage weather: the game's presets (good, bad, or random as the game draws it from the stage's \
                       season), or one of the stage's own season rows, exactly as authored. Custom: our sky over the \
                       stage's good weather, from a few looks or set by hand (needs the Sky and Clouds additions)";
            ui.label("Weather").on_hover_text(tip);
            let mut custom = self.clouds.is_some();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut custom, false, "Stage weather");
                ui.selectable_value(&mut custom, true, "Custom");
            });
            if custom != self.clouds.is_some() {
                // Custom's other choices stay for going back to it (the Stage weather does not read them).
                if custom {
                    self.apply_look(LOOKS[1]);
                } else {
                    self.clouds = None;
                }
            }
            ui.end_row();

            if custom {
                ui.label("Look").on_hover_text("Sets the clouds, the rain or snow, the fog and the road's wetness");
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(FIELD);
                        for look in LOOKS {
                            let name = look.name(self.snowy());
                            if ui.selectable_label(self.look() == Some(name), name).clicked() {
                                self.apply_look(look);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        let summary = if self.look().is_some() { "Adjust…" } else { "Adjust… (own mix)" };
                        ui.toggle_value(&mut self.weather_adjust, summary);
                        let base = weather::name(self.custom_base());
                        ui.weak(format!("over the stage's {base} weather"))
                            .on_hover_text("Its baked light and road; the bad weather's on a damp or wet road");
                    });
                });
                ui.end_row();
                if self.weather_adjust {
                    ui.label("Clouds");
                    let selected = self.clouds.map_or("Clear", Clouds::label);
                    ComboBox::from_id_salt("clouds").width(FIELD).selected_text(selected).show_ui(ui, |ui| {
                        for clouds in Clouds::ALL {
                            ui.selectable_value(&mut self.clouds, Some(clouds), clouds.label());
                        }
                    });
                    ui.end_row();
                    let (falls, tip) = if self.snowy() {
                        ("Snow", "What falls and is heard")
                    } else {
                        ("Rain", "What falls and is heard; snow on a snow stage")
                    };
                    ui.label(falls).on_hover_text(tip);
                    let rain = self.precipitation.unwrap_or(Precipitation::None);
                    ComboBox::from_id_salt("precipitation").width(FIELD).selected_text(rain.label()).show_ui(
                        ui,
                        |ui| {
                            for precipitation in Precipitation::ALL {
                                ui.selectable_value(
                                    &mut self.precipitation,
                                    Some(precipitation),
                                    precipitation.label(),
                                );
                            }
                        },
                    );
                    ui.end_row();
                    self.fog_row(ui);
                }
            } else {
                ui.label("Conditions");
                if let Some((_, weather)) = &ghost {
                    ui.label(format!("from the ghost: {}", weather_label(weather)));
                } else {
                    let map = &self.catalog.maps[self.map];
                    let season = map.season(&self.season);
                    let rows = catalog::environment_rows(&season, map.number);
                    let mut selected = match self.weather_row.clone() {
                        Some(section) if rows.iter().any(|(_, s, _)| s == &section) => WeatherSel::Row(section),
                        _ => WeatherSel::Preset(self.options.weather),
                    };
                    let selected_text = match &selected {
                        WeatherSel::Preset(preset) => weather::name(*preset).to_owned(),
                        WeatherSel::Row(section) => rows
                            .iter()
                            .find(|(_, s, _)| s == section)
                            .map(|(_, _, label)| label.clone())
                            .unwrap_or_else(|| weather::name(self.options.weather).to_owned()),
                    };
                    ComboBox::from_id_salt("weather").width(FIELD).selected_text(selected_text).show_ui(ui, |ui| {
                        for preset in [Preset::Good, Preset::Random, Preset::Bad] {
                            ui.selectable_value(&mut selected, WeatherSel::Preset(preset), weather::name(preset));
                        }
                        for (_, section, label) in &rows {
                            ui.selectable_value(&mut selected, WeatherSel::Row(section.clone()), label);
                        }
                    });
                    match selected {
                        WeatherSel::Preset(preset) => {
                            self.options.weather = preset;
                            self.weather_row = None;
                        }
                        WeatherSel::Row(section) => self.weather_row = Some(section),
                    }
                }
                ui.end_row();
            }

            if custom {
                let tip = "Ours: relights the stage at this local time on its rally's date: the sun, its light and \
                           shadows, and dusk and night lit by the car's lamps. As baked is the sun the stage's light \
                           was baked with";
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
            }
        });

        section(ui, "Road and tyres");
        fields(ui, "road", |ui| {
            // The Stage weather's wetness, and the good and random weather's wear, are the variant's: not shown.
            let ghost = self.ghost_weather();
            let tip = "The road's wetness over the weather's: its grip, the viewer's road, the auto tyre and \
                       rbr-standalone follow it";
            if let Some((_, weather)) = &ghost {
                ui.label("Road wetness").on_hover_text(tip);
                ui.label(format!("following the ghost: {}", weather.mat_words().1));
                ui.end_row();
            } else if self.custom() {
                ui.label("Road wetness").on_hover_text(tip);
                let selected_text = ["Dry", "Damp", "Wet"][self.wetness.unwrap_or(0).min(2) as usize];
                ComboBox::from_id_salt("wetness").width(FIELD).selected_text(selected_text).show_ui(ui, |ui| {
                    for (wetness, name) in [(0, "Dry"), (1, "Damp"), (2, "Wet")] {
                        ui.selectable_value(&mut self.wetness, Some(wetness), name);
                    }
                });
                ui.end_row();
            }

            // A specific row, the bad weather and the Custom weather take the chosen wear.
            let wear_tip = "The road surface: new, normal or worn (the stage's grip table and textures)";
            if let Some((_, weather)) = &ghost {
                ui.label("Road wear").on_hover_text(wear_tip);
                ui.label(format!("following the ghost: {}", weather.mat_words().0));
                ui.end_row();
            } else if self.custom() || self.weather_row.is_some() || self.options.weather == Preset::Bad {
                ui.label("Road wear").on_hover_text(wear_tip);
                ComboBox::from_id_salt("wear")
                    .width(FIELD)
                    .selected_text(catalog::WEARS[self.wear.min(2) as usize])
                    .show_ui(ui, |ui| {
                        for (wear, name) in catalog::WEARS.into_iter().enumerate() {
                            ui.selectable_value(&mut self.wear, wear as u32, name);
                        }
                    });
                ui.end_row();
            }

            let default_tyre = self.selection().ok().and_then(|(_, weather)| self.tyre_id(&weather).ok());
            let tyre_label = match self.tyre {
                TyreChoice::Auto => default_tyre
                    .map_or_else(|| "Auto".to_owned(), |tyre| format!("Auto — {}", catalog::tyre_label(tyre))),
                TyreChoice::Fixed(tyre) => catalog::tyre_label(tyre).to_owned(),
            };
            ui.label("Tyre").on_hover_text("Auto takes the game's tyre for the surface and the road's wetness");
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
        });
    }

    /// The Car field: the selection, starred if it is a favourite, with Browse…, and the favourites' quick picks under
    /// it. The ones that don't fit go behind "+N", which opens the browser on the favourites.
    fn car_field(&mut self, ui: &mut egui::Ui) {
        let Some(selected) = self.car_browser.row(self.car) else { return };
        let favourite = self.favourite_cars.contains(&selected.name);
        if browser::field_button(ui, favourite, &selected.title, &selected.detail(), FIELD) {
            self.car_browser.open_on(Target::Drive, self.car, &self.favourite_cars, None);
        }
        // The starred cars that can start, in the order starred.
        let starred: Vec<&car_browser::CarRow> =
            self.favourite_cars.iter().filter_map(|n| self.car_browser.row_of(n)).filter(|r| r.startable()).collect();
        let chips: Vec<(&str, String, bool)> =
            starred.iter().map(|r| (r.title.as_str(), r.detail(), r.index == self.car)).collect();
        match browser::chips(ui, &chips) {
            Some(Pick::Chip(n)) => {
                let index = starred[n].index;
                self.choose_car(index);
            }
            Some(Pick::More) => {
                let favourites = Some(car_browser::Collection::Favourites);
                self.car_browser.open_on(Target::Drive, self.car, &self.favourite_cars, favourites);
            }
            None => {}
        }
    }

    /// Makes car `index` of the catalog the car, and the most recent pick.
    fn choose_car(&mut self, index: usize) {
        self.car = index;
        browser::remember(&mut self.recent_cars, self.catalog.cars[index].name.clone());
    }

    /// The car browser in place of the page's body: the Drive page's car, or the car a new season drives.
    pub(crate) fn car_browser_page(&mut self, ui: &mut egui::Ui) {
        // "Add cars…" sits in the browser; its progress and result are above it.
        if self.has_adding_output() {
            ui.horizontal_wrapped(|ui| self.adding_job_ui(ui));
            self.adding_status_ui(ui);
        }
        let rsf = self.rsf();
        let can_add = self.can_add_cars();
        let env = car_browser::Env {
            cars: &self.catalog.cars,
            favourites: &self.favourite_cars,
            recent: &self.recent_cars,
            previews: &mut self.previews,
            rsf_root: &rsf,
            can_add,
        };
        let target = self.car_browser.target;
        match self.car_browser.ui(ui, env) {
            Some(car_browser::Action::Choose(index)) => {
                match target {
                    Target::Drive => self.choose_car(index),
                    Target::Season => self.season_pick = index,
                }
                self.car_browser.open = false;
            }
            Some(car_browser::Action::Favourite(name)) => match self.favourite_cars.iter().position(|n| *n == name) {
                Some(at) => drop(self.favourite_cars.remove(at)),
                None => self.favourite_cars.push(name),
            },
            Some(car_browser::Action::Back) => self.car_browser.open = false,
            Some(car_browser::Action::AddCars) => self.add_cars(),
            None => {}
        }
    }

    /// The Stage field: the selection, starred if it is a favourite, with Browse…, and the favourites' quick picks
    /// under it. The ones that don't fit go behind "+N", which opens the browser on the favourites.
    fn stage_field(&mut self, ui: &mut egui::Ui) {
        let Some(selected) = self.stage_browser.row(self.map) else { return };
        let favourite = self.favourite_maps.contains(&selected.number);
        if browser::field_button(ui, favourite, &selected.name, &selected.detail(), FIELD) {
            self.stage_browser.open_on(self.map, &self.favourite_maps, None);
        }
        // The starred stages that can start, in the order starred.
        let starred: Vec<&StageRow> = self
            .favourite_maps
            .iter()
            .filter_map(|&n| self.stage_browser.row_of(n))
            .filter(|r| r.startable())
            .collect();
        let chips: Vec<(&str, String, bool)> =
            starred.iter().map(|r| (r.name.as_str(), r.detail(), r.index == self.map)).collect();
        match browser::chips(ui, &chips) {
            Some(Pick::Chip(n)) => {
                let index = starred[n].index;
                self.choose_stage(index);
            }
            Some(Pick::More) => {
                self.stage_browser.open_on(self.map, &self.favourite_maps, Some(Collection::Favourites))
            }
            None => {}
        }
    }

    /// Makes map `index` of the catalog the stage, and the most recent pick.
    fn choose_stage(&mut self, index: usize) {
        self.map = index;
        browser::remember(&mut self.recent_maps, self.catalog.maps[index].number);
        self.validate_weather_row();
    }

    /// Drops the specific weather row when it is not the new stage's: the choice falls back to Good.
    pub(crate) fn validate_weather_row(&mut self) {
        let map = &self.catalog.maps[self.map];
        let valid = self
            .weather_row
            .as_deref()
            .is_some_and(|section| catalog::row_by_section(&map.season(&self.season), map.number, section).is_some());
        if !valid && self.weather_row.is_some() {
            self.weather_row = None;
            self.options.weather = Preset::Good;
        }
    }

    /// The Weather choice as the summary shows it: the specific row's label, else the preset's name (a saved
    /// ghost brings its own weather, as the row below the fields says).
    fn weather_name(&self) -> String {
        if self.ghost_weather().is_some() {
            return weather::name(self.options.weather).to_owned();
        }
        if self.clouds.is_some() {
            return self.look().unwrap_or("custom").to_owned();
        }
        if let Some(section) = &self.weather_row {
            let map = &self.catalog.maps[self.map];
            let season = map.season(&self.season);
            if let Some(k) = catalog::row_by_section(&season, map.number, section) {
                return catalog::row_label(&season.rows[k]);
            }
        }
        weather::name(self.options.weather).to_owned()
    }

    /// The stage browser in place of the page's body.
    fn stage_browser_page(&mut self, ui: &mut egui::Ui) {
        let records = self.records();
        let (catalog, season) = (&self.catalog, &self.season);
        let facts = |row: &StageRow| {
            let weather = |preset| {
                let found = catalog.stage_weather(season, row.number, preset, weather::Draws::default());
                found.map_or("none".to_owned(), |(_, w)| w.mat_words().1.to_owned())
            };
            let (good, bad) = (weather(Preset::Good), weather(Preset::Bad));
            let weathers = if good == bad { good } else { format!("good {good} · bad {bad}") };
            let ghost =
                if ghosts::ghost_files(&records, row.number).is_empty() { "none yet" } else { "kept from your drives" };
            vec![("Weather", weathers), ("Ghost", ghost.to_owned())]
        };
        let env = stage_browser::Env {
            maps: &self.catalog.maps,
            favourites: &self.favourite_maps,
            recent: &self.recent_maps,
            previews: &mut self.previews,
            facts: &facts,
        };
        match self.stage_browser.ui(ui, env) {
            Some(Action::Choose(index)) => {
                self.choose_stage(index);
                self.stage_browser.open = false;
            }
            Some(Action::Favourite(number)) => match self.favourite_maps.iter().position(|&n| n == number) {
                Some(at) => drop(self.favourite_maps.remove(at)),
                None => self.favourite_maps.push(number),
            },
            Some(Action::Back) => self.stage_browser.open = false,
            None => {}
        }
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
                let car =
                    self.catalog.cars.iter().find(|c| c.rsf.as_ref().is_some_and(|r| !r.added && r.id == gch.car));
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
