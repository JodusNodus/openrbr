//! The keys, gamepad presses and the pause menu.

use super::App;
use crate::args::Args;
use rbr_gameplay::pause_menu::{Item, Page};
use rbr_render::bindings::Action;
use rbr_render::hud::camera_page::CameraPage;
use rbr_render::hud::pause::{Nav, Outcome, PauseMenu};
use rbr_render::live::{self, Controls, Live};
use rbr_render::playback::Drive;
use rbr_render::shifter::Shifter;
use std::collections::HashSet;
use std::time::Instant;
use winit::keyboard::KeyCode;

impl App {
    /// A live-mode press of a gear, restart, call-for-help or pause key or gamepad input, or in either mode of the
    /// wipers, headlights, camera or gauge mode ones. While the pause menu is open only the pause goes through: it
    /// closes the menu. While a message box is up the pause accepts it.
    pub fn press(&mut self, action: Action) {
        if self.results.is_some() {
            match action {
                Action::Pause => self.results_nav(Nav::Back),
                Action::Restart
                    if self
                        .scene
                        .live
                        .as_ref()
                        .is_some_and(|p| Page::of(rbr_gameplay::flow::Mode(p.menu.mode)) == Page::Race) =>
                {
                    self.restart_live()
                }
                _ => {}
            }
            return;
        }
        if action == Action::Pause && self.accept_message() {
            return;
        }
        if action == Action::Pause && self.scene.live.is_some() {
            match self.menu {
                Some(_) => self.menu_nav(Nav::Back),
                None => self.open_menu(),
            }
            return;
        }
        if self.menu.is_some() {
            return;
        }
        if let (Action::Wipers, Some(glass)) = (action, &mut self.scene.glass) {
            glass.switch = glass.switch.next();
            eprintln!("viewer: wipers {}", glass.switch.name());
        }
        if let (Action::Headlights, Some(playback)) = (action, &mut self.scene.playback) {
            playback.headlights = playback.headlights.next();
            eprintln!("viewer: headlights {}", playback.headlights.name());
        }
        if action == Action::Camera && self.scene.playback.is_some() && self.shown.may_cycle() {
            self.view = self.view.next();
            self.scene.cameras.chose(self.view);
        }
        if let (Action::GaugeMode, Some(dials)) = (action, &mut self.scene.dials) {
            dials.next_mode();
        }
        let Some(process) = &self.scene.live else {
            return;
        };
        match action {
            // The poll gives the shifts and the direct gear actions (`rbr_render::devices`).
            Action::GearUp | Action::GearDown => {}
            a if a.gear().is_some() => {}
            Action::CallForHelp => self.controls.flags |= live::CALL_FOR_HELP,
            // The game restarts from its pause page 1 only: a championship stage has no restart.
            Action::Restart if Page::of(rbr_gameplay::flow::Mode(process.menu.mode)) == Page::Race => {
                self.restart_live()
            }
            _ => {}
        }
    }

    /// Accepts the race's message box (a retire's) if one is up, or confirms the start of a championship stage, whose
    /// countdown waits for it (`rbr_gameplay::flow::Mode::confirm_arms`); whether either was. The game's box keeps the
    /// menu's input until it is accepted, so the pause, Enter, the gamepad's South and a click all accept it.
    pub fn accept_message(&mut self) -> bool {
        let up = self.scene.live.as_ref().is_some_and(|live| {
            let waits =
                rbr_gameplay::flow::Mode(live.menu.mode).confirm_arms() && !live.menu.driving && !live.menu.over;
            live.hud_model.view().message_box.is_some() || waits
        });
        if up {
            self.controls.flags |= live::ACCEPT_MESSAGE;
        }
        up
    }

    /// Starts the stage over in the live process (the stage's start).
    pub fn restart_live(&mut self) {
        let (Some(process), Some(playback)) = (&mut self.scene.live, &mut self.scene.playback) else {
            return;
        };
        match restart(process) {
            Ok(drive) => {
                playback.reset(drive);
                self.running.renderer.reset_history();
                self.results = None;
                self.ended = None;
                self.controls = Controls::default();
                if let Some(shifter) = &mut self.shifter {
                    *shifter = Shifter::new(shifter.gearbox, shifter.clutch_threshold, shifter.auto_neutral);
                }
            }
            Err(e) => eprintln!("viewer: {e}"),
        }
    }

