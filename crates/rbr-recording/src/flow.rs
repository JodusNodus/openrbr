//! The race-flow record a live stepping process writes after the dash record (`rbr-standalone --live`): what the race
//! flow (`rbr_gameplay::flow`) emitted in the step and where it stands, so the viewer can run what the game runs on it
//! (the rescue helicopter's flight, the HUD's full-screen tints, the message box, the end page). Ours: the game's
//! receivers take the signals, not a record. Little-endian:
//!
//! ```text
//! u32 tag     'FLW1'
//! u32 length  16 (8 from an older process: no tints)
//! u32 flags   bit 0: the rescue helicopter was called; bit 1: CSigRallyIncident; bit 2: the finish line was passed;
//!             bit 3: the race ended (CSigEndRace); bit 4: the helicopter is on the stage; bit 5: the box
//!             MSG_RETIRE_BYSTAND is up; bit 6: the box MSG_RETIRE_OFFSTAGE is up; bit 7: a championship's next event
//!             is a service (page 0x18's text)
//! u32 page    the end page the race's end opens (`flow::EndPage::value`), 0 before the end
//! f32 red     the rally incident's red tint's alpha (`flow::Tints`), NaN when not drawn
//! f32 black   the off-stage fade's alpha, NaN when not drawn
//! ```
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length. A step
//! the game does not run (after the race's end) writes it with no flags.

use crate::word;

/// The tag.
pub const FLW1: [u8; 4] = *b"FLW1";
const PAYLOAD_BYTES: usize = 16;
const OLD_PAYLOAD_BYTES: usize = 8;
const HELICOPTER: u32 = 1;
const INCIDENT: u32 = 2;
const FINISH_LINE: u32 = 4;
const ENDED: u32 = 8;
const HELICOPTER_ACTIVE: u32 = 0x10;
const BOX_BYSTAND: u32 = 0x20;
const BOX_OFFSTAGE: u32 = 0x40;
const SERVICE_NEXT: u32 = 0x80;

/// The message box that is up (`flow::Message`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBox {
    RetireBystand,
    RetireOffstage,
}

/// One step's race-flow events and state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Flow {
    pub helicopter_called: bool,
    pub rally_incident: bool,
    pub finish_line: bool,
    pub ended: bool,
    pub page: u32,
    pub helicopter_active: bool,
    pub message: Option<MessageBox>,
    pub service_next: bool,
    pub red: Option<f32>,
    pub black: Option<f32>,
}

impl Flow {
    /// The tagged record.
    pub fn to_bytes(&self) -> Vec<u8> {
        let flags = [
            (self.helicopter_called, HELICOPTER),
            (self.rally_incident, INCIDENT),
            (self.finish_line, FINISH_LINE),
            (self.ended, ENDED),
            (self.helicopter_active, HELICOPTER_ACTIVE),
            (self.message == Some(MessageBox::RetireBystand), BOX_BYSTAND),
            (self.message == Some(MessageBox::RetireOffstage), BOX_OFFSTAGE),
            (self.service_next, SERVICE_NEXT),
        ]
        .iter()
        .filter(|(set, _)| *set)
        .fold(0, |flags, (_, bit)| flags | bit);
        let mut payload = flags.to_le_bytes().to_vec();
        payload.extend_from_slice(&self.page.to_le_bytes());
        for alpha in [self.red, self.black] {
            payload.extend_from_slice(&alpha.unwrap_or(f32::NAN).to_le_bytes());
        }
        crate::live::record(FLW1, &payload)
    }

    /// Reads the record's payload (the bytes after its tag and length); `None` when it is neither 16 nor 8 bytes.
    pub fn from_payload(payload: &[u8]) -> Option<Flow> {
        if payload.len() != PAYLOAD_BYTES && payload.len() != OLD_PAYLOAD_BYTES {
            return None;
        }
        let alpha =
            |i: usize| payload.get(4 * i..4 * i + 4).map(|_| f32::from_bits(word(payload, i))).filter(|a| !a.is_nan());
        let flags = word(payload, 0);
        let message = if flags & BOX_BYSTAND != 0 {
            Some(MessageBox::RetireBystand)
        } else if flags & BOX_OFFSTAGE != 0 {
            Some(MessageBox::RetireOffstage)
        } else {
            None
        };
        Some(Flow {
            helicopter_called: flags & HELICOPTER != 0,
            rally_incident: flags & INCIDENT != 0,
            finish_line: flags & FINISH_LINE != 0,
            ended: flags & ENDED != 0,
            page: word(payload, 1),
            helicopter_active: flags & HELICOPTER_ACTIVE != 0,
            message,
            service_next: flags & SERVICE_NEXT != 0,
            red: alpha(2),
            black: alpha(3),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let flow = Flow { helicopter_called: true, ended: true, page: 0x12, ..Flow::default() };
        let bytes = flow.to_bytes();
        assert_eq!(&bytes[..4], b"FLW1");
        assert_eq!(bytes[4..8], 16u32.to_le_bytes());
        assert_eq!(Flow::from_payload(&bytes[8..]), Some(flow));
        assert_eq!(Flow::from_payload(&bytes[8..12]), None);
        let up = Flow {
            helicopter_active: true,
            message: Some(MessageBox::RetireOffstage),
            service_next: true,
            red: Some(0.25),
            black: Some(0.5),
            ..Flow::default()
        };
        assert_eq!(Flow::from_payload(&up.to_bytes()[8..]), Some(up));
        let bystand = Flow { message: Some(MessageBox::RetireBystand), ..Flow::default() };
        assert_eq!(Flow::from_payload(&bystand.to_bytes()[8..]), Some(bystand));
    }

    /// An older process's 8-byte record reads without tints.
    #[test]
    fn an_old_record_reads() {
        let bytes = Flow { finish_line: true, page: 0x10, ..Flow::default() }.to_bytes();
        assert_eq!(Flow::from_payload(&bytes[8..16]), Some(Flow { finish_line: true, page: 0x10, ..Flow::default() }));
    }
}
