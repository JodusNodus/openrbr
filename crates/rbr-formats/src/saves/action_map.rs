//! The controller action map `SavedGames/pf<name>.acm` (kind 6; `E28FileMap::Save` 0x4c3ad0, `Load` 0x4c5800). Not
//! encrypted and not checksummed: the tag `AMAP` (`PAMA` on disk), the version 0x300 and the record count, then per
//! record its device count, its binding count, the devices' DirectInput instance GUIDs (16 bytes each) and the
//! bindings (three words each). `Load` ignores a file whose tag or version differs.

use super::{Error, Reader, Writer};

const TAG: [u8; 4] = *b"PAMA";
pub const VERSION: u32 = 0x300;

/// The game's actions, by the index a binding stores (`CActionMap`'s switch, `RBR_E28_Action.cpp`): the
/// localisation keys of the controller page.
pub const ACTIONS: [&str; 21] = [
    "STEERING",
    "LEFT",
    "RIGHT",
    "THROTTLE",
    "ACCELERATE_AND_BRAKE",
    "BRAKE",
    "HANDBRAKE",
    "GEARUP",
    "GEARDWN",
    "CH_CAM",
    "PAUSE",
    "CLUTCH",
    "IGNITION",
    "REVERSE",
    "NEUTRAL",
    "FIRSTGEAR",
    "SECONDGEAR",
    "THIRDGEAR",
    "FOURTHGEAR",
    "FIFTHGEAR",
    "SIXTHGEAR",
];

/// DirectInput's `GUID_SysKeyboard` as the file stores it.
pub const SYS_KEYBOARD: [u8; 16] =
    [0x61, 0x2b, 0x1d, 0x6f, 0xa0, 0xd5, 0xcf, 0x11, 0xbf, 0xc7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionMap {
    pub records: Vec<Record>,
}

/// One record: the devices it names and its bindings (`FillMaps` 0x4c4da0 writes one per bound action slot).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub devices: Vec<[u8; 16]>,
    pub bindings: Vec<Binding>,
}

/// One binding: `device` indexes [`Record::devices`]; `object` is the device object's DirectInput type word
/// (`DIDFT_*`: the kind in the low byte, the instance in bits 8..24, the force-feedback flags above); `action` indexes
/// [`ACTIONS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub device: u32,
    pub object: u32,
    pub action: u32,
}

/// What a binding's object is, from its `DIDFT_*` kind bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Object {
    /// An axis (`DIDFT_ABSAXIS` or `DIDFT_RELAXIS`) and its instance.
    Axis(u32),
    /// A button (`DIDFT_PSHBUTTON` or `DIDFT_TGLBUTTON`); on the keyboard the instance is the DirectInput key code.
    Button(u32),
    /// A point-of-view hat (`DIDFT_POV`).
    Pov(u32),
    Other(u32),
}

impl Binding {
    /// The action's localisation key, if the index is one of the game's.
    pub fn action_key(&self) -> Option<&'static str> {
        ACTIONS.get(self.action as usize).copied()
    }

    pub fn object(&self) -> Object {
        let instance = (self.object >> 8) & 0xffff;
        match self.object & 0xff {
            k if k & 0x03 != 0 => Object::Axis(instance),
            k if k & 0x0c != 0 => Object::Button(instance),
            k if k & 0x10 != 0 => Object::Pov(instance),
            _ => Object::Other(self.object),
        }
    }
}

