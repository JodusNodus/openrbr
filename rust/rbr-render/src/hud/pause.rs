//! The live drive's pause menu over the paused game (`rbr_gameplay::pause_menu`): the game's items for the mode and
//! the moment (the live process's MNU1 record), moved through with the keys or the gamepad, and the game's confirm box
//! before an item that ends or leaves the race. The texts are 1.02's English string table's (`bstrings.eng`); the
//! panels' look is ours, as the rest of the HUD's.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};
use gilrs::Button;
use rbr_gameplay::flow::Mode;
use rbr_gameplay::pause_menu::{self, Item, MenuState};
use rbr_recording::menu::Menu as Record;
use winit::keyboard::KeyCode;

const PANEL: Color32 = Color32::from_rgba_premultiplied(10, 10, 12, 235);
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(110);
const HINT: Color32 = Color32::from_gray(160);
const SELECTED: Color32 = Color32::from_rgb(200, 60, 40);
/// The paused game behind the menu.
const PAUSE: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 140);
const WIDTH: f32 = 360.0;
const ROW: f32 = 40.0;

/// A move in the menu: the arrows or the gamepad's d-pad, the select (Enter, the gamepad's South) and the back
/// (Escape, the pause binding, the gamepad's East).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Select,
    Back,
}

impl Nav {
    /// A key's move in the open menu: the arrows (and W, S), Enter or Space, and Backspace (Escape and the pause
    /// binding go through the pause action).
    pub fn of_key(key: KeyCode) -> Option<Nav> {
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => Some(Nav::Up),
            KeyCode::ArrowDown | KeyCode::KeyS => Some(Nav::Down),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Some(Nav::Select),
            KeyCode::Backspace => Some(Nav::Back),
            _ => None,
        }
    }

    /// A gamepad button's move: the d-pad, South (A, cross) and East (B, circle); Start is the pause binding's.
    pub fn of_button(button: Button) -> Option<Nav> {
        match button {
            Button::DPadUp => Some(Nav::Up),
            Button::DPadDown => Some(Nav::Down),
            Button::South => Some(Nav::Select),
            Button::East => Some(Nav::Back),
            _ => None,
        }
    }
}

/// The open menu: its items, the selected one and the confirm box that is up.
#[derive(Debug, Clone, PartialEq)]
pub struct PauseMenu {
    mode: Mode,
    over: bool,
    items: Vec<(Item, bool)>,
    selected: usize,
    /// The item asking, and whether its box has Yes selected.
    confirm: Option<(Item, bool)>,
}

impl PauseMenu {
    /// The menu as the race stands (the last MNU1).
    pub fn open(record: Record) -> PauseMenu {
        let state =
            MenuState { mode: Mode(record.mode), driving: record.driving, help: record.help, over: record.over };
        PauseMenu { mode: state.mode, over: state.over, items: pause_menu::items(state), selected: 0, confirm: None }
    }

    /// Takes one move; returns the item chosen (a confirmed one, or `Item::Resume` for the back), which closes the
    /// menu.
    pub fn nav(&mut self, nav: Nav) -> Option<Item> {
        if let Some((item, yes)) = self.confirm {
            match nav {
                Nav::Up | Nav::Down => self.confirm = Some((item, !yes)),
                Nav::Select if yes => return Some(item),
                Nav::Select | Nav::Back => self.confirm = None,
            }
            return None;
        }
        let step = |from: usize, by: usize| {
            let n = self.items.len();
            (1..=n).map(|k| (from + by * k) % n).find(|&i| self.items[i].1).unwrap_or(from)
        };
        match nav {
            Nav::Up => self.selected = step(self.selected, self.items.len() - 1),
            Nav::Down => self.selected = step(self.selected, 1),
            Nav::Back => return Some(Item::Resume),
            Nav::Select => {
                let item = self.items[self.selected].0;
                match item.confirm(self.mode, self.over) {
                    Some(_) => self.confirm = Some((item, true)),
                    None => return Some(item),
                }
            }
        }
        None
    }
}

/// The English text of a `bstrings` key the menu uses.
pub fn text(key: &str) -> &'static str {
    match key {
        "PAUSED" => "Paused",
        "RESUME" => "Resume",
        "HELPCALL" => "Call for help",
        "RESTART" => "Restart",
        "RETIRE" => "Retire",
        "EXIT2MM" => "Exit To Main Menu",
        "CONTINUE" => "Continue",
        "YES" => "Yes",
        "NO" => "No",
        "MSG_RESTART" => "Are you sure you want to restart the stage?",
        "MSG_RET_STAGE" => "Are you sure you want to retire from the stage?",
        "MSG_RET_RALLY" => {
            "Are you sure you want to retire from the rally? You will be unplaced in this rally and not awarded any \
             points!"
        }
        "MSG_CONFIRM_EXIT" => "Exit to Main Menu?\nAll progress will be lost.",
        _ => "",
    }
}

