//! A Linux device as the game sees it under Proton, for the live poll: winebus's SDL backend (`bus_sdl.c`) reads it
//! through SDL's Linux joystick (evdev) and reports a HID joystick, and dinput's HID joystick (`joystick_hid.c`) gives
//! the game its objects and values.
//!
//! - Objects: one axis per evdev ABS code in code order (the hat codes SDL takes as a hat left out), then one button
//!   per KEY code from `BTN_JOYSTICK` up, then those below it (SDL's order). The game takes axes and buttons only
//!   ([`objects`]); RSF's G923 and T500 RS formats (FMT1) are that layout.
//! - Axis values: SDL scales the evdev value from its `absinfo` range to -32768..32767 ([`sdl_axis`]); dinput scales
//!   that HID range to the default 0..65535 around its centre, which for this range is the value plus 32768
//!   ([`axis`]). RSF's recorded pedal values are exactly that lattice for the pedals' `absinfo` (G923: Y 25..255,
//!   Z 13..255, RZ 51..255).
//! - Until an axis or button first changes, winebus's report holds 0 (the T500 RS's resting axes read 32768).
//! - The `.acm` names a device by its DirectInput instance GUID; the prefix's `user.reg` keeps each one under its
//!   vendor and product ids.

use super::{Object, AXIS_KIND};

/// `DIDFT_PSHBUTTON`.
const BUTTON_KIND: u32 = 0x04;

/// SDL2's `AxisCorrect` without dead zones (`SDL_sysjoystick.c`): `floor((value - min) * scale - 32768 + 0.5)` in
/// f32, `scale = 65535 / (max - min)`, clamped.
pub fn sdl_axis(value: i32, min: i32, max: i32) -> i32 {
    if min == max {
        return value.clamp(-32768, 32767);
    }
    let scale = 65535.0f32 / (max - min) as f32;
    (((value - min) as f32 * scale + -32768.0 + 0.5).floor() as i32).clamp(-32768, 32767)
}

/// dinput's `scale_axis_value` from winebus's -32768..32767 to 0..65535, dead zone 0 and saturation 10000: the centre
/// is `round(-0.5)` = -1, so both halves map one to one, `value + 32768`.
pub fn axis(sdl: i32) -> i32 {
    sdl + 32768
}

/// The data format the game sets for a device of `axes` axes and `buttons` buttons: the axes' DWORDs, then a byte per
/// button (`dwType` without dinput's force-feedback flags; a binding names the instance).
pub fn objects(axes: u32, buttons: u32) -> Vec<Object> {
    let axes_ = (0..axes).map(|k| Object { kind: k << 8 | AXIS_KIND, ofs: 4 * k });
    axes_.chain((0..buttons).map(|k| Object { kind: k << 8 | BUTTON_KIND, ofs: 4 * axes + k })).collect()
}

/// The device state for [`objects`]: the axes' dinput values, the buttons 0x80 when down.
pub fn state(axes: &[i32], buttons: &[bool]) -> Vec<u8> {
    let axes_ = axes.iter().flat_map(|v| v.to_le_bytes());
    axes_.chain(buttons.iter().map(|&b| if b { 0x80 } else { 0 })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdl_scales_to_the_full_range() {
        // The G923's pedals (Z 13..255, Y 25..255, RZ 51..255): the lattices RSF recorded, `MulDiv(k, 65535, max - min)`.
        assert_eq!(axis(sdl_axis(13, 13, 255)), 0);
        assert_eq!(axis(sdl_axis(14, 13, 255)), 271);
        assert_eq!(axis(sdl_axis(13 + 121, 13, 255)), 32768);
        assert_eq!(axis(sdl_axis(255, 13, 255)), 65535);
        for min in [13, 25, 51] {
            let n = 255 - min;
            for k in 0..=n {
                assert_eq!(axis(sdl_axis(min + k, min, 255)), (k * 65535 + n / 2) / n, "{min} {k}");
            }
        }
        // A 16-bit axis is the value.
        for v in [0, 1, 31046, 65534, 65535] {
            assert_eq!(axis(sdl_axis(v, 0, 65535)), v);
        }
    }

    #[test]
    fn objects_and_state() {
        let o = objects(2, 2);
        assert_eq!(o.iter().map(|o| (o.kind, o.ofs)).collect::<Vec<_>>(), [(2, 0), (0x102, 4), (4, 8), (0x104, 9)]);
        assert_eq!(state(&[1, 2], &[false, true]), [1, 0, 0, 0, 2, 0, 0, 0, 0, 0x80]);
    }
}
