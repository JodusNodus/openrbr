//! The Drive page's car and map pickers (ours, after Content Manager's lists). A button shows the selection; it opens
//! a search box over the names, numbers, groups, countries and authors, a few filters, and the entries grouped: the
//! recent picks first, then the retail ones, then the RallySimFans ones by car group or country, and those that cannot
//! start greyed at the end with the reason. The list draws only its visible rows, so a full RallySimFans install
//! (about 450 stages and 120 cars) opens and filters at once. The highlighted entry, hovered or reached with the arrow
//! keys, is previewed once it stays a moment ([`Picker::previewed`]); a click or Enter picks it, Escape closes.

use egui::{Align2, Color32, FontId, Key, Modifiers, PopupCloseBehavior, Sense};

use crate::catalog::{self, Car, Map};
use crate::preview;

/// The recent picks kept.
pub const RECENT: usize = 6;
/// The list's row height, its width and its height, points.
const ROW: f32 = 20.0;
/// The list's width: it leaves the previews to its right in view.
const WIDTH: f32 = 360.0;
const LIST_HEIGHT: f32 = 420.0;
/// How long, seconds, an entry stays highlighted before it is previewed.
const DWELL: f64 = 0.15;
const GREY: Color32 = Color32::from_gray(110);

/// One car or map of a picker.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its position in the catalog's cars or maps.
    pub index: usize,
    pub label: String,
    /// Shown after the label, greyed: the number, length and surface, or the group.
    pub detail: String,
    pub group: String,
    /// Why it cannot start.
    pub problem: Option<String>,
    /// More on hover: an RSF car's stand-in model.
    pub note: Option<String>,
    /// A map's surface (0 gravel, 1 tarmac, 2 snow) and length, km.
    pub surface: Option<u32>,
    pub length: Option<f32>,
    /// Whether the chosen physics drives the car (a map: always).
    pub drivable: bool,
    /// The groups' order: 0 retail, 1 RallySimFans.
    rank: u8,
    /// The order within the group.
    sort: String,
    /// What the search matches, lower case.
    haystack: String,
}

/// The game's rally of a retail map, by its base map (ours: the game's menus group them by rally too).
fn rally(map: u32) -> Option<&'static str> {
    Some(match preview::base_map(map) {
        10..=15 => "Finland",
        20..=25 => "Great Britain",
        31..=36 => "Australia",
        41..=46 => "France",
        51..=56 => "Japan",
        61..=66 => "USA",
        _ => return None,
    })
}

pub const SURFACES: [&str; 3] = ["Gravel", "Tarmac", "Snow"];

/// The picker entry of map `index`, `problem` its reason not to start ([`catalog::Catalog::availability`]).
pub fn map_entry(index: usize, map: &Map, problem: Option<String>) -> Entry {
    let problem = problem.map(plain_problem);
    let surface = map.surface();
    let length = match &map.rsf {
        Some(stage) => stage.length,
        None => preview::game_length(map.number).map(|m| m / 1000.0),
    };
    let mut detail = vec![map.number.to_string()];
    detail.extend(length.map(|km| format!("{km:.1} km")));
    detail.push(SURFACES[surface.min(2) as usize].to_owned());
    let (label, group, rank, sort, extra) = match &map.rsf {
        Some(stage) => {
            let country = stage.country.clone().unwrap_or_else(|| "no country".into());
            detail.extend(stage.author.as_ref().map(|a| format!("by {a}")));
            let extra = format!("rsf {country} {}", stage.author.as_deref().unwrap_or(""));
            (stage.name.clone(), format!("RSF · {country}"), 1, stage.name.to_lowercase(), extra)
        }
        None => {
            let label = catalog::title(map.number).map_or_else(|| format!("Map {}", map.number), str::to_owned);
            let group = format!("Retail · {}", rally(map.number).unwrap_or("other"));
            (label, group, 0, format!("{:04}", map.number), "retail".to_owned())
        }
    };
    let detail = detail.join(" · ");
    let haystack = format!("{label} {detail} {group} {extra}").to_lowercase();
    let surface = Some(surface);
    Entry { index, label, detail, group, problem, note: None, surface, length, drivable: true, rank, sort, haystack }
}

