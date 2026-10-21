//! The Drive page's stage browser (docs/proposals/track-picker.md): the page's body while it is open. Collections on
//! the left (favourites, recent, all stages, each country with the 1.02 and the RallySimFans stages together, those
//! that cannot start), the list in the middle (surface and length filters, sort, search over everything), and the
//! highlighted stage on the right: its picture, weathers, and the buttons to favourite and choose it.
//!
//! [`lines`] makes the list from the rows and what the player filtered, apart from any drawing. The list draws only its
//! visible rows, so a full RallySimFans install (about 450 stages) opens and filters at once. A moving pointer takes
//! the highlight along, the arrow keys and Page Up/Down move it, Enter or a double click chooses, Ctrl/Cmd+D
//! favourites, Escape goes back without choosing. A stage that cannot start is listed greyed with its reason, under
//! "Cannot start", and last in the favourites, the recent picks and a search.

use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Id, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke};

use crate::catalog::Map;
use crate::pages::ACCENT;
use crate::preview::{self, Previews};

pub const SURFACES: [&str; 3] = ["Gravel", "Tarmac", "Snow"];

const GREY: Color32 = Color32::from_gray(130);
const DIM: Color32 = Color32::from_gray(95);
pub const STAR: Color32 = Color32::from_rgb(240, 190, 60);
const SURFACE_DOTS: [Color32; 3] =
    [Color32::from_rgb(190, 145, 95), Color32::from_rgb(150, 155, 170), Color32::from_rgb(185, 215, 250)];
/// The list's row height, and its width, points.
const ROW: f32 = 30.0;
const LIST_WIDTH: f32 = 450.0;
const SIDEBAR_WIDTH: f32 = 176.0;
/// The list's visible height before the first draw has measured it.
const LIST_HEIGHT: f32 = 420.0;
/// Rows a Page Up or Page Down moves.
const PAGE: usize = 10;
const SEARCH: &str = "stage search";

/// The ISO country codes RallySimFans gives its stages, and the names the browser shows.
const COUNTRIES: [(&str, &str); 39] = [
    ("AR", "Argentina"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("BE", "Belgium"),
    ("BR", "Brazil"),
    ("CA", "Canada"),
    ("CH", "Switzerland"),
    ("CN", "China"),
    ("CY", "Cyprus"),
    ("CZ", "Czechia"),
    ("DE", "Germany"),
    ("EE", "Estonia"),
    ("ES", "Spain"),
    ("FI", "Finland"),
    ("FR", "France"),
    ("GB", "Great Britain"),
    ("GR", "Greece"),
    ("HU", "Hungary"),
    ("IE", "Ireland"),
    ("IT", "Italy"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("LB", "Lebanon"),
    ("LT", "Lithuania"),
    ("LV", "Latvia"),
    ("MC", "Monaco"),
    ("MG", "Madagascar"),
    ("MX", "Mexico"),
    ("NL", "Netherlands"),
    ("NZ", "New Zealand"),
    ("PL", "Poland"),
    ("PT", "Portugal"),
    ("RO", "Romania"),
    ("SE", "Sweden"),
    ("SI", "Slovenia"),
    ("SK", "Slovakia"),
    ("SM", "San Marino"),
    ("UA", "Ukraine"),
    ("US", "USA"),
];

/// A country's name by its code; the code itself when the table has none.
pub fn country_name(code: &str) -> &str {
    COUNTRIES.iter().find(|(c, _)| c.eq_ignore_ascii_case(code)).map_or(code, |(_, name)| name)
}

/// The country of a retail map, by its base map (the game's menus group them by rally).
fn retail_country(map: u32) -> Option<&'static str> {
    Some(match rbr_render::stage_picture::base_map(map) {
        10..=15 => "FI",
        20..=25 => "GB",
        31..=36 => "AU",
        41..=46 => "FR",
        51..=56 => "JP",
        61..=66 => "US",
        _ => return None,
    })
}

/// One stage of the catalog as the browser lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct StageRow {
    /// Its position in the catalog's maps.
    pub index: usize,
    pub number: u32,
    pub name: String,
    /// The country's code, upper case.
    pub country: Option<String>,
    pub author: Option<String>,
    /// A 1.02 stage, not a RallySimFans one.
    pub retail: bool,
    pub km: Option<f32>,
    /// 0 gravel, 1 tarmac, 2 snow.
    pub surface: u32,
    /// Why it cannot start.
    pub problem: Option<String>,
    /// What the search matches, lower case.
    haystack: String,
}

impl StageRow {
    /// The row of map `index`, `problem` its reason not to start ([`catalog::Catalog::availability`]).
    pub fn new(index: usize, map: &Map, problem: Option<String>) -> StageRow {
        let (country, author, km) = match &map.rsf {
            Some(stage) => (stage.country.as_ref().map(|c| c.to_uppercase()), stage.author.clone(), stage.length),
            None => (
                retail_country(map.number).map(str::to_owned),
                Some("Warthog Games".to_owned()),
                preview::game_length(map.number).map(|m| m / 1000.0),
            ),
        };
        let name = map.name();
        let origin = if map.rsf.is_some() { "rsf rallysimfans" } else { "retail 1.02" };
        let place = country.as_deref().map(|c| format!("{c} {}", country_name(c))).unwrap_or_default();
        let haystack =
            format!("{name} {} {place} {} {origin}", map.number, author.as_deref().unwrap_or("")).to_lowercase();
        StageRow {
            index,
            number: map.number,
            name,
            country,
            author,
            retail: map.rsf.is_none(),
            km,
            surface: map.surface().min(2),
            problem: problem.map(plain_problem),
            haystack,
        }
    }

    /// The country's name, or "No country".
    pub fn country_name(&self) -> &str {
        self.country.as_deref().map_or("No country", country_name)
    }

