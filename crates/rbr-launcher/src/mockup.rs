//! Throwaway: GUI mockups of the proposed stage and car browsers (docs/proposals/track-picker.md). Not for merging
//! into the launcher.
//!
//! `MOCKUP_OUT=DIR MOCKUP_CAPTURES=DIR cargo test -p rbr-launcher --bin rerbr mockup` where the captures folder holds
//! `rerbr --screenshot` folders `map52`, `map11` (MG ZR on Sipirkakim and on Mustaselka), `xsara` and `escort` (the
//! Citroen Xsara Kit Car and the Ford Escort Mk II RS Grp4 on Mustaselka). Writes `mockup-*.png`; they show the
//! game's pictures, so they stay out of git.

use std::path::Path;

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};

use crate::catalog;
use crate::pages::ACCENT;

const GREY: Color32 = Color32::from_gray(130);
const DIM: Color32 = Color32::from_gray(95);
const STAR: Color32 = Color32::from_rgb(240, 190, 60);
const BG: Color32 = Color32::from_rgb(27, 27, 27);
const SURFACES: [&str; 3] = ["Gravel", "Tarmac", "Snow"];

/// One row of a browser's list, and what its detail panel shows.
#[derive(Clone)]
struct Item {
    fav: bool,
    name: String,
    /// The third column, the fourth (right-aligned), the fifth (a dot and a word).
    third: String,
    fourth: String,
    fifth: Option<(Color32, String)>,
    /// Why it cannot start.
    problem: Option<&'static str>,
    /// Under the name in the detail panel.
    subtitle: String,
    chips: Vec<(Option<Color32>, String)>,
    facts: Vec<(&'static str, String)>,
}

fn surface_colour(s: u32) -> Color32 {
    [Color32::from_rgb(190, 145, 95), Color32::from_rgb(150, 155, 170), Color32::from_rgb(185, 215, 250)][s as usize]
}

fn rally(map: u32) -> &'static str {
    match map / 10 {
        1 => "Finland",
        2 => "Great Britain",
        3 => "Australia",
        4 => "France",
        5 => "Japan",
        _ => "USA",
    }
}

/// A stage row; `country_column` puts the country in the third column, else the author.
fn stage(
    name: &str,
    country: &str,
    km: f32,
    surface: u32,
    author: Option<&str>,
    fav: bool,
    country_column: bool,
) -> Item {
    let origin = if author.is_some() { "RSF" } else { "RBR 1.02" };
    let author = author.unwrap_or("Warthog");
    let surface_name = SURFACES[surface as usize].to_owned();
    Item {
        fav,
        name: name.into(),
        third: if country_column { country.into() } else { author.into() },
        fourth: format!("{km:.1} km"),
        fifth: Some((surface_colour(surface), surface_name.clone())),
        problem: None,
        subtitle: format!("{country} · {origin} · {author}"),
        chips: vec![(None, format!("{km:.1} km")), (Some(surface_colour(surface)), surface_name)],
        facts: vec![("Weather", "good · damp · bad".into())],
    }
}

fn retail(map: u32, fav: bool, country_column: bool) -> Item {
    let km = crate::preview::game_length(map).unwrap_or(0.0) / 1000.0;
    let name = catalog::title(map).unwrap();
    stage(name, rally(map), km, catalog::surface_kind(map), None, fav, country_column)
}

/// Made-up community stages (no third-party names).
fn rsf(name: &str, country: &str, km: f32, surface: u32, author: &str, fav: bool, country_column: bool) -> Item {
    stage(name, country, km, surface, Some(author), fav, country_column)
}

/// A car row: `class_column` puts the class in the third column, else where it comes from.
fn car(name: &str, class: &str, source: &str, model: &str, fav: bool, class_column: bool) -> Item {
    let own = model == "own model";
    Item {
        fav,
        name: name.into(),
        third: if class_column { class.into() } else { source.into() },
        fourth: String::new(),
        fifth: Some((
            if own { Color32::from_rgb(110, 190, 120) } else { Color32::from_rgb(220, 170, 70) },
            model.into(),
        )),
        problem: None,
        subtitle: format!("{class} · {source}"),
        chips: vec![(None, class.into()), (None, model.into())],
        facts: vec![("Setups", "2 saved · default".into())],
    }
}