/// A map's reason not to start in the player's words (`--list` prints it as it is).
fn plain_problem(problem: String) -> String {
    if problem.contains("encrypted by RallySimFans") {
        "encrypted by RallySimFans: ReRBR can't read it".into()
    } else if problem.contains("weather row") {
        "the game has no weather for it".into()
    } else {
        problem
    }
}

/// The picker entry of car `index`: the RallySimFans cars and the added ones by group.
pub fn car_entry(index: usize, car: &Car) -> Entry {
    let problem = car.unavailable();
    let drivable = problem.is_none();
    let note = match car.rsf.as_ref().map(|r| &r.model) {
        Some(Ok(Some(stand_in))) => Some(format!("stand-in model: {stand_in}")),
        _ => None,
    };
    let (detail, group, rank, sort, extra) = match &car.rsf {
        Some(rsf) => {
            let group = rsf.group.clone().unwrap_or_else(|| "no group".into());
            let pack = rsf.pack.as_ref().ok().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
            let stand_in = if note.is_some() { " · stand-in" } else { "" };
            let (detail, group) = match rsf.added {
                true => (format!("added{stand_in}"), format!("Added · {group}")),
                false => (format!("RSF {}{stand_in}", rsf.id), format!("RSF · {group}")),
            };
            (detail, group, 1, car.name.to_lowercase(), pack.unwrap_or_default())
        }
        None => (format!("car {}", car.index), "Retail".to_owned(), 0, format!("{:04}", car.index), "retail".into()),
    };
    let haystack = format!("{} {detail} {group} {extra}", car.name).to_lowercase();
    let label = car.name.clone();
    Entry { index, label, detail, group, problem, note, surface: None, length: None, drivable, rank, sort, haystack }
}

/// Sorts entries for [`rows`]: the startable ones first, each part by group and within it.
pub fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        (a.problem.is_some(), a.rank, &a.group, &a.sort).cmp(&(b.problem.is_some(), b.rank, &b.group, &b.sort))
    });
    entries
}

/// The stage length filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Length {
    #[default]
    Any,
    Short,
    Medium,
    Long,
    Longest,
}

impl Length {
    pub const ALL: [Length; 5] = [Length::Any, Length::Short, Length::Medium, Length::Long, Length::Longest];

    pub fn label(self) -> &'static str {
        match self {
            Length::Any => "any",
            Length::Short => "< 5 km",
            Length::Medium => "5-10",
            Length::Long => "10-15",
            Length::Longest => "> 15 km",
        }
    }

    /// Whether a stage of `km` passes; one of unknown length passes only `Any`.
    pub fn contains(self, km: Option<f32>) -> bool {
        let range = match self {
            Length::Any => return true,
            Length::Short => 0.0..5.0,
            Length::Medium => 5.0..10.0,
            Length::Long => 10.0..15.0,
            Length::Longest => 15.0..f32::INFINITY,
        };
        km.is_some_and(|km| range.contains(&km))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter {
    /// Words that must all be in the entry's name, number, group, country or author.
    pub query: String,
    pub surface: Option<u32>,
    pub length: Length,
    /// Hide the cars that cannot start.
    pub drivable_only: bool,
}

impl Filter {
    fn accepts(&self, entry: &Entry, words: &[String]) -> bool {
        words.iter().all(|w| entry.haystack.contains(w.as_str()))
            && self.surface.is_none_or(|s| entry.surface == Some(s))
            && self.length.contains(entry.length)
            && (entry.drivable || !self.drivable_only)
    }
}

/// A line of the list: a group's header, or an entry (its position in the entries).
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header(String),
    Item(usize),
}

