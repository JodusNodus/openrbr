//! Throwaway: GUI mockups of the proposed stage browser (docs/proposals/track-picker.md). Not for merging.
//!
//! `MOCKUP_OUT=DIR MOCKUP_REAL=DIR52 MOCKUP_REAL11=DIR11 cargo test -p rbr-launcher --bin rerbr mockup -- --nocapture`
//! where DIR52 and DIR11 hold `rerbr --screenshot` captures with map 52 and map 11 selected.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};

use crate::catalog;
use crate::pages::ACCENT;

const GREY: Color32 = Color32::from_gray(130);
const DIM: Color32 = Color32::from_gray(95);
const STAR: Color32 = Color32::from_rgb(240, 190, 60);

#[derive(Clone)]
struct Stage {
    fav: bool,
    name: String,
    country: &'static str,
    km: f32,
    surface: u32,
    author: String,
    /// "RBR 1.02" or "RSF".
    origin: &'static str,
    problem: Option<&'static str>,
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

fn retail(map: u32, fav: bool) -> Stage {
    Stage {
        fav,
        name: catalog::title(map).unwrap().to_owned(),
        country: rally(map),
        km: crate::preview::game_length(map).unwrap_or(0.0) / 1000.0,
        surface: catalog::surface_kind(map),
        author: "Warthog".into(),
        origin: "RBR 1.02",
        problem: None,
    }
}

/// Made-up community stages (no third-party names).
fn rsf(name: &str, country: &'static str, km: f32, surface: u32, author: &str, fav: bool) -> Stage {
    Stage { fav, name: name.into(), country, km, surface, author: author.into(), origin: "RSF", problem: None }
}

fn surface_colour(s: u32) -> Color32 {
    [Color32::from_rgb(190, 145, 95), Color32::from_rgb(150, 155, 170), Color32::from_rgb(185, 215, 250)][s as usize]
}

fn load(path: &std::path::Path) -> egui::ColorImage {
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
    egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
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
    });
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(" ").size(14.0));
        ui.add_space(4.0);
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
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(14.0),
        icon_colour,
    );
    painter.text(
        rect.left_center() + egui::vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(14.5),
        text,
    );
    let count_colour = if selected { Color32::from_gray(210) } else { GREY };
    painter.text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.5),
        count_colour,
    );
}

fn side_header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).size(11.5).color(GREY).strong());
    ui.add_space(2.0);
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Favourites,
    Country(&'static str),
}

struct Browser<'a> {
    view: View,
    stages: Vec<Stage>,
    selected: usize,
    hovered: Option<usize>,
    surface: Option<u32>,
    query: String,
    picture: &'a TextureHandle,
    best: Option<&'static str>,
}

