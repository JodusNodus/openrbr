//! The retire's screens over the HUD, from the live race-flow record (`rbr_recording::flow`):
//! - the full-screen tints `HUD::Update` draws last (0x573630, 0x5736b0): red over the 641x482 screen with the alpha
//!   `1 - t` of the rally incident's timer (HUD +0x27c), and black with the off-stage timer's (+0x280), each while its
//!   timer is not below 0 and the helicopter is not on the stage (`flow::Tints`);
//! - a retire's message box (`MSG_RETIRE_BYSTAND`, `MSG_RETIRE_OFFSTAGE`, posted by `M0058b060`), over the paused
//!   game until the player accepts it (the viewer's accept key, `Action::Accept`); its accept ends the race;
//! - the championship's FAILED page after a finish in a car that can't drive (end page 0x18,
//!   `CFailedSeasonFinishedWithBadCarUI`): `MSG_UNFINISH_SERVICE` when the season's queue holds a service, else
//!   `MSG_UNFINISH_STAGE`.
//!
//! The texts are 1.02's English string table's (`bstrings.eng`); the panels' look is ours, as the rest of the HUD's.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};
use rbr_recording::flow::MessageBox;
use rbr_recording::splits::Splits;

use super::model::HudView;
use super::time_label;

/// The end page of a finish in a car that can't drive, before the rally's last stage.
pub const PAGE_FAILED: u32 = 0x18;
const PANEL: Color32 = Color32::from_rgba_premultiplied(10, 10, 12, 235);
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(160);
const FAILED: Color32 = Color32::from_rgb(220, 70, 60);
/// The pause behind a message box: the game shows its pause menu's page there (ours: a dimmed screen).
const PAUSE: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 110);

/// The box's text, `bstrings.eng`'s.
pub fn message_text(message: MessageBox) -> &'static str {
    match message {
        MessageBox::RetireBystand => "You have collided with a bystander.\nYou have been retired from this event.",
        MessageBox::RetireOffstage => {
            "You've gone off the stage and your car is un-drivable.\nYou have been retired from this event."
        }
    }
}

/// Page 0x18's text by what follows the stage.
pub fn failed_text(service_next: bool) -> &'static str {
    if service_next {
        "Your car is undrivable.\n\nAs you cannot get to the service park you have been retired from the rally."
    } else {
        "Your car is undrivable.\n\nAs you cannot get to the next stage you have been retired from the rally."
    }
}

/// Draws the tints, the message box and page 0x18 over the HUD.
pub fn paint(ctx: &egui::Context, state: &HudView, splits: Option<&Splits>) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("retire")));
    let screen = ctx.content_rect();
    let tint = |alpha: Option<f32>, [r, g, b]: [u8; 3]| {
        if let Some(a) = alpha {
            let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
            painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(r, g, b, a));
        }
    };
    tint(state.red, [255, 0, 0]);
    tint(state.black, [0, 0, 0]);
    if let Some(message) = state.message_box {
        painter.rect_filled(screen, 0.0, PAUSE);
        dialog(&painter, screen, "Retired", message_text(message), "accept: Enter", DIM);
    }
    if state.end_page == Some(PAGE_FAILED) {
        let time = splits.map(|s| format!("stage time {} (it counts)", time_label(f64::from(s.race_time))));
        dialog(&painter, screen, "Failed", failed_text(state.service_next), &time.unwrap_or_default(), FAILED);
    }
}

/// A centred panel: the title, the text and a footer line.
fn dialog(painter: &Painter, screen: Rect, title: &str, text: &str, footer: &str, title_colour: Color32) {
    let lines = text.lines().count() as f32;
    let size = Vec2::new(560.0, 96.0 + 20.0 * lines);
    let panel = Rect::from_center_size(screen.center(), size);
    painter.rect_filled(panel, 8.0, PANEL);
    let mut at = Pos2::new(panel.center().x, panel.min.y + 16.0);
    painter.text(at, Align2::CENTER_TOP, title, FontId::proportional(24.0), title_colour);
    at.y += 40.0;
    for line in text.lines() {
        painter.text(at, Align2::CENTER_TOP, line, FontId::proportional(16.0), TEXT);
        at.y += 20.0;
    }
    painter.text(Pos2::new(at.x, panel.max.y - 26.0), Align2::CENTER_TOP, footer, FontId::proportional(13.0), DIM);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts are the English table's (`RBR_DATA`, `RBR_PATCH`).
    #[test]
    fn the_texts_are_the_games() {
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        let Ok(strings) = rbr_formats::strings::english(&data) else { return };
        let game = |key: &str| strings.get(key).unwrap().trim_end().to_owned();
        assert_eq!(message_text(MessageBox::RetireBystand), game("MSG_RETIRE_BYSTAND"));
        assert_eq!(message_text(MessageBox::RetireOffstage), game("MSG_RETIRE_OFFSTAGE"));
        assert_eq!(failed_text(true), game("MSG_UNFINISH_SERVICE"));
        assert_eq!(failed_text(false), game("MSG_UNFINISH_STAGE"));
    }
}