/// The list's lines for `entries` ([`sorted`]) passing `filter`: the recent picks first (catalog indices), then each
/// group under its header, then the ones that cannot start under one header.
pub fn rows(entries: &[Entry], filter: &Filter, recent: &[usize]) -> Vec<Row> {
    let words: Vec<String> = filter.query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let mut rows = Vec::new();
    let recent: Vec<usize> = recent
        .iter()
        .filter_map(|&i| entries.iter().position(|e| e.index == i))
        .filter(|&k| filter.accepts(&entries[k], &words))
        .collect();
    if !recent.is_empty() && words.is_empty() {
        rows.push(Row::Header("Recent".into()));
        rows.extend(recent.into_iter().map(Row::Item));
    }
    let mut group: Option<&str> = None;
    for (k, entry) in entries.iter().enumerate().filter(|(_, e)| filter.accepts(e, &words)) {
        let header = if entry.problem.is_some() { "Cannot start" } else { entry.group.as_str() };
        if group != Some(header) {
            rows.push(Row::Header(header.to_owned()));
            group = Some(header);
        }
        rows.push(Row::Item(k));
    }
    rows
}

/// Which list: a map's filters are the surface and the length, a car's whether it can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Map,
    Car,
}

pub struct Picker {
    pub kind: Kind,
    pub open: bool,
    pub filter: Filter,
    entries: Vec<Entry>,
    rows: Vec<Row>,
    /// What `rows` was made for.
    rows_of: Option<(Filter, Vec<usize>)>,
    /// The row the arrow keys are on.
    cursor: Option<usize>,
    scroll_to_cursor: bool,
    /// The entry highlighted (catalog index), since when (the ui's time), and the one previewed.
    highlight: Option<usize>,
    since: f64,
    previewed: Option<usize>,
    /// Focus the search box on the next draw.
    focus: bool,
}

