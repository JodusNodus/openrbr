//! The in-race pause menu: which page the game opens, which items it shows, which it greys out, and the confirm box
//! each item asks first. Pure; the viewer draws it and the stepping process carries out what is chosen.
//!
//! Original: the pause (`CSigMenuActivate`, the PAUSE action) reaches `CApplication::M0047f9c0` (0x47f9c0), which in
//! the driving state (1) opens the page `FUN_0056c360` (0x56c360) picks by the flow word and sets the application
//! state 2: the car is not stepped and the HUD's clocks wait. Modes 3-5 (the championship and two multiplayer modes)
//! get page 2, `CGameMenuDaemon::M56ee00` (0x56ee00); the others page 1, `M56eb40` (0x56eb40). Both are titled
//! `OPTIONS`. Each item is a `CInvokeMenuItem`; `M584ce0` (0x584ce0) attaches a predicate, which `M5c0670` (0x5c0670)
//! applies through slot 0x10 (op 0x25: item flag 2, drawn or not, `CStdButtonUI::vf0c` 0x5bc590) or slot 0x14 (op
//! 0x26: flag 4, selectable or greyed, 0x5bc6xx's draw style 3 or 4).
//!
//! - Page 1: `RESUME` (0x59b6d0), `HELPCALL` (0x56e6d0, shown when 0x56a620 holds), `OPTIONS` (shown when the flow's
//!   +0x1785c is 0, 0x5826a0), `RESTART` (0x56a600: box 0x3b `MSG_RESTART`, then `SetUnk724_3`, the restart),
//!   `CRetireFromRaceUI` (0x56eac0, selectable in mode 1 only, 0x56a6c0), `EXIT2MM` (0x56a6a0: box 0x3d
//!   `MSG_CONFIRM_EXIT`, then 0x56a680, the main menu; no result).
//! - Page 2: `RESUME`, `HELPCALL` (as on page 1), `OPTIONS`, `CRetireFromRaceUI`, `CMPEndButtonUI` (0x56a740,
//!   selectable in modes 4 and 5, 0x56a6e0). No restart and no exit: a championship stage is left by retiring.
//! - The retire (0x56eac0) asks by mode: 3 box 0x1a `MSG_RET_RALLY` (0x56e820), 4 box 0x19 (0x56e9d0), 5 box 0x10
//!   (0x56ea40), else box 0x19 `MSG_RET_STAGE` (0x56e8a0). Each callback, on yes, emits `CSigEndRace` with 1: the race
//!   ends retired and the end page follows (`flow::EndPage`: 0x12 in the quick rally, 0x17 FAILED in the championship,
//!   whose season retires this and the later stages).
//! - `HELPCALL` emits `CSigRespawnRequest(1)` and resumes (`game::FrameInput::help_call`).
//!
//! Ours: no `OPTIONS` item (the launcher holds the settings) and no multiplayer item; the menu also opens outside the
//! driving state (the start orbit, the end page), where only Resume, Restart and the exit of page 1 are offered, and
//! after a championship stage's end `Continue` (back to the launcher, which goes on with the rally).

use crate::flow::Mode;

/// The pause page `FUN_0056c360` opens in the driving state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Page 1 (`M56eb40`): the quick rally's and every mode's but 3-5.
    Race,
    /// Page 2 (`M56ee00`): modes 3-5, the championship's.
    Rally,
}

impl Page {
    /// `FUN_0056c360`'s switch on the flow word (mode 7's school pages and mode 9's page 4 are out of scope).
    pub fn of(mode: Mode) -> Page {
        match mode.0 {
            3..=5 => Page::Rally,
            _ => Page::Race,
        }
    }
}

/// A pause-menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// `RESUME`: closes the menu.
    Resume,
    /// `HELPCALL`: the call for help, then resumes.
    CallForHelp,
    /// `RESTART`: the stage again from its start.
    Restart,
    /// `CRetireFromRaceUI`: the race ends retired.
    Retire,
    /// `EXIT2MM`: leaves the race with no result.
    Exit,
    /// Ours, after the race's end in a championship: back to the launcher, which goes on with the rally.
    Continue,
}

impl Item {
    /// The text key in `bstrings`.
    pub fn key(self) -> &'static str {
        match self {
            Item::Resume => "RESUME",
            Item::CallForHelp => "HELPCALL",
            Item::Restart => "RESTART",
            Item::Retire => "RETIRE",
            Item::Exit => "EXIT2MM",
            Item::Continue => "CONTINUE",
        }
    }

    /// The confirm box the item asks before it acts, by its text key, in `mode`; `None` acts at once.
    pub fn confirm(self, mode: Mode, over: bool) -> Option<&'static str> {
        match self {
            Item::Restart => Some("MSG_RESTART"),
            Item::Retire if Page::of(mode) == Page::Rally => Some("MSG_RET_RALLY"),
            Item::Retire => Some("MSG_RET_STAGE"),
            Item::Exit if !over => Some("MSG_CONFIRM_EXIT"),
            _ => None,
        }
    }
}