fn load(path: &Path) -> egui::ColorImage {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &buf[..info.buffer_size()])
}

fn crop(image: &egui::ColorImage, rect: [usize; 4]) -> egui::ColorImage {
    let [x, y, w, h] = rect;
    let mut pixels = Vec::with_capacity(w * h);
    for row in y..y + h {
        pixels.extend_from_slice(&image.pixels[row * image.size[0] + x..row * image.size[0] + x + w]);
    }
    egui::ColorImage { size: [w, h], source_size: Vec2::new(w as f32, h as f32), pixels }
}

fn tabs(ctx: &egui::Context) {
    ctx.root_ui(|root| {
        egui::Panel::top("tabs").show(root, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ReRBR").size(20.0).strong());
                ui.add_space(20.0);
                for (i, name) in ["Drive", "Championship", "Car setup", "Controls", "Graphics & sound", "Settings"]
                    .into_iter()
                    .enumerate()
                {
                    let tab = egui::Button::selectable(i == 0, RichText::new(name).size(15.5));
                    ui.add(tab.min_size(egui::vec2(0.0, 28.0)));
                }
            });
            ui.add_space(4.0);
        })
    });
    ctx.root_ui(|root| {
        egui::Panel::bottom("status").show(root, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(" ").size(14.0));
            ui.add_space(4.0);
        })
    });
}

/// A sidebar line: an icon, a name and a count; the selected one filled.
fn side_row(ui: &mut egui::Ui, icon: &str, name: &str, count: usize, selected: bool, grey: bool) {
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
}

