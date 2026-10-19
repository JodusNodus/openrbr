//! The car setup save slots (save-file kind 3): `SavedGames/<car>slot<n>_<name>.lsp`, three per car.
//!
//! - Name: `E20MakeName` (0x49e230) `"%dslot%d_%s.lsp"` with the car (`0x1660808`, its `Cars.ini` index), the slot
//!   (`0x16608e8`, 0-2: the three `CSetupSaveSlotButtonUI` rows) and the name the player typed (`0x1660a24`).
//! - Write: `E56_SaveSlotCommit` (0x558510) → `0x48dca0`: the bytes of `E16_BuildSetup` (0x48d850), which prints the
//!   setup tree (`CBase::GetValueText` 0x52f590 on the root, `("CarSetup" ...)`), puts it in parentheses and pads it
//!   with zeros to a multiple of four. Kind 3 is not encrypted (`CPC_SaveFileManager::Vf14` encrypts kind 1 only).
//!   `CPC_SaveFileManager::Save` deletes the slot's old file (same `<car>slot<n>_` prefix, `Vf1c`) first.
//! - Read: `0x48dd70` → `LoadFromMemory` (0x48d940): a text that does not start with `((` is decrypted first
//!   ([`super::decrypt`]), then tokenized and parsed over the current setup (`CBase::Parse`), as a `Setups/*.lsp` is.
//! - `CSaveFileManager::TestSlot` / `Vf70` (0x453dc0) find a slot's file by its `"%dslot%d_"` prefix.
//!
//! The text is the setup file's whole text, `(("CarSetup" ...))`: the same form `Setups/*.lsp` files have.

use std::fs;
use std::io;

use super::{Error, SaveDir};

/// The slots per car (`CSetupSaveSlotButtonUI` 0, 1, 2).
pub const SLOTS: u32 = 3;

/// The file name of `car`'s slot `slot` named `name`.
pub fn file_name(car: u32, slot: u32, name: &str) -> String {
    format!("{car}slot{slot}_{name}.lsp")
}

/// The car, slot and name of a slot file's name (`sscanf("%dslot%d")`, then the name to the extension).
pub fn parse_name(file: &str) -> Option<(u32, u32, String)> {
    let (car, rest) = file.split_once("slot")?;
    let (slot, rest) = rest.split_once('_')?;
    let name = rest.strip_suffix(".lsp").or_else(|| rest.strip_suffix(".LSP"))?;
    Some((car.parse().ok()?, slot.parse().ok()?, name.to_owned()))
}

/// A setup file's text as a slot file holds it (`E16_BuildSetup`'s bytes): zero-padded to a multiple of four.
pub fn to_bytes(text: &str) -> Vec<u8> {
    let mut out = text.trim_end().as_bytes().to_vec();
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

/// A slot file's setup text (`LoadFromMemory`): decrypted unless it starts with `((`, without the zero padding.
pub fn text(bytes: &[u8]) -> String {
    let mut bytes = bytes.to_vec();
    if !bytes.starts_with(b"((") && !bytes.is_empty() {
        super::decrypt(&mut bytes);
    }
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

impl SaveDir {
    /// `car`'s slots: each one's name, `None` when empty.
    pub fn setup_slots(&self, car: u32) -> Result<Vec<Option<String>>, Error> {
        let mut slots = vec![None; SLOTS as usize];
        for (c, slot, name) in self.slot_files()?.into_iter().filter_map(|f| parse_name(&f)) {
            if c == car && slot < SLOTS {
                slots[slot as usize].get_or_insert(name);
            }
        }
        Ok(slots)
    }

    /// The name and setup text of `car`'s slot `slot`, `None` when it is empty.
    pub fn load_setup_slot(&self, car: u32, slot: u32) -> Result<Option<(String, String)>, Error> {
        let Some(file) = self.slot_file(car, slot)? else {
            return Ok(None);
        };
        let (_, _, name) = parse_name(&file).unwrap_or_default();
        Ok(Some((name, text(&fs::read(self.path.join(&file))?))))
    }

    /// Saves a setup file's `text` into `car`'s slot `slot` as `name`, replacing the slot's old file.
    pub fn save_setup_slot(&self, car: u32, slot: u32, name: &str, text: &str) -> Result<(), Error> {
        let file = file_name(car, slot, name);
        if let Some(old) = self.slot_file(car, slot)?.filter(|old| *old != file) {
            fs::remove_file(self.path.join(old))?;
        }
        self.store(&file, &to_bytes(text))
    }

    /// The file holding `car`'s slot `slot` (`TestSlot`'s prefix test; the first in name order).
    fn slot_file(&self, car: u32, slot: u32) -> Result<Option<String>, Error> {
        let prefix = format!("{car}slot{slot}_");
        Ok(self.slot_files()?.into_iter().find(|f| f.starts_with(&prefix)))
    }

    /// Every `.lsp` in the directory, sorted (the game's kind 3 listing).
    fn slot_files(&self) -> Result<Vec<String>, Error> {
        let entries = match fs::read_dir(&self.path) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name.to_ascii_lowercase().ends_with(".lsp") {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "((\"CarSetup\"\n SetupGeneral (\":-D\"\n  SteeringLock 540\n  )\n ))\n";

    #[test]
    fn names_are_the_games() {
        assert_eq!(file_name(5, 2, "fast gravel"), "5slot2_fast gravel.lsp");
        assert_eq!(parse_name("5slot2_fast gravel.lsp"), Some((5, 2, "fast gravel".to_owned())));
        assert_eq!(parse_name("pfthomas.rbr"), None);
    }

    #[test]
    fn bytes_are_padded_and_read_back_plain_or_encrypted() {
        let bytes = to_bytes(TEXT);
        assert!(bytes.len().is_multiple_of(4));
        assert!(bytes.starts_with(b"((\"CarSetup\""));
        assert_eq!(text(&bytes), TEXT.trim_end());
        let mut hidden = bytes.clone();
        crate::saves::encrypt(&mut hidden);
        assert_eq!(text(&hidden), TEXT.trim_end());
    }

    #[test]
    fn a_slot_is_saved_listed_replaced_and_loaded() {
        let dir = std::env::temp_dir().join(format!("rbr-slots-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let saves = SaveDir { path: dir.clone() };
        assert_eq!(saves.setup_slots(6).unwrap(), vec![None, None, None]);
        saves.save_setup_slot(6, 1, "a", TEXT).unwrap();
        saves.save_setup_slot(6, 1, "b", TEXT).unwrap();
        saves.save_setup_slot(7, 0, "c", TEXT).unwrap();
        assert_eq!(saves.setup_slots(6).unwrap(), vec![None, Some("b".to_owned()), None]);
        assert!(!dir.join("6slot1_a.lsp").exists());
        assert_eq!(saves.load_setup_slot(6, 1).unwrap(), Some(("b".to_owned(), TEXT.trim_end().to_owned())));
        assert_eq!(saves.load_setup_slot(6, 2).unwrap(), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
