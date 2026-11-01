//! The car browser: the Drive page's body, or the Championship page's, while it is
//! open. Collections on the left (favourites, recent, all cars, the classes newest to oldest with the 1.02 cars in
//! their real classes, the cars that cannot start), the list in the middle (a drive filter, sorting, a search over
//! every car), and the highlighted car turning on the right with its buttons. A car RSF has not downloaded stays in
//! its class, greyed at the end with the reason, and cannot be chosen. [`lines`] makes the list from the rows and what
//! the player filtered, apart from any drawing; the keys and the list's plumbing are the stage browser's
//! (`browser`).

use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke};

use crate::browser::{
    block, favourite_button, header_line, items, key_action, list_rows, paint_selection, paint_star, random_row,
    row_slot, side_list, step_nav, top_line, Line, Nav, SideEntry, DIM, GREY, ROW,
};
use crate::catalog::Car;
use crate::pages::{combo, ACCENT};
use crate::preview::{self, Previews};

const GOOD_DOT: Color32 = Color32::from_rgb(110, 190, 120);
const STAND_IN_DOT: Color32 = Color32::from_rgb(220, 170, 70);
const LIST_WIDTH: f32 = 450.0;
const SIDEBAR_WIDTH: f32 = 176.0;

/// The classes of the sidebar, newest to oldest, with the RSF base groups each takes in (RSF splits some finely).
const CLASSES: [(&str, &[&str]); 14] = [
    ("WRC 1.6", &["WRC 1.6"]),
    ("WRC 2.0", &["WRC 2.0"]),
    ("Rally 2 / R5", &["Rally 2", "Group R5"]),
    ("Rally 3 / R3", &["Rally 3", "Group R3"]),
    ("Rally 4 / R2", &["Rally 4", "Group R2"]),
    ("Rally 5 / R1", &["Rally 5", "Group R1"]),
    ("Group R4 / N4", &["Group R4", "Group N4"]),
    ("Super 2000", &["Super 2000"]),
    ("Super 1600", &["Super 1600"]),
    ("Group RGT", &["Group RGT"]),
    ("Group A", &["Group A5", "Group A6", "Group A7", "Group A8"]),
    ("Group B", &["Group B"]),
    ("Group 4", &["Group 4"]),
    ("Group 2", &["Group 2"]),
];

/// The 1.02 cars by their folder (`Cars.ini`): the name (`Root/audio.dat`'s car names; its slot 7 repeats the Corolla's,
/// so the Impreza's is the folder's), the RSF group it belongs to, and the drive. The classes are ours.
const RETAIL: [(&str, &str, &str, Drive); 8] = [
    ("XSARA", "Citroen Xsara 00", "WRC 2.0", Drive::Awd),
    ("ACCENT", "Hyundai Accent 00", "WRC 2.0", Drive::Awd),
    ("ZR", "MG ZR Super 1600", "Super 1600", Drive::Fwd),
    ("LANCER_EVO_VII", "Mitsubishi Lancer Evo VII", "Group N4", Drive::Awd),
    ("PEUGEOT_206", "Peugeot 206 99", "WRC 2.0", Drive::Awd),
    ("IMPREZA03", "Subaru Impreza 2001 A8", "Group A8", Drive::Awd),
    ("COROLLA", "Toyota Corolla 98", "WRC 2.0", Drive::Awd),
    ("IMPREZA00", "Subaru Impreza 2000", "WRC 2.0", Drive::Awd),
];

/// The class for cars without one: the added cars' packs have none.
const OTHER: &str = "Other";

/// Which wheels drive the car.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drive {
    Awd,
    Fwd,
    Rwd,
}

impl Drive {
    pub const ALL: [Drive; 3] = [Drive::Awd, Drive::Fwd, Drive::Rwd];