/// A sidebar section: its header and its lines (icon, name, count, grey).
type Section = (&'static str, Vec<(&'static str, &'static str, usize, bool)>);

struct Browser<'a> {
    /// "stage" or "car".
    noun: &'static str,
    search: &'static str,
    sections: Vec<Section>,
    /// The selected collection's name.
    view: &'static str,
    /// The filters' line, drawn above the list.
    filters: fn(&mut egui::Ui),
    /// The list's column names, from the third.
    columns: [&'static str; 3],
    items: Vec<Item>,
    selected: usize,
    hovered: Option<usize>,
    picture: &'a TextureHandle,
    /// Added under the sidebar: the car browser's Add cars….
    sidebar_button: Option<&'static str>,
}

fn browser(ctx: &egui::Context, b: &Browser) {
    tabs(ctx);
    ctx.root_ui(|root| {
        egui::CentralPanel::default().show(root, |ui| {
            // The top line: back, title, search, random.
            ui.horizontal(|ui| {
                ui.add(egui::Button::new(RichText::new("⏴ Drive").size(14.5)).min_size(egui::vec2(0.0, 30.0)));
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Choose a {}", b.noun)).size(20.0).strong());
                ui.add_space(18.0);
                let mut query = String::new();
                ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text(b.search)
                        .desired_width(if b.noun == "car" { 490.0 } else { 470.0 })
                        .min_size(egui::vec2(0.0, 30.0))
                        .align(egui::Align2::LEFT_CENTER),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Button::new(RichText::new("🎲 Random").size(14.5)).min_size(egui::vec2(0.0, 30.0)));
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(186.0);
                    for (header, rows) in &b.sections {
                        if !header.is_empty() {
                            ui.add_space(10.0);
                            ui.label(RichText::new(*header).size(11.5).color(GREY).strong());
                            ui.add_space(2.0);
                        } else {
                            ui.add_space(6.0);
                        }
                        for (icon, name, count, grey) in rows {
                            side_row(ui, icon, name, *count, *name == b.view, *grey);
                        }
                    }
                    if let Some(label) = b.sidebar_button {
                        ui.add_space(10.0);
                        ui.add(egui::Button::new(label).min_size(egui::vec2(ui.available_width(), 28.0)));
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                list(ui, b);
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                detail(ui, b);
            });
        })
    });
}

fn stage_filters(ui: &mut egui::Ui) {
    ui.label(RichText::new("Surface").color(GREY));
    let mut surface: Option<u32> = None;
    ui.selectable_value(&mut surface, None, "Any");
    for (s, name) in SURFACES.into_iter().enumerate() {
        ui.selectable_value(&mut surface, Some(s as u32), name);
    }
    ui.add_space(10.0);
    ui.label(RichText::new("Length").color(GREY));
    egui::ComboBox::from_id_salt("length").width(70.0).selected_text("Any").show_ui(ui, |_| {});
}

fn car_filters(ui: &mut egui::Ui) {
    ui.label(RichText::new("Drive").color(GREY));
    let mut drive = 0;
    for (k, name) in ["Any", "4WD", "FWD", "RWD"].into_iter().enumerate() {
        ui.selectable_value(&mut drive, k, name);
    }
    ui.add_space(10.0);
    let mut startable = true;
    ui.checkbox(&mut startable, "only cars that can start");
}

fn list(ui: &mut egui::Ui, b: &Browser) {
    ui.vertical(|ui| {
        ui.set_width(468.0);
        ui.add_space(4.0);
        ui.horizontal(|ui| (b.filters)(ui));
        ui.horizontal(|ui| {
            let what = match b.view {
                "Favourites" => "favourites".to_owned(),
                view if b.noun == "stage" => format!("stages in {view}"),
                view => format!("cars in {view}"),
            };
            ui.label(RichText::new(format!("{} {what}", b.items.len())).color(GREY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort").width(110.0).selected_text("Name A–Z").show_ui(ui, |_| {});
                ui.label(RichText::new("Sort").color(GREY));
            });
        });
        ui.add_space(4.0);
        let width = ui.available_width();
        // star, name, third, fourth (right edge), fifth
        let (cols, name_width): (fn(f32) -> [f32; 5], f32) = if b.noun == "car" {
            (|x| [x + 6.0, x + 30.0, x + 262.0, x + 356.0, x + 356.0], 228.0)
        } else {
            (|x| [x + 6.0, x + 30.0, x + 222.0, x + 380.0, x + 394.0], 188.0)
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());
        let c = cols(rect.left());
        let p = ui.painter();
        let small = FontId::proportional(11.5);
        let y = rect.center().y;
        let name_header = if b.noun == "car" { "CAR" } else { "STAGE" };
        p.text(Pos2::new(c[1], y), Align2::LEFT_CENTER, name_header, small.clone(), GREY);
        p.text(Pos2::new(c[2], y), Align2::LEFT_CENTER, b.columns[0], small.clone(), GREY);
        p.text(Pos2::new(c[3], y), Align2::RIGHT_CENTER, b.columns[1], small.clone(), GREY);
        p.text(Pos2::new(c[4], y), Align2::LEFT_CENTER, b.columns[2], small, GREY);
        p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, Color32::from_gray(60)));
        ui.add_space(2.0);
        for (i, item) in b.items.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::click());
            let p = ui.painter();
            let c = cols(rect.left());
            let y = rect.center().y;
            if i == b.selected {
                p.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.55));
                let bar = Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
                p.rect_filled(bar, 1.0, ui.visuals().selection.bg_fill);
            } else if Some(i) == b.hovered {
                p.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let text = if item.problem.is_some() { DIM } else { ui.visuals().strong_text_color() };
            let (star, star_colour) = match (item.fav, Some(i) == b.hovered) {
                (true, _) => ("★", STAR),
                (false, true) => ("☆", Color32::from_gray(170)),
                (false, false) => ("☆", Color32::from_gray(70)),
            };
            p.text(Pos2::new(c[0] + 6.0, y), Align2::CENTER_CENTER, star, FontId::proportional(16.0), star_colour);
            let name = p.layout_no_wrap(item.name.clone(), FontId::proportional(15.0), text);
            let room = Rect::from_min_max(Pos2::new(c[1], rect.top()), Pos2::new(c[1] + name_width, rect.bottom()));
            p.with_clip_rect(room).galley(Pos2::new(c[1], y - name.size().y / 2.0), name, text);
            let third_colour = if item.problem.is_some() { DIM } else { GREY };
            p.text(Pos2::new(c[2], y), Align2::LEFT_CENTER, &item.third, FontId::proportional(13.0), third_colour);
            p.text(Pos2::new(c[3], y), Align2::RIGHT_CENTER, &item.fourth, FontId::proportional(13.0), GREY);
            match (item.problem, &item.fifth) {
                (Some(problem), _) => {
                    p.text(Pos2::new(c[4], y), Align2::LEFT_CENTER, problem, FontId::proportional(13.0), DIM);
                }
                (None, Some((dot, word))) => {
                    p.circle_filled(Pos2::new(c[4] + 4.0, y), 4.0, *dot);
                    p.text(Pos2::new(c[4] + 14.0, y), Align2::LEFT_CENTER, word, FontId::proportional(13.0), GREY);
                }
                (None, None) => {}
            }
        }
        ui.add_space(10.0);
        let keys = "⏶⏷ move    Enter choose    Ctrl+D favourite    Esc back";
        ui.label(RichText::new(keys).size(12.0).color(DIM));
    });
}

