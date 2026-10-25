//! Stage-result actions and pointer coordinates in the HUD's screen.

use super::App;
use egui::{Pos2, Rect, Vec2};
use rbr_render::hud::pause::Nav;
use rbr_render::hud::results::ResultsMenu;
use rbr_render::hud::REFERENCE_HEIGHT;
use winit::keyboard::KeyCode;

impl App {
    pub fn sync_results(&mut self) {
        if self.results.is_none() && self.menu.is_none() {
            self.results = self.scene.live.as_ref().and_then(|process| ResultsMenu::open(process.menu));
            if self.results.is_some() {
                self.looking = false;
                if let Some(wheel) = &self.wheel {
                    wheel.set(0.0, 0.0);
                }
            }
        }
    }

    pub fn results_nav(&mut self, nav: Nav) {
        if let Some(item) = self.results.as_mut().and_then(|menu| menu.nav(nav)) {
            self.choose_item(item);
        }
    }

    pub fn results_key(&mut self, code: KeyCode, repeat: bool) {
        if let Some(nav) = Nav::of_key(code) {
            if !repeat || !matches!(nav, Nav::Select | Nav::Back) {
                self.results_nav(nav);
            }
        } else if !repeat {
            if code == KeyCode::Escape {
                self.results_nav(Nav::Back);
            } else if let Some(action) = self.bindings.action_for_key(code) {
                self.press(action);
            }
        }
    }

    fn results_pointer(&self) -> Option<(Rect, Pos2)> {
        let pointer = self.pointer?;
        let config = &self.running.config;
        let scale = config.height.max(1) as f32 / REFERENCE_HEIGHT;
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(config.width as f32, config.height as f32) / scale);
        Some((screen, Pos2::new(pointer.x as f32, pointer.y as f32) / scale))
    }

    pub fn results_hover(&mut self) {
        if let Some((screen, pointer)) = self.results_pointer() {
            if let Some(menu) = &mut self.results {
                menu.hover(screen, pointer);
            }
        }
    }

    pub fn results_click(&mut self) {
        let item = self.results_pointer().and_then(|(screen, pointer)| self.results.as_ref()?.click(screen, pointer));
        if let Some(item) = item {
            self.choose_item(item);
        }
    }
}
