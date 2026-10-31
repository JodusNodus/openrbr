//! RSF's cache files (`rsfdata/cache/*.json`): arrays of flat objects, each value kept as its text.

use std::collections::HashMap;

/// A row of RSF's cache: an object of scalars, each kept as its text (`null` left out).
pub(super) type Row = HashMap<String, String>;

/// The objects of a JSON array of flat objects whose values are strings, numbers, booleans or `null` (`null` left
/// out). Anything else ends the read with the objects read whole before it.
pub(super) fn rows(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    rows_of(&mut text.trim_start_matches('\u{feff}').chars().peekable(), &mut rows);
    rows
}

fn space(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.next_if(|c| c.is_whitespace()).is_some() {}
}

fn expect(chars: &mut std::iter::Peekable<std::str::Chars>, want: char) -> Option<()> {
    space(chars);
    (chars.next()? == want).then_some(())
}

fn string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    expect(chars, '"')?;
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'u' => {
                    let hex: String = (0..4).map(|_| chars.next()).collect::<Option<_>>()?;
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('\u{fffd}'));
                }
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' | 'f' => {}
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
}

fn rows_of(chars: &mut std::iter::Peekable<std::str::Chars>, rows: &mut Vec<Row>) -> Option<()> {
    expect(chars, '[')?;
    space(chars);
    if chars.next_if_eq(&']').is_some() {
        return Some(());
    }
    loop {
        expect(chars, '{')?;
        let mut row = Row::new();
        space(chars);
        if chars.next_if_eq(&'}').is_none() {
            loop {
                let key = string(chars)?;
                expect(chars, ':')?;
                space(chars);
                if chars.peek() == Some(&'"') {
                    row.insert(key, string(chars)?);
                } else {
                    let word: String =
                        std::iter::from_fn(|| chars.next_if(|c| !matches!(c, ',' | '}') && !c.is_whitespace()))
                            .collect();
                    if word.is_empty() || word.starts_with(['{', '[']) {
                        return None;
                    }
                    if word != "null" {
                        row.insert(key, word);
                    }
                }
                space(chars);
                match chars.next()? {
                    ',' => continue,
                    '}' => break,
                    _ => return None,
                }
            }
        }
        rows.push(row);
        space(chars);
        match chars.next()? {
            ',' => continue,
            ']' => return Some(()),
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_rows_are_flat_objects_of_scalars() {
        let read = rows(
            "\u{feff} [{\"id\":\"24\",\"name\":\"Evo \\/ IX \\u00e9\",\"audio\":null,\"n\":5},{\"note\":\"a, b\"}] ",
        );
        assert_eq!(read.len(), 2);
        assert_eq!(read[0]["name"], "Evo / IX \u{e9}");
        assert_eq!((read[0]["id"].as_str(), read[0]["n"].as_str()), ("24", "5"));
        assert!(!read[0].contains_key("audio"));
        assert_eq!(read[1]["note"], "a, b");
    }

    #[test]
    fn a_bad_value_keeps_the_objects_before_it() {
        assert_eq!(
            rows(r#"[{"id":"1"},{"id":"2","a":{"b":1}},{"id":"3"}]"#),
            vec![Row::from([("id".into(), "1".into())])]
        );
        assert_eq!(rows(r#"[{"id":"1"},{"id":"2""#).len(), 1);
        assert!(rows("[1]").is_empty());
        assert!(rows("").is_empty());
    }
}