impl Picker {
    pub fn new(kind: Kind) -> Picker {
        Picker {
            kind,
            open: false,
            filter: Filter { drivable_only: true, ..Filter::default() },
            entries: Vec::new(),
            rows: Vec::new(),
            rows_of: None,
            cursor: None,
            scroll_to_cursor: false,
            highlight: None,
            since: 0.0,
            previewed: None,
            focus: false,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = sorted(entries);
        self.rows_of = None;
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.iter().find(|e| e.index == index)
    }

    /// Opens the list on `selected` (catalog index).
    pub fn open_on(&mut self, selected: usize, recent: &[usize]) {
        self.open = true;
        self.focus = true;
        self.highlight = None;
        self.previewed = None;
        self.refresh(recent);
        self.cursor = self.rows.iter().position(|r| matches!(r, Row::Item(k) if self.entries[*k].index == selected));
        self.scroll_to_cursor = true;
    }

    fn refresh(&mut self, recent: &[usize]) {
        let key = (self.filter.clone(), recent.to_vec());
        if self.rows_of.as_ref() != Some(&key) {
            self.rows = rows(&self.entries, &self.filter, recent);
            self.rows_of = Some(key);
            self.cursor = self.rows.iter().position(|r| matches!(r, Row::Item(_)));
            self.scroll_to_cursor = true;
        }
    }

    /// The entry to preview: the highlighted one once it has stayed [`DWELL`], else the last previewed, else the
    /// selection.
    pub fn previewed(&mut self, ctx: &egui::Context, selected: usize) -> usize {
        if !self.open {
            self.previewed = None;
            return selected;
        }
        if let Some(h) = self.highlight.filter(|&h| Some(h) != self.previewed) {
            let wait = DWELL - (ctx.input(|i| i.time) - self.since);
            if wait > 0.0 {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            } else {
                self.previewed = Some(h);
            }
        }
        self.previewed.unwrap_or(selected)
    }

    /// The button showing the selection and, when open, the list; whether an entry was picked into `selected`.
    pub fn ui(&mut self, ui: &mut egui::Ui, selected: &mut usize, recent: &[usize]) -> bool {
        let mut text = egui::text::LayoutJob::default();
        if let Some(entry) = self.entry(*selected) {
            for part in
                [egui::RichText::new(&entry.label), egui::RichText::new(format!("  {}", entry.detail)).color(GREY)]
            {
                part.append_to(&mut text, ui.style(), egui::FontSelection::Default, egui::Align::Min);
            }
        }
        let button = ui.add(egui::Button::new(text).right_text("⏷").truncate().min_size(egui::vec2(320.0, 0.0)));
        if button.clicked() {
            if self.open {
                self.open = false;
            } else {
                self.open_on(*selected, recent);
            }
        }
        let mut open = self.open;
        let mut picked = None;
        egui::Popup::from_response(&button)
            .open_bool(&mut open)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .width(WIDTH)
            .show(|ui| picked = self.list(ui, recent));
        self.open = open;
        if let Some(index) = picked {
            *selected = index;
            self.open = false;
        }
        picked.is_some()
    }

    /// The search box, the filters and the rows; the entry picked.
    fn list(&mut self, ui: &mut egui::Ui, recent: &[usize]) -> Option<usize> {
        ui.set_width(WIDTH);
        // The arrow keys and Enter belong to the list, not to the search box or the Start button.
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.filter.query)
                .hint_text("Search: name, number, group, country, author")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.focus) {
            search.request_focus();
        }
        match self.kind {
            Kind::Map => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Surface").color(GREY));
                    ui.selectable_value(&mut self.filter.surface, None, "any");
                    for (s, name) in SURFACES.iter().enumerate() {
                        ui.selectable_value(&mut self.filter.surface, Some(s as u32), *name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Length").color(GREY));
                    for length in Length::ALL {
                        ui.selectable_value(&mut self.filter.length, length, length.label());
                    }
                });
            }
            Kind::Car => {
                ui.checkbox(&mut self.filter.drivable_only, "only the cars that can start");
            }
        }
        self.refresh(recent);
        // The recent picks are also in their groups.
        let mut shown: Vec<usize> =
            self.rows.iter().filter_map(|r| if let Row::Item(k) = r { Some(*k) } else { None }).collect();
        shown.sort_unstable();
        shown.dedup();
        ui.label(egui::RichText::new(format!("{} of {}", shown.len(), self.entries.len())).small().color(GREY));
        let step = |from: Option<usize>, forward: bool| {
            let mut k = from;
            loop {
                k = match (k, forward) {
                    (None, true) => Some(0),
                    (None, false) => None,
                    (Some(k), true) => Some(k + 1).filter(|&k| k < self.rows.len()),
                    (Some(k), false) => k.checked_sub(1),
                };
                match k {
                    Some(r) if matches!(self.rows[r], Row::Item(_)) => return Some(r),
                    Some(_) => {}
                    None => return from,
                }
            }
        };
        if down || up {
            self.cursor = step(self.cursor, down);
            self.scroll_to_cursor = true;
        }
        let item_of = |r: usize| match self.rows.get(r) {
            Some(Row::Item(k)) => Some(*k),
            _ => None,
        };
        if enter {
            return self.cursor.and_then(item_of).map(|k| self.entries[k].index);
        }
        let row_step = ROW + ui.spacing().item_spacing.y;
        let mut area = egui::ScrollArea::vertical().max_height(LIST_HEIGHT).auto_shrink([false, true]);
        if let Some(cursor) = self.cursor.filter(|_| std::mem::take(&mut self.scroll_to_cursor)) {
            area = area.vertical_scroll_offset((cursor as f32 * row_step - LIST_HEIGHT * 0.4).max(0.0));
        }
        let mut hovered = None;
        let mut picked = None;
        area.show_rows(ui, ROW, self.rows.len(), |ui, range| {
            for r in range {
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), Sense::click());
                let painter = ui.painter();
                let entry = match &self.rows[r] {
                    Row::Header(name) => {
                        let font = FontId::proportional(12.0);
                        let at = rect.left_center() + egui::vec2(2.0, 0.0);
                        painter.text(at, Align2::LEFT_CENTER, name, font, ui.visuals().strong_text_color());
                        continue;
                    }
                    Row::Item(k) => &self.entries[*k],
                };
                let visuals = ui.visuals();
                if Some(r) == self.cursor || response.hovered() {
                    painter.rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
                }
                let colour = if entry.problem.is_some() { GREY } else { visuals.text_color() };
                let left = rect.left_center() + egui::vec2(12.0, 0.0);
                let label = painter.text(left, Align2::LEFT_CENTER, &entry.label, FontId::proportional(14.0), colour);
                // The detail, or why it cannot start, right-aligned, cut at its end when it does not fit.
                let detail = entry.problem.as_ref().map_or(entry.detail.clone(), String::clone);
                let galley = painter.layout_no_wrap(detail, FontId::proportional(12.0), GREY);
                let room = egui::Rect::from_min_max(egui::pos2(label.right() + 12.0, rect.top()), rect.right_bottom())
                    .shrink2(egui::vec2(4.0, 0.0));
                let left = (room.right() - galley.size().x).max(room.left());
                painter.with_clip_rect(room).galley(
                    egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                    galley,
                    GREY,
                );
                let tip = [entry.problem.as_ref().map(|p| format!("cannot start: {p}")), entry.note.clone()];
                let tip: Vec<String> = [Some(entry.detail.clone())].into_iter().chain(tip).flatten().collect();
                let response = if tip.len() > 1 { response.on_hover_text(tip.join("\n")) } else { response };
                if response.hovered() {
                    hovered = Some(r);
                }
                if response.clicked() {
                    picked = Some(entry.index);
                }
            }
        });
        if shown.is_empty() {
            ui.label(egui::RichText::new("Nothing matches.").color(GREY));
        }
        // A moving pointer takes the arrow keys' row along; a still one leaves it to the keys.
        if let Some(r) = hovered.filter(|_| ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)) {
            self.cursor = Some(r);
        }
        let highlight = self.cursor.and_then(item_of).map(|k| self.entries[k].index);
        if highlight != self.highlight {
            self.highlight = highlight;
            self.since = ui.input(|i| i.time);
        }
        picked
    }
}

