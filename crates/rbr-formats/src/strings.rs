//! Reader for the game's string tables, `Strings/bstrings.<lang>` in `Root/strings.rbz`.
//!
//! Layout (read from the retail files): the tag `FCOL`, the entry count (u32), then per entry the file offsets (u32
//! each) of its key, a NUL-terminated ASCII string, and of its text, NUL-terminated UTF-16LE. The keys are upper case
//! (`RADIATOR`, `MSG_REPAIRS`); the repair trees' debug names (`Radiator`) match them ignoring case.

use std::collections::HashMap;

/// A string table: key to text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strings(pub HashMap<String, String>);

impl Strings {
    /// The text of a key, ignoring the key's case.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(&key.to_ascii_uppercase()).map(String::as_str)
    }
}

pub fn parse(b: &[u8]) -> Result<Strings, String> {
    let word = |at: usize| -> Result<usize, String> {
        let w = b.get(at..at + 4).ok_or(format!("truncated at {at:#x}"))?;
        Ok(u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as usize)
    };
    if b.get(..4) != Some(b"FCOL") {
        return Err("not an FCOL string table".into());
    }
    let mut out = HashMap::new();
    for k in 0..word(4)? {
        let (key_at, text_at) = (word(8 + 8 * k)?, word(12 + 8 * k)?);
        let key = b.get(key_at..).and_then(|s| s.split(|&c| c == 0).next()).ok_or(format!("entry {k}: bad key"))?;
        let text: Vec<u16> = b
            .get(text_at..)
            .ok_or(format!("entry {k}: bad text"))?
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        out.insert(String::from_utf8_lossy(key).to_ascii_uppercase(), String::from_utf16_lossy(&text));
    }
    Ok(Strings(out))
}

/// The English table (`Strings/bstrings.eng`) from `Root/strings.rbz`.
pub fn english(data: &crate::data::Data) -> Result<Strings, String> {
    let archive = data.resolve("Root/strings.rbz").ok_or("Root/strings.rbz: not found")?;
    let mut zip = crate::rbz::open(&archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name("Strings/bstrings.eng").map_err(|e| e.to_string())?, &mut bytes)
        .map_err(|e| e.to_string())?;
    parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_reads_keys_and_utf16_texts() {
        let mut b = b"FCOL".to_vec();
        b.extend(1u32.to_le_bytes());
        b.extend(16u32.to_le_bytes());
        b.extend(23u32.to_le_bytes());
        b.extend(b"D_LOST\0");
        b.extend("Lost\0".encode_utf16().flat_map(u16::to_le_bytes));
        let s = parse(&b).unwrap();
        assert_eq!(s.get("d_lost"), Some("Lost"));
        assert!(parse(b"FCOX").is_err());
    }

    /// The retail English table holds the service park's texts (`RBR_DATA`).
    #[test]
    fn retail_english_table() {
        let Some(data) = crate::data::Data::from_env() else { return };
        let s = english(&data).unwrap();
        assert_eq!(s.0.len(), 1685);
        assert_eq!(s.get("MSG_REPAIRS"), Some("All repairs finished!"));
        assert_eq!(s.get("Radiator"), Some("Radiator"));
        assert_eq!(s.get("TopMount_LF"), Some("Damper Mount L.F."));
    }
}