    /// The number, length and surface, as the Drive page's field shows them.
    pub fn detail(&self) -> String {
        let mut parts = vec![self.number.to_string()];
        parts.extend(self.km.map(|km| format!("{km:.1} km")));
        parts.push(SURFACES[self.surface as usize].to_owned());
        parts.join(" · ")
    }

    pub fn startable(&self) -> bool {
        self.problem.is_none()
    }
}

/// A stage's reason not to start in the player's words (`--list` prints it as it is).
fn plain_problem(problem: String) -> String {
    if problem.contains("encrypted by RallySimFans") {
        "encrypted by RallySimFans: ReRBR can't read it".into()
    } else if problem.contains("weather row") {
        "the game has no weather for it".into()
    } else {
        problem
    }
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
            Length::Any => "Any",
            Length::Short => "< 5 km",
            Length::Medium => "5-10 km",
            Length::Long => "10-15 km",
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

/// What the player narrowed the list to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter {
    /// Words that must all be in the stage's name, number, country, author or origin; they search every stage.
    pub query: String,
    pub surface: Option<u32>,
    pub length: Length,
}

impl Filter {
    fn accepts(&self, row: &StageRow) -> bool {
        self.surface.is_none_or(|s| row.surface == s) && self.length.contains(row.km)
    }
}

/// How a list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    /// The shortest first, those of unknown length last.
    Length,
    /// The ones driven most recently first.
    Recent,
}

impl Sort {
    const ALL: [Sort; 3] = [Sort::Name, Sort::Length, Sort::Recent];

    fn label(self) -> &'static str {
        match self {
            Sort::Name => "Name A–Z",
            Sort::Length => "Length",
            Sort::Recent => "Recently driven",
        }
    }
}

/// A set of stages the sidebar offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Collection {
    Favourites,
    Recent,
    All,
    /// A country, by its code (`None`: the stages without one).
    Country(Option<String>),
    CannotStart,
}

/// A line of the list: a header, or a stage (its position in the rows).
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Header(String),
    Row(usize),
}

/// `members` under `header`, nothing when there are none.
fn block(header: Option<&str>, members: Vec<usize>) -> impl Iterator<Item = Line> {
    let header = if members.is_empty() { None } else { header.map(|h| Line::Header(h.to_owned())) };
    header.into_iter().chain(members.into_iter().map(Line::Row))
}

/// The list of `collection` for `filter` sorted by `sort`. `favourites` and `recent` are map numbers, in the order
/// starred and newest first. A search covers every stage whatever the collection: the favourites first, then each
/// country, then the ones that cannot start.
pub fn lines(
    rows: &[StageRow],
    collection: &Collection,
    filter: &Filter,
    sort: Sort,
    favourites: &[u32],
    recent: &[u32],
) -> Vec<Line> {
    let words: Vec<String> = filter.query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let accepted = |k: &usize| filter.accepts(&rows[*k]);
    let sorted = |mut members: Vec<usize>| {
        match sort {
            Sort::Name => members.sort_by_cached_key(|&k| (rows[k].name.to_lowercase(), rows[k].number)),
            Sort::Length => members.sort_by(|&a, &b| {
                let km = |k: usize| rows[k].km.unwrap_or(f32::INFINITY);
                km(a).total_cmp(&km(b)).then_with(|| rows[a].name.cmp(&rows[b].name))
            }),
            Sort::Recent => members.sort_by_cached_key(|&k| {
                (recent.iter().position(|&n| n == rows[k].number).unwrap_or(usize::MAX), rows[k].name.to_lowercase())
            }),
        }
        members
    };
    let split =
        |members: Vec<usize>| -> (Vec<usize>, Vec<usize>) { members.into_iter().partition(|&k| rows[k].startable()) };
    let mut out = Vec::new();
    if !words.is_empty() {
        let found = (0..rows.len()).filter(|&k| words.iter().all(|w| rows[k].haystack.contains(w.as_str())));
        let (ok, blocked) = split(found.filter(accepted).collect());
        let (starred, rest): (Vec<usize>, Vec<usize>) =
            ok.into_iter().partition(|&k| favourites.contains(&rows[k].number));
        out.extend(block(Some("Favourites"), sorted(starred)));
        let mut countries: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for k in rest {
            countries.entry(rows[k].country_name()).or_default().push(k);
        }
        for (name, members) in countries {
            out.extend(block(Some(name), sorted(members)));
        }
        out.extend(block(Some("Cannot start"), sorted(blocked)));
        return out;
    }
    match collection {
        Collection::Favourites | Collection::Recent => {
            let starred = *collection == Collection::Favourites;
            let numbers = if starred { favourites } else { recent };
            let found = numbers.iter().filter_map(|n| rows.iter().position(|r| r.number == *n)).filter(accepted);
            let (ok, blocked) = split(found.collect());
            // The recent picks keep their order of use whatever the sort says.
            let (ok, blocked) = if starred { (sorted(ok), sorted(blocked)) } else { (ok, blocked) };
            out.extend(block(None, ok));
            out.extend(block(Some("Cannot start"), blocked));
        }
        Collection::All | Collection::Country(_) => {
            let wanted = |r: &StageRow| match collection {
                Collection::Country(code) => r.country == *code,
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

/// The sidebar: each collection with its number of stages, the countries by name.
fn collections(rows: &[StageRow], favourites: &[u32], recent: &[u32]) -> Vec<(Collection, String, usize)> {
    let known = |numbers: &[u32]| numbers.iter().filter(|n| rows.iter().any(|r| r.number == **n)).count();
    let startable = rows.iter().filter(|r| r.startable()).count();
    let mut countries: BTreeMap<(String, Option<String>), usize> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.startable()) {
        *countries.entry((row.country_name().to_owned(), row.country.clone())).or_default() += 1;
    }
    let mut out = vec![
        (Collection::Favourites, "Favourites".to_owned(), known(favourites)),
        (Collection::Recent, "Recent".to_owned(), known(recent)),
        (Collection::All, "All stages".to_owned(), startable),
    ];
    out.extend(countries.into_iter().map(|((name, code), n)| (Collection::Country(code), name, n)));
    let blocked = rows.len() - startable;
    if blocked > 0 {
        out.push((Collection::CannotStart, "Cannot start".to_owned(), blocked));
    }
    out
}

/// What the browser asks of the Drive page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Choose the map at this position of the catalog and go back.
    Choose(usize),
    /// Star or unstar the map with this number.
    Favourite(u32),
    /// Go back without choosing.
    Back,
}

/// What the page gives the browser to draw.
pub struct Env<'a> {
    pub maps: &'a [Map],
    /// Map numbers: starred in that order, and recently chosen newest first.
    pub favourites: &'a [u32],
    pub recent: &'a [u32],
    pub previews: &'a mut Previews,
    /// The detail panel's rows under the picture for a stage (its weathers, whether it was driven).
    pub facts: &'a dyn Fn(&StageRow) -> Vec<(&'static str, String)>,
}

