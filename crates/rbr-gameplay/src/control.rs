//! The car control at the boundary to the physics step.
//!
//! Each substep the input filter's channels are quantized into nine bytes (`E16SteerState::Pack`, 0x48c910, in
//! [`QuantizedControl::pack`]), the value 1.02's own replay stores per step, and `E16Quant::NotifyFilt` (0x48d130)
//! unpacks them into the 0x20-byte `CSigCarControl` payload the car handlers read (`notes/gameplay.md`, the word
//! table). The harness recordings store that payload per step (`rbr_recording::Frame::control`).

use crate::filter::Filtered;

/// Flag bits of the quantized byte 0 and the payload bits `NotifyFilt` maps them to.
const FLAG_MAP: [(u8, u32); 5] = [
    (0x01, 0x01),          // gear up
    (0x02, 0x02),          // gear down
    (0x04, 0x04),          // select the gear in word 6
    (CALL_FOR_HELP, 0x10), // `CCar::UpdateSeed` (0x4e91f0): the motion probe, then the assist or `CSigCarProblem`
    (0x40, 0x20), // `CSigRespawnRequest(2)` (`E65_EmitRespawn`, 0x5826c0, which nothing calls): the reset branch
];
/// The flag `E16Quant::NotifyRespawn` (0x48d080) sets for `CSigRespawnRequest(1)`, the pause menu's call for help.
pub const CALL_FOR_HELP: u8 = 0x20;
/// Payload flag bit that makes word 6 hold the selected gear.
const SELECT_GEAR: u32 = 0x04;
/// Pedal step, `1/255` (the f32 at 0x72ba84).
const PEDAL_SCALE: f32 = 1.0 / 255.0;
/// Steering step, `2/255` (the f32 at 0x72ba80).
const STEERING_SCALE: f32 = 2.0 / 255.0;
/// The steering byte is `(s + 1) * 127.5 - 128`, truncated (the f32s at 0x72f8b4 and 0x72f8b0).
const STEER_SPAN: f32 = 127.5;
const STEER_BIAS: f32 = 128.0;
/// A pedal byte is `x * 255 + 0.5`, truncated (the f32s at 0x795e1c and 0x795e18).
const PEDAL_SPAN: f32 = 255.0;
const PEDAL_ROUND: f32 = 0.5;

/// The nine quantized control bytes, in `E16SteerState` +0x74..+0x7c order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuantizedControl {
    /// Byte 0: action flags (raw input +0x00).
    pub flags: u8,
    /// Byte 1: raw input +0x04; only bit 0 reaches the payload.
    pub aux: u8,
    /// Byte 2: raw input +0x08; kept by the unpack (`E16SteerState` +0x50), not in the payload.
    pub byte2: u8,
    /// Byte 3: steering, a signed step of 2/255, positive to the right.
    pub steering: i8,
    /// Byte 4: filtered +0x18, the payload's word 0.
    pub throttle: u8,
    /// Byte 5: filtered +0x1c, word 1.
    pub brake: u8,
    /// Byte 6: filtered +0x20, word 2.
    pub handbrake: u8,
    /// Byte 7: filtered +0x24, word 3.
    pub clutch: u8,
    /// Byte 8: the gear `flags` bit 2 selects (raw input +0x28).
    pub gear: u8,
}

/// The `CSigCarControl` payload (word order of the table in `notes/gameplay.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarControl {
    pub throttle: f32,
    pub brake: f32,
    pub handbrake: f32,
    pub clutch: f32,
    /// -1 (left) to 1 (right).
    pub steering: f32,
    /// Bit 0 gear up, 1 gear down, 2 select `gear`; 0x10 and 0x20 the car-problem and reset branches.
    pub flags: u32,
    /// Word 6, written only with flag bit 2; otherwise the original leaves its stack word uninitialised.
    pub gear: Option<u32>,
    /// Word 7: one bit.
    pub aux: u32,
}