    pub fn label(self) -> &'static str {
        match self {
            Drive::Awd => "4WD",
            Drive::Fwd => "FWD",
            Drive::Rwd => "RWD",
        }
    }

    /// RSF's words for the drive train (`AWD with torque split`, `FWD`).
    fn parse(words: &str) -> Option<Drive> {
        let words = words.trim().to_ascii_uppercase();
        match words.get(..3)? {
            "AWD" | "4WD" => Some(Drive::Awd),
            "FWD" => Some(Drive::Fwd),
            "RWD" => Some(Drive::Rwd),
            _ => None,
        }
    }
}

/// The model a car is drawn with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Model {
    Own,
    /// A retail car's model standing in for its own (why).
    StandIn(String),
    /// None: the car cannot be drawn.
    Missing,
}

/// The class of the sidebar RSF base group `group` is in, and its place among them.
fn family(group: Option<&str>) -> (String, usize) {
    let Some(group) = group else { return (OTHER.to_owned(), CLASSES.len() + 1) };
    match CLASSES.iter().position(|(_, groups)| groups.contains(&group)) {
        Some(at) => (CLASSES[at].0.to_owned(), at),
        None => (group.to_owned(), CLASSES.len()),
    }
}

/// One car of the catalog as the browser lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct CarRow {
    /// Its position in the catalog's cars.
    pub index: usize,
    /// The catalog's name: what the favourites and the recent picks keep.
    pub name: String,
    /// The name shown: the 1.02 cars' are ours ([`RETAIL`]).
    pub title: String,
    /// Its group (`Group A7`) and the class of the sidebar it is in (`Group A`), which comes with its place.
    pub group: String,
    pub class: String,
    rank: usize,
    /// Where it comes from: `RBR 1.02`, `RSF 94`, `Added`.
    pub source: String,
    pub drive: Option<Drive>,
    pub model: Model,
    /// Why it cannot start.
    pub problem: Option<String>,
    /// What the search matches, lower case.
    haystack: String,
}

impl CarRow {
    pub fn new(index: usize, car: &Car) -> CarRow {
        let problem = car.unavailable();
        let (title, group, source, drive, model) = match &car.rsf {
            Some(rsf) => {
                let source = if rsf.added { "Added".to_owned() } else { format!("RSF {}", rsf.id) };
                let model = match &rsf.model {
                    Ok(None) => Model::Own,
                    Ok(Some(stand_in)) => Model::StandIn(stand_in.clone()),
                    Err(_) => Model::Missing,
                };
                let drive = rsf.drive_train.as_deref().and_then(Drive::parse);
                (car.name.clone(), rsf.group.clone(), source, drive, model)
            }
            None => match RETAIL.iter().find(|(folder, ..)| *folder == car.name) {
                Some((_, title, group, drive)) => {
                    (title.to_string(), Some(group.to_string()), "RBR 1.02".to_owned(), Some(*drive), Model::Own)
                }
                None => (car.name.clone(), None, "RBR 1.02".to_owned(), None, Model::Own),
            },
        };
        let (class, rank) = family(group.as_deref());
        let group = group.unwrap_or_else(|| OTHER.to_owned());
        let haystack = format!("{title} {} {group} {class} {source}", car.name).to_lowercase();
        CarRow { index, name: car.name.clone(), title, group, class, rank, source, drive, model, problem, haystack }
    }

    pub fn startable(&self) -> bool {
        self.problem.is_none()
    }

    /// The group and the source, as the Drive page's field shows them.
    pub fn detail(&self) -> String {
        format!("{} · {}", self.group, self.source)
    }

    /// Why it cannot start, shortly.
    fn reason(&self) -> Option<&str> {
        self.problem.as_deref().map(|p| if p.starts_with("not downloaded") { "not downloaded" } else { p })
    }
}

/// What the player narrowed the list to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter {
    /// Words that must all be in the car's name, group, class, source or RSF number; they search every car.
    pub query: String,
    pub drive: Option<Drive>,
}

impl Filter {
    fn accepts(&self, row: &CarRow) -> bool {
        self.drive.is_none_or(|d| row.drive == Some(d))
    }
}

/// How a list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    /// The ones driven most recently first.
    Recent,
}

impl Sort {
    const ALL: [Sort; 2] = [Sort::Name, Sort::Recent];