/// What a list was made for.
type Made = (Collection, Filter, Sort, Vec<u32>, Vec<u32>);

pub struct StageBrowser {
    pub open: bool,
    rows: Vec<StageRow>,
    collection: Collection,
    filter: Filter,
    sort: Sort,
    lines: Vec<Line>,
    made: Option<Made>,
    /// The highlighted stage: its position in the catalog.
    cursor: usize,
    /// The list's scroll offset and visible height as last drawn, and whether it must bring the cursor into view
    /// (`Some(true)`: centred) or go to the top.
    scroll: (f32, f32),
    reveal: Option<bool>,
    to_top: bool,
    /// Focus the search box on the next draw.
    focus: bool,
}

impl Default for StageBrowser {
    fn default() -> Self {
        StageBrowser {
            open: false,
            rows: Vec::new(),
            collection: Collection::All,
            filter: Filter::default(),
            sort: Sort::Name,
            lines: Vec::new(),
            made: None,
            cursor: 0,
            scroll: (0.0, LIST_HEIGHT),
            reveal: None,
            to_top: false,
            focus: false,
        }
    }
}

impl StageBrowser {
    /// Lists the stages: each map of the catalog with its reason not to start.
    pub fn set_rows(&mut self, rows: Vec<StageRow>) {
        self.rows = rows;
        self.made = None;
    }

    pub fn row(&self, index: usize) -> Option<&StageRow> {
        self.rows.get(index)
    }

    /// The row of the stage with `number`.
    pub fn row_of(&self, number: u32) -> Option<&StageRow> {
        self.rows.iter().find(|r| r.number == number)
    }

    /// Opens the browser with `selected` (a catalog position) highlighted: on the favourites when it is one, else
    /// on all stages. `collection` opens it on that instead.
    pub fn open_on(&mut self, selected: usize, favourites: &[u32], collection: Option<Collection>) {
        let favourite = self.rows.get(selected).is_some_and(|r| favourites.contains(&r.number));
        self.collection = collection.unwrap_or(if favourite { Collection::Favourites } else { Collection::All });
        self.filter = Filter::default();
        self.sort = Sort::Name;
        self.cursor = selected;
        self.open = true;
        self.reveal = Some(true);
        self.made = None;
        self.focus = true;
    }

    /// The list, made again when what it depends on changed; the cursor moves to a stage in it when it is not.
    fn refresh(&mut self, favourites: &[u32], recent: &[u32]) {
        let made = (self.collection.clone(), self.filter.clone(), self.sort, favourites.to_vec(), recent.to_vec());
        if self.made.as_ref() == Some(&made) {
            return;
        }
        let changed = self.made.as_ref().is_none_or(|m| m.0 != made.0 || m.1 != made.1 || m.2 != made.2);
        self.lines = lines(&self.rows, &made.0, &made.1, made.2, favourites, recent);
        self.made = Some(made);
        if changed {
            if self.position().is_none() {
                self.cursor = self
                    .lines
                    .iter()
                    .find_map(|l| if let Line::Row(k) = l { Some(*k) } else { None })
                    .unwrap_or(self.cursor);
                self.to_top = true;
            } else {
                self.reveal.get_or_insert(true);
            }
        }
    }

    /// The cursor's line.
    fn position(&self) -> Option<usize> {
        self.lines.iter().position(|l| *l == Line::Row(self.cursor))
    }

    /// Moves the cursor `by` stages down (negative: up) the list.
    fn step(&mut self, by: isize) {
        let rows: Vec<usize> =
            self.lines.iter().filter_map(|l| if let Line::Row(k) = l { Some(*k) } else { None }).collect();
        if rows.is_empty() {
            return;
        }
        let at = rows.iter().position(|&k| k == self.cursor);
        let next = match at {
            Some(at) => (at as isize + by).clamp(0, rows.len() as isize - 1) as usize,
            None => 0,
        };
        self.cursor = rows[next];
        self.reveal = Some(false);
    }