impl QuantizedControl {
    /// `E16Quant::NotifyFilt` (0x48d130): the payload emitted for these bytes.
    pub fn car_control(&self) -> CarControl {
        let pedal = |b: u8| f32::from(b) * PEDAL_SCALE;
        let flags = FLAG_MAP.iter().filter(|(bit, _)| self.flags & bit != 0).fold(0, |acc, (_, out)| acc | out);
        CarControl {
            throttle: pedal(self.throttle),
            brake: pedal(self.brake),
            handbrake: pedal(self.handbrake),
            clutch: pedal(self.clutch),
            steering: f32::from(self.steering) * STEERING_SCALE,
            flags,
            gear: (flags & SELECT_GEAR != 0).then_some(u32::from(self.gear)),
            aux: u32::from(self.aux & 1),
        }
    }

    /// The quantized bytes `NotifyFilt` turns into `words` (a recorded payload; word 6 is read only with flag bit 2),
    /// or `None` when no bytes give a value of that size or a flag it cannot emit. Not an original function: the
    /// recordings hold the payload, and this recovers the loop's input from it. Rounds each value to its nearest
    /// step and does not check the bits; [`Self::car_control`] on the result shows whether they reproduce.
    pub fn from_payload(words: [u32; 8]) -> Option<Self> {
        let pedal = |w: u32| -> Option<u8> {
            let steps = (f32::from_bits(w) * 255.0).round();
            (0.0..=255.0).contains(&steps).then_some(steps as u8)
        };
        let steps = (f32::from_bits(words[4]) * 127.5).round();
        let steering = (-128.0..=127.0).contains(&steps).then_some(steps as i8)?;
        let mut rest = words[5];
        let mut flags = 0;
        for (bit, out) in FLAG_MAP {
            if rest & out != 0 {
                flags |= bit;
                rest &= !out;
            }
        }
        if rest != 0 || words[7] > 1 {
            return None;
        }
        let gear = if words[5] & SELECT_GEAR != 0 { u8::try_from(words[6]).ok()? } else { 0 };
        Some(Self {
            flags,
            aux: words[7] as u8,
            byte2: 0,
            steering,
            throttle: pedal(words[0])?,
            brake: pedal(words[1])?,
            handbrake: pedal(words[2])?,
            clutch: pedal(words[3])?,
            gear,
        })
    }
}

impl QuantizedControl {
    /// `E16SteerState::Pack` (0x48c910): the filtered control to the nine bytes. The bytes are the asm's: byte 0 is the
    /// flags' low byte, bytes 1 and 2 the aux and byte-2 low bytes, byte 3 the steering, bytes 4 to 7 the channels 1 to 4
    /// (filtered +0x18 to +0x24), byte 8 the gear. Byte 4 is channel 1 (raw +0x18), the car's throttle: the payload's
    /// word 0, measured (`notes/gameplay.md`). The poll's names for raw +0x18 and +0x1c (brake, throttle) are the
    /// other way round, so the filter's channel names do not say what the car does.
    pub fn pack(f: &Filtered) -> Self {
        // x87 at the game's 24-bit precision: each operation rounds to f32, as written.
        let pedal = |x: f32| (x * PEDAL_SPAN + PEDAL_ROUND) as i64 as u8;
        Self {
            flags: f.flags as u8,
            aux: f.aux as u8,
            byte2: f.byte2 as u8,
            steering: cvttss2si((f.channels[0] + 1.0) * STEER_SPAN - STEER_BIAS) as i8,
            throttle: pedal(f.channels[1]),
            brake: pedal(f.channels[2]),
            handbrake: pedal(f.channels[3]),
            clutch: pedal(f.channels[4]),
            gear: f.gear,
        }
    }
}