    fn label(self) -> &'static str {
        match self {
            Sort::Name => "Name A–Z",
            Sort::Recent => "Recently driven",
        }
    }
}

/// A set of cars the sidebar offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Collection {
    Favourites,
    Recent,
    All,
    /// A class of the sidebar, by name.
    Class(String),
    CannotStart,
}

/// The list of `collection` for `filter` sorted by `sort`. `favourites` and `recent` are car names, in the order
/// starred and newest first. The library and the classes list the cars that can start; the favourites, the recent
/// picks and a search list the others last, and [`Collection::CannotStart`] lists only them. A search covers every
/// car whatever the collection: the favourites first, then each class.
pub fn lines(
    rows: &[CarRow],
    collection: &Collection,
    filter: &Filter,
    sort: Sort,
    favourites: &[String],
    recent: &[String],
) -> Vec<Line> {
    let words: Vec<String> = filter.query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let accepted = |k: &usize| filter.accepts(&rows[*k]);
    let sorted = |mut members: Vec<usize>| {
        match sort {
            Sort::Name => members.sort_by_cached_key(|&k| (rows[k].title.to_lowercase(), rows[k].index)),
            Sort::Recent => members.sort_by_cached_key(|&k| {
                let at = recent.iter().position(|n| *n == rows[k].name);
                (at.unwrap_or(usize::MAX), rows[k].title.to_lowercase())
            }),
        }
        members
    };
    let split =
        |members: Vec<usize>| -> (Vec<usize>, Vec<usize>) { members.into_iter().partition(|&k| rows[k].startable()) };
    let named = |names: &[String]| -> Vec<usize> {
        names.iter().filter_map(|n| rows.iter().position(|r| r.name == *n)).filter(accepted).collect()
    };
    let mut out = Vec::new();
    if !words.is_empty() {
        let found = (0..rows.len()).filter(|&k| words.iter().all(|w| rows[k].haystack.contains(w.as_str())));
        let (ok, blocked) = split(found.filter(accepted).collect());
        let (starred, rest): (Vec<usize>, Vec<usize>) =
            ok.into_iter().partition(|&k| favourites.contains(&rows[k].name));
        out.extend(block(Some("Favourites"), sorted(starred)));
        let mut classes: BTreeMap<(usize, &str), Vec<usize>> = BTreeMap::new();
        for k in rest {
            classes.entry((rows[k].rank, rows[k].class.as_str())).or_default().push(k);
        }
        for ((_, name), members) in classes {
            out.extend(block(Some(name), sorted(members)));
        }
        out.extend(block(Some("Cannot start"), sorted(blocked)));
        return out;
    }
    match collection {
        Collection::Favourites | Collection::Recent => {
            let starred = *collection == Collection::Favourites;
            let (ok, blocked) = split(named(if starred { favourites } else { recent }));
            // The recent picks keep their order of use whatever the sort says.
            let (ok, blocked) = if starred { (sorted(ok), sorted(blocked)) } else { (ok, blocked) };
            out.extend(block(None, ok));
            out.extend(block(Some("Cannot start"), blocked));
        }
        Collection::All | Collection::Class(_) => {
            let wanted = |r: &CarRow| match collection {
                Collection::Class(name) => r.class == *name,
                _ => true,
            };
            let members = (0..rows.len()).filter(|&k| rows[k].startable() && wanted(&rows[k])).filter(accepted);
            out.extend(block(None, sorted(members.collect())));
        }
        Collection::CannotStart => {
            let members = (0..rows.len()).filter(|&k| !rows[k].startable()).filter(accepted);
            out.extend(block(None, sorted(members.collect())));
        }
    }
    out
}

