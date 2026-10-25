//! Actions shown alongside the live stage result, including a retirement or failed season stage.
//! The pause menu supplies the mode's rules; continuing returns to the launcher, which saves and advances the season.

use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2};
use rbr_gameplay::flow::Mode;
use rbr_gameplay::pause_menu::{self, Item, MenuState};
use rbr_recording::menu::Menu as Record;

use super::pause::Nav;

const BUTTON: Vec2 = Vec2::new(220.0, 42.0);
const GAP: f32 = 12.0;
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(160);
const SELECTED: Color32 = Color32::from_rgb(255, 196, 0);
const PANEL: Color32 = Color32::from_rgba_premultiplied(10, 10, 12, 235);

#[derive(Debug, Clone)]
pub struct ResultsMenu {
    items: Vec<Item>,
    selected: usize,
}

impl ResultsMenu {
    /// Only the end page offers these actions. Crossing the finish line still lets the car drive to the marshal.
    pub fn open(record: Record) -> Option<Self> {
        if !record.over {
            return None;
        }
        let state = MenuState { mode: Mode(record.mode), driving: false, help: false, over: true };
        let items = pause_menu::items(state)
            .into_iter()
            .filter_map(|(item, enabled)| {
                (enabled && matches!(item, Item::Restart | Item::Exit | Item::Continue)).then_some(item)
            })
            .collect();
        Some(Self { items, selected: 0 })
    }

    pub fn nav(&mut self, nav: Nav) -> Option<Item> {
        let n = self.items.len();
        match nav {
            Nav::Left | Nav::Up => self.selected = (self.selected + n - 1) % n,
            Nav::Right | Nav::Down => self.selected = (self.selected + 1) % n,
            Nav::Select => return Some(self.items[self.selected]),
            Nav::Back => return self.items.last().copied(),
        }
        None
    }

    fn button(&self, screen: Rect, index: usize) -> Rect {
        let width = self.items.len() as f32 * (BUTTON.x + GAP) - GAP;
        let at = Pos2::new(screen.center().x - width / 2.0 + index as f32 * (BUTTON.x + GAP), screen.max.y - 128.0);
        Rect::from_min_size(at, BUTTON)
    }

    /// Pointer coordinates use the same 720-point-high screen as the HUD.
    pub fn hover(&mut self, screen: Rect, pointer: Pos2) {
        if let Some(index) = (0..self.items.len()).find(|&i| self.button(screen, i).contains(pointer)) {
            self.selected = index;
        }
    }

    pub fn click(&self, screen: Rect, pointer: Pos2) -> Option<Item> {
        self.items.iter().enumerate().find_map(|(i, &item)| self.button(screen, i).contains(pointer).then_some(item))
    }
}

pub fn paint(ctx: &egui::Context, menu: &ResultsMenu) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("results-actions")));
    let screen = ctx.content_rect();
    for (i, &item) in menu.items.iter().enumerate() {
        let rect = menu.button(screen, i);
        painter.rect_filled(rect, 6.0, if i == menu.selected { SELECTED } else { PANEL });
        let label = match item {
            Item::Restart => "Restart stage",
            Item::Exit => "Quit to menu",
            Item::Continue => "Continue",
            _ => unreachable!(),
        };
        let colour = if i == menu.selected { Color32::from_gray(15) } else { TEXT };
        painter.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(18.0), colour);
    }
    let hint = if menu.items.contains(&Item::Continue) {
        "Return to the championship to continue your season. Enter / A select, Esc returns."
    } else {
        "Choose an action with the arrows or D-pad, then Enter or A. Esc returns to the menu."
    };
    painter.text(
        Pos2::new(screen.center().x, screen.max.y - 72.0),
        Align2::CENTER_TOP,
        hint,
        FontId::proportional(13.0),
        DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(mode: u32) -> ResultsMenu {
        ResultsMenu::open(Record { mode, over: true, driving: false, help: false }).unwrap()
    }

    #[test]
    fn finishing_and_starting_still_allow_driving() {
        for driving in [true, false] {
            assert!(ResultsMenu::open(Record { driving, ..Record::default() }).is_none());
        }
    }

    #[test]
    fn quick_rally_can_restart_or_leave_and_season_can_only_continue() {
        let mut quick = open(1);
        assert_eq!(quick.nav(Nav::Select), Some(Item::Restart));
        quick.nav(Nav::Left);
        assert_eq!(quick.nav(Nav::Select), Some(Item::Exit));
        quick.nav(Nav::Down);
        assert_eq!(quick.nav(Nav::Select), Some(Item::Restart));
        assert_eq!(quick.nav(Nav::Back), Some(Item::Exit));
        let mut season = open(3);
        for nav in [Nav::Up, Nav::Down, Nav::Left, Nav::Right] {
            season.nav(nav);
            assert_eq!(season.nav(Nav::Select), Some(Item::Continue));
        }
        assert_eq!(season.nav(Nav::Back), Some(Item::Continue));
    }

    #[test]
    fn mouse_targets_match_the_drawn_buttons_at_different_aspects() {
        for width in [960.0, 1280.0, 1920.0] {
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 720.0));
            for mode in [1, 3] {
                let mut menu = open(mode);
                assert_eq!(menu.click(screen, screen.center()), None);
                for i in 0..menu.items.len() {
                    let pointer = menu.button(screen, i).center();
                    menu.hover(screen, pointer);
                    assert_eq!(menu.nav(Nav::Select), Some(menu.items[i]));
                    assert_eq!(menu.click(screen, pointer), Some(menu.items[i]));
                }
            }
        }
    }
}
