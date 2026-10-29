//! The damaged-input loop the format tests share (`#[cfg(test)]` only): every truncation of a valid input, and
//! every copy with one byte replaced by a damage value. The parser must return, never panic, hang or allocate
//! wildly; whether it accepts or rejects each damaged input is not checked.

/// Feeds `parse` every truncation of `valid` (`0..=valid.len()`) and every copy with the byte at one position set to
/// each of `damages`, ignoring the results. Keep `valid` small (a few hundred bytes): the loop runs
/// `valid.len() * (1 + damages.len())` parses.
pub(crate) fn never_panics(valid: &[u8], damages: &[u8], parse: impl Fn(&[u8])) {
    for cut in 0..=valid.len() {
        parse(&valid[..cut]);
    }
    let mut copy = valid.to_vec();
    for at in 0..valid.len() {
        for &value in damages {
            copy[at] = value;
            parse(&copy);
        }
        copy[at] = valid[at];
    }
}
