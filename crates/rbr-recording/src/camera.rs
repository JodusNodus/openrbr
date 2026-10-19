//! The camera record `rbr-standalone --live` writes after each frame's DMG1 record (a tagged record: tag, u32 length,
//! body), which the viewer's start camera follows. Little-endian body:
//!
//! ```text
//! u32 mode   the camera host's mode (`E15CamHost` +0x00): 10 while the start orbit runs, 1 from the go on
//! f32 angle  the start orbit's swing angle (`g_1660da0`), radians; 0 outside mode 10
//! ```
//!
//! The game side is `rbr_gameplay::camera_go`.

use std::io;

/// The tag.
pub const CAM1: [u8; 4] = *b"CAM1";
/// The body's size.
pub const BODY_BYTES: usize = 8;
/// The start orbit's mode, and the go's.
pub const MODE_START_ORBIT: u32 = 10;
pub const MODE_CHASE: u32 = 1;

/// One camera record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub mode: u32,
    pub angle: f32,
}

impl Camera {
    /// The swing angle while the start orbit runs.
    pub fn start_orbit(self) -> Option<f32> {
        (self.mode == MODE_START_ORBIT).then_some(self.angle)
    }

    /// The tagged record.
    pub fn record(&self) -> Vec<u8> {
        let mut body = [0; BODY_BYTES];
        body[..4].copy_from_slice(&self.mode.to_le_bytes());
        body[4..].copy_from_slice(&self.angle.to_le_bytes());
        let mut out = Vec::with_capacity(8 + BODY_BYTES);
        crate::live::write_record(&mut out, CAM1, &body);
        out
    }

    /// The record's body (the bytes after the tag and length).
    pub fn read_body(body: &[u8]) -> io::Result<Camera> {
        if body.len() != BODY_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("a CAM1 body of {} bytes", body.len())));
        }
        let word = |i: usize| u32::from_le_bytes(body[4 * i..4 * i + 4].try_into().unwrap());
        Ok(Camera { mode: word(0), angle: f32::from_bits(word(1)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_round_trips() {
        let camera = Camera { mode: MODE_START_ORBIT, angle: 1.5 };
        let bytes = camera.record();
        assert_eq!(&bytes[..4], b"CAM1");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, BODY_BYTES);
        let read = Camera::read_body(&bytes[8..]).unwrap();
        assert_eq!(read, camera);
        assert_eq!(read.start_orbit(), Some(1.5));
        assert_eq!(Camera { mode: MODE_CHASE, angle: 0.0 }.start_orbit(), None);
        assert!(Camera::read_body(&bytes[8..12]).is_err());
    }
}