fn detail(ui: &mut egui::Ui, b: &Browser) {
    let item = &b.items[b.selected];
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.add_space(6.0);
        let w = ui.available_width();
        let size = b.picture.size_vec2();
        ui.add(egui::Image::new((b.picture.id(), egui::vec2(w, w * size.y / size.x))).corner_radius(6.0));
        ui.add_space(8.0);
        ui.label(RichText::new(&item.name).size(22.0).strong());
        ui.label(RichText::new(&item.subtitle).color(GREY));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (dot, text) in &item.chips {
                let frame = egui::Frame::new()
                    .fill(Color32::from_gray(48))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(10, 3));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(dot) = dot {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, *dot);
                        }
                        ui.label(RichText::new(text).size(13.5));
                    });
                });
            }
        });
        ui.add_space(8.0);
        egui::Grid::new("facts").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            for (label, value) in &item.facts {
                ui.label(RichText::new(*label).color(GREY));
                ui.label(value);
                ui.end_row();
            }
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let label = if item.fav {
                RichText::new("★ Favourite").size(15.0).color(STAR)
            } else {
                RichText::new("☆ Add to favourites").size(15.0)
            };
            ui.add(egui::Button::new(label).fill(Color32::from_gray(48)).min_size(egui::vec2(0.0, 44.0)));
            let go = RichText::new(format!("Choose {}", b.noun)).size(18.0).strong().color(Color32::WHITE);
            ui.add(egui::Button::new(go).fill(ACCENT).min_size(egui::vec2(ui.available_width(), 44.0)));
        });
    });
}

/// A Drive page field: the selection, starred, its detail, and Browse….
fn field(ui: &mut egui::Ui, name: &str, detail: &str) {
    let mut job = egui::text::LayoutJob::default();
    for part in [RichText::new("★ ").color(STAR), RichText::new(name), RichText::new(detail).color(GREY)] {
        part.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
    }
    ui.add(egui::Button::new(job).right_text("Browse…").min_size(egui::vec2(340.0, 26.0)));
}

/// The favourites' quick picks under a field, the first one selected.
fn chips(ui: &mut egui::Ui, names: &[&str], more: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (k, name) in names.iter().enumerate() {
            let mut job = egui::text::LayoutJob::default();
            for part in [RichText::new("★ ").size(12.5).color(STAR), RichText::new(*name).size(13.0)] {
                part.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
            }
            ui.add(egui::Button::selectable(k == 0, job).corner_radius(11.0).min_size(egui::vec2(0.0, 24.0)));
        }
        ui.add(egui::Button::new(RichText::new(format!("+{more}")).size(13.0).color(GREY)).frame(false));
    });
}