/// The sidebar: the library, then each class with its number of cars that can start, then the cars that cannot.
fn collections(rows: &[CarRow], favourites: &[String], recent: &[String]) -> Vec<(Collection, String, usize)> {
    let known = |names: &[String]| names.iter().filter(|n| rows.iter().any(|r| r.name == **n)).count();
    let startable = rows.iter().filter(|r| r.startable()).count();
    let mut classes: BTreeMap<(usize, &str), usize> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.startable()) {
        *classes.entry((row.rank, row.class.as_str())).or_default() += 1;
    }
    let mut out = vec![
        (Collection::Favourites, "Favourites".to_owned(), known(favourites)),
        (Collection::Recent, "Recent".to_owned(), known(recent)),
        (Collection::All, "All cars".to_owned(), startable),
    ];
    out.extend(classes.into_iter().map(|((_, name), n)| (Collection::Class(name.to_owned()), name.to_owned(), n)));
    let blocked = rows.len() - startable;
    if blocked > 0 {
        out.push((Collection::CannotStart, "Cannot start".to_owned(), blocked));
    }
    out
}

/// What the browser asks of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Choose the car at this position of the catalog and go back.
    Choose(usize),
    /// Star or unstar the car with this name.
    Favourite(String),
    /// Go back without choosing.
    Back,
}

/// What the page gives the browser to draw.
pub struct Env<'a> {
    pub cars: &'a [Car],
    /// Car names: starred in that order, and recently chosen newest first.
    pub favourites: &'a [String],
    pub recent: &'a [String],
    pub previews: &'a mut Previews,
    /// The RSF folder the cars' models are read from.
    pub rsf_root: &'a str,
}

/// What a list was made for.
type Made = (Collection, Filter, Sort, Vec<String>, Vec<String>);

/// Whose car the browser chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    /// The Drive page's.
    #[default]
    Drive,
    /// The car a new season drives.
    Season,
}

#[derive(Default)]
pub struct CarBrowser {
    pub open: bool,
    pub target: Target,
    rows: Vec<CarRow>,
    collection: Option<Collection>,
    filter: Filter,
    sort: Sort,
    lines: Vec<Line>,
    made: Option<Made>,
    /// The highlighted car (its position in the catalog) and the list's scrolling.
    nav: Nav,
    /// Focus the search box on the next draw.
    focus: bool,
    visited: bool,
}

impl CarBrowser {
    /// Lists the cars of the catalog.
    pub fn set_rows(&mut self, rows: Vec<CarRow>) {
        let state = self.state();
        self.rows = rows;
        self.made = None;
        self.restore(&state);
    }

    pub fn row(&self, index: usize) -> Option<&CarRow> {
        self.rows.get(index)
    }

    pub fn state(&self) -> crate::browser_state::State {
        if !self.visited {
            return Default::default();
        }
        let collection = match self.collection.as_ref().unwrap_or(&Collection::All) {
            Collection::Favourites => "favourites".into(),
            Collection::Recent => "recent".into(),
            Collection::All => "all".into(),
            Collection::CannotStart => "unavailable".into(),
            Collection::Class(name) => format!("class:{name}"),
        };
        crate::browser_state::State {
            collection: Some(collection),
            query: self.filter.query.clone(),
            sort: if self.sort == Sort::Recent { "recent" } else { "name" }.into(),
            filter: self.filter.drive.map(|d| d.label().to_owned()).unwrap_or_default(),
            length: String::new(),
            selected: self.rows.get(self.nav.cursor).map(|r| r.name.clone()),
            scroll: self.nav.offset(),
        }
    }

    pub fn restore(&mut self, state: &crate::browser_state::State) {
        let Some(collection) = state.collection.as_deref() else { return };
        self.collection = Some(match collection {
            "favourites" => Collection::Favourites,
            "recent" => Collection::Recent,
            "unavailable" => Collection::CannotStart,
            key if key.starts_with("class:") && self.rows.iter().any(|r| r.class == key[6..]) => {
                Collection::Class(key[6..].into())
            }
            _ => Collection::All,
        });
        self.filter = Filter { query: state.query.clone(), drive: Drive::parse(&state.filter) };
        self.sort = if state.sort == "recent" { Sort::Recent } else { Sort::Name };
        let cursor = self.rows.iter().position(|r| Some(&r.name) == state.selected.as_ref()).unwrap_or(0);
        self.nav.restore(cursor, state.scroll);
        self.visited = true;
        self.made = None;
    }

