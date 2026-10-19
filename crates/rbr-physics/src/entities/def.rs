//! The def of a CEntity (`E43EntityDef`, src/rbr/RBR_E43_EntityC.cpp:85-115; asm 0x51a530 and 0x51a560): a
//! `CBase` (the name) with a list link at `+0x0c`, and the list it is in at `+0x14` (its count at +8 drops when
//! the def is released). The construction and the release are the original's; the unlink is a call the caller
//! answers. The base's constructor is `layout::construct_base`.

pub use crate::ntweak::layout::BASE_VTABLE;

/// The def's vtable after construction (0x73cc04), and the base's after release (0x72e444).
pub const DEF_VTABLE: u32 = 0x73CC04;

/// `E43_Unlink180(link)` (0x51a180): removes the link at `link` from its list.
pub trait Unlink {
    fn unlink(&mut self, link: u32);
}

/// `E43EntityDef::~E43EntityDef` (0x51a560) on a def whose list is `list` (0 for none): unlinks the def's link
/// (`link`, `+0x0c`) and returns the list whose count drops by one. The count (at the list's +8) and the unlink
/// write different words, so their order does not matter.
pub fn release(list: u32, link: u32, unlink: &mut impl Unlink) -> Option<u32> {
    if list == 0 {
        return None;
    }
    unlink.unlink(link);
    Some(list)
}