    /// Draws the browser in `ui`; what the player asked for.
    pub fn ui(&mut self, ui: &mut egui::Ui, mut env: Env) -> Option<Action> {
        self.refresh(env.favourites, env.recent);
        let mut action = None;
        // The arrow keys, Enter and Escape belong to the list, not to the search box or the Start button.
        let (down, up, page_down, page_up, enter, escape, favourite) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::PageDown),
                i.consume_key(Modifiers::NONE, Key::PageUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::COMMAND, Key::D),
            )
        });
        for (pressed, by) in [(down, 1), (up, -1), (page_down, PAGE as isize), (page_up, -(PAGE as isize))] {
            if pressed {
                self.step(by);
            }
        }
        let cursor_row = self.rows.get(self.cursor).cloned();
        if let Some(row) = &cursor_row {
            if enter && row.startable() {
                action = Some(Action::Choose(row.index));
            }
            if favourite {
                action = Some(Action::Favourite(row.number));
            }
        }
        if escape {
            action = Some(Action::Back);
        }

        // The top line: back, title, search, random.
        let search_id = Id::new(SEARCH);
        ui.horizontal(|ui| {
            let back = egui::Button::new(RichText::new("⏴ Drive").size(14.5)).min_size(egui::vec2(0.0, 30.0));
            if ui.add(back).on_hover_text("Back without changing the stage (Esc)").clicked() {
                action = Some(Action::Back);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Choose a stage").size(20.0).strong());
            ui.add_space(18.0);
            // Typing anywhere searches.
            let typing = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
            let focus = std::mem::take(&mut self.focus);
            if (focus || typing) && ui.memory(|m| m.focused()).is_none_or(|f| f != search_id) {
                ui.memory_mut(|m| m.request_focus(search_id));
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.query)
                    .id(search_id)
                    .hint_text("Search all stages: name, country, author, map number")
                    .desired_width(470.0)
                    .min_size(egui::vec2(0.0, 30.0))
                    .vertical_align(egui::Align::Center),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let random = egui::Button::new(RichText::new("🎲 Random").size(14.5)).min_size(egui::vec2(0.0, 30.0));
                if ui.add(random).on_hover_text("A random stage of the list as filtered").clicked() {
                    self.random();
                }
            });
        });
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(SIDEBAR_WIDTH);
                // The countries of a full install do not fit the window's height.
                egui::ScrollArea::vertical().id_salt("collections").auto_shrink([false, false]).show(ui, |ui| {
                    self.sidebar(ui, env.favourites, env.recent);
                });
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let chosen = ui.vertical(|ui| self.list(ui, env.favourites, env.recent)).inner;
            action = chosen.or(action);
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let chosen = ui.vertical(|ui| self.detail(ui, &mut env)).inner;
            action = chosen.or(action);
        });
        action
    }

    /// A random stage of the list as filtered becomes the highlighted one.
    fn random(&mut self) {
        let rows: Vec<usize> = self
            .lines
            .iter()
            .filter_map(|l| if let Line::Row(k) = l { Some(*k) } else { None })
            .filter(|&k| self.rows[k].startable())
            .collect();
        if rows.is_empty() {
            return;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        self.cursor = rows[nanos as usize % rows.len()];
        self.reveal = Some(true);
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, favourites: &[u32], recent: &[u32]) {
        let all = collections(&self.rows, favourites, recent);
        for (n, (collection, name, count)) in all.iter().enumerate() {
            // The headers: where the countries start, and the stages that cannot start.
            if n == 3 {
                ui.add_space(10.0);
                ui.label(RichText::new("COUNTRIES").size(11.5).color(GREY).strong());
                ui.add_space(2.0);
            } else if *collection == Collection::CannotStart {
                ui.add_space(10.0);
            } else if n == 0 {
                ui.add_space(6.0);
            }
            let icon = match collection {
                Collection::Favourites => "★",
                Collection::Recent | Collection::All | Collection::Country(_) => "",
                Collection::CannotStart => "⚠",
            };
            let grey = *collection == Collection::CannotStart;
            if side_row(ui, icon, name, *count, self.collection == *collection, grey).clicked() {
                self.collection = collection.clone();
                self.filter.query.clear();
            }
        }
    }

    /// The filters, sort, the list and the keys; a stage chosen or starred with the mouse.
    fn list(&mut self, ui: &mut egui::Ui, favourites: &[u32], recent: &[u32]) -> Option<Action> {
        let mut action = None;
        ui.set_width(LIST_WIDTH);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Surface").color(GREY));
            ui.selectable_value(&mut self.filter.surface, None, "Any");
            for (s, name) in SURFACES.into_iter().enumerate() {
                ui.selectable_value(&mut self.filter.surface, Some(s as u32), name);
            }
            ui.add_space(10.0);
            ui.label(RichText::new("Length").color(GREY));
            egui::ComboBox::from_id_salt("length").width(80.0).selected_text(self.filter.length.label()).show_ui(
                ui,
                |ui| {
                    for length in Length::ALL {
                        ui.selectable_value(&mut self.filter.length, length, length.label());
                    }
                },
            );
        });
        let count = self.lines.iter().filter(|l| matches!(l, Line::Row(_))).count();
        ui.horizontal(|ui| {
            let what = if self.filter.query.trim().is_empty() {
                match &self.collection {
                    Collection::Favourites => "favourites".to_owned(),
                    Collection::Recent => "recent".to_owned(),
                    Collection::All => "stages".to_owned(),
                    Collection::Country(code) => {
                        format!("stages in {}", code.as_deref().map_or("No country", country_name))
                    }
                    Collection::CannotStart => "stages that cannot start".to_owned(),
                }
            } else {
                "found in all stages".to_owned()
            };
            ui.label(RichText::new(format!("{count} {what}")).color(GREY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort").width(120.0).selected_text(self.sort.label()).show_ui(ui, |ui| {
                    for sort in Sort::ALL {
                        ui.selectable_value(&mut self.sort, sort, sort.label());
                    }
                });
                ui.label(RichText::new("Sort").color(GREY));
            });
        });
        ui.add_space(4.0);
        let by_country = !matches!(self.collection, Collection::Country(_)) || !self.filter.query.trim().is_empty();
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());
        let c = columns(rect.left());
        let small = FontId::proportional(11.5);
        let y = rect.center().y;
        let p = ui.painter();
        p.text(Pos2::new(c[1], y), Align2::LEFT_CENTER, "STAGE", small.clone(), GREY);
        let third = if by_country { "COUNTRY" } else { "AUTHOR" };
        p.text(Pos2::new(c[2], y), Align2::LEFT_CENTER, third, small.clone(), GREY);
        p.text(Pos2::new(c[3], y), Align2::RIGHT_CENTER, "LENGTH", small.clone(), GREY);
        p.text(Pos2::new(c[4], y), Align2::LEFT_CENTER, "SURFACE", small, GREY);
        p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, Color32::from_gray(60)));
        ui.add_space(2.0);

        if count == 0 {
            let hint = match (&self.collection, self.filter.query.trim().is_empty()) {
                (Collection::Favourites, true) if favourites.is_empty() => "Click ☆ next to a stage to keep it here.",
                (Collection::Recent, true) if recent.is_empty() => "The stages you choose are listed here.",
                _ => "Nothing matches.",
            };
            ui.add_space(10.0);
            ui.label(RichText::new(hint).color(GREY));
        }
        // The keys line sits below the list; it keeps its room.
        let keys_room = 34.0;
        let height = (ui.available_height() - keys_room).max(ROW * 3.0);
        let mut area = egui::ScrollArea::vertical().id_salt("stages").max_height(height).auto_shrink([false, true]);
        let position = self.position();
        if std::mem::take(&mut self.to_top) {
            area = area.vertical_scroll_offset(0.0);
        } else if let (Some(centre), Some(at)) = (self.reveal.take(), position) {
            let (offset, visible) = self.scroll;
            let top = at as f32 * ROW;
            let want = if centre {
                (top - visible * 0.4).max(0.0)
            } else if top < offset {
                top
            } else if top + ROW > offset + visible {
                top + ROW - visible
            } else {
                offset
            };
            area = area.vertical_scroll_offset(want);
        }
        let mut hovered = None;
        let moving = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
        let output = ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            area.show_rows(ui, ROW, self.lines.len(), |ui, range| {
                for at in range {
                    let k = match &self.lines[at] {
                        Line::Header(name) => {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), Sense::hover());
                            let font = FontId::proportional(12.5);
                            let at = rect.left_bottom() + egui::vec2(8.0, -5.0);
                            ui.painter().text(at, Align2::LEFT_BOTTOM, name.to_uppercase(), font, GREY);
                            continue;
                        }
                        Line::Row(k) => *k,
                    };
                    let row = &self.rows[k];
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), Sense::click());
                    let star_rect =
                        Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), egui::vec2(28.0, ROW));
                    let star = ui.interact(star_rect, ui.id().with(("star", k)), Sense::click());
                    let favourite = favourites.contains(&row.number);
                    draw_row(ui, rect, row, by_country, k == self.cursor, response.hovered(), favourite);
                    let response = match &row.problem {
                        Some(problem) => response.on_hover_text(format!("cannot start: {problem}")),
                        None => response,
                    };
                    if response.hovered() && moving {
                        hovered = Some(k);
                    }
                    if star.clicked() {
                        action = Some(Action::Favourite(row.number));
                    } else if response.double_clicked() && row.startable() {
                        action = Some(Action::Choose(row.index));
                    } else if response.clicked() {
                        self.cursor = k;
                    }
                }
            })
        });
        let output = output.inner;
        self.scroll = (output.state.offset.y, output.inner_rect.height());
        if let Some(k) = hovered {
            self.cursor = k;
        }
        ui.add_space(10.0);
        let keys = "⏶⏷ move    Enter choose    Ctrl+D favourite    Esc back";
        ui.label(RichText::new(keys).size(12.0).color(DIM));
        action
    }

    /// The highlighted stage: name, picture, facts, and the buttons.
    fn detail(&mut self, ui: &mut egui::Ui, env: &mut Env) -> Option<Action> {
        let row = self.rows.get(self.cursor)?;
        let map = env.maps.get(row.index)?;
        let mut action = None;
        ui.set_width(preview::WIDTH);
        ui.add_space(6.0);
        ui.label(RichText::new(&row.name).size(22.0).strong());
        let origin = if row.retail { "RBR 1.02" } else { "RallySimFans" };
        let mut subtitle = vec![row.country_name().to_owned(), origin.to_owned()];
        subtitle.extend(row.author.clone());
        ui.label(RichText::new(subtitle.join(" · ")).color(GREY));
        ui.add_space(6.0);
        env.previews.stage_ui(ui, map);
        ui.add_space(6.0);
        egui::Grid::new("facts").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            for (label, value) in (env.facts)(row) {
                ui.label(RichText::new(label).color(GREY));
                ui.label(value);
                ui.end_row();
            }
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let favourite = env.favourites.contains(&row.number);
            let label = if favourite {
                RichText::new("★ Favourite").size(15.0).color(STAR)
            } else {
                RichText::new("☆ Add to favourites").size(15.0)
            };
            let button = egui::Button::new(label).fill(Color32::from_gray(48)).min_size(egui::vec2(0.0, 44.0));
            if ui.add(button).on_hover_text("Ctrl/Cmd+D").clicked() {
                action = Some(Action::Favourite(row.number));
            }
            let go = RichText::new("Choose stage").size(18.0).strong().color(Color32::WHITE);
            let go = egui::Button::new(go).fill(ACCENT).min_size(egui::vec2(ui.available_width(), 44.0));
            let go = ui.add_enabled(row.startable(), go).on_hover_text("Enter, or a double click on the stage");
            if go.clicked() {
                action = Some(Action::Choose(row.index));
            }
        });
        action
    }
}