/// Draws the menu over the dimmed game, or the confirm box in its place.
pub fn paint(ctx: &egui::Context, menu: &PauseMenu) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("pause")));
    let screen = ctx.content_rect();
    painter.rect_filled(screen, 0.0, PAUSE);
    if let Some((item, yes)) = menu.confirm {
        confirm(&painter, screen, text(item.confirm(menu.mode, menu.over).unwrap_or_default()), yes);
        return;
    }
    let rows = menu.items.len() as f32;
    let panel = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, 84.0 + ROW * rows));
    painter.rect_filled(panel, 8.0, PANEL);
    let mut at = Pos2::new(panel.center().x, panel.min.y + 18.0);
    painter.text(at, Align2::CENTER_TOP, text("PAUSED"), FontId::proportional(26.0), TEXT);
    at.y += 54.0;
    for (i, &(item, enabled)) in menu.items.iter().enumerate() {
        let row = Rect::from_center_size(Pos2::new(at.x, at.y + ROW / 2.0), Vec2::new(WIDTH - 40.0, ROW - 6.0));
        if i == menu.selected {
            painter.rect_filled(row, 4.0, SELECTED);
        }
        let colour = if enabled { TEXT } else { DIM };
        painter.text(row.center(), Align2::CENTER_CENTER, text(item.key()), FontId::proportional(20.0), colour);
        at.y += ROW;
    }
    let hint = Pos2::new(panel.center().x, panel.max.y + 10.0);
    painter.text(
        hint,
        Align2::CENTER_TOP,
        "Up/Down choose    Enter select    Esc resume",
        FontId::proportional(13.0),
        HINT,
    );
}

/// The confirm box: the question, wrapped, and Yes and No.
fn confirm(painter: &Painter, screen: Rect, question: &str, yes: bool) {
    let wrap = 440.0;
    let galley = painter.layout(question.to_owned(), FontId::proportional(18.0), TEXT, wrap);
    let panel = Rect::from_center_size(screen.center(), Vec2::new(wrap + 60.0, galley.size().y + 110.0));
    painter.rect_filled(panel, 8.0, PANEL);
    let top = Pos2::new(panel.center().x - galley.size().x / 2.0, panel.min.y + 26.0);
    painter.galley(top, galley, TEXT);
    let buttons = [("YES", yes), ("NO", !yes)];
    for (k, (key, selected)) in buttons.into_iter().enumerate() {
        let centre = Pos2::new(panel.center().x + (k as f32 - 0.5) * 140.0, panel.max.y - 34.0);
        let button = Rect::from_center_size(centre, Vec2::new(120.0, 34.0));
        if selected {
            painter.rect_filled(button, 4.0, SELECTED);
        }
        painter.text(centre, Align2::CENTER_CENTER, text(key), FontId::proportional(18.0), TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick(driving: bool) -> PauseMenu {
        PauseMenu::open(Record { mode: 1, driving, help: true, over: false })
    }

    #[test]
    fn the_keys_move_choose_and_confirm() {
        let mut menu = quick(true);
        assert_eq!(menu.nav(Nav::Select), Some(Item::Resume));
        assert_eq!(menu.nav(Nav::Down), None);
        assert_eq!(menu.nav(Nav::Select), Some(Item::CallForHelp));
        // Up from the top wraps to the exit, which asks first; No goes back to the menu.
        let mut menu = quick(true);
        menu.nav(Nav::Up);
        assert_eq!(menu.nav(Nav::Select), None);
        assert_eq!(menu.confirm, Some((Item::Exit, true)));
        menu.nav(Nav::Down);
        assert_eq!(menu.nav(Nav::Select), None);
        assert_eq!(menu.confirm, None);
        menu.nav(Nav::Select);
        assert_eq!(menu.nav(Nav::Select), Some(Item::Exit));
        // The back closes the box, then the menu.
        let mut menu = quick(true);
        menu.nav(Nav::Up);
        menu.nav(Nav::Select);
        assert_eq!(menu.nav(Nav::Back), None);
        assert_eq!(menu.nav(Nav::Back), Some(Item::Resume));
    }

    #[test]
    fn a_greyed_item_is_skipped() {
        // Page 1 outside mode 1 greys the retire.
        let mut menu = PauseMenu::open(Record { mode: 2, driving: true, help: false, over: false });
        let shown: Vec<Item> = menu.items.iter().map(|&(item, _)| item).collect();
        assert_eq!(shown, [Item::Resume, Item::Restart, Item::Retire, Item::Exit]);
        menu.nav(Nav::Down);
        menu.nav(Nav::Down);
        assert_eq!(menu.items[menu.selected].0, Item::Exit);
    }

    #[test]
    fn a_championship_retire_asks_the_rally_s_question() {
        let mut menu = PauseMenu::open(Record { mode: 3, driving: true, help: false, over: false });
        menu.nav(Nav::Down);
        assert_eq!(menu.nav(Nav::Select), None);
        assert_eq!(menu.confirm, Some((Item::Retire, true)));
        assert_eq!(menu.nav(Nav::Select), Some(Item::Retire));
    }

    /// The texts are the English table's (`RBR_DATA`, `RBR_PATCH`).
    #[test]
    fn the_texts_are_the_games() {
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        let Ok(strings) = rbr_formats::strings::english(&data) else { return };
        let keys = [
            "PAUSED",
            "RESUME",
            "HELPCALL",
            "RESTART",
            "RETIRE",
            "EXIT2MM",
            "CONTINUE",
            "YES",
            "NO",
            "MSG_RESTART",
            "MSG_RET_STAGE",
            "MSG_RET_RALLY",
            "MSG_CONFIRM_EXIT",
        ];
        for key in keys {
            assert_eq!(text(key), strings.get(key).unwrap().trim_end(), "{key}");
        }
    }
}
