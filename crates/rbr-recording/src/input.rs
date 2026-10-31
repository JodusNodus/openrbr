//! The live control payload of the device input (ours): what the viewer's devices give in one poll, at full precision,
//! for `rbr-standalone --live` to put through the game's input layer as the original's poll does
//! (`USInputManagerImpl::Tic` 0x4cbf50 fills `CSigCarControlInput`; the filter and, under 1.02, the quantizer follow
//! in the game, `rbr_gameplay::filter` and `rbr_gameplay::control`).
//!
//! It is 32 bytes, the size of the recordings' car-control payload ([`crate::CONTROL_BYTES`]), which `--live` also
//! takes (a recorded drive played through the live process: `rbr-sound`, the tests). Word 7 tells them apart: a
//! recorded payload's is the aux bit (0 or 1), this payload's is [`INPUT_MAGIC`]. A process that knows neither
//! (an older `rbr-standalone`, `harness/live.py`) refuses the payload instead of driving with it. Little-endian:
//!
//! ```text
//! f32 throttle   0..1, the poll's +0x18 (the car's throttle, payload word 0)
//! f32 brake      0..1, +0x1c
//! f32 handbrake  0..1, +0x20
//! f32 clutch     0..1, +0x24
//! f32 steering   -1 (left) .. 1 (right), +0x14
//! u32 bits       0..7 the action flags (+0x00: 1 gear up, 2 gear down, 4 select `gear`, 0x20 call for help; ours:
//!                0x80 the player accepts the race's message box, `ACCEPT_MESSAGE`, 0x08 the player confirmed the pause
//!                menu's retire, `RETIRE`),
//!                8..17 the filter's cell per channel (+0x2c..+0x3c), 2 bits each in the order steering, throttle,
//!                brake, handbrake, clutch (0 an axis, 3 a digital button), 24 the aux bit (+0x04 bit 0, IGNITION)
//! u32 gear       0..7 the gear `select` selects (+0x28): 0 reverse, 1 neutral, 2 first, ...; 8..15 the payload
//!                periods the frame stands for (ours, `Input::frames`; 0 is 1); 16..23 the direct gear actions
//!                (reverse, neutral, first to sixth) held, 24..31 those bound (NGP's poll reads them,
//!                `rbr_gameplay::control::ngp_direct_gears`)
//! u32 magic      INPUT_MAGIC
//! ```

use crate::CONTROL_BYTES;

/// Word 7 of an input payload: `'INP2'` little-endian. A later layout takes a new value.
pub const INPUT_MAGIC: u32 = u32::from_le_bytes(*b"INP2");
/// Action flag (ours): the player accepts the race's message box (the box's accept button, `M0058a170`); not a car
/// control, so the game does not pass it to the input filter.
pub const ACCEPT_MESSAGE: u8 = 0x80;
/// Action flag (ours): the player confirmed the pause menu's retire (`rbr_gameplay::pause_menu::Item::Retire`); not a
/// car control either.
pub const RETIRE: u8 = 0x08;
/// The filter's cells (`rbr_gameplay::filter::Grid`): a device axis and a digital button.
pub const CELL_AXIS: u8 = 0;
pub const CELL_BUTTON: u8 = 3;
const AUX_BIT: u32 = 1 << 24;

/// One device poll.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Input {
    pub throttle: f32,
    pub brake: f32,
    pub handbrake: f32,
    pub clutch: f32,
    pub steering: f32,
    pub flags: u8,
    /// The filter's cell for steering, throttle, brake, handbrake and clutch.
    pub cells: [u8; 5],
    pub aux: bool,
    pub gear: u8,
    /// The payload periods of wall time this game frame stands for (ours): a viewer behind real time sends the
    /// periods it missed as one longer frame, as the game's frame clock takes the time since the last frame
    /// (`CPC_Application::Vf0c` 0x44a3d0, `rbr_gameplay::tick`). 0 and 1 are one period.
    pub frames: u8,
    /// The direct gear actions held past NGP's press (`rbr_gameplay::control::NGP_GEAR_PRESS`), bit `k` for action
    /// `k` (reverse, neutral, first to sixth), and those bound.
    pub gears_held: u8,
    pub gears_bound: u8,
}