/// The Drive page with the new Car and Stage fields: the real capture (`xsara`), its left column opened up for the
/// favourites' quick picks. The car's take the place of Add cars…, which moves into the car browser.
fn drive(ctx: &egui::Context, page: &TextureHandle) {
    ctx.root_ui(|root| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            let p = ui.painter();
            let full = page.size_vec2();
            let uv =
                |r: Rect| Rect::from_min_max((r.min.to_vec2() / full).to_pos2(), (r.max.to_vec2() / full).to_pos2());
            let blit =
                |src: Rect, at: Pos2| p.image(page.id(), Rect::from_min_size(at, src.size()), uv(src), Color32::WHITE);
            let shift = 56.0;
            let split = 188.0;
            blit(Rect::from_min_max(Pos2::ZERO, Pos2::new(1100.0, 760.0)), Pos2::ZERO);
            // The left column below the Stage row moved down by `shift`.
            p.rect_filled(Rect::from_min_max(Pos2::new(0.0, split), Pos2::new(548.0, 719.0)), 0.0, BG);
            blit(
                Rect::from_min_max(Pos2::new(0.0, split), Pos2::new(548.0, 719.0 - shift)),
                Pos2::new(0.0, split + shift),
            );
            // The Car and Stage fields redrawn, the Stage label moved down.
            p.rect_filled(Rect::from_min_max(Pos2::new(150.0, 88.0), Pos2::new(548.0, split + shift)), 0.0, BG);
            p.rect_filled(Rect::from_min_max(Pos2::new(0.0, 150.0), Pos2::new(150.0, split + shift)), 0.0, BG);
            let label = ui.visuals().text_color();
            p.text(Pos2::new(8.0, 186.0), Align2::LEFT_CENTER, "Stage", FontId::proportional(14.5), label);
            // The incomplete-install warnings of the capture, not part of the proposal.
            p.rect_filled(Rect::from_min_max(Pos2::new(550.0, 596.0), Pos2::new(1100.0, 640.0)), 0.0, BG);
            p.rect_filled(Rect::from_min_max(Pos2::new(0.0, 724.0), Pos2::new(1100.0, 760.0)), 0.0, BG);
        })
    });
    egui::Area::new(egui::Id::unique("car field")).fixed_pos(Pos2::new(154.0, 93.0)).show(ctx, |ui| {
        field(ui, "Citroen Xsara Kit Car", "  Group A7");
        ui.add_space(8.0);
        chips(ui, &["Xsara Kit Car", "Escort Mk II", "MG ZR"], 3);
    });
    egui::Area::new(egui::Id::unique("stage field")).fixed_pos(Pos2::new(154.0, 173.0)).show(ctx, |ui| {
        field(ui, "Mustaselka", "  Finland · 7.9 km · Snow");
        ui.add_space(8.0);
        chips(ui, &["Mustaselka", "Sipirkakim", "Tanner", "Joux Plane"], 4);
    });
}

fn stage_sections() -> Vec<Section> {
    vec![
        ("LIBRARY", vec![("★", "Favourites", 8, false), ("🕘", "Recent", 6, false), ("☰", "All stages", 292, false)]),
        (
            "COUNTRIES",
            [
                ("Australia", 6),
                ("Czechia", 64),
                ("Finland", 41),
                ("France", 33),
                ("Germany", 22),
                ("Great Britain", 18),
                ("Japan", 6),
                ("Poland", 27),
                ("Slovakia", 25),
                ("USA", 6),
            ]
            .into_iter()
            .map(|(name, n)| ("", name, n, false))
            .chain([("", "14 more…", 44, true)])
            .collect(),
        ),
        ("", vec![("⚠", "Can't start", 160, true)]),
    ]
}

