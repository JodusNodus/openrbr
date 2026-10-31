//! The loader's in-place fixups of a `.col` image ([`relocate`]).

use super::*;

/// Applies the loader's in-place fixups (`FUN_004fe570` → `FUN_004fdff0`)
/// to a file image that will live at `base`, mutating `bytes` the way the
/// game does:
///
/// - the root object (type 4/3) has its tree relocated with the
///   placeholder mask (`0xffe00000`); node pointers become absolute
///   (`tree address + relative offset`);
/// - each mode-1 cell with vertices takes the next vertex slot (written
///   to `+2`, stamped into its leaves' slot bits), is marked relocated
///   (`[1] = 2`), has its tree relocated with no mask, and is grafted
///   onto the root placeholder its flags select (32 bytes copied, bit 21
///   cleared);
/// - cells without vertices are skipped (no slot, no graft), exactly as
///   the loader's early return does.
///
/// Host-side effects are not reproduced: the vertex slot table itself
/// (`FUN_004fd630` writes the vertex base into the collision system's
/// table), the brake-wall build (`SetWall`), and the mesh-list objects
/// (`FUN_004fb220` allocates `CCollisionMeshWater`s) all live outside the
/// image. Slots are handed out from 0 in table order, which assumes a
/// fresh collision system; the game crashes past 0x400 slots and so does
/// this function ([`Error::TooManyCells`]). Mode 2 (re-entry) is refused:
/// no retail file has it.
///
/// All reads and writes are bounds-checked; like the loader, shared tree
/// pairs would corrupt (the second visit re-adds the base), surfacing as
/// [`Error::Truncated`] instead of an access violation.
pub fn relocate(bytes: &mut [u8], base: u32) -> Result<Relocated, Error> {
    let magic = read_u32_le(bytes, 0)?;
    if magic != MAGIC {
        return Err(Error::BadMagic { found: magic });
    }
    let root_off = read_u32_le(bytes, 4)? as usize;
    let cell_count = read_u32_le(bytes, 8)? as usize;
    let table_off = read_u32_le(bytes, 12)? as usize;
    let table_len = cell_count.checked_mul(4).ok_or(Error::Truncated {
        offset: table_off,
        needed: usize::MAX,
        len: bytes.len(),
    })?;
    bytes_at(bytes, table_off, table_len)?;

    let root_raw = bytes_at(bytes, root_off, ROOT_HEADER_LEN)?;
    if root_raw[0] != ROOT_SUB || root_raw[1] != ROOT_MODE {
        return Err(Error::BadRoot { offset: root_off });
    }
    let tree_off = root_off + u32_of(&root_raw[0x10..]) as usize;
    let root_vertices = u32_of(&root_raw[0x08..]);

    let mut next_slot: u32 = 0;
    // Next free vertex slot, or [`Error::TooManyCells`] past 0x400.
    fn take_slot(next_slot: &mut u32, slotted: &mut usize) -> Result<u32, Error> {
        if *next_slot as usize >= SLOT_COUNT {
            return Err(Error::TooManyCells { with_vertices: *slotted + 1 });
        }
        *slotted += 1;
        let slot = *next_slot;
        *next_slot += 1;
        Ok(slot)
    }

    let mut out = Relocated { slotted: 0, grafted: 0, skipped: 0 };
    // The root carries no vertices in retail, so its slot is 0 and no
    // slot-table entry is consumed (the loader skips `FUN_004fd630`).
    let root_slot = if root_vertices != 0 { take_slot(&mut next_slot, &mut out.slotted)? } else { 0 };
    relocate_tree(bytes, tree_off, abs_at(base, tree_off)?, PLACEHOLDER_MASK, root_slot)?;
    let root_abs = abs_at(base, tree_off)?;

    for i in 0..cell_count {
        let cell_off = read_u32_le(bytes, table_off + 4 * i)? as usize;
        let mode = bytes_at(bytes, cell_off, CELL_HEADER_LEN)?[1];
        if mode == 2 {
            return Err(Error::AlreadyRelocated { offset: cell_off });
        }
        if mode != CELL_MODE {
            return Err(Error::BadCellMode { offset: cell_off, mode });
        }
        if read_u32_le(bytes, cell_off + 8)? == 0 {
            out.skipped += 1;
            continue;
        }
        let slot = take_slot(&mut next_slot, &mut out.slotted)?;
        bytes_at_mut(bytes, cell_off + 1, 1)?[0] = 2;
        bytes_at_mut(bytes, cell_off + 2, 2)?.copy_from_slice(&(slot as u16).to_le_bytes());
        let cell_tree_off = cell_off + read_u32_le(bytes, cell_off + 16)? as usize;
        relocate_tree(bytes, cell_tree_off, abs_at(base, cell_tree_off)?, NO_MASK, slot)?;
        if graft(bytes, base, root_abs, cell_tree_off, read_u32_le(bytes, cell_off + 4)?, i)? {
            out.grafted += 1;
        }
    }
    Ok(out)
}