impl CarControl {
    /// NGP 7.4.778's live handler of `CSigCarControlFiltered` (NGP.dll `0x1008e8a0`, `notes/ngp.md`, "Controls under
    /// NGP"), which replaces the quantizer's path while driving: the filtered channels and the steering go to the
    /// payload as they are, unquantized; the flags map as `NotifyFilt` maps them; a gear selection needs a gear byte,
    /// which NGP's input poll ([`ngp_direct_gears`]) writes as the gear plus one, and selects the byte less one, dropped
    /// when it is the drive's gear already (`current`, the drive state's +0x570); the aux word goes whole. The payload
    /// starts zeroed (`0x1002c760`), so word 6 is 0 without a selection.
    pub fn ngp(f: &Filtered, current: Option<u32>) -> CarControl {
        let bits = f.flags as u8;
        let mut flags =
            FLAG_MAP.iter().filter(|(bit, _)| *bit != 0x04 && bits & bit != 0).fold(0, |acc, (_, out)| acc | out);
        let mut gear = Some(0);
        if bits & 0x04 != 0 && f.gear != 0 {
            let selected = u32::from(f.gear) - 1;
            gear = Some(selected);
            if current != Some(selected) {
                flags |= SELECT_GEAR;
            }
        }
        CarControl {
            throttle: f.channels[1],
            brake: f.channels[2],
            handbrake: f.channels[3],
            clutch: f.channels[4],
            steering: f.channels[0],
            flags,
            gear,
            aux: f.aux,
        }
    }
}

/// NGP's direct gear actions (R, N, first to sixth: actions 0xd to 0x14, in that order).
pub const NGP_GEAR_ACTIONS: usize = 8;
/// A direct gear action's value above this selects its gear (`0x10146ecc`, 0.999).
pub const NGP_GEAR_PRESS: f32 = 0.999;
/// The IGNITION action's value above this holds the ignition (`0x10006e50`, `0x101459d0`, 0.99): NGP's poll reads it
/// as a level where 1.02's `IsActionDown` (0x4c25d0) takes a change.
pub const NGP_IGNITION_PRESS: f32 = 0.99;
/// The gear in use the sensor data reports (drive state +0x54c) for first gear.
const FIRST_GEAR: u32 = 2;

/// NGP's addition to the input poll (`0x1003c450`, from its `CSigCarControlInput` emit `0x100ed540`, which calls it
/// only with the automatic gearbox off: the car's VCU scratch +0x850, its +0x50, `0x1005dd30`): the raw control's action `flags` and gear
/// byte (+0x28) from the direct gear `actions` (`None` unbound, else the action's value) and `gear_in_use` (the
/// input manager's +0x250, word 1 of the last `CSigCarSensorData`: the drive state's +0x54c, 2 first gear).
/// - Without a gear up or down flag: the first action held (above [`NGP_GEAR_PRESS`]) selects its gear (flag 4, the
///   byte the gear word plus one); with an action bound but none held and neutral unbound, neutral is selected. Each
///   poll selects again; the handler drops a selection of the gear in use ([`CarControl::ngp`]).
/// - With one: a gear down in first gear is dropped when neutral is bound.
pub fn ngp_direct_gears(flags: u32, gear: u8, actions: [Option<f32>; NGP_GEAR_ACTIONS], gear_in_use: u32) -> (u32, u8) {
    let neutral_bound = actions[1].is_some();
    if flags & 0x03 == 0 {
        if let Some(k) = actions.iter().position(|v| v.is_some_and(|v| v > NGP_GEAR_PRESS)) {
            return (flags | SELECT_GEAR, k as u8 + 1);
        }
        if actions.iter().any(Option::is_some) && !neutral_bound {
            return (flags | SELECT_GEAR, 2);
        }
    } else if flags & 0x02 != 0 && neutral_bound && gear_in_use == FIRST_GEAR {
        return (flags & !0x02, gear);
    }
    (flags, gear)
}