/// Where the race stands when the menu opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuState {
    /// The flow word.
    pub mode: Mode,
    /// The application state is 1 (driving), the state the game opens its pause page in, and no box is up.
    pub driving: bool,
    /// `0x56a620` holds (`Game::help_offered`).
    pub help: bool,
    /// The race is over (its end page).
    pub over: bool,
}

/// The items the menu shows, in the page's order, each with whether it can be chosen (else drawn greyed).
pub fn items(state: MenuState) -> Vec<(Item, bool)> {
    let page = Page::of(state.mode);
    let mut out = vec![(Item::Resume, true)];
    if state.driving && state.help {
        out.push((Item::CallForHelp, true));
    }
    if page == Page::Race {
        out.push((Item::Restart, true));
    }
    if state.driving {
        // 0x56a6c0 on page 1 (mode 1 only); page 2 always.
        out.push((Item::Retire, page == Page::Rally || state.mode == Mode::QUICK_RALLY));
    }
    match (page, state.over) {
        (Page::Race, _) => out.push((Item::Exit, true)),
        (Page::Rally, true) => out.push((Item::Continue, true)),
        // A championship stage that is not over is left by retiring: the game has no exit there.
        (Page::Rally, false) => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(state: MenuState) -> Vec<Item> {
        items(state).into_iter().map(|(item, _)| item).collect()
    }

    #[test]
    fn the_page_follows_the_mode() {
        assert_eq!(Page::of(Mode::QUICK_RALLY), Page::Race);
        assert_eq!(Page::of(Mode::MENU), Page::Race);
        assert_eq!(Page::of(Mode::CHAMPIONSHIP), Page::Rally);
        assert_eq!(Page::of(Mode(4)), Page::Rally);
        assert_eq!(Page::of(Mode(5)), Page::Rally);
        assert_eq!(Page::of(Mode(6)), Page::Race);
    }

    #[test]
    fn a_quick_rally_offers_restart_retire_and_exit() {
        let racing = MenuState { mode: Mode::QUICK_RALLY, driving: true, help: true, over: false };
        assert_eq!(
            items(racing),
            [
                (Item::Resume, true),
                (Item::CallForHelp, true),
                (Item::Restart, true),
                (Item::Retire, true),
                (Item::Exit, true)
            ]
        );
        // The call for help is not drawn while 0x56a620 fails.
        assert_eq!(shown(MenuState { help: false, ..racing }), [Item::Resume, Item::Restart, Item::Retire, Item::Exit]);
        // Page 1 in another mode greys the retire (0x56a6c0).
        assert!(items(MenuState { mode: Mode(2), ..racing }).contains(&(Item::Retire, false)));
        assert_eq!(Item::Retire.confirm(Mode::QUICK_RALLY, false), Some("MSG_RET_STAGE"));
        assert_eq!(Item::Exit.confirm(Mode::QUICK_RALLY, false), Some("MSG_CONFIRM_EXIT"));
        assert_eq!(Item::Restart.confirm(Mode::QUICK_RALLY, false), Some("MSG_RESTART"));
        assert_eq!(Item::Resume.confirm(Mode::QUICK_RALLY, false), None);
    }

    #[test]
    fn a_championship_has_no_restart_and_no_exit_while_racing() {
        let racing = MenuState { mode: Mode::CHAMPIONSHIP, driving: true, help: true, over: false };
        assert_eq!(items(racing), [(Item::Resume, true), (Item::CallForHelp, true), (Item::Retire, true)]);
        assert_eq!(Item::Retire.confirm(Mode::CHAMPIONSHIP, false), Some("MSG_RET_RALLY"));
        let over = MenuState { driving: false, help: false, over: true, ..racing };
        assert_eq!(shown(over), [Item::Resume, Item::Continue]);
        let start = MenuState { driving: false, help: false, ..racing };
        assert_eq!(shown(start), [Item::Resume]);
    }

    #[test]
    fn outside_the_driving_state_there_is_no_help_and_no_retire() {
        let start = MenuState { mode: Mode::QUICK_RALLY, driving: false, help: true, over: false };
        assert_eq!(shown(start), [Item::Resume, Item::Restart, Item::Exit]);
        assert_eq!(Item::Exit.confirm(Mode::QUICK_RALLY, true), None);
    }
}