fn car_sections() -> Vec<Section> {
    vec![
        ("LIBRARY", vec![("★", "Favourites", 6, false), ("🕘", "Recent", 6, false), ("☰", "All cars", 45, false)]),
        (
            "CLASSES",
            [
                ("WRC 2.0", 4),
                ("WRC 1.6", 3),
                ("Rally 2 / R5", 6),
                ("Super 1600", 3),
                ("Group R2 / R3", 5),
                ("Group A", 9),
                ("Group B", 2),
                ("Group 4", 3),
                ("Group 2", 2),
                ("Group RGT", 2),
            ]
            .into_iter()
            .map(|(name, n)| ("", name, n, false))
            .chain([("", "3 more…", 6, true)])
            .collect(),
        ),
        ("", vec![("⚠", "Not downloaded", 60, true)]),
    ]
}

/// Draws one mockup with the loaded textures: the Drive page capture and the pictures by name.
type Shot<'a> = dyn Fn(&egui::Context, &Textures) + 'a;

struct Textures {
    page: TextureHandle,
    map52: TextureHandle,
    map11: TextureHandle,
    xsara: TextureHandle,
    escort: TextureHandle,
}

#[test]
fn mockup() {
    let (Some(out), Some(captures)) = (std::env::var_os("MOCKUP_OUT"), std::env::var_os("MOCKUP_CAPTURES")) else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let page = |name: &str| load(&Path::new(&captures).join(name).join("drive.png"));
    let (map52, map11, xsara, escort) = (page("map52"), page("map11"), page("xsara"), page("escort"));
    let stage_picture = |p: &egui::ColorImage| crop(p, [556, 57, 384, 182]);
    let car_picture = |p: &egui::ColorImage| crop(p, [556, 281, 384, 216]);

    let mut favourites = vec![
        retail(11, true, true),
        rsf("Hradní Okruh", "Czechia", 6.2, 1, "kamil_r", true, true),
        rsf("Kalvola Ridge", "Finland", 12.6, 0, "juhani", true, true),
        retail(44, true, true),
        rsf("Lesní Údolí", "Czechia", 11.4, 0, "kamil_r", true, true),
        rsf("Puijo Winter", "Finland", 9.8, 2, "nordic_stages", true, true),
        retail(52, true, true),
        retail(54, true, true),
    ];
    let mut finland: Vec<Item> = (10..=15).map(|m| retail(m, m == 11, false)).collect();
    finland.extend([
        rsf("Harju Sprint", "Finland", 3.4, 0, "juhani", false, false),
        rsf("Kalvola Ridge", "Finland", 12.6, 0, "juhani", true, false),
        rsf("Lahti Night", "Finland", 7.1, 1, "mr_mika", false, false),
        rsf("Ouninpohja Tribute", "Finland", 16.3, 0, "nordic_stages", false, false),
        rsf("Puijo Winter", "Finland", 9.8, 2, "nordic_stages", true, false),
    ]);
    finland.sort_by(|a, b| a.name.cmp(&b.name));
    for item in favourites.iter_mut().chain(finland.iter_mut()) {
        let best = match item.name.as_str() {
            "Sipirkakim" => "4:51.62 · ZR · good",
            "Mustaselka" => "4:38.05 · ZR · good",
            _ => continue,
        };
        item.facts.insert(0, ("Your best", best.into()));
    }

    let car_favourites = vec![
        car("Citroen Xsara Kit Car", "Group A7", "RSF 94", "own model", true, true),
        car("Ford Escort Mk II RS Grp4", "Group 4", "RSF 78", "own model", true, true),
        car("MG ZR S1600", "Super 1600", "RBR 1.02", "own model", true, true),
        car("Peugeot 206 WRC", "WRC 2.0", "RBR 1.02", "own model", true, true),
        car("Peugeot 306 Maxi Kit Car", "Group A7", "RSF 95", "own model", true, true),
        car("VW Golf II GTI 16V GrpA", "Group A6", "RSF 75", "stand-in model", true, true),
    ];
    let mut group4 = vec![
        car("Fiat 131 Abarth Grp4", "Group 4", "RSF 100", "own model", false, false),
        car("Ford Escort Mk II RS Grp4", "Group 4", "RSF 78", "own model", true, false),
        car("Porsche 911 SC 3.0 Grp4", "Group 4", "RSF 52", "own model", false, false),
        car("Audi quattro Grp4", "Group 4", "RSF 18", "", false, false),
    ];
    group4[3].problem = Some("not downloaded");
    group4[3].fifth = None;

    let stage_columns = |country: bool| [if country { "COUNTRY" } else { "AUTHOR" }, "LENGTH", "SURFACE"];
    let shots: [(&str, Box<Shot<'_>>); 5] = [
        (
            "mockup-1-favourites.png",
            Box::new(|ctx, t| {
                let b = Browser {
                    noun: "stage",
                    search: "🔍  Search 452 stages: name, country, author, number",
                    sections: stage_sections(),
                    view: "Favourites",
                    filters: stage_filters,
                    columns: stage_columns(true),
                    items: favourites.clone(),
                    selected: 6,
                    hovered: None,
                    picture: &t.map52,
                    sidebar_button: None,
                };
                browser(ctx, &b);
            }),
        ),
        (
            "mockup-2-country.png",
            Box::new(|ctx, t| {
                let b = Browser {
                    noun: "stage",
                    search: "🔍  Search 452 stages: name, country, author, number",
                    sections: stage_sections(),
                    view: "Finland",
                    filters: stage_filters,
                    columns: stage_columns(false),
                    items: finland.clone(),
                    selected: finland.iter().position(|s| s.name == "Mustaselka").unwrap(),
                    hovered: finland.iter().position(|s| s.name == "Lahti Night"),
                    picture: &t.map11,
                    sidebar_button: None,
                };
                browser(ctx, &b);
            }),
        ),
        ("mockup-3-drive-page.png", Box::new(|ctx, t| drive(ctx, &t.page))),
        (
            "mockup-4-car-favourites.png",
            Box::new(|ctx, t| {
                let b = Browser {
                    noun: "car",
                    search: "🔍  Search 105 cars: name, class, RSF number",
                    sections: car_sections(),
                    view: "Favourites",
                    filters: car_filters,
                    columns: ["CLASS", "", "MODEL"],
                    items: car_favourites.clone(),
                    selected: 0,
                    hovered: None,
                    picture: &t.xsara,
                    sidebar_button: Some("Add cars…"),
                };
                browser(ctx, &b);
            }),
        ),
        (
            "mockup-5-car-class.png",
            Box::new(|ctx, t| {
                let b = Browser {
                    noun: "car",
                    search: "🔍  Search 105 cars: name, class, RSF number",
                    sections: car_sections(),
                    view: "Group 4",
                    filters: car_filters,
                    columns: ["FROM", "", "MODEL"],
                    items: group4.clone(),
                    selected: 1,
                    hovered: Some(0),
                    picture: &t.escort,
                    sidebar_button: Some("Add cars…"),
                };
                browser(ctx, &b);
            }),
        ),
    ];
    for (name, shot) in &shots {
        let mut textures = None;
        let rgba = crate::capture::draw(|ctx| {
            crate::pages::theme(ctx);
            let t = textures.get_or_insert_with(|| {
                let o = egui::TextureOptions::LINEAR;
                Textures {
                    page: ctx.load_texture("page", xsara.clone(), o),
                    map52: ctx.load_texture("map52", stage_picture(&map52), o),
                    map11: ctx.load_texture("map11", stage_picture(&map11), o),
                    xsara: ctx.load_texture("xsara", car_picture(&xsara), o),
                    escort: ctx.load_texture("escort", car_picture(&escort), o),
                }
            });
            shot(ctx, t);
        })
        .unwrap();
        crate::capture::write_png(&out.join(name), &rgba).unwrap();
    }
}
