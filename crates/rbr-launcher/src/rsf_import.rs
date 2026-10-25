//! "Import from RSF" (offered once when an RSF folder is first set, and a button on the Controls page): the RSF
//! install's controls, force feedback, aids and NGP settings written into ours, once. Drives then use ours only; an
//! RSF folder is never read again for them (`rbr_formats::rsf::settings`, `notes/ngp.md` "RSF settings").
//!
//! - The newest profile's `.acm` (`SavedGames/pf*.acm`) into the bindings (`Bindings::import`), its devices' GUIDs
//!   resolved to vendor, product and index through the Wine prefix's `user.reg` (`rbr_formats::wine`). Of the records
//!   whose devices all resolve, the first that names exactly the keyboard and the joysticks connected, as the game's
//!   `FindRec` takes it, else the one naming the most devices.
//! - The profile's (`pf*.rbr`) force feedback on, strength and direction, and its aids.
//! - `[NGP]`'s sensitivity per surface, and per RSF car where RSF gives that car its own (`rallysimfans_personal.ini`,
//!   the `_Car5` keys of RSF's slot); `ffbDeadZone`, `ffbDeadZoneThreshold`, `ffbRate`, `inputRate` (the poll's rate)
//!   and `headGasketDurability`.
//! - `input.ini`'s `Invert*Axis`.

use std::path::{Path, PathBuf};

use rbr_formats::rsf::settings::{newest, personal, Sources, SURFACES};
use rbr_formats::rsf::SLOT;
use rbr_formats::saves::action_map::ActionMap;
use rbr_gameplay::poll::{Invert, KEYBOARD};
use rbr_gameplay::race_options::DriverAids;
use rbr_render::bindings::{Bindings, Device};
use rbr_render::ffb;

/// What an import writes into.
pub struct Target<'a> {
    pub bindings: &'a mut Bindings,
    pub ffb: &'a mut ffb::Options,
    pub aids: &'a mut DriverAids,
    pub head_gasket: &'a mut i32,
}

/// Imports the RSF install at `root` into `to`: `user_reg` is the Wine prefix's `user.reg` (none: the profile's
/// bindings are not imported), `cars` the RSF cars' ids, `connected` the joysticks connected. Returns what it set,
/// one line each.
pub fn import(root: &Path, user_reg: Option<&str>, cars: &[u32], connected: &[Device], to: Target) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match user_reg {
        Some(reg) => import_map(root, reg, connected, to.bindings),
        None => "controls: no Wine prefix user.reg, not imported".to_owned(),
    });
    let sources = Sources::install(root, 0);
    // Without a car's keys and in no game slot: `[NGP]`'s own.
    let global = Sources { car: Vec::new(), ..sources.clone() }.resolve(u32::MAX);
    let personal_ini = std::fs::read(root.join("rallysimfans_personal.ini")).ok();
    let personal_ini = personal_ini.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    to.ffb.sensitivity = global.sensitivity;
    to.ffb.car_sensitivity = cars
        .iter()
        .filter_map(|&id| {
            let s = Sources { car: personal(&personal_ini, id), ..sources.clone() }.resolve(SLOT).sensitivity;
            (s != global.sensitivity).then_some((id, s))
        })
        .collect();
    let [g, t, s] = global.sensitivity;
    lines.push(format!(
        "sensitivity {}: {g}, {t}, {s}%{}",
        SURFACES.join("/").to_ascii_lowercase(),
        match to.ffb.car_sensitivity.len() {
            0 => String::new(),
            n => format!(" ({n} cars their own)"),
        }
    ));
    to.ffb.settings.dead_zone = global.dead_zone;
    to.ffb.settings.dead_zone_threshold = global.dead_zone_threshold;
    // 0 stops NGP's FFB thread (the game thread sends then); ours keeps its rate.
    if global.ffb_rate > 0 {
        to.ffb.rate = global.ffb_rate.min(1000);
    }
    if global.input_rate > 0 {
        to.bindings.input_rate = global.input_rate.min(1000);
    }
    lines.push(format!(
        "dead zone {} threshold {}, ffb rate {} Hz, input rate {} Hz",
        global.dead_zone, global.dead_zone_threshold, to.ffb.rate, to.bindings.input_rate
    ));
    *to.head_gasket = global.head_gasket_durability.max(1);
    lines.push(format!("head gasket durability {}", to.head_gasket));
    if let Some((game, source)) = &global.game {
        to.ffb.settings.enabled = game.enabled;
        to.ffb.settings.strength = game.strength.clamp(0.0, 1.0);
        to.ffb.settings.inverse = game.inverse;
        (to.aids.gear_guard, to.aids.neutral_lock, to.aids.clutch_help) =
            (game.gear_guard, game.neutral_lock, game.clutch_help);
        lines.push(format!(
            "force feedback {}, strength {}, {}; gear protection, neutral lock, clutch help {} ({})",
            if game.enabled { "on" } else { "off" },
            game.strength,
            if game.inverse { "inverse" } else { "normal" },
            [game.gear_guard, game.neutral_lock, game.clutch_help].map(|a| if a { "on" } else { "off" }).join(", "),
            source.name()
        ));
    }
    let [steering, throttle, brake, combined, handbrake, clutch] = global.invert;
    to.bindings.invert = Invert { steering, throttle, brake, combined, handbrake, clutch };
    if global.invert.contains(&true) {
        lines.push("inverted axes from input.ini".to_owned());
    }
    lines
}

