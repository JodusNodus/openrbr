//! The steering wheel's lock-to-lock rotation (ours; RSF's launcher sets it through the wheel makers' tools): written,
//! in degrees, to the `range` attribute that Linux's wheel drivers (hid-logitech-hidpp, hid-lg4ff, new-lg4ff,
//! hid-fanatecff) give the wheel's HID device. The wheel then stops at that rotation and its steering axis spans it,
//! so the full axis stays full lock. Each driver clamps the value to what its wheel can do (the G923's: 180 to 900);
//! the rotation read back is the one set. Writing it needs the udev rule the wheel tools install (oversteer's
//! `99-logitech-wheel-perms.rules`). Elsewhere no wheel is found.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// The rotations offered, degrees.
pub const DEGREES: RangeInclusive<u16> = 90..=2520;

/// Where the HID devices are.
const HID_DEVICES: &str = "/sys/bus/hid/devices";

/// A wheel whose rotation can be set.
#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    /// The device's name (`HID_NAME`).
    pub name: String,
    /// Its rotation now, degrees.
    pub degrees: u16,
    range: PathBuf,
}

/// The wheels whose rotation can be set.
pub fn wheels() -> Vec<Wheel> {
    wheels_in(Path::new(HID_DEVICES))
}

/// Sets every wheel's rotation to `degrees`; the wheels as they are after, or why none could be set.
pub fn set(degrees: u16) -> Result<Vec<Wheel>, String> {
    set_in(Path::new(HID_DEVICES), degrees)
}

fn wheels_in(root: &Path) -> Vec<Wheel> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut wheels: Vec<Wheel> = entries
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let range = dir.join("range");
            let degrees = std::fs::read_to_string(&range).ok()?.trim().parse().ok()?;
            let uevent = std::fs::read_to_string(dir.join("uevent")).unwrap_or_default();
            let name = uevent.lines().find_map(|l| l.strip_prefix("HID_NAME=")).unwrap_or("wheel").to_owned();
            Some(Wheel { name, degrees, range })
        })
        .collect();
    wheels.sort_by(|a, b| a.range.cmp(&b.range));
    wheels
}

fn set_in(root: &Path, degrees: u16) -> Result<Vec<Wheel>, String> {
    let wheels = wheels_in(root);
    if wheels.is_empty() {
        return Err("no wheel whose rotation can be set (Linux wheel drivers only)".to_owned());
    }
    let failed: Vec<String> = wheels
        .iter()
        .filter_map(|w| std::fs::write(&w.range, degrees.to_string()).err().map(|e| format!("{}: {e}", w.name)))
        .collect();
    if failed.len() == wheels.len() {
        return Err(format!("cannot set the rotation ({}); the wheel tools' udev rule allows it", failed.join(", ")));
    }
    Ok(wheels_in(root))
}

#[cfg(test)]
mod tests {
    use super::{set_in, wheels_in};

    #[test]
    fn sets_the_wheels_with_a_range() {
        let root = std::env::temp_dir().join(format!("wheel-rotation-{}", std::process::id()));
        let wheel = root.join("0003:046D:C26E.0001");
        let mouse = root.join("0003:046D:C52B.0002");
        for dir in [&wheel, &mouse] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(wheel.join("range"), "900\n").unwrap();
        std::fs::write(wheel.join("uevent"), "DRIVER=logitech-hidpp-device\nHID_NAME=Logitech G923\n").unwrap();
        std::fs::write(mouse.join("uevent"), "HID_NAME=Logitech Mouse\n").unwrap();
        let before = wheels_in(&root);
        assert_eq!((before.len(), before[0].name.as_str(), before[0].degrees), (1, "Logitech G923", 900));
        assert_eq!(set_in(&root, 540).unwrap()[0].degrees, 540);
        assert!(set_in(&mouse, 540).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