fn browser(ctx: &egui::Context, b: &mut Browser) {
    tabs(ctx);
    egui::CentralPanel::default().show(ctx, |ui| {
        // The top line: back, title, search, random.
        ui.horizontal(|ui| {
            ui.add(egui::Button::new(RichText::new("⏴ Drive").size(14.5)).min_size(egui::vec2(0.0, 30.0)));
            ui.add_space(6.0);
            ui.label(RichText::new("Choose a stage").size(20.0).strong());
            ui.add_space(18.0);
            ui.add(
                egui::TextEdit::singleline(&mut b.query)
                    .hint_text("🔍  Search 452 stages: name, country, author, number")
                    .desired_width(470.0)
                    .min_size(egui::vec2(0.0, 30.0))
                    .vertical_align(egui::Align::Center),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Button::new(RichText::new("🎲 Random").size(14.5)).min_size(egui::vec2(0.0, 30.0)))
                    .on_hover_text("A random stage from the list shown");
            });
        });
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_top(|ui| {
            // The collections.
            ui.vertical(|ui| {
                ui.set_width(186.0);
                side_header(ui, "LIBRARY");
                side_row(ui, "★", "Favourites", 8, b.view == View::Favourites, false);
                side_row(ui, "🕘", "Recent", 6, false, false);
                side_row(ui, "☰", "All stages", 292, false, false);
                side_header(ui, "COUNTRIES");
                for (name, n) in [
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
                ] {
                    side_row(ui, "", name, n, b.view == View::Country(name), false);
                }
                side_row(ui, "", "14 more…", 44, false, true);
                ui.add_space(6.0);
                side_row(ui, "⚠", "Can't start", 160, false, true);
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
    });
}

fn list(ui: &mut egui::Ui, b: &mut Browser) {
    ui.vertical(|ui| {
        ui.set_width(468.0);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Surface").color(GREY));
            ui.selectable_value(&mut b.surface, None, "Any");
            for (s, name) in ["Gravel", "Tarmac", "Snow"].into_iter().enumerate() {
                ui.selectable_value(&mut b.surface, Some(s as u32), name);
            }
            ui.add_space(10.0);
            ui.label(RichText::new("Length").color(GREY));
            egui::ComboBox::from_id_salt("length").width(70.0).selected_text("Any").show_ui(ui, |_| {});
        });
        ui.horizontal(|ui| {
            let shown = b.stages.iter().filter(|s| b.surface.is_none_or(|x| x == s.surface)).count();
            let what = match b.view {
                View::Favourites => "favourites".to_owned(),
                View::Country(c) => format!("stages in {c}"),
            };
            ui.label(RichText::new(format!("{shown} {what}")).color(GREY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort").width(110.0).selected_text("Name A–Z").show_ui(ui, |_| {});
                ui.label(RichText::new("Sort").color(GREY));
            });
        });
        ui.add_space(4.0);
        let show_country = b.view == View::Favourites;
        let width = ui.available_width();
        let cols = |x: f32| {
            // star, name, third column, length (right edge), surface
            [x + 6.0, x + 30.0, x + 222.0, x + 380.0, x + 394.0]
        };
        // The header.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());
        let c = cols(rect.left());
        let p = ui.painter();
        let small = FontId::proportional(11.5);
        let y = rect.center().y;
        p.text(Pos2::new(c[1], y), Align2::LEFT_CENTER, "STAGE", small.clone(), GREY);
        p.text(
            Pos2::new(c[2], y),
            Align2::LEFT_CENTER,
            if show_country { "COUNTRY" } else { "AUTHOR" },
            small.clone(),
            GREY,
        );
        p.text(Pos2::new(c[3], y), Align2::RIGHT_CENTER, "LENGTH", small.clone(), GREY);
        p.text(Pos2::new(c[4], y), Align2::LEFT_CENTER, "SURFACE", small, GREY);
        p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, Color32::from_gray(60)));
        ui.add_space(2.0);
        for (i, s) in b.stages.iter().enumerate() {
            if b.surface.is_some_and(|x| x != s.surface) {
                continue;
            }
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::click());
            let p = ui.painter();
            let c = cols(rect.left());
            let y = rect.center().y;
            if i == b.selected {
                p.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.55));
                p.rect_filled(
                    Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                    1.0,
                    ui.visuals().selection.bg_fill,
                );
            } else if Some(i) == b.hovered {
                p.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let text = if s.problem.is_some() { DIM } else { ui.visuals().strong_text_color() };
            let (star, star_colour) = match (s.fav, Some(i) == b.hovered) {
                (true, _) => ("★", STAR),
                (false, true) => ("☆", Color32::from_gray(170)),
                (false, false) => ("☆", Color32::from_gray(70)),
            };
            p.text(Pos2::new(c[0] + 6.0, y), Align2::CENTER_CENTER, star, FontId::proportional(16.0), star_colour);
            p.text(Pos2::new(c[1], y), Align2::LEFT_CENTER, &s.name, FontId::proportional(15.0), text);
            let third = if show_country { s.country.to_owned() } else { s.author.clone() };
            p.text(Pos2::new(c[2], y), Align2::LEFT_CENTER, third, FontId::proportional(13.0), GREY);
            p.text(
                Pos2::new(c[3], y),
                Align2::RIGHT_CENTER,
                format!("{:.1} km", s.km),
                FontId::proportional(13.0),
                GREY,
            );
            p.circle_filled(Pos2::new(c[4] + 4.0, y), 4.0, surface_colour(s.surface));
            let name = ["Gravel", "Tarmac", "Snow"][s.surface as usize];
            p.text(Pos2::new(c[4] + 14.0, y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), GREY);
        }
        ui.add_space(10.0);
        ui.label(RichText::new("⏶⏷ move    Enter choose    Ctrl+D favourite    Esc back").size(12.0).color(DIM));
    });
}