/// The list's columns: the star, the name, the third (country or author), the length (its right edge) and the surface.
fn columns(left: f32) -> [f32; 5] {
    [left + 6.0, left + 30.0, left + 206.0, left + 366.0, left + 380.0]
}

/// The room the name has before the third column.
const NAME_WIDTH: f32 = 170.0;

/// A stage's row: its star, name, country or author, length and surface; the highlighted one filled.
fn draw_row(ui: &egui::Ui, rect: Rect, row: &StageRow, by_country: bool, cursor: bool, hovered: bool, favourite: bool) {
    let p = ui.painter();
    let c = columns(rect.left());
    let y = rect.center().y;
    if cursor {
        p.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.55));
        p.rect_filled(
            Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            1.0,
            ui.visuals().selection.bg_fill,
        );
    } else if hovered {
        p.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let text = if row.startable() { ui.visuals().strong_text_color() } else { DIM };
    let (star, colour) = match (favourite, hovered || cursor) {
        (true, _) => ("★", STAR),
        (false, true) => ("☆", Color32::from_gray(170)),
        (false, false) => ("☆", Color32::from_gray(70)),
    };
    p.text(Pos2::new(c[0] + 6.0, y), Align2::CENTER_CENTER, star, FontId::proportional(16.0), colour);
    let name = p.layout_no_wrap(row.name.clone(), FontId::proportional(15.0), text);
    let room = Rect::from_min_max(Pos2::new(c[1], rect.top()), Pos2::new(c[1] + NAME_WIDTH, rect.bottom()));
    p.with_clip_rect(room).galley(Pos2::new(c[1], y - name.size().y / 2.0), name, text);
    let third = if by_country { row.country_name() } else { row.author.as_deref().unwrap_or("") };
    let room = Rect::from_min_max(Pos2::new(c[2], rect.top()), Pos2::new(c[3] - 56.0, rect.bottom()));
    let third_colour = if row.startable() { GREY } else { DIM };
    let galley = p.layout_no_wrap(third.to_owned(), FontId::proportional(13.0), third_colour);
    p.with_clip_rect(room).galley(Pos2::new(c[2], y - galley.size().y / 2.0), galley, third_colour);
    let km = row.km.map_or(String::new(), |km| format!("{km:.1} km"));
    p.text(Pos2::new(c[3], y), Align2::RIGHT_CENTER, km, FontId::proportional(13.0), GREY);
    match &row.problem {
        Some(problem) => {
            let room = Rect::from_min_max(Pos2::new(c[4], rect.top()), rect.right_bottom());
            let galley = p.layout_no_wrap(problem.clone(), FontId::proportional(12.0), DIM);
            p.with_clip_rect(room).galley(Pos2::new(c[4], y - galley.size().y / 2.0), galley, DIM);
        }
        None => {
            p.circle_filled(Pos2::new(c[4] + 4.0, y), 4.0, SURFACE_DOTS[row.surface as usize]);
            let font = FontId::proportional(13.0);
            p.text(Pos2::new(c[4] + 14.0, y), Align2::LEFT_CENTER, SURFACES[row.surface as usize], font, GREY);
        }
    }
}