/// The newest profile's action map into `bindings`; the line saying what it did.
fn import_map(root: &Path, user_reg: &str, connected: &[Device], bindings: &mut Bindings) -> String {
    let Some(acm) = newest(&root.join("SavedGames"), "pf", "acm") else {
        return "controls: no profile .acm in SavedGames".to_owned();
    };
    let name = acm.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let map = match std::fs::read(&acm)
        .map_err(|e| e.to_string())
        .and_then(|b| ActionMap::read(&b).map_err(|e| format!("{e:?}")))
    {
        Ok(map) => map,
        Err(e) => return format!("controls: {name}: {e}"),
    };
    let instances = rbr_formats::wine::instances(user_reg);
    let resolve = |g: &[u8; 16]| match *g {
        KEYBOARD => Some(None),
        g => instances
            .iter()
            .find(|i| i.guid == g)
            .map(|i| Some(Device { vendor: i.vendor, product: i.product, index: i.index })),
    };
    // The record the game would bind with these joysticks, else the one naming the most devices.
    let resolved = |r: &&rbr_formats::saves::action_map::Record| r.devices.iter().all(|g| resolve(g).is_some());
    let exact = |r: &rbr_formats::saves::action_map::Record| {
        r.devices.len() == connected.len() + 1
            && r.devices.iter().all(|g| resolve(g).is_some_and(|d| d.is_none_or(|d| connected.contains(&d))))
    };
    let best = map.records.iter().filter(resolved).min_by_key(|r| (!exact(r), std::cmp::Reverse(r.devices.len())));
    let Some(record) = best else {
        return format!("controls: {name}: none of its records' devices are in the prefix's user.reg");
    };
    let chosen = ActionMap { records: vec![record.clone()] };
    bindings.import(&chosen, resolve);
    let devices: Vec<String> = record.devices.iter().filter_map(|g| resolve(g)?).map(Device::format).collect();
    format!("controls from {name}: the keyboard{}", devices.iter().map(|d| format!(", {d}")).collect::<String>())
}

/// A Wine prefix's `user.reg`: Proton's prefix keeps it in `pfx/`.
pub fn user_reg(prefix: &str) -> PathBuf {
    let proton = Path::new(prefix).join("pfx/user.reg");
    if proton.exists() {
        proton
    } else {
        Path::new(prefix).join("user.reg")
    }
}

/// The Wine prefix to read by default: `$WINEPREFIX`, else `~/.wine`.
pub fn default_prefix() -> String {
    let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".wine"));
    let prefix = std::env::var_os("WINEPREFIX").map(PathBuf::from).or(home);
    prefix.map(|p| p.display().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::saves::action_map::Object;
    use rbr_formats::saves::action_map::{Binding, Record};
    use rbr_render::bindings::{Action, PadInput};
    use winit::keyboard::KeyCode;

    /// A made-up RSF install: its ini files, a profile's `.acm`, and a prefix's `user.reg` naming the wheel.
    #[test]
    fn an_install_s_settings_and_controls() {
        let root = std::env::temp_dir().join(format!("rsf-import-{}", std::process::id()));
        std::fs::create_dir_all(root.join("SavedGames")).unwrap();
        let write = |path: &str, text: &[u8]| std::fs::write(root.join(path), text).unwrap();
        write(
            "RichardBurnsRally.ini",
            b"[NGP]\nForceFeedbackSensitivityGravel=280\nForceFeedbackSensitivityTarmac=250\n\
              ForceFeedbackSensitivitySnow=280\nffbDeadZone=0.02\nffbRate=500\ninputRate=750\nheadGasketDurability=10\n",
        );
        write("rallysimfans_personal.ini", b"[car88]\nforcefeedbacksensitivitygravel=300\nlogin=secret\n");
        write("input.ini", b"[Settings]\nInvertThrottleAxis = true\n");
        let wheel = [7; 16];
        let b = |device, object, action| Binding { device, object, action };
        let map = ActionMap {
            records: vec![
                Record { devices: vec![KEYBOARD], bindings: vec![b(0, 0xcb << 8 | 4, 1)] },
                Record {
                    devices: vec![KEYBOARD, wheel],
                    bindings: vec![b(1, 0x0002, 0), b(1, 2 << 8 | 2, 3), b(0, 0x39 << 8 | 4, 6)],
                },
            ],
        };
        write("SavedGames/pfX.acm", &map.write());
        let reg = "[System\\\\CurrentControlSet\\\\Control\\\\MediaProperties\\\\PrivateProperties\\\\DirectInput\\\\VID_046D&PID_C266\\\\Calibration\\\\0] 1\n\
                   \"GUID\"=hex:07,07,07,07,07,07,07,07,07,07,07,07,07,07,07,07\n";
        let (mut bindings, mut ffb, mut aids, mut head_gasket) =
            (Bindings::default(), ffb::Options::default(), DriverAids::default(), 1);
        let g923 = Device { vendor: 0x046d, product: 0xc266, index: 0 };
        let to = Target { bindings: &mut bindings, ffb: &mut ffb, aids: &mut aids, head_gasket: &mut head_gasket };
        let lines = import(&root, Some(reg), &[87, 88], &[g923], to);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(lines[0], "controls from pfX.acm: the keyboard, 046d:c266/0", "{lines:?}");
        let axis = |n| Some(PadInput { device: g923, object: Object::Axis(n) });
        assert_eq!((bindings.get(Action::SteerLeft).pad, bindings.get(Action::Throttle).pad), (axis(0), axis(2)));
        assert_eq!(bindings.get(Action::Handbrake).key, Some(KeyCode::Space));
        assert_eq!(
            (bindings.get(Action::SteerLeft).key, bindings.invert.throttle, bindings.input_rate),
            (None, true, 750)
        );
        assert_eq!((ffb.sensitivity, ffb.car_sensitivity.clone()), ([280, 250, 280], vec![(88, [300, 250, 280])]));
        assert_eq!((ffb.settings.dead_zone, ffb.rate, head_gasket), (0.02, 500, 10));
    }
}
