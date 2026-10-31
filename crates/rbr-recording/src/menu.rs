//! The pause-menu record `rbr-standalone --live` writes after each answer's other tagged records, before SCN1 (ours):
//! where the race stands for the viewer's pause menu (`rbr_gameplay::pause_menu`), so it shows the game's items for
//! the mode and the moment. A reader without the record (`harness/live.py`, an older process) offers the quick rally's
//! page without the call for help and the retire. Little-endian body:
//!
//! ```text
//! u32 mode     the flow word (1 the quick rally, 3 the championship)
//! u32 flags    bit 0: the application state is 1 (driving) with no message box up; bit 1: the call for help is
//!              offered (`0x56a620`); bit 2: the race is over
//! ```

use std::io;

use crate::{sized, word};

/// The tag.
pub const MNU1: [u8; 4] = *b"MNU1";
/// The body's size.
pub const BODY_BYTES: usize = 8;
const DRIVING: u32 = 1;
const HELP: u32 = 2;
const OVER: u32 = 4;

/// One pause-menu record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    pub mode: u32,
    pub driving: bool,
    pub help: bool,
    pub over: bool,
}

impl Default for Menu {
    /// The quick rally's, nothing offered beyond the page.
    fn default() -> Menu {
        Menu { mode: 1, driving: false, help: false, over: false }
    }
}

impl Menu {
    /// The tagged record.
    pub fn record(&self) -> Vec<u8> {
        let flags = [(self.driving, DRIVING), (self.help, HELP), (self.over, OVER)]
            .iter()
            .filter(|(set, _)| *set)
            .fold(0, |flags, (_, bit)| flags | bit);
        let mut body = self.mode.to_le_bytes().to_vec();
        body.extend_from_slice(&u32::to_le_bytes(flags));
        crate::live::record(MNU1, &body)
    }

    /// The record's body (the bytes after the tag and length).
    pub fn read_body(body: &[u8]) -> io::Result<Menu> {
        let body: &[u8; BODY_BYTES] = sized(body, format!("an MNU1 body of {} bytes", body.len()))?;
        let flags = word(body, 1);
        Ok(Menu {
            mode: word(body, 0),
            driving: flags & DRIVING != 0,
            help: flags & HELP != 0,
            over: flags & OVER != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_round_trips() {
        let menu = Menu { mode: 3, driving: true, help: true, over: false };
        let bytes = menu.record();
        assert_eq!(&bytes[..4], b"MNU1");
        assert_eq!(bytes[4..8], 8u32.to_le_bytes());
        assert_eq!(Menu::read_body(&bytes[8..]).unwrap(), menu);
        let over = Menu { over: true, ..Menu::default() };
        assert_eq!(Menu::read_body(&over.record()[8..]).unwrap(), over);
        assert!(Menu::read_body(&bytes[8..12]).is_err());
    }
}