fn detail(ui: &mut egui::Ui, b: &mut Browser) {
    let s = b.stages[b.selected].clone();
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.add_space(6.0);
        let w = ui.available_width();
        let size = b.picture.size_vec2();
        ui.add(egui::Image::new((b.picture.id(), egui::vec2(w, w * size.y / size.x))).corner_radius(6.0));
        ui.add_space(8.0);
        ui.label(RichText::new(&s.name).size(22.0).strong());
        ui.label(RichText::new(format!("{} · {} · {}", s.country, s.origin, s.author)).color(GREY));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (dot, text) in [
                (None, format!("{:.1} km", s.km)),
                (Some(s.surface), ["Gravel", "Tarmac", "Snow"][s.surface as usize].to_owned()),
            ] {
                let frame = egui::Frame::new()
                    .fill(Color32::from_gray(48))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(10, 3));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(s) = dot {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, surface_colour(s));
                        }
                        ui.label(RichText::new(text).size(13.5));
                    });
                });
            }
        });
        ui.add_space(8.0);
        egui::Grid::new("facts").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("Your best").color(GREY));
            match b.best {
                Some(best) => ui.label(best),
                None => ui.label(RichText::new("not driven yet").color(GREY)),
            };
            ui.end_row();
            ui.label(RichText::new("Weather").color(GREY));
            ui.label("good · damp · bad");
            ui.end_row();
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let (label, fill) = if s.fav {
                (RichText::new("★ Favourite").size(15.0).color(STAR), Color32::from_gray(48))
            } else {
                (RichText::new("☆ Add to favourites").size(15.0), Color32::from_gray(48))
            };
            ui.add(egui::Button::new(label).fill(fill).min_size(egui::vec2(0.0, 44.0)));
            let go = RichText::new("Choose stage").size(18.0).strong().color(Color32::WHITE);
            ui.add(egui::Button::new(go).fill(ACCENT).min_size(egui::vec2(ui.available_width(), 44.0)));
        });
    });
}

/// The Drive page with the new Stage field: the real capture, its left column opened up under the Stage row for the
/// favourites' quick picks.
fn drive(ctx: &egui::Context, page: &TextureHandle, picture: &TextureHandle) {
    let _ = picture;
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let p = ui.painter();
        let full = page.size_vec2();
        let uv = |r: Rect| Rect::from_min_max((r.min.to_vec2() / full).to_pos2(), (r.max.to_vec2() / full).to_pos2());
        let blit =
            |src: Rect, at: Pos2| p.image(page.id(), Rect::from_min_size(at, src.size()), uv(src), Color32::WHITE);
        let shift = 40.0;
        let split = 188.0;
        // Right column and top bar as they are; the left column below the Car rows moved down by `shift`.
        blit(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1100.0, 760.0)), Pos2::ZERO);
        let bg = Color32::from_rgb(27, 27, 27);
        p.rect_filled(Rect::from_min_max(Pos2::new(0.0, split), Pos2::new(548.0, 719.0)), 0.0, bg);
        blit(Rect::from_min_max(Pos2::new(0.0, split), Pos2::new(548.0, 719.0 - shift)), Pos2::new(0.0, split + shift));
        // The Stage row redrawn, and the quick picks under it.
        p.rect_filled(Rect::from_min_max(Pos2::new(150.0, 152.0), Pos2::new(548.0, split + shift)), 0.0, bg);
        // The incomplete-install warnings of the capture, not part of the proposal.
        p.rect_filled(Rect::from_min_max(Pos2::new(550.0, 596.0), Pos2::new(1100.0, 640.0)), 0.0, bg);
        p.rect_filled(Rect::from_min_max(Pos2::new(0.0, 724.0), Pos2::new(1100.0, 760.0)), 0.0, bg);
    });
    egui::Area::new("stage field".into()).fixed_pos(Pos2::new(154.0, 157.0)).show(ctx, |ui| {
        let mut job = egui::text::LayoutJob::default();
        for part in [
            RichText::new("★ ").color(STAR),
            RichText::new("Mustaselka"),
            RichText::new("  Finland · 7.9 km · Snow").color(GREY),
        ] {
            part.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
        }
        ui.add(egui::Button::new(job).right_text("Browse…").min_size(egui::vec2(340.0, 26.0)));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (name, on) in [("Mustaselka", true), ("Sipirkakim", false), ("Tanner", false), ("Joux Plane", false)] {
                let mut job = egui::text::LayoutJob::default();
                for part in [RichText::new("★ ").size(12.5).color(STAR), RichText::new(name).size(13.0)] {
                    part.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
                }
                ui.add(egui::Button::selectable(on, job).corner_radius(11.0).min_size(egui::vec2(0.0, 24.0)));
            }
            ui.add(egui::Button::new(RichText::new("+4").size(13.0).color(GREY)).frame(false));
        });
    });
}