/// File offset `off` as the absolute address it has when the image lives
/// at `base` (wrapping, like the loader's 32-bit add).
fn abs_at(base: u32, off: usize) -> Result<u32, Error> {
    u32::try_from(off).map(|off| base.wrapping_add(off)).map_err(|_| Error::Truncated {
        offset: off,
        needed: 1,
        len: off,
    })
}

/// An absolute address back to its file offset, bounds-checked to `len`.
fn off_at(base: u32, abs: u32, len: usize, cell: usize) -> Result<usize, Error> {
    let off = abs.wrapping_sub(base) as usize;
    if (abs.wrapping_sub(base) as u64) + NODE_LEN as u64 > len as u64 {
        return Err(Error::BadGraft { cell });
    }
    Ok(off)
}

/// The `FUN_004fd6b0` walk over raw bytes: node pointers become
/// `tree_abs + relative`, leaves take `slot` in bits 22..31 (bit 21
/// preserved), and nodes hitting `mask` are skipped wholesale.
fn relocate_tree(bytes: &mut [u8], tree_off: usize, tree_abs: u32, mask: u32, slot: u32) -> Result<(), Error> {
    let mut pending = vec![tree_off];
    while let Some(off) = pending.pop() {
        let raw = bytes_at_mut(bytes, off, NODE_LEN)?;
        let flags = u32_of(&raw[0x18..]);
        let pointer = u32_of(&raw[0x1c..]);
        if flags & mask != 0 {
            continue;
        }
        if flags & COUNT_MASK == 0 {
            if pointer != 0 {
                put_u32_at(bytes, off + 0x1c, pointer.wrapping_add(tree_abs))?;
                let pair = tree_off + pointer as usize;
                bytes_at(bytes, pair, 2 * NODE_LEN)?;
                pending.push(pair);
                pending.push(pair + NODE_LEN);
            }
        } else {
            let stamped = flags & 0x003f_ffff | slot << 22;
            put_u32_at(bytes, off + 0x18, stamped)?;
            put_u32_at(bytes, off + 0x1c, pointer.wrapping_add(tree_abs))?;
        }
    }
    Ok(())
}

/// The mode-1 graft: follow `depth` path bits from the (relocated) root
/// tree, copy the cell tree's 32 root bytes over the node reached, and
/// clear bit 21. Depth 0 relocates without grafting (loader's early
/// return); an unreachable path is [`Error::BadGraft`].
fn graft(
    bytes: &mut [u8],
    base: u32,
    root_abs: u32,
    cell_tree_off: usize,
    flags: u32,
    cell: usize,
) -> Result<bool, Error> {
    let len = bytes.len();
    let depth = flags >> 24;
    if depth == 0 {
        return Ok(false);
    }
    let mut node = root_abs;
    let mut bits = flags;
    for _ in 0..depth - 1 {
        let off = off_at(base, node, len, cell)?;
        node = read_u32_le(bytes, off + 0x1c)?.wrapping_add((bits & 1) * NODE_LEN as u32);
        bits >>= 1;
    }
    let off = off_at(base, node, len, cell)?;
    let target = read_u32_le(bytes, off + 0x1c)?.wrapping_add((bits & 1) * NODE_LEN as u32);
    let target_off = off_at(base, target, len, cell)?;
    let head = bytes_at(bytes, cell_tree_off, NODE_LEN)?.to_vec();
    bytes_at_mut(bytes, target_off, NODE_LEN)?.copy_from_slice(&head);
    let cleared = read_u32_le(bytes, target_off + 0x18)? & !0x0020_0000;
    put_u32_at(bytes, target_off + 0x18, cleared)?;
    Ok(true)
}

fn bytes_at_mut(bytes: &mut [u8], offset: usize, len: usize) -> Result<&mut [u8], Error> {
    let file_len = bytes.len();
    offset.checked_add(len).and_then(move |end| bytes.get_mut(offset..end)).ok_or(Error::Truncated {
        offset,
        needed: len,
        len: file_len,
    })
}

fn put_u32_at(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), Error> {
    bytes_at_mut(bytes, offset, 4)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