/// `cvttss2si`: truncation toward zero; out of range or NaN gives the x86 "integer indefinite" value.
fn cvttss2si(v: f32) -> i32 {
    if v.is_nan() || !(-2147483648.0..2147483648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

impl CarControl {
    /// The payload words; `None` for word 6 when the original leaves it uninitialised.
    pub fn words(&self) -> [Option<u32>; 8] {
        [
            Some(self.throttle.to_bits()),
            Some(self.brake.to_bits()),
            Some(self.handbrake.to_bits()),
            Some(self.clutch.to_bits()),
            Some(self.steering.to_bits()),
            Some(self.flags),
            self.gear,
            Some(self.aux),
        ]
    }

    /// The first word that differs from a recorded payload, skipping the uninitialised word 6.
    pub fn first_difference(&self, recorded: [u32; 8]) -> Option<usize> {
        self.words().iter().zip(recorded).position(|(ours, theirs)| ours.is_some_and(|w| w != theirs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_are_the_exe_constants() {
        assert_eq!(PEDAL_SCALE.to_bits(), 0x3b80_8081);
        assert_eq!(STEERING_SCALE.to_bits(), 0x3c00_8081);
    }

    #[test]
    fn full_pedal_and_lock_are_exact() {
        let q = QuantizedControl { throttle: 255, steering: -128, ..Default::default() };
        let c = q.car_control();
        assert_eq!(c.throttle, 1.0);
        assert_eq!(c.steering.to_bits(), (-128.0f32 * STEERING_SCALE).to_bits());
        assert!(c.steering < -1.0);
    }

    #[test]
    fn flags_are_remapped_and_others_dropped() {
        for (bit, out) in FLAG_MAP {
            let q = QuantizedControl { flags: bit, ..Default::default() };
            assert_eq!(q.car_control().flags, out);
        }
        let q = QuantizedControl { flags: 0x98, ..Default::default() };
        assert_eq!(q.car_control().flags, 0);
    }

    #[test]
    fn gear_word_only_with_its_flag() {
        let mut q = QuantizedControl { gear: 3, aux: 0xff, ..Default::default() };
        assert_eq!(q.car_control().gear, None);
        assert_eq!(q.car_control().aux, 1);
        q.flags = 0x04;
        assert_eq!(q.car_control().gear, Some(3));
    }

    #[test]
    fn every_quantized_value_survives_the_payload() {
        for b in 0..=255u8 {
            for s in [i8::MIN, -1, 0, 1, i8::MAX] {
                let q = QuantizedControl {
                    flags: 0x67,
                    aux: 1,
                    byte2: 0,
                    steering: s,
                    throttle: b,
                    brake: 255 - b,
                    handbrake: b / 2,
                    clutch: b,
                    gear: b,
                };
                let words = q.car_control().words().map(Option::unwrap);
                assert_eq!(QuantizedControl::from_payload(words), Some(q));
            }
        }
    }

    #[test]
    fn pack_takes_the_asm_bytes_from_the_filtered_channels() {
        let f = Filtered { flags: 0x161, aux: 1, byte2: 2, channels: [-1.0, 0.5, 1.0, 0.0, 0.25], gear: 3 };
        let q = QuantizedControl::pack(&f);
        assert_eq!((q.flags, q.aux, q.byte2, q.steering, q.gear), (0x61, 1, 2, -128, 3));
        assert_eq!((q.throttle, q.brake, q.handbrake, q.clutch), (128, 255, 0, 64));
    }

    #[test]
    fn pack_steering_truncates_toward_zero() {
        let steering = |s: f32| {
            let f = Filtered { channels: [s, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
            QuantizedControl::pack(&f).steering
        };
        assert_eq!(steering(0.0), 0);
        assert_eq!(steering(1.0), 127);
        assert_eq!(steering(-1.0), -128);
    }

    #[test]
    fn values_off_the_steps_are_rejected() {
        let ok = QuantizedControl::default().car_control().words().map(|w| w.unwrap_or(0));
        let with = |i: usize, w: u32| {
            let mut words = ok;
            words[i] = w;
            QuantizedControl::from_payload(words)
        };
        assert!(with(0, 1.5f32.to_bits()).is_none());
        assert!(with(1, (-0.5f32).to_bits()).is_none());
        assert!(with(4, 2.0f32.to_bits()).is_none());
        assert!(with(5, 0x08).is_none());
        assert!(with(7, 2).is_none());
        let first = QuantizedControl { flags: 0x04, gear: 1, ..Default::default() }.car_control();
        let recorded = first.words().map(|w| w.unwrap_or(0));
        assert_eq!(first.first_difference(recorded), None);
        let mut off = recorded;
        off[6] = 2;
        assert_eq!(first.first_difference(off), Some(6));
    }

    #[test]
    fn ngp_passes_the_filtered_values_unquantized() {
        let f = Filtered {
            flags: 0x01 | 0x04 | 0x20,
            aux: 3,
            byte2: 0,
            channels: [-0.123_456, 0.300_001, 0.2, 0.0, 1.0],
            gear: 3,
        };
        let c = CarControl::ngp(&f, Some(1));
        assert_eq!((c.steering, c.throttle, c.brake, c.clutch), (-0.123_456, 0.300_001, 0.2, 1.0));
        assert_eq!((c.flags, c.gear, c.aux), (0x01 | 0x04 | 0x10, Some(2), 3));
        // The drive's gear already: no selection; no gear byte: no selection either.
        assert_eq!(CarControl::ngp(&f, Some(2)).flags, 0x01 | 0x10);
        assert_eq!(CarControl::ngp(&Filtered { gear: 0, ..f }, None).flags & SELECT_GEAR, 0);
        assert_eq!(CarControl::ngp(&Filtered { flags: 0, ..f }, None).gear, Some(0), "the zeroed payload's word 6");
    }

    /// `0x1003c450`: the first held action (by action order) selects; the threshold is strict.
    #[test]
    fn ngp_direct_gears_select_the_held_gear_each_poll() {
        let mut actions = [Some(0.0); NGP_GEAR_ACTIONS];
        actions[3] = Some(1.0);
        actions[5] = Some(1.0);
        assert_eq!(ngp_direct_gears(0x20, 0, actions, 1), (0x24, 4), "second gear: word 3, byte 4");
        actions[3] = Some(NGP_GEAR_PRESS);
        assert_eq!(ngp_direct_gears(0, 0, actions, 1), (0x04, 6));
        actions[0] = Some(1.0);
        assert_eq!(ngp_direct_gears(0, 0, actions, 1), (0x04, 1), "reverse first");
        // None held: nothing with neutral bound, neutral without.
        let none = [Some(0.0); NGP_GEAR_ACTIONS];
        assert_eq!(ngp_direct_gears(0, 7, none, 4), (0, 7));
        let mut no_neutral = none;
        no_neutral[1] = None;
        assert_eq!(ngp_direct_gears(0, 7, no_neutral, 4), (0x04, 2));
        assert_eq!(ngp_direct_gears(0, 7, [None; NGP_GEAR_ACTIONS], 4), (0, 7), "nothing bound");
    }

    /// `0x1003c4f6`: a sequential shift skips the scan; a gear down from first is dropped with neutral bound.
    #[test]
    fn ngp_direct_gears_keep_sequential_shifts() {
        let held = [Some(1.0); NGP_GEAR_ACTIONS];
        assert_eq!(ngp_direct_gears(0x01, 0, held, 2), (0x01, 0));
        assert_eq!(ngp_direct_gears(0x02, 0, held, 2), (0x00, 0));
        assert_eq!(ngp_direct_gears(0x02, 0, held, 3), (0x02, 0));
        let mut no_neutral = held;
        no_neutral[1] = None;
        assert_eq!(ngp_direct_gears(0x02, 0, no_neutral, 2), (0x02, 0));
    }
}
