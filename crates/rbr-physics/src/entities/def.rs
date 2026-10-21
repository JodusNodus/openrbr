//! An entity's definition: a named base with a link in a list, and the list it is in (whose count drops when the definition
//! is released). The construction and the release are layout (`layout::construct_entity_def`,
//! `layout::release_entity_def`); the unlink is a call the caller answers.

/// Removes a definition's link from its list.
pub trait Unlink {
    fn unlink(&mut self, link: u32);
}

/// A definition's release when its list is `list` (0 for none): unlinks the definition's `link` and returns the list whose
/// count drops by one. The count and the unlink write different words, so their order does not matter.
pub fn release(list: u32, link: u32, unlink: &mut impl Unlink) -> Option<u32> {
    if list == 0 {
        return None;
    }
    unlink.unlink(link);
    Some(list)
}
