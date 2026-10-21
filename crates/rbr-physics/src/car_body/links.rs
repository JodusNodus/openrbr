//! The car's connection lists: appending a link to a list and unlinking it. A list head has first, last and count; a link has next, prev and owner, the list it is in (none when it is
//! free). The original's objects are addressed by their memory address. `LinkRef` and `ListRef` are opaque handles to
//! those addresses, and `Links` is the store the ports read and write through (`layout.rs` implements it on the
//! memory). The writes happen in the original's order.

/// The address of a link object (`CBodyLink`, 12 bytes). Never null: a free link has no owner, not a null link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkRef(u32);

impl LinkRef {
    pub fn new(addr: u32) -> Self {
        LinkRef(addr)
    }

    pub fn addr(self) -> u32 {
        self.0
    }
}

/// The address of a list head (`E34LinkList10`, 12 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRef(u32);

impl ListRef {
    pub fn new(addr: u32) -> Self {
        ListRef(addr)
    }

    pub fn addr(self) -> u32 {
        self.0
    }
}

/// A link: the neighbours in its list and the list it is in (`None` for the null pointers the original stores).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub next: Option<LinkRef>,
    pub prev: Option<LinkRef>,
    pub owner: Option<ListRef>,
}

/// A list head: its first and last links, and the number of links in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct List {
    pub first: Option<LinkRef>,
    pub last: Option<LinkRef>,
    pub count: u32,
}

/// The words the list code reads and writes, by handle. A neighbour is changed one word at a time (as the original does),
/// so a neighbour is read only where the original reads it.
pub trait Links {
    fn link(&self, at: LinkRef) -> Link;
    fn set_link(&mut self, at: LinkRef, link: Link);
    fn set_next(&mut self, at: LinkRef, next: Option<LinkRef>);
    fn set_prev(&mut self, at: LinkRef, prev: Option<LinkRef>);
    fn list(&self, at: ListRef) -> List;
    fn set_list(&mut self, at: ListRef, list: List);
}

/// Takes `a` out of its list (if it has one) and clears its three words. It writes
/// the neighbours' links and the list's first or last (both cleared when `a` was the only member).
pub fn unlink(store: &mut impl Links, a: LinkRef) {
    let link = store.link(a);
    if let Some(owner) = link.owner {
        let mut list = store.list(owner);
        match (link.next, link.prev) {
            (Some(next), prev) => {
                store.set_prev(next, prev);
                match prev {
                    Some(prev) => store.set_next(prev, Some(next)),
                    None => list.first = Some(next),
                }
            }
            (None, Some(prev)) => {
                store.set_next(prev, None);
                list.last = Some(prev);
            }
            (None, None) => {
                list.first = None;
                list.last = None;
            }
        }
        store.set_list(owner, list);
    }
    store.set_link(a, Link::default());
}

/// Appends `a` to the list `head`: takes `a` out of its current list first (its owner's count
/// goes down, then `unlink`), then puts it at the front of `head` and counts it. The head is read after the unlink, so
/// `a` can come back to the list it was in. A null car (`None`) only takes `a` out, as the original skips the append.
pub fn append(store: &mut impl Links, head: Option<ListRef>, a: LinkRef) {
    if let Some(owner) = store.link(a).owner {
        let mut list = store.list(owner);
        list.count = list.count.wrapping_sub(1);
        store.set_list(owner, list);
        unlink(store, a);
    }
    let Some(head) = head else {
        return;
    };
    let mut list = store.list(head);
    let first = list.first;
    store.set_link(a, Link { next: first, prev: None, owner: Some(head) });
    match first {
        Some(front) => store.set_prev(front, Some(a)),
        None => list.last = Some(a),
    }
    list.first = Some(a);
    list.count = list.count.wrapping_add(1);
    store.set_list(head, list);
}

/// The signal connections' `push_back` of the free link `a` on the list `head`: `a`
/// goes after the list's last link, and the list counts it.
pub fn push_back(store: &mut impl Links, head: ListRef, a: LinkRef) {
    let mut list = store.list(head);
    let last = list.last;
    store.set_link(a, Link { next: None, prev: last, owner: Some(head) });
    match last {
        Some(back) => store.set_next(back, Some(a)),
        None => list.first = Some(a),
    }
    list.last = Some(a);
    list.count = list.count.wrapping_add(1);
    store.set_list(head, list);
}
