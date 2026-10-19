//! E33 intrusive broadphase and cell links.

use crate::mem::Mem;

use super::layout::Link;

/// Visitor slot called by CBodyLink::NotifyFiltered (0x4e0080).
pub trait NotifyFilteredHost {
    fn vf04(&mut self, mem: &mut Mem, visitor: u32, body: u32) -> u32;
}

/// CBodyLink::Head (0x4e0000).
pub fn head(mem: &Mem, link: u32) -> u32 {
    mem.u32(link)
}

/// CBodyLink::CBodyLink (0x4e0010), returning the constructor's `this` value.
pub fn body_link_ctor(mem: &mut Mem, link: u32) -> u32 {
    Link::default().store(mem, link);
    link
}

/// CCellLink::CCellLink (0x4e01d0), returning the constructor's `this` value.
pub fn cell_link_ctor(mem: &mut Mem, link: u32) -> u32 {
    Link::default().store(mem, link);
    link
}

/// CBodyLink::NotifyFiltered (0x4e0080): visit unfiltered, non-null bodies after this head.
pub fn notify_filtered(mem: &mut Mem, head: u32, visitor: u32, host: &mut impl NotifyFilteredHost) {
    let mut link = mem.u32(head);
    while link != 0 {
        let body = mem.u32(link + 0x0c);
        if body != 0 && mem.u32(body + 0x60) & 0x20 == 0 {
            let _ = host.vf04(mem, visitor, body);
        }
        link = mem.u32(link);
    }
}