    /// Opens the pause menu and pauses the drive: the process's clock stopped (`Live::pause`), the sound stopped
    /// where it is, the wheel's force at 0, and the viewer's clocks (the characters, the renderer's time) held.
    pub fn open_menu(&mut self) {
        let Some(process) = &mut self.scene.live else {
            return;
        };
        self.menu = Some(PauseMenu::open(process.menu, CameraPage::new(self.view, self.scene.cameras.nudges())));
        process.pause();
        if let Some(sound) = &self.sound {
            sound.set_paused(true);
        }
        if let Some(wheel) = &self.wheel {
            wheel.set(0.0, 0.0);
        }
        self.paused_at = Some(Instant::now());
    }

    /// One move in the open menu, and what the chosen item does or the Camera page changed.
    pub fn menu_nav(&mut self, nav: Nav) {
        let item = match self.menu.as_mut().and_then(|menu| menu.nav(nav)) {
            None => return,
            Some(Outcome::Camera) => return self.camera_changed(),
            Some(Outcome::Chosen(item)) => item,
        };
        self.menu = None;
        if let Some(process) = &mut self.scene.live {
            process.resume();
        }
        if let Some(sound) = &self.sound {
            sound.set_paused(false);
        }
        if let Some(at) = self.paused_at.take() {
            self.paused += at.elapsed();
        }
        self.choose_item(item);
    }

    /// Both the pause menu and stage results use the same live actions.
    pub fn choose_item(&mut self, item: Item) {
        match item {
            Item::Resume => {}
            Item::CallForHelp => self.controls.flags |= live::CALL_FOR_HELP,
            Item::Retire => self.controls.flags |= live::RETIRE,
            Item::Restart => self.restart_live(),
            Item::Exit | Item::Continue => self.exit = true,
        }
    }

    /// Shows the view and the car's nudges the Camera page set, and writes them to the file.
    fn camera_changed(&mut self) {
        let Some(page) = self.menu.as_ref().and_then(PauseMenu::camera) else {
            return;
        };
        self.view = page.view;
        self.scene.cameras.set(page.view, page.nudges);
        self.scene.cameras.save();
        if let (Some(mounts), Some(playback)) = (&mut self.scene.mounts, &mut self.scene.playback) {
            self.scene.cameras.apply(mounts, playback);
        }
    }
}

/// The advanced H-shifter of a live drive, when the bindings turn it on and the car's gearbox (`--car-gearbox`) is
/// not sequential.
pub fn shifter(args: &Args, live: bool) -> Option<Shifter> {
    let settings = args.bindings.shifter;
    let sequential = args.car_gearbox.is_some_and(|c| c.sequential);
    (live && settings.advanced && !sequential).then(|| {
        let gearbox = settings.gearbox(args.car_gearbox);
        eprintln!("viewer: advanced H-shifter, {}", gearbox.arg());
        Shifter::new(gearbox, settings.clutch_threshold, args.bindings.neutral_on_release())
    })
}

/// Sends `gear` with the next step (`CDrive::M6840`).
pub fn select_gear(controls: &mut Controls, gear: u8) {
    controls.flags |= live::SELECT_GEAR;
    controls.gear = gear;
}

/// The movement the held keys ask for, in the camera's axes.
pub fn movement(held: &HashSet<KeyCode>) -> rbr_render::camera::Movement {
    let axis = |plus: KeyCode, minus: KeyCode| (held.contains(&plus) as i32 - held.contains(&minus) as i32) as f32;
    rbr_render::camera::Movement {
        forward: axis(KeyCode::KeyW, KeyCode::KeyS),
        right: axis(KeyCode::KeyD, KeyCode::KeyA),
        up: axis(KeyCode::Space, KeyCode::KeyC),
    }
}

/// The live process started over, with its first frame.
fn restart(process: &mut Live) -> Result<Drive, String> {
    let first = process.restart()?;
    Drive::parse_with(&first, process.clock)
}