    /// The row of the car named `name`.
    pub fn row_of(&self, name: &str) -> Option<&CarRow> {
        self.rows.iter().find(|r| r.name == name)
    }

    /// Opens the browser for `target` with `selected` (a catalog position) highlighted: on the favourites when it is
    /// one, else on all cars. `collection` opens it on that instead.
    pub fn open_on(&mut self, target: Target, selected: usize, favourites: &[String], collection: Option<Collection>) {
        let favourite = self.rows.get(selected).is_some_and(|r| favourites.contains(&r.name));
        if !self.visited || collection.is_some() {
            self.collection =
                Some(collection.unwrap_or(if favourite { Collection::Favourites } else { Collection::All }));
            self.filter.query.clear();
            self.nav.show(selected);
        } else {
            self.nav.resume();
        }
        self.visited = true;
        self.target = target;
        self.open = true;
        self.focus = true;
    }

    /// The list, made again when what it depends on changed.
    fn refresh(&mut self, favourites: &[String], recent: &[String]) {
        let collection = self.collection.clone().unwrap_or(Collection::All);
        let made = (collection, self.filter.clone(), self.sort, favourites.to_vec(), recent.to_vec());
        if self.made.as_ref() == Some(&made) {
            return;
        }
        let changed = self.made.as_ref().is_none_or(|m| m.0 != made.0 || m.1 != made.1 || m.2 != made.2);
        self.lines = lines(&self.rows, &made.0, &made.1, made.2, favourites, recent);
        self.made = Some(made);
        if changed {
            self.nav.settle(&self.lines);
        }
    }