/// Draws one mockup: the context, the Drive page capture, the pictures of maps 52 and 11.
type Shot<'a> = dyn Fn(&egui::Context, &TextureHandle, &TextureHandle, &TextureHandle) + 'a;

#[test]
fn mockup() {
    let (Some(out), Some(real52), Some(real11)) =
        (std::env::var_os("MOCKUP_OUT"), std::env::var_os("MOCKUP_REAL"), std::env::var_os("MOCKUP_REAL11"))
    else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let page52 = load(&std::path::Path::new(&real52).join("drive.png"));
    let page11 = load(&std::path::Path::new(&real11).join("drive.png"));
    let pic52 = crop(&page52, [556, 57, 384, 182]);
    let pic11 = crop(&page11, [556, 57, 384, 182]);

    let favourites = vec![
        retail(11, true),
        rsf("Hradní Okruh", "Czechia", 6.2, 1, "kamil_r", true),
        rsf("Kalvola Ridge", "Finland", 12.6, 0, "juhani", true),
        retail(44, true),
        rsf("Lesní Údolí", "Czechia", 11.4, 0, "kamil_r", true),
        rsf("Puijo Winter", "Finland", 9.8, 2, "nordic_stages", true),
        retail(52, true),
        retail(54, true),
    ];
    let mut finland: Vec<Stage> = (10..=15).map(|m| retail(m, m == 11)).collect();
    finland.extend([
        rsf("Harju Sprint", "Finland", 3.4, 0, "juhani", false),
        rsf("Kalvola Ridge", "Finland", 12.6, 0, "juhani", true),
        rsf("Lahti Night", "Finland", 7.1, 1, "mr_mika", false),
        rsf("Ouninpohja Tribute", "Finland", 16.3, 0, "nordic_stages", false),
        rsf("Puijo Winter", "Finland", 9.8, 2, "nordic_stages", true),
    ]);
    finland.sort_by(|a, b| a.name.cmp(&b.name));

    let shots: [(&str, Box<Shot<'_>>); 3] = [
        (
            "mockup-1-favourites.png",
            Box::new(|ctx, _, p52, _| {
                let mut b = Browser {
                    view: View::Favourites,
                    stages: favourites.clone(),
                    selected: 6,
                    hovered: None,
                    surface: None,
                    query: String::new(),
                    picture: p52,
                    best: Some("4:51.62 · ZR · good"),
                };
                browser(ctx, &mut b);
            }),
        ),
        (
            "mockup-2-country.png",
            Box::new(|ctx, _, _, p11| {
                let mut b = Browser {
                    view: View::Country("Finland"),
                    stages: finland.clone(),
                    selected: finland.iter().position(|s| s.name == "Mustaselka").unwrap(),
                    hovered: finland.iter().position(|s| s.name == "Lahti Night"),
                    surface: None,
                    query: String::new(),
                    picture: p11,
                    best: Some("4:38.05 · ZR · good"),
                };
                browser(ctx, &mut b);
            }),
        ),
        ("mockup-3-drive-page.png", Box::new(|ctx, page, _, p11| drive(ctx, page, p11))),
    ];
    for (name, shot) in shots {
        let mut textures = None;
        let rgba = crate::capture::draw(|ctx| {
            crate::pages::theme(ctx);
            let (page, p52, p11) = textures.get_or_insert_with(|| {
                let o = egui::TextureOptions::LINEAR;
                (
                    ctx.load_texture("page", page11.clone(), o),
                    ctx.load_texture("p52", pic52.clone(), o),
                    ctx.load_texture("p11", pic11.clone(), o),
                )
            });
            shot(ctx, page, p52, p11);
        })
        .unwrap();
        crate::capture::write_png(&out.join(name), &rgba).unwrap();
        eprintln!("wrote {name}");
    }
}
