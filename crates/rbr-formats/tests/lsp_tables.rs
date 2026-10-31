//! The exe-read `lsp_tables` against retail data.
//!
//! Runs only when `RBR_DATA` points at the extracted install tree, like the
//! other retail tests. Every DefManager registry class must appear as a
//! block in some retail `.lsp` (physics and car overlays in
//! `Root/physics.rbz` and `Root/misc.rbz`), and the `bit_names` lookup must
//! cover exactly the proven bitfield keys.

use rbr_formats::{lsp, lsp_keys, lsp_tables};
use std::collections::BTreeSet;
use std::path::Path;

mod common;

use lsp::Node;

fn block_head(list: &[Node]) -> Option<&str> {
    match list.first()? {
        Node::Atom(s) | Node::Quoted(s) => Some(s.as_str()),
        Node::List(_) => None,
    }
}

fn collect_heads(items: &[Node], heads: &mut BTreeSet<String>) {
    for item in items {
        let Node::List(inner) = item else {
            continue;
        };
        if let Some(name) = block_head(inner) {
            heads.insert(name.to_owned());
            // Recurse with the instance body skipped like the key test.
            let body = match inner.get(1) {
                Some(Node::List(b)) if block_head(b).is_some_and(|h| h != name) => &b[1..],
                _ => &inner[1..],
            };
            let body = match body.first() {
                Some(Node::Quoted(_)) => &body[1..],
                _ => body,
            };
            collect_heads(body, heads);
        } else {
            collect_heads(inner, heads);
        }
    }
}

#[test]
fn registry_classes_appear_in_retail() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let mut heads = BTreeSet::new();
    let mut files = 0;
    for archive_name in ["Root/physics.rbz", "Root/misc.rbz"] {
        let path = Path::new(&data_dir).join(archive_name);
        common::each_rbz_entry(&path, |entry_name, bytes| {
            if !entry_name.ends_with(".lsp") {
                return;
            }
            let text = std::str::from_utf8(bytes).expect("utf-8");
            let doc = lsp::parse(text).unwrap_or_else(|e| panic!("{entry_name}: {e}"));
            files += 1;
            let Node::List(root) = &doc[0] else {
                panic!("{entry_name}: expected one top-level list");
            };
            let root = match &root[..] {
                [Node::List(inner)] => inner,
                _ => panic!("{entry_name}: expected single root list"),
            };
            collect_heads(root, &mut heads);
        });
    }
    assert!(files > 80, "expected most retail .lsp files, got {files}");
    for entry in lsp_tables::REGISTRY {
        assert_ne!(entry.dict, 0, "{}: registry dict", entry.name);
        assert_ne!(entry.factory, 0, "{}: registry factory", entry.name);
        assert!(heads.contains(entry.name), "{}: registry class never a block in retail", entry.name);
    }
    eprintln!("registry: {} classes over {files} .lsp files", lsp_tables::REGISTRY.len());
}

#[test]
fn bit_names_cover_proven_keys_only() {
    use lsp_keys::Class as C;
    assert_eq!(lsp_tables::bit_names(C::PhysDef, "Flags"), Some(lsp_tables::BITFIELD_FLAGS));
    assert_eq!(lsp_tables::bit_names(C::PhysDef, "DbgFlags"), Some(lsp_tables::BITFIELD_DBGFLAGS));
    assert_eq!(lsp_tables::bit_names(C::PhysDef, "LogFlags"), Some(lsp_tables::BITFIELD_LOGFLAGS));
    assert_eq!(lsp_tables::bit_names(C::Material, "Flags"), Some(lsp_tables::BITFIELD_MATERIAL));
    assert_eq!(lsp_tables::bit_names(C::Material, "Type"), None);
    assert_eq!(lsp_tables::bit_names(C::PhysDef, "MaxTimeStep"), None);
    assert_eq!(lsp_tables::bit_names(C::SolidDef, "Flags"), None);
}