    /// Draws the browser in `ui`; what the player asked for.
    pub fn ui(&mut self, ui: &mut egui::Ui, mut env: Env) -> Option<Action> {
        self.refresh(env.favourites, env.recent);
        let keys = step_nav(ui, &mut self.nav, &self.lines);
        let mut action = key_action(
            &self.nav,
            &self.rows,
            &keys,
            |row| row.startable().then_some(Action::Choose(row.index)),
            |row| Action::Favourite(row.name.clone()),
            Action::Back,
        );

        let mut random = false;
        let hint = "Search all cars: name, class, RSF number";
        if top_line(ui, "Choose a car", hint, &mut self.filter.query, &mut self.focus, &mut random) {
            action = Some(Action::Back);
        }
        if random {
            self.random();
        }
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(SIDEBAR_WIDTH);
                self.sidebar(ui, &env);
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let chosen = ui.vertical(|ui| self.list(ui, env.favourites, env.recent)).inner;
            action = chosen.or(action.take());
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let chosen = ui.vertical(|ui| self.detail(ui, &mut env)).inner;
            action = chosen.or(action.take());
        });
        action
    }

    /// A random car of the list as filtered becomes the highlighted one.
    fn random(&mut self) {
        if let Some(k) = random_row(&self.lines, |k| self.rows[k].startable()) {
            self.nav.show(k);
        }
    }

    /// The collections of the sidebar.
    fn sidebar(&mut self, ui: &mut egui::Ui, env: &Env) {
        let all = collections(&self.rows, env.favourites, env.recent);
        let current = self.collection.clone().unwrap_or(Collection::All);
        let entries: Vec<SideEntry> = all
            .iter()
            .map(|(collection, name, count)| SideEntry {
                icon: match collection {
                    Collection::Favourites => "★",
                    Collection::CannotStart => "⚠",
                    _ => "",
                },
                name,
                count: *count,
                selected: current == *collection,
                grey: *collection == Collection::CannotStart,
            })
            .collect();
        // The classes of a full install do not fit the window's height: the list scrolls, the button stays.
        let room = (ui.available_height() - 44.0).max(ROW * 3.0);
        egui::ScrollArea::vertical().id_salt("collections").max_height(room).auto_shrink([false, true]).show(
            ui,
            |ui| {
                if let Some(n) = side_list(ui, &entries, (3, "CLASSES")) {
                    self.collection = Some(all[n].0.clone());
                    self.filter.query.clear();
                }
            },
        );
        ui.add_space(8.0);
    }

    /// The filters, sort, the list and the keys; a car chosen or starred with the mouse.
    fn list(&mut self, ui: &mut egui::Ui, favourites: &[String], recent: &[String]) -> Option<Action> {
        let mut action = None;
        let collection = self.collection.clone().unwrap_or(Collection::All);
        ui.set_width(LIST_WIDTH);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Drive").color(GREY));
            ui.selectable_value(&mut self.filter.drive, None, "Any");
            for drive in Drive::ALL {
                ui.selectable_value(&mut self.filter.drive, Some(drive), drive.label());
            }
        });
        let count = items(&self.lines).count();
        ui.horizontal(|ui| {
            let what = if self.filter.query.trim().is_empty() {
                match &collection {
                    Collection::Favourites => "favourites".to_owned(),
                    Collection::Recent => "recent".to_owned(),
                    Collection::All => "cars".to_owned(),
                    Collection::Class(name) => format!("cars in {name}"),
                    Collection::CannotStart => "cars that cannot start".to_owned(),
                }
            } else {
                "found in all cars".to_owned()
            };
            ui.label(RichText::new(format!("{count} {what}")).color(GREY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                combo(ui, "sort", 120.0, &mut self.sort, &Sort::ALL, Sort::label);
                ui.label(RichText::new("Sort").color(GREY));
            });
        });
        ui.add_space(4.0);
        let by_class = !matches!(collection, Collection::Class(_)) || !self.filter.query.trim().is_empty();
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());
        let c = columns(rect.left());
        let small = FontId::proportional(11.5);
        let y = rect.center().y;
        let p = ui.painter();
        p.text(Pos2::new(c[1], y), Align2::LEFT_CENTER, "CAR", small.clone(), GREY);
        p.text(Pos2::new(c[2], y), Align2::LEFT_CENTER, if by_class { "CLASS" } else { "FROM" }, small.clone(), GREY);
        p.text(Pos2::new(c[3], y), Align2::LEFT_CENTER, "MODEL", small, GREY);
        p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, Color32::from_gray(60)));
        ui.add_space(2.0);

        if count == 0 {
            let empty = self.filter.query.trim().is_empty();
            let hint = match &collection {
                Collection::Favourites if empty && favourites.is_empty() => "Click ☆ next to a car to keep it here.",
                Collection::Recent if empty && recent.is_empty() => "The cars you choose are listed here.",
                _ => "Nothing matches.",
            };
            ui.add_space(10.0);
            ui.label(RichText::new(hint).color(GREY));
        }
        list_rows(ui, &mut self.nav, "cars", &self.lines, |ui, nav, range| {
            for at in range {
                let k = match &self.lines[at] {
                    Line::Header(name) => {
                        header_line(ui, name);
                        continue;
                    }
                    Line::Row(k) => *k,
                };
                let row = &self.rows[k];
                let (rect, response, star) = row_slot(ui, k);
                let favourite = favourites.contains(&row.name);
                draw_row(ui, rect, row, by_class, k == nav.cursor, response.hovered(), favourite);
                let response = match row.problem.as_deref() {
                    Some(problem) => response.on_hover_text(format!("cannot start: {problem}")),
                    None => response,
                };
                if star.clicked() {
                    action = Some(Action::Favourite(row.name.clone()));
                } else if response.double_clicked() && row.startable() {
                    action = Some(Action::Choose(row.index));
                } else if response.clicked() {
                    nav.point(k);
                }
            }
        });
        action
    }

    /// The highlighted car: its turntable, name, facts and the buttons.
    fn detail(&mut self, ui: &mut egui::Ui, env: &mut Env) -> Option<Action> {
        let row = self.rows.get(self.nav.cursor)?;
        let car = env.cars.get(row.index)?;
        let mut action = None;
        ui.set_width(preview::WIDTH);
        ui.add_space(6.0);
        ui.label(RichText::new(&row.title).size(22.0).strong());
        ui.label(RichText::new(row.detail()).color(GREY));
        ui.add_space(6.0);
        env.previews.car_ui(ui, car, env.rsf_root);
        ui.add_space(6.0);
        egui::Grid::new("facts").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("Drive").color(GREY));
            ui.label(row.drive.map_or("unknown", Drive::label));
            ui.end_row();
            ui.label(RichText::new("Model").color(GREY));
            ui.label(match &row.model {
                Model::Own => "its own".to_owned(),
                Model::StandIn(_) => "a stand-in".to_owned(),
                Model::Missing => "none".to_owned(),
            });
            ui.end_row();
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if favourite_button(ui, env.favourites.contains(&row.name)).clicked() {
                action = Some(Action::Favourite(row.name.clone()));
            }
            let go = RichText::new("Choose car").size(18.0).strong().color(Color32::WHITE);
            let go = egui::Button::new(go).fill(ACCENT).min_size(egui::vec2(ui.available_width(), 44.0));
            let go = ui.add_enabled(row.startable(), go).on_hover_text("Enter, or a double click on the car");
            if go.clicked() {
                action = Some(Action::Choose(row.index));
            }
        });
        if let Some(reason) = row.reason() {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Cannot start: {reason}")).color(crate::pages::WARN));
        }
        action
    }
}