impl ActionMap {
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        r.tag(TAG)?;
        let version = r.u32()?;
        if version != VERSION {
            return Err(Error::Version { found: version, want: VERSION });
        }
        let count = r.u32()?;
        let records = (0..count)
            .map(|_| {
                let (devices, bindings) = (r.u32()?, r.u32()?);
                let devices = (0..devices).map(|_| r.array()).collect::<Result<_, _>>()?;
                let bindings = (0..bindings)
                    .map(|_| Ok(Binding { device: r.u32()?, object: r.u32()?, action: r.u32()? }))
                    .collect::<Result<_, Error>>()?;
                Ok(Record { devices, bindings })
            })
            .collect::<Result<_, Error>>()?;
        if r.at() != bytes.len() {
            return Err(Error::TooLarge { len: bytes.len(), size: r.at() });
        }
        Ok(Self { records })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&TAG);
        w.words(&[VERSION, self.records.len() as u32]);
        for rec in &self.records {
            w.words(&[rec.devices.len() as u32, rec.bindings.len() as u32]);
            rec.devices.iter().for_each(|g| w.bytes(g));
            for b in &rec.bindings {
                w.words(&[b.device, b.object, b.action]);
            }
        }
        w.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_by_kind() {
        let b = |object| Binding { device: 0, object, action: 0 };
        assert_eq!(b(0x0100_0002).object(), Object::Axis(0));
        assert_eq!(b(0x0302).object(), Object::Axis(3));
        assert_eq!(b(0xcb04).object(), Object::Button(0xcb));
        assert_eq!(b(0x0200_0b04).object(), Object::Button(0xb));
        assert_eq!(b(0x0210).object(), Object::Pov(2));
        assert_eq!(Binding { action: 11, ..b(0) }.action_key(), Some("CLUTCH"));
        assert_eq!(Binding { action: 21, ..b(0) }.action_key(), None);
    }

    #[test]
    fn truncation_points_end_where_they_stop() {
        let map = ActionMap {
            records: vec![Record {
                devices: vec![SYS_KEYBOARD],
                bindings: vec![Binding { device: 0, object: 0xcb04, action: 1 }],
            }],
        };
        let bytes = map.write();
        // Tag, version, count, the two counts, one GUID, one binding.
        assert_eq!(bytes.len(), 12 + 8 + 16 + 12);
        for (cut, at) in [(0, 0), (2, 0), (10, 8), (25, 20), (47, 44)] {
            assert_eq!(ActionMap::read(&bytes[..cut]), Err(Error::Eof { at }), "cut at {cut}");
        }
    }

    #[test]
    fn a_map_round_trips() {
        let map = ActionMap {
            records: vec![Record {
                devices: vec![SYS_KEYBOARD],
                bindings: vec![Binding { device: 0, object: 0xcb04, action: 1 }],
            }],
        };
        let bytes = map.write();
        assert_eq!(bytes.len(), 12 + 8 + 16 + 12);
        assert_eq!(ActionMap::read(&bytes).unwrap(), map);
        assert!(matches!(ActionMap::read(&bytes[..bytes.len() - 1]), Err(Error::Eof { .. })));
    }

    /// The maps the original 1.02 wrote (`pf<name>.acm`), when `RBR_SAVES` names their directory: each reads and
    /// writes back byte for byte. The maintainers' (`pfthomas.acm`) is for a wheel, its pedals and the keyboard.
    #[test]
    fn original_map_round_trips() {
        let Some(dir) = std::env::var_os("RBR_SAVES") else {
            eprintln!("skipping: RBR_SAVES not set");
            return;
        };
        let saves = crate::saves::SaveDir { path: dir.into() };
        for name in saves.profiles().unwrap() {
            let Ok(bytes) = std::fs::read(saves.path.join(crate::saves::Kind::ActionMap.file_name(&name))) else {
                continue;
            };
            let map = ActionMap::read(&bytes).unwrap();
            assert_eq!(map.write(), bytes, "{name}");
            if name == "thomas" {
                check_wheel_map(&map);
            }
        }
    }

    fn check_wheel_map(map: &ActionMap) {
        let [rec] = &map.records[..] else { panic!("{} records", map.records.len()) };
        assert_eq!(rec.devices.len(), 3);
        assert_eq!(rec.devices[2], SYS_KEYBOARD);
        let bound: Vec<_> = rec.bindings.iter().map(|b| (b.action_key().unwrap(), b.device, b.object())).collect();
        assert_eq!(bound[0], ("STEERING", 1, Object::Axis(0)));
        // The steering keys are the keyboard's arrows (DIK_LEFT, DIK_RIGHT).
        assert_eq!(bound[1], ("LEFT", 2, Object::Button(0xcb)));
        assert_eq!(bound[2], ("RIGHT", 2, Object::Button(0xcd)));
        assert!(rec.bindings.iter().all(|b| (b.device as usize) < rec.devices.len()));
    }
}
