//! The reader sees every retail `.lsp` file as the game's tokenizer does.
//!
//! `harness/lsp_tokens.py` runs the original tokenizer (`0x52ec80`) in the emulator over each retail file and each
//! group token, leaves out the tokens in comments, and writes the trees. This test parses the same files with
//! [`lsp::parse`] and compares. Runs only when `RBR_DATA` (the install tree) and `RBR_LSP_TOKENS` (the harness's
//! output) are set.

use rbr_formats::{lsp, rbz};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use lsp::Node;

/// The harness's form: an atom as it is, a quoted string in quotes, a list in parentheses, single spaces between.
fn canonical(items: &[Node]) -> String {
    let item = |n: &Node| match n {
        Node::Atom(s) => s.clone(),
        Node::Quoted(s) => format!("\"{s}\""),
        Node::List(items) => format!("({})", canonical(items)),
    };
    items.iter().map(item).collect::<Vec<_>>().join(" ")
}

#[test]
fn retail_lsp_matches_the_game_tokenizer() {
    let (Ok(data_dir), Ok(tokens)) = (std::env::var("RBR_DATA"), std::env::var("RBR_LSP_TOKENS")) else {
        eprintln!("skipping: RBR_DATA or RBR_LSP_TOKENS not set");
        return;
    };
    let game: BTreeMap<String, String> = std::fs::read_to_string(&tokens)
        .unwrap_or_else(|e| panic!("{tokens}: {e}"))
        .lines()
        .map(|l| {
            let (name, tree) = l.split_once('\t').expect("name<TAB>tree");
            (name.to_owned(), tree.to_owned())
        })
        .collect();
    let mut mismatches = Vec::new();
    let mut files = 0;
    for archive_name in ["Root/physics.rbz", "Root/misc.rbz"] {
        let path = Path::new(&data_dir).join(archive_name);
        let mut archive = rbz::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).expect("entry");
            let entry_name = entry.name().expect("ascii name").into_owned();
            if !entry_name.ends_with(".lsp") {
                continue;
            }
            let name = format!("{archive_name}!{entry_name}");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let text = std::str::from_utf8(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let doc = lsp::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            // The game reads the first list only: its tokenizer strips the opening parenthesis and stops at the
            // matching one.
            let Some(Node::List(outer)) = doc.first() else {
                panic!("{name}: no outer list");
            };
            let expected = game.get(&name).unwrap_or_else(|| panic!("{name}: not in {tokens}"));
            files += 1;
            let ours = canonical(outer);
            if &ours != expected || doc.len() != 1 {
                let at = ours
                    .bytes()
                    .zip(expected.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(ours.len().min(expected.len()));
                let from = at.saturating_sub(60);
                mismatches.push(format!(
                    "{name} ({} top-level items): ours ...{}... the game's ...{}...",
                    doc.len(),
                    &ours[from..(at + 60).min(ours.len())],
                    &expected[from..(at + 60).min(expected.len())]
                ));
            }
        }
    }
    assert_eq!(files, game.len(), "files in {tokens} not in the archives");
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