/// A sidebar line: an icon, a name and a count; the selected one filled.
fn side_row(ui: &mut egui::Ui, icon: &str, name: &str, count: usize, selected: bool, grey: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let text = if grey {
        GREY
    } else if selected {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
    let icon_colour = if icon == "★" { STAR } else { text };
    let left = rect.left_center();
    painter.text(left + egui::vec2(8.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(14.0), icon_colour);
    painter.text(left + egui::vec2(28.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(14.5), text);
    let count_colour = if selected { Color32::from_gray(210) } else { GREY };
    let right = rect.right_center() - egui::vec2(8.0, 0.0);
    painter.text(right, Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.5), count_colour);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Stage;
    use rbr_formats::rsf;
    use std::time::{Duration, Instant};

    const COUNTRIES: [&str; 8] = ["FI", "CZ", "FR", "PL", "SK", "ES", "GB", "DE"];

    /// A made-up RallySimFans stage, encrypted 4 in 11 (as about 160 of a full install's 450); no third-party data.
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

    fn rows_of(maps: &[Map]) -> Vec<StageRow> {
        let problem = |m: &Map| m.rsf.as_ref().and_then(|s| s.problem.clone());
        maps.iter().enumerate().map(|(i, m)| StageRow::new(i, m, problem(m))).collect()
    }

    /// The names of `lines`, headers in brackets.
    fn names(rows: &[StageRow], lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Header(h) => format!("[{h}]"),
                Line::Row(k) => rows[*k].name.clone(),
            })
            .collect()
    }

    fn count(lines: &[Line]) -> usize {
        lines.iter().filter(|l| matches!(l, Line::Row(_))).count()
    }

    /// Tanner (retail 54, Japan, gravel, 3.9 km) among the synthetic stages 1 (Czechia, tarmac, blocked), 3 (Poland,
    /// gravel, blocked), 5 (Spain, snow, 7 km), 8 (Finland, snow, 10 km) and 10 (France, tarmac, 12 km).
    fn some() -> Vec<StageRow> {
        let retail = Map { number: 54, stages: Vec::new(), rsf: None };
        rows_of(&[retail, rsf_map(5), rsf_map(1), rsf_map(8), rsf_map(3), rsf_map(10)])
    }

    #[test]
    fn a_retail_stage_has_its_rallys_country_and_the_game_length() {
        let rows = some();
        assert_eq!(rows[0].country.as_deref(), Some("JP"));
        assert_eq!(rows[0].detail(), "54 · 3.9 km · Gravel");
        assert_eq!(rows[0].country_name(), "Japan");
        assert!(rows[0].retail && rows[0].startable());
        assert_eq!(rows[1].country_name(), "Spain");
        assert_eq!(country_name("ZZ"), "ZZ");
    }

    #[test]
    fn all_stages_lists_the_startable_ones_and_the_blocked_ones_have_their_own_collection() {
        let rows = some();
        let all = lines(&rows, &Collection::All, &Filter::default(), Sort::Name, &[], &[]);
        assert_eq!(names(&rows, &all), ["Synthetic 10 Pass", "Synthetic 5 Hills", "Synthetic 8 Forest", "Tanner"]);
        let blocked = lines(&rows, &Collection::CannotStart, &Filter::default(), Sort::Name, &[], &[]);
        assert_eq!(names(&rows, &blocked), ["Synthetic 1 Hills", "Synthetic 3 Valley"]);
        let reasons: Vec<_> = rows.iter().filter_map(|r| r.problem.clone()).collect();
        assert!(reasons.iter().all(|r| r == "encrypted by RallySimFans: ReRBR can't read it"));
        let by_length = lines(&rows, &Collection::All, &Filter::default(), Sort::Length, &[], &[]);
        assert_eq!(
            names(&rows, &by_length),
            ["Tanner", "Synthetic 5 Hills", "Synthetic 8 Forest", "Synthetic 10 Pass"]
        );
    }

    #[test]
    fn a_country_has_the_retail_and_the_rallysimfans_stages_together() {
        let mut maps = vec![Map { number: 54, stages: Vec::new(), rsf: None }, rsf_map(5)];
        maps[1].rsf.as_mut().unwrap().country = Some("jp".into());
        let rows = rows_of(&maps);
        let japan = lines(&rows, &Collection::Country(Some("JP".into())), &Filter::default(), Sort::Name, &[], &[]);
        assert_eq!(names(&rows, &japan), ["Synthetic 5 Hills", "Tanner"]);
        let sidebar = collections(&rows, &[], &[]);
        let japan = sidebar.iter().find(|(c, _, _)| *c == Collection::Country(Some("JP".into()))).unwrap();
        assert_eq!((japan.1.as_str(), japan.2), ("Japan", 2));
    }

    #[test]
    fn favourites_sort_like_any_list_and_keep_the_ones_that_cannot_start_last() {
        let rows = some();
        // Starred in the order 3 (blocked), 8, 54, and 999 (not in the catalog).
        let starred = [103, 108, 54, 999];
        let by_name = lines(&rows, &Collection::Favourites, &Filter::default(), Sort::Name, &starred, &[]);
        assert_eq!(names(&rows, &by_name), ["Synthetic 8 Forest", "Tanner", "[Cannot start]", "Synthetic 3 Valley"]);
        let snow = Filter { surface: Some(2), ..Filter::default() };
        let snowy = lines(&rows, &Collection::Favourites, &snow, Sort::Name, &starred, &[]);
        assert_eq!(names(&rows, &snowy), ["Synthetic 8 Forest"]);
        assert_eq!(collections(&rows, &starred, &[])[0].2, 3);
        // No favourites: an empty list.
        assert!(lines(&rows, &Collection::Favourites, &Filter::default(), Sort::Name, &[], &[]).is_empty());
    }

    #[test]
    fn the_recent_picks_keep_their_order_of_use() {
        let rows = some();
        let recent = [108, 54, 105];
        let list = lines(&rows, &Collection::Recent, &Filter::default(), Sort::Name, &[], &recent);
        assert_eq!(names(&rows, &list), ["Synthetic 8 Forest", "Tanner", "Synthetic 5 Hills"]);
        // Sorting by recently driven puts the others after them.
        let all = lines(&rows, &Collection::All, &Filter::default(), Sort::Recent, &[], &recent);
        assert_eq!(names(&rows, &all), ["Synthetic 8 Forest", "Tanner", "Synthetic 5 Hills", "Synthetic 10 Pass"]);
    }

    #[test]
    fn a_search_covers_every_stage_grouped_by_country_with_the_favourites_first() {
        let rows = some();
        let find = |query: &str, collection: Collection, favourites: &[u32]| {
            let filter = Filter { query: query.into(), ..Filter::default() };
            names(&rows, &lines(&rows, &collection, &filter, Sort::Name, favourites, &[]))
        };
        // Whatever the collection is: the country's name and code, the author, the number, retail.
        assert_eq!(find("japan", Collection::CannotStart, &[]), ["[Japan]", "Tanner"]);
        assert_eq!(find("jp", Collection::Favourites, &[]), ["[Japan]", "Tanner"]);
        assert_eq!(find("author5", Collection::All, &[]), ["[Spain]", "Synthetic 5 Hills"]);
        assert_eq!(find("108", Collection::All, &[]), ["[Finland]", "Synthetic 8 Forest"]);
        assert_eq!(find("retail", Collection::All, &[]), ["[Japan]", "Tanner"]);
        assert_eq!(
            find("SYNTHETIC hills", Collection::All, &[]),
            ["[Spain]", "Synthetic 5 Hills", "[Cannot start]", "Synthetic 1 Hills"]
        );
        assert_eq!(
            find("synthetic", Collection::All, &[108]),
            [
                "[Favourites]",
                "Synthetic 8 Forest",
                "[France]",
                "Synthetic 10 Pass",
                "[Spain]",
                "Synthetic 5 Hills",
                "[Cannot start]",
                "Synthetic 1 Hills",
                "Synthetic 3 Valley"
            ]
        );
        assert!(find("nothing", Collection::All, &[]).is_empty());
    }

    #[test]
    fn the_surface_and_length_filters_narrow_every_list() {
        let rows = some();
        let list = |filter: Filter| count(&lines(&rows, &Collection::All, &filter, Sort::Name, &[], &[]));
        assert_eq!(list(Filter { surface: Some(1), ..Filter::default() }), 1);
        assert_eq!(list(Filter { surface: Some(2), ..Filter::default() }), 2);
        assert_eq!(list(Filter { length: Length::Short, ..Filter::default() }), 1);
        assert_eq!(list(Filter { length: Length::Medium, ..Filter::default() }), 1);
        assert_eq!(list(Filter { length: Length::Long, ..Filter::default() }), 2);
        assert_eq!(list(Filter { length: Length::Longest, ..Filter::default() }), 0);
        assert!(Length::Short.contains(Some(3.9)) && !Length::Short.contains(None) && Length::Any.contains(None));
    }

    /// Draws the browser open in a window-sized frame, without a GPU; the time it took.
    fn frame(ctx: &egui::Context, browser: &mut StageBrowser, maps: &[Map], previews: &mut Previews) -> Duration {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 700.0))),
            ..Default::default()
        };
        let facts = |_: &StageRow| vec![("Weather", "dry".to_owned())];
        let start = Instant::now();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let env =
                    Env { maps, favourites: &[105, 110], recent: &[120], previews: &mut *previews, facts: &facts };
                browser.ui(ui, env)
            });
        });
        start.elapsed()
    }

    /// The scale of a full RallySimFans install, and more: 500 stages open, list and filter within a frame.
    #[test]
    fn a_big_catalog_opens_and_filters_at_once() {
        let maps: Vec<Map> = (0..500).map(rsf_map).collect();
        let start = Instant::now();
        let rows = rows_of(&maps);
        let built = start.elapsed();
        let mut browser = StageBrowser::default();
        browser.set_rows(rows);
        let mut previews = Previews::new(rbr_formats::data::Data::new("/nonexistent", None));
        let ctx = egui::Context::default();
        browser.open_on(250, &[105, 110], None);
        let mut slowest = Duration::ZERO;
        for query in ["", "s", "sy", "syn", "synthetic 4", "synthetic 4 cz", "zzz", ""] {
            browser.filter.query = query.into();
            slowest = slowest.max(frame(&ctx, &mut browser, &maps, &mut previews));
        }
        browser.filter = Filter { surface: Some(2), length: Length::Long, ..Filter::default() };
        slowest = slowest.max(frame(&ctx, &mut browser, &maps, &mut previews));
        eprintln!("stage browser: rows built in {built:?}, slowest frame {slowest:?}");
        // The snowy 10-15 km stages that start.
        let snowy = (0..500).filter(|k| k % 3 == 2 && (8..13).contains(&(k % 25)) && k % 11 >= 4).count();
        assert_eq!(count(&browser.lines), snowy);
        assert!(browser.open);
        // Debug builds too: well within one 60 Hz frame for the rows and a 20 fps frame for a draw.
        assert!(built < Duration::from_millis(16), "{built:?}");
        assert!(slowest < Duration::from_millis(50), "{slowest:?}");
    }

    #[test]
    fn it_opens_on_the_favourites_with_the_selection_and_moves_over_the_stages_only() {
        let mut browser = StageBrowser::default();
        browser.set_rows(some());
        browser.open_on(0, &[54], None);
        assert_eq!(browser.collection, Collection::Favourites);
        browser.open_on(3, &[54], None);
        assert_eq!(browser.collection, Collection::All);
        assert_eq!(browser.cursor, 3);
        // All: Synthetic 10 (position 5 in the rows), Synthetic 5 (1), Synthetic 8 (3), Tanner (0).
        browser.refresh(&[54], &[]);
        browser.step(1);
        assert_eq!(browser.cursor, 0);
        browser.step(1);
        assert_eq!(browser.cursor, 0, "stays on the last one");
        browser.step(-10);
        assert_eq!(browser.cursor, 5);
        // A collection without the cursor's stage moves it to the first stage of the list.
        browser.collection = Collection::CannotStart;
        browser.refresh(&[54], &[]);
        assert_eq!(browser.cursor, 2);
    }

    /// One frame with `key` pressed (with `modifiers`); what the browser asked for.
    fn press(browser: &mut StageBrowser, maps: &[Map], key: Key, modifiers: Modifiers) -> Option<Action> {
        let mut previews = Previews::new(rbr_formats::data::Data::new("/nonexistent", None));
        let event = egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 700.0))),
            events: vec![event],
            modifiers,
            ..Default::default()
        };
        let facts = |_: &StageRow| Vec::new();
        let mut asked = None;
        let _ = egui::Context::default().run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let env = Env { maps, favourites: &[], recent: &[], previews: &mut previews, facts: &facts };
                // A second pass of the frame (egui's layout) has the key consumed already.
                asked = browser.ui(ui, env).or(asked);
            });
        });
        asked
    }

    #[test]
    fn the_keys_choose_favourite_and_go_back() {
        let maps = [
            Map { number: 54, stages: Vec::new(), rsf: None },
            rsf_map(5),
            rsf_map(1), // cannot start
        ];
        let mut browser = StageBrowser::default();
        browser.set_rows(rows_of(&maps));
        browser.open_on(1, &[], None);
        assert_eq!(press(&mut browser, &maps, Key::Enter, Modifiers::NONE), Some(Action::Choose(1)));
        assert_eq!(press(&mut browser, &maps, Key::D, Modifiers::COMMAND), Some(Action::Favourite(105)));
        assert_eq!(press(&mut browser, &maps, Key::Escape, Modifiers::NONE), Some(Action::Back));
        // The arrow moves the highlight to the next stage of the list (Tanner after Synthetic 5), which Enter chooses.
        assert_eq!(press(&mut browser, &maps, Key::ArrowDown, Modifiers::NONE), None);
        assert_eq!(press(&mut browser, &maps, Key::Enter, Modifiers::NONE), Some(Action::Choose(0)));
        // A stage that cannot start is not chosen.
        browser.collection = Collection::CannotStart;
        assert_eq!(press(&mut browser, &maps, Key::Enter, Modifiers::NONE), None);
        assert_eq!(browser.cursor, 2);
    }

    /// `RBR_LAUNCHER_SHOTS=DIR`: the big catalog's browser drawn to `DIR/stage-browser-*.png`.
    #[test]
    fn big_catalog_screenshots() {
        let Some(dir) = std::env::var_os("RBR_LAUNCHER_SHOTS").map(std::path::PathBuf::from) else {
            return;
        };
        let mut maps: Vec<Map> = (0..500).map(rsf_map).collect();
        maps.insert(0, Map { number: 54, stages: Vec::new(), rsf: None });
        let favourites = [54, 105, 110, 128];
        let mut previews = Previews::new(rbr_formats::data::Data::new("/nonexistent", None));
        for (name, collection) in [("favourites", Collection::Favourites), ("all", Collection::All)] {
            let mut browser = StageBrowser::default();
            browser.set_rows(rows_of(&maps));
            browser.open_on(1, &favourites, Some(collection));
            let facts = |_: &StageRow| vec![("Weather", "good dry · bad wet".to_owned()), ("Ghost", "none yet".into())];
            let rgba = crate::capture::draw(|ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let env = Env {
                        maps: &maps,
                        favourites: &favourites,
                        recent: &[110],
                        previews: &mut previews,
                        facts: &facts,
                    };
                    browser.ui(ui, env)
                });
            })
            .unwrap();
            crate::capture::write_png(&dir.join(format!("stage-browser-{name}.png")), &rgba).unwrap();
        }
    }
}
