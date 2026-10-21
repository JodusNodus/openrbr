//! What the stage and the car browsers share (docs/proposals/track-picker.md): the lines of a list, the sidebar's rows,
//! the keys, the highlighted row with its scrolling, and the recent picks. The browsers differ in their collections,
//! filters, columns and detail panel.

use egui::{Align2, Color32, FontId, Key, Modifiers, Sense};

pub const GREY: Color32 = Color32::from_gray(130);
pub const DIM: Color32 = Color32::from_gray(95);
pub const STAR: Color32 = Color32::from_rgb(240, 190, 60);
/// The list's row height, points.
pub const ROW: f32 = 30.0;
/// The list's visible height before the first draw has measured it.
const LIST_HEIGHT: f32 = 420.0;
/// Rows a Page Up or Page Down moves.
const PAGE: usize = 10;
/// The recent picks kept.
pub const RECENT: usize = 6;

/// A line of a list: a header, or an item (its position in the browser's rows).
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Header(String),
    Row(usize),
}

/// `members` under `header`, nothing when there are none.
pub fn block(header: Option<&str>, members: Vec<usize>) -> impl Iterator<Item = Line> {
    let header = if members.is_empty() { None } else { header.map(|h| Line::Header(h.to_owned())) };
    header.into_iter().chain(members.into_iter().map(Line::Row))
}

/// The items of `lines`, in order.
pub fn items(lines: &[Line]) -> impl Iterator<Item = usize> + '_ {
    lines.iter().filter_map(|l| if let Line::Row(k) = l { Some(*k) } else { None })
}

/// Puts `item` first in `recent`, keeping [`RECENT`].
pub fn remember<T: PartialEq>(recent: &mut Vec<T>, item: T) {
    recent.retain(|r| *r != item);
    recent.insert(0, item);
    recent.truncate(RECENT);
}

/// The keys a browser takes before the search box or the page does.
#[derive(Debug, Clone, Copy, Default)]
pub struct Keys {
    pub down: bool,
    pub up: bool,
    pub page_down: bool,
    pub page_up: bool,
    pub enter: bool,
    pub escape: bool,
    /// Ctrl/Cmd+D.
    pub favourite: bool,
}

impl Keys {
    pub fn take(ui: &egui::Ui) -> Keys {
        ui.input_mut(|i| Keys {
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            page_down: i.consume_key(Modifiers::NONE, Key::PageDown),
            page_up: i.consume_key(Modifiers::NONE, Key::PageUp),
            enter: i.consume_key(Modifiers::NONE, Key::Enter),
            escape: i.consume_key(Modifiers::NONE, Key::Escape),
            favourite: i.consume_key(Modifiers::COMMAND, Key::D),
        })
    }

    /// How far the arrow and page keys move the highlight, rows.
    pub fn movement(&self) -> isize {
        let page = PAGE as isize;
        [(self.down, 1), (self.up, -1), (self.page_down, page), (self.page_up, -page)]
            .into_iter()
            .map(|(pressed, by)| if pressed { by } else { 0 })
            .sum()
    }
}

/// The highlighted row (its position in the browser's rows) and the list's scrolling.
#[derive(Debug)]
pub struct Nav {
    pub cursor: usize,
    /// The scroll offset and visible height as last drawn.
    scroll: (f32, f32),
    /// Bring the cursor into view at the next draw: `Some(true)` centred.
    reveal: Option<bool>,
    to_top: bool,
}

impl Default for Nav {
    fn default() -> Self {
        Nav { cursor: 0, scroll: (0.0, LIST_HEIGHT), reveal: None, to_top: false }
    }
}

impl Nav {
    /// Highlights `cursor` and brings it to the middle of the list.
    pub fn show(&mut self, cursor: usize) {
        self.cursor = cursor;
        self.reveal = Some(true);
    }

    /// Highlights `cursor` where it is.
    pub fn point(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    /// The cursor's line of `lines`.
    pub fn position(&self, lines: &[Line]) -> Option<usize> {
        lines.iter().position(|l| *l == Line::Row(self.cursor))
    }

    /// Moves the cursor `by` rows down (negative: up) the list, headers skipped.
    pub fn step(&mut self, lines: &[Line], by: isize) {
        let rows: Vec<usize> = items(lines).collect();
        if rows.is_empty() {
            return;
        }
        let next = match rows.iter().position(|&k| k == self.cursor) {
            Some(at) => (at as isize + by).clamp(0, rows.len() as isize - 1) as usize,
            None => 0,
        };
        self.cursor = rows[next];
        self.reveal = Some(false);
    }

    /// After the list changed: the cursor stays when its row is still there, else it moves to the first row and the
    /// list to its top.
    pub fn settle(&mut self, lines: &[Line]) {
        if self.position(lines).is_some() {
            self.reveal.get_or_insert(true);
        } else if let Some(first) = items(lines).next() {
            self.cursor = first;
            self.to_top = true;
        }
    }

    /// The list's scroll area, set to bring the cursor (on line `position`) into view when asked to.
    pub fn area(&mut self, id: &'static str, height: f32, position: Option<usize>) -> egui::ScrollArea {
        let mut area = egui::ScrollArea::vertical().id_salt(id).max_height(height).auto_shrink([false, true]);
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
        area
    }

    /// Keeps where the list is scrolled and how much of it shows.
    pub fn scrolled<R>(&mut self, output: &egui::scroll_area::ScrollAreaOutput<R>) {
        self.scroll = (output.state.offset.y, output.inner_rect.height());
    }
}

/// The browsers' top line: a Back button, the title, the search box and room for Random on the right. The search box
/// takes focus when `focus` is set, and when the player types with nothing else focused on it.
pub fn top_line(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    query: &mut String,
    focus: &mut bool,
    random: &mut bool,
) -> bool {
    let id = egui::Id::new(("browser search", title));
    let mut back = false;
    ui.horizontal(|ui| {
        let button = egui::Button::new(egui::RichText::new("⏴ Drive").size(14.5)).min_size(egui::vec2(0.0, 30.0));
        back = ui.add(button).on_hover_text("Back without changing the choice (Esc)").clicked();
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).size(20.0).strong());
        ui.add_space(18.0);
        // Typing anywhere searches.
        let typing = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
        if (std::mem::take(focus) || typing) && ui.memory(|m| m.focused()) != Some(id) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        ui.add(
            egui::TextEdit::singleline(query)
                .id(id)
                .hint_text(hint)
                .desired_width(470.0)
                .min_size(egui::vec2(0.0, 30.0))
                .vertical_align(egui::Align::Center),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = egui::Button::new(egui::RichText::new("🎲 Random").size(14.5)).min_size(egui::vec2(0.0, 30.0));
            *random = ui.add(button).on_hover_text("A random pick of the list as filtered").clicked();
        });
    });
    back
}