/// Puts `item` first in `recent`, keeping [`RECENT`].
pub fn remember<T: PartialEq>(recent: &mut Vec<T>, item: T) {
    recent.retain(|r| *r != item);
    recent.insert(0, item);
    recent.truncate(RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RsfCar, Stage};
    use rbr_formats::rsf;
    use std::time::{Duration, Instant};

    const COUNTRIES: [&str; 8] = ["FI", "CZ", "FR", "PL", "SK", "ES", "GB", "DE"];

    /// A made-up RallySimFans stage, encrypted 4 in 11 (as about 160 of a full install's 450), and car (no
    /// third-party data).
    fn rsf_map(k: u32) -> Map {
        let id = 100 + k;
        let surface = [rsf::Surface::Gravel, rsf::Surface::Tarmac, rsf::Surface::Snow][k as usize % 3];
        let stage = rsf::Stage {
            id,
            name: format!("Synthetic {k} {}", ["Forest", "Hills", "Pass", "Valley"][k as usize % 4]),
            folder: format!("/synthetic/Maps/{id}-x").into(),
            track: format!("track-{id}"),
            variants: vec!['M'],
            particles: None,
            surface: Some(surface),
            length: Some(2.0 + (k % 25) as f32),
            author: Some(format!("author{}", k % 17)),
            country: Some(COUNTRIES[k as usize % COUNTRIES.len()].into()),
            reference_time: None,
            default_wetness: None,
            animation_path: None,
            weathers: Vec::new(),
            problem: (k % 11 < 4).then(|| format!("track-{id}_M.lbs is encrypted by RallySimFans")),
        };
        Map { number: id, stages: vec![Stage { name: stage.stage('M'), map: id, letter: 'M' }], rsf: Some(stage) }
    }

    fn rsf_car(k: u32) -> Car {
        let rsf = RsfCar {
            id: k,
            added: false,
            group: Some(["Group N", "Group A", "WRC", "R5", "Rally2", "Historic"][k as usize % 6].into()),
            pack: if k.is_multiple_of(10) {
                Err("not downloaded in RallySimFans".into())
            } else {
                Ok(format!("/p/car{k}").into())
            },
            model: Ok(None),
        };
        Car { index: rsf::SLOT, name: format!("Synthetic Car {k}"), rsf: Some(rsf) }
    }

    fn items(rows: &[Row]) -> usize {
        rows.iter().filter(|r| matches!(r, Row::Item(_))).count()
    }

    #[test]
    fn the_lists_group_retail_first_and_the_unstartable_last() {
        let retail = Map { number: 54, stages: Vec::new(), rsf: None };
        let maps = [rsf_map(5), retail, rsf_map(1), rsf_map(3)];
        let entries = sorted(
            maps.iter()
                .enumerate()
                .map(|(i, m)| map_entry(i, m, m.rsf.as_ref().and_then(|s| s.problem.clone())))
                .collect(),
        );
        let rows = rows(&entries, &Filter::default(), &[0]);
        let names: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Header(h) => format!("[{h}]"),
                Row::Item(k) => entries[*k].label.clone(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "[Recent]",
                "Synthetic 5 Hills",
                "[Retail · Japan]",
                "Tanner",
                "[RSF · ES]",
                "Synthetic 5 Hills",
                "[Cannot start]",
                "Synthetic 1 Hills",
                "Synthetic 3 Valley"
            ]
        );
        assert_eq!(entries.iter().find(|e| e.index == 1).unwrap().detail, "54 · 3.9 km · Gravel");
        // A search hides the recent picks' header and matches the country, the author and the number.
        let find =
            |query: &str| items(&super::rows(&entries, &Filter { query: query.into(), ..Filter::default() }, &[0]));
        assert_eq!(find("tanner"), 1);
        assert_eq!(find("japan"), 1);
        assert_eq!(find("cz author1"), 1);
        assert_eq!(find("101"), 1);
        assert_eq!(find("SYNTHETIC"), 3);
        assert_eq!(find("nothing"), 0);
        let surface = Filter { surface: Some(1), ..Filter::default() };
        assert_eq!(items(&super::rows(&entries, &surface, &[])), 1);
        let medium = Filter { length: Length::Medium, ..Filter::default() };
        assert_eq!(items(&super::rows(&entries, &medium, &[])), 2);
        let longest = Filter { length: Length::Longest, ..Filter::default() };
        assert_eq!(items(&super::rows(&entries, &longest, &[])), 0);
        assert!(Length::Short.contains(Some(3.9)) && !Length::Short.contains(None) && Length::Any.contains(None));
    }

    #[test]
    fn the_filter_hides_the_cars_that_cannot_start() {
        let cars = [rsf_car(1), rsf_car(10), rsf_car(2)];
        let list = |drivable_only: bool| {
            let entries = sorted(cars.iter().enumerate().map(|(i, c)| car_entry(i, c)).collect());
            let rows = rows(&entries, &Filter { drivable_only, ..Filter::default() }, &[]);
            rows.iter()
                .filter_map(|r| match r {
                    Row::Item(k) => Some((entries[*k].label.clone(), entries[*k].problem.clone())),
                    Row::Header(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let names = |l: Vec<(String, Option<String>)>| l.into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(names(list(true)), ["Synthetic Car 1", "Synthetic Car 2"]);
        // A car without a pack stays listed, last, with why.
        let all = list(false);
        assert_eq!(all[2], ("Synthetic Car 10".into(), Some("not downloaded in RallySimFans".into())));
    }

    #[test]
    fn recent_picks_stay_few_and_unique() {
        let mut recent = vec![1, 2, 3, 4, 5, 6];
        remember(&mut recent, 4);
        assert_eq!(recent, [4, 1, 2, 3, 5, 6]);
        remember(&mut recent, 9);
        assert_eq!(recent, [9, 4, 1, 2, 3, 5]);
    }

    /// Draws `picker` open in a window-sized frame, without a GPU; the time it took.
    fn frame(ctx: &egui::Context, picker: &mut Picker, selected: &mut usize) -> Duration {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 760.0))),
            ..Default::default()
        };
        let start = Instant::now();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| picker.ui(ui, selected, &[3, 8]));
        });
        start.elapsed()
    }

    /// The scale of a full RallySimFans install, and more: 500 stages and 120 cars open and filter within a frame.
    #[test]
    fn a_big_catalog_opens_and_filters_at_once() {
        let maps: Vec<Map> = (0..500).map(rsf_map).collect();
        let cars: Vec<Car> = (0..120).map(rsf_car).collect();
        let start = Instant::now();
        let mut map_picker = Picker::new(Kind::Map);
        map_picker.set_entries(
            maps.iter()
                .enumerate()
                .map(|(i, m)| map_entry(i, m, m.rsf.as_ref().and_then(|s| s.problem.clone())))
                .collect(),
        );
        let mut car_picker = Picker::new(Kind::Car);
        car_picker.set_entries(cars.iter().enumerate().map(|(i, c)| car_entry(i, c)).collect());
        let built = start.elapsed();
        let ctx = egui::Context::default();
        let (mut map, mut car) = (250, 60);
        let mut slowest = Duration::ZERO;
        for (picker, selected) in [(&mut map_picker, &mut map), (&mut car_picker, &mut car)] {
            picker.open_on(*selected, &[3, 8]);
            for query in ["", "s", "sy", "syn", "synthetic 4", "synthetic 4 cz", "zzz", ""] {
                picker.filter.query = query.into();
                slowest = slowest.max(frame(&ctx, picker, selected));
            }
        }
        map_picker.filter = Filter { surface: Some(2), length: Length::Long, ..Filter::default() };
        slowest = slowest.max(frame(&ctx, &mut map_picker, &mut map));
        eprintln!("picker: entries built in {built:?}, slowest frame {slowest:?}");
        // The snowy 10-15 km stages, recent map 8 among them twice.
        let snowy = (0..500).filter(|k| k % 3 == 2 && (8..13).contains(&(k % 25))).count();
        assert_eq!(items(&map_picker.rows), snowy + 1);
        assert_eq!(map_picker.rows[0], Row::Header("Recent".into()));
        assert!(matches!(map_picker.rows[1], Row::Item(k) if map_picker.entries[k].index == 8));
        assert!(map_picker.open && car_picker.open);
        // Debug builds too: well within one 60 Hz frame for the entries and a 20 fps frame for a draw.
        assert!(built < Duration::from_millis(16), "{built:?}");
        assert!(slowest < Duration::from_millis(50), "{slowest:?}");
    }

    /// `RBR_LAUNCHER_SHOTS=DIR`: the big catalog's lists drawn to `DIR/picker-{maps,cars}.png`.
    #[test]
    fn big_catalog_screenshots() {
        let Some(dir) = std::env::var_os("RBR_LAUNCHER_SHOTS").map(std::path::PathBuf::from) else {
            return;
        };
        let maps: Vec<Map> = (0..500).map(rsf_map).collect();
        let cars: Vec<Car> = (0..120).map(rsf_car).collect();
        let mut map_picker = Picker::new(Kind::Map);
        map_picker.set_entries(
            maps.iter()
                .enumerate()
                .map(|(i, m)| map_entry(i, m, m.rsf.as_ref().and_then(|s| s.problem.clone())))
                .collect(),
        );
        map_picker.filter = Filter { query: "cz".into(), surface: Some(0), ..Filter::default() };
        let mut car_picker = Picker::new(Kind::Car);
        car_picker.set_entries(cars.iter().enumerate().map(|(i, c)| car_entry(i, c)).collect());
        for (name, picker, mut selected) in [("maps", &mut map_picker, 1), ("cars", &mut car_picker, 6)] {
            picker.open_on(selected, &[3, 8]);
            let rgba = crate::capture::draw(|ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label(format!("Synthetic catalog: 500 stages, 120 cars ({name})"));
                    picker.ui(ui, &mut selected, &[3, 8]);
                });
            })
            .unwrap();
            crate::capture::write_png(&dir.join(format!("picker-{name}.png")), &rgba).unwrap();
        }
    }
}
