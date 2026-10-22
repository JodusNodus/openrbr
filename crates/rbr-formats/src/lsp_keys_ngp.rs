//! NGP key names and offsets come from the player's DLL (`crate::ngp`); stock reader information stays here.

use crate::lsp_keys::{Class, KeyInfo, ReaderKind, KEYS, PARENTS};

/// The key record of `key` in `class` under NGP, else in its parents (`None` when neither has it), as
/// [`crate::lsp_keys::key_info`] for 1.02. An NGP record takes the reader, child and composite of the stock
/// record at its offset (a renamed one keeps its member), or for a surface the material's; another appended one
/// has none of them, and the parser reads its member by the member object's own type, as the original's
/// `CBase::Parse` does.
pub fn ngp_key_info(class: Class, key: &str) -> Option<KeyInfo> {
    if let Some(record) = crate::ngp::current().and_then(|t| t.keys.iter().find(|r| r.class == class && r.name == key))
    {
        let (key, offset) = (record.name.as_str(), record.offset);
        // A renamed record keeps its member's; a surface NGP appends is a material like every other.
        let stock = KEYS.iter().find(|i| i.class == class && (i.offset == offset || class == Class::MaterialManager));
        let stock = stock.map(|info| KeyInfo { offset, ..*info });
        return Some(match stock {
            Some(info) => KeyInfo { key, used_in_retail: false, ..info },
            None => KeyInfo {
                class,
                key,
                offset,
                reader: ReaderKind::Unknown,
                child: None,
                composite: None,
                used_in_retail: false,
            },
        });
    }
    (!is_removed(class, key))
        .then(|| KEYS.iter().find(|i| i.class == class && i.key == key).copied())
        .flatten()
        .or_else(|| {
            PARENTS.iter().filter(|(child, _)| *child == class).find_map(|(_, parent)| ngp_key_info(*parent, key))
        })
}

/// The offset of `key` in `class` under NGP, else in its parents (`None` when neither has it), as
/// [`crate::lsp_keys::key_offset`] for 1.02.
pub fn ngp_key_offset(class: Class, key: &str) -> Option<u32> {
    ngp_key_info(class, key).map(|info| info.offset)
}

/// Whether NGP renamed this stock key.
pub fn is_removed(class: Class, key: &str) -> bool {
    crate::ngp::current().is_some_and(|t| t.removed.iter().any(|(c, k)| *c == class && k == key))
}