/// A sidebar line: an icon, a name and a count; the selected one filled.
pub fn side_row(ui: &mut egui::Ui, icon: &str, name: &str, count: usize, selected: bool, grey: bool) -> egui::Response {
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

/// A list header's line.
pub fn header_line(ui: &mut egui::Ui, name: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), Sense::hover());
    let at = rect.left_bottom() + egui::vec2(8.0, -5.0);
    ui.painter().text(at, Align2::LEFT_BOTTOM, name.to_uppercase(), FontId::proportional(12.5), GREY);
}

/// A button for a star: the highlighted row's ★ Favourite / ☆ Add to favourites.
pub fn favourite_button(ui: &mut egui::Ui, favourite: bool) -> egui::Response {
    let label = if favourite {
        egui::RichText::new("★ Favourite").size(15.0).color(STAR)
    } else {
        egui::RichText::new("☆ Add to favourites").size(15.0)
    };
    let button = egui::Button::new(label).fill(Color32::from_gray(48)).min_size(egui::vec2(0.0, 44.0));
    ui.add(button).on_hover_text("Ctrl/Cmd+D")
}

/// The Drive page's field for a stage or a car: the selection, starred when it is a favourite, with its detail and
/// Browse… on the right; whether it was pressed.
pub fn field_button(ui: &mut egui::Ui, favourite: bool, name: &str, detail: &str, width: f32) -> bool {
    let mut text = egui::text::LayoutJob::default();
    let parts = [
        egui::RichText::new("★ ").color(STAR),
        egui::RichText::new(name),
        egui::RichText::new(format!("  {detail}")).color(Color32::GRAY),
    ];
    for part in parts.into_iter().skip(usize::from(!favourite)) {
        part.append_to(&mut text, ui.style(), egui::FontSelection::Default, egui::Align::Min);
    }
    let button = egui::Button::new(text).right_text("Browse…").truncate().min_size(egui::vec2(width, 0.0));
    ui.add(button).clicked()
}

/// What a click in a row of [`chips`] chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// The chip at this position.
    Chip(usize),
    /// The "+N" button for the chips that did not fit.
    More,
}

/// The favourites' quick picks under a field: a chip each (its name, its tooltip, whether it is the selection) in one
/// row. The ones that don't fit go behind "+N".
pub fn chips(ui: &mut egui::Ui, chips: &[(&str, String, bool)]) -> Option<Pick> {
    if chips.is_empty() {
        return None;
    }
    let (font, padding) = (FontId::proportional(13.0), ui.spacing().button_padding.x * 2.0 + 4.0);
    let widths: Vec<f32> = (chips.iter())
        .map(|(name, ..)| {
            ui.painter().layout_no_wrap(format!("★ {name}"), font.clone(), Color32::WHITE).size().x + padding
        })
        .collect();
    let gap = 6.0;
    // How many chips fit, leaving room for the "+N" button when some do not.
    let room = ui.available_width();
    let used = |count: usize| widths[..count].iter().sum::<f32>() + gap * count as f32;
    let shown = if used(widths.len()) <= room {
        widths.len()
    } else {
        (0..widths.len()).rev().find(|&n| used(n) + 44.0 <= room).unwrap_or(0)
    };
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (n, (name, tip, selected)) in chips[..shown].iter().enumerate() {
            let mut job = egui::text::LayoutJob::default();
            let parts = [egui::RichText::new("★ ").size(12.5).color(STAR), egui::RichText::new(*name).size(13.0)];
            for part in parts {
                part.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
            }
            let chip = egui::Button::selectable(*selected, job).corner_radius(11.0).min_size(egui::vec2(0.0, 24.0));
            if ui.add(chip).on_hover_text(tip).clicked() {
                picked = Some(Pick::Chip(n));
            }
        }
        if shown < chips.len() {
            let rest = egui::RichText::new(format!("+{}", chips.len() - shown)).size(13.0);
            let button = egui::Button::new(rest).corner_radius(11.0).min_size(egui::vec2(0.0, 24.0));
            if ui.add(button).on_hover_text("More favourites").clicked() {
                picked = Some(Pick::More);
            }
        }
    });
    picked
}