/// What a 32-byte live payload holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Payload {
    /// A device poll.
    Input(Input),
    /// A recorded car-control payload (the recordings' layout), as its words.
    Recorded([u32; 8]),
}

impl Input {
    pub fn to_bytes(&self) -> [u8; CONTROL_BYTES] {
        let cells = self.cells.iter().enumerate().fold(0, |acc, (i, &c)| acc | u32::from(c & 3) << (8 + 2 * i));
        let bits = u32::from(self.flags) | cells | if self.aux { AUX_BIT } else { 0 };
        let floats = [self.throttle, self.brake, self.handbrake, self.clutch, self.steering].map(f32::to_bits);
        let gear = u32::from(self.gear)
            | u32::from(self.frames) << 8
            | u32::from(self.gears_held) << 16
            | u32::from(self.gears_bound) << 24;
        let words = [floats[0], floats[1], floats[2], floats[3], floats[4], bits, gear, INPUT_MAGIC];
        let mut out = [0; CONTROL_BYTES];
        for (chunk, word) in out.chunks_exact_mut(4).zip(words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        out
    }
}

impl Payload {
    /// The payload periods the frame stands for: an input payload's [`Input::frames`] (at least 1), else 1.
    pub fn periods(&self) -> u32 {
        match self {
            Payload::Input(input) => u32::from(input.frames.max(1)),
            Payload::Recorded(_) => 1,
        }
    }

    /// Reads a payload; an error for a word 7 that is neither a recorded payload's nor [`INPUT_MAGIC`], or input bits
    /// outside the layout.
    pub fn read(bytes: &[u8; CONTROL_BYTES]) -> Result<Payload, String> {
        let words: [u32; 8] = std::array::from_fn(|i| crate::word(bytes, i));
        match words[7] {
            0 | 1 => Ok(Payload::Recorded(words)),
            INPUT_MAGIC => {
                let bits = words[5];
                if bits & !(0xff | 0x3_ff00 | AUX_BIT) != 0 {
                    return Err(format!("an input payload with bits {bits:#x} and gear {}", words[6]));
                }
                let f = |i: usize| f32::from_bits(words[i]);
                Ok(Payload::Input(Input {
                    throttle: f(0),
                    brake: f(1),
                    handbrake: f(2),
                    clutch: f(3),
                    steering: f(4),
                    flags: bits as u8,
                    cells: std::array::from_fn(|i| (bits >> (8 + 2 * i) & 3) as u8),
                    aux: bits & AUX_BIT != 0,
                    gear: words[6] as u8,
                    frames: (words[6] >> 8) as u8,
                    gears_held: (words[6] >> 16) as u8,
                    gears_bound: (words[6] >> 24) as u8,
                }))
            }
            other => Err(format!(
                "a control payload with word 7 {other:#010x}: neither a recorded car control nor an input payload \
                 ({INPUT_MAGIC:#010x}); the viewer and rbr-standalone are from different versions"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_round_trips_at_full_precision() {
        let input = Input {
            throttle: 0.123_456_79,
            brake: 1.0,
            handbrake: 0.0,
            clutch: 0.5,
            steering: -0.333_333_34,
            flags: 0x25,
            cells: [3, 0, 0, 3, 0],
            aux: true,
            gear: 4,
            frames: 3,
            gears_held: 0x04,
            gears_bound: 0xfd,
        };
        assert_eq!(Payload::read(&input.to_bytes()), Ok(Payload::Input(input)));
    }

    #[test]
    fn a_recorded_payload_is_kept_and_an_unknown_one_refused() {
        let mut bytes = [0; CONTROL_BYTES];
        bytes[..4].copy_from_slice(&1.0f32.to_le_bytes());
        bytes[28] = 1;
        assert!(matches!(Payload::read(&bytes), Ok(Payload::Recorded(w)) if w[0] == 1.0f32.to_bits() && w[7] == 1));
        bytes[28..].copy_from_slice(&u32::from_le_bytes(*b"INP1").to_le_bytes());
        assert!(Payload::read(&bytes).unwrap_err().contains("different versions"));
        let mut bad = Input::default().to_bytes();
        bad[23] = 0x80;
        assert!(Payload::read(&bad).is_err());
    }
}