/// The list's columns: the star, the name, the third (class or source), and the model's dot.
fn columns(left: f32) -> [f32; 4] {
    [left + 6.0, left + 30.0, left + 226.0, left + 336.0]
}

/// The room the name has before the third column.
const NAME_WIDTH: f32 = 190.0;

/// A car's row: its star, name, class or source, and model; the highlighted one filled.
fn draw_row(ui: &egui::Ui, rect: Rect, row: &CarRow, by_class: bool, cursor: bool, hovered: bool, favourite: bool) {
    let p = ui.painter();
    let c = columns(rect.left());
    let y = rect.center().y;
    paint_selection(ui, rect, cursor, hovered);
    let text = if row.startable() { ui.visuals().strong_text_color() } else { DIM };
    paint_star(p, Pos2::new(c[0] + 6.0, y), favourite, hovered || cursor);
    let name = p.layout_no_wrap(row.title.clone(), FontId::proportional(15.0), text);
    let room = Rect::from_min_max(Pos2::new(c[1], rect.top()), Pos2::new(c[1] + NAME_WIDTH, rect.bottom()));
    p.with_clip_rect(room).galley(Pos2::new(c[1], y - name.size().y / 2.0), name, text);
    let third = if by_class { &row.group } else { &row.source };
    let third_colour = if row.startable() { GREY } else { DIM };
    let room = Rect::from_min_max(Pos2::new(c[2], rect.top()), Pos2::new(c[3] - 6.0, rect.bottom()));
    let galley = p.layout_no_wrap(third.clone(), FontId::proportional(13.0), third_colour);
    p.with_clip_rect(room).galley(Pos2::new(c[2], y - galley.size().y / 2.0), galley, third_colour);
    let room = Rect::from_min_max(Pos2::new(c[3], rect.top()), rect.right_bottom());
    match (row.reason(), &row.model) {
        (Some(reason), _) => {
            let galley = p.layout_no_wrap(reason.to_owned(), FontId::proportional(12.0), DIM);
            p.with_clip_rect(room).galley(Pos2::new(c[3], y - galley.size().y / 2.0), galley, DIM);
        }
        (None, model) => {
            let (dot, word) = match model {
                Model::Own => (GOOD_DOT, "own model"),
                Model::StandIn(_) => (STAND_IN_DOT, "stand-in"),
                Model::Missing => (DIM, "no model"),
            };
            p.circle_filled(Pos2::new(c[3] + 4.0, y), 4.0, dot);
            p.with_clip_rect(room).text(
                Pos2::new(c[3] + 14.0, y),
                Align2::LEFT_CENTER,
                word,
                FontId::proportional(13.0),
                GREY,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(index: usize, name: &str, group: &str, problem: Option<&str>) -> CarRow {
        let (class, rank) = family(Some(group));
        CarRow {
            index,
            name: name.into(),
            title: name.into(),
            group: group.into(),
            class,
            rank,
            source: format!("RSF {index}"),
            drive: Some(Drive::Awd),
            model: Model::Own,
            problem: problem.map(str::to_owned),
            haystack: format!("{name} {group}").to_lowercase(),
        }
    }

    fn rows() -> Vec<CarRow> {
        vec![
            row(0, "Zeta", "Group A7", None),
            row(1, "Alpha", "Group A8", Some("not downloaded in RallySimFans")),
            row(2, "Beta", "WRC 2.0", None),
            row(3, "Gamma", "Group A6", None),
        ]
    }

    #[test]
    fn reopening_and_restoring_keep_the_car_picker_state() {
        let mut browser = CarBrowser::default();
        browser.set_rows(rows());
        browser.open_on(Target::Drive, 0, &[], None);
        browser.collection = Some(Collection::Class("Group A".into()));
        browser.filter = Filter { query: "gamma".into(), drive: Some(Drive::Awd) };
        browser.sort = Sort::Recent;
        browser.nav.restore(3, 90.0);
        browser.refresh(&[], &[]);
        let state = browser.state();
        browser.open = false;
        browser.open_on(Target::Season, 0, &["Zeta".into()], None);
        browser.refresh(&[], &[]);
        assert_eq!(browser.state(), state);
        assert_eq!(browser.target, Target::Season);
        let mut restored = CarBrowser::default();
        let mut rows = rows();
        rows.swap(0, 3);
        restored.set_rows(rows);
        restored.restore(&state);
        restored.open_on(Target::Drive, 0, &[], None);
        restored.refresh(&[], &[]);
        assert_eq!(restored.state(), state, "highlight follows the car name across catalogue changes");
        restored.open_on(Target::Drive, 0, &[], Some(Collection::Favourites));
        assert_eq!(restored.collection, Some(Collection::Favourites));
        assert!(restored.filter.query.is_empty());
        assert_eq!(restored.sort, Sort::Recent);
    }

    fn names(rows: &[CarRow], lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Header(h) => format!("#{h}"),
                Line::Row(k) => rows[*k].name.clone(),
            })
            .collect()
    }

    #[test]
    fn the_a_groups_share_a_class_of_the_cars_that_can_start() {
        let rows = rows();
        let class = Collection::Class("Group A".into());
        let found = lines(&rows, &class, &Filter::default(), Sort::Name, &[], &[]);
        assert_eq!(names(&rows, &found), ["Gamma", "Zeta"]);
        let blocked = lines(&rows, &Collection::CannotStart, &Filter::default(), Sort::Name, &[], &[]);
        assert_eq!(names(&rows, &blocked), ["Alpha"]);
        let rwd = Filter { drive: Some(Drive::Rwd), ..Filter::default() };
        assert!(lines(&rows, &Collection::CannotStart, &rwd, Sort::Name, &[], &[]).is_empty());
    }

    #[test]
    fn a_search_covers_every_car_by_class() {
        let rows = rows();
        let filter = Filter { query: "group a".into(), ..Filter::default() };
        let found = lines(&rows, &Collection::Favourites, &filter, Sort::Name, &["Zeta".into()], &[]);
        assert_eq!(names(&rows, &found), ["#Favourites", "Zeta", "#Group A", "Gamma", "#Cannot start", "Alpha"]);
    }

    #[test]
    fn the_sidebar_orders_classes_newest_first() {
        let rows = rows();
        let all = collections(&rows, &[], &[]);
        let labels: Vec<(&str, usize)> = all.iter().map(|(_, n, count)| (n.as_str(), *count)).collect();
        assert_eq!(
            labels,
            [("Favourites", 0), ("Recent", 0), ("All cars", 3), ("WRC 2.0", 1), ("Group A", 2), ("Cannot start", 1)]
        );
    }

    #[test]
    fn drive_words_of_rsf_parse() {
        assert_eq!(Drive::parse("AWD with torque split"), Some(Drive::Awd));
        assert_eq!(Drive::parse("fwd"), Some(Drive::Fwd));
        assert_eq!(Drive::parse("?"), None);
    }
}
