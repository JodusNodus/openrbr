//! Finding what the decoders read in another `NGP.dll`: the same tables and code, moved. A DLL built from the same
//! sources with other code added shifts every later address, so the decoders' fixed 7.4.778 addresses
//! ([`super::reads`]) are looked up by content in the candidate: data by its bytes, code by its first and last
//! instructions, with the fields that hold addresses (pointers, call and jump displacements) not compared. The
//! result, a [`Rebase`], is what the decoders read their addresses through.
//!
//! The same masking turns whole sections into streams that equal where the code and data do ([`diff_section`]), which
//! tells what changed between two builds even when everything moved.

use std::collections::HashSet;

use iced_x86::{Decoder, DecoderOptions, Instruction, OpKind};

use super::{reads, Read, ReadKind};
use crate::pe;

/// The shortest start and end anchor of code, in bytes; longer when that is not unique.
const ANCHOR: usize = 48;
/// The context added to data that is found in more than one place, in bytes either side, tried in turn.
const CONTEXT: [usize; 4] = [16, 64, 256, 1024];
/// The block compared by [`diff_section`], in bytes.
const BLOCK: usize = 32;

/// Where the decoders' addresses are in a DLL: spans of the supported DLL's addresses and where they start here. The
/// empty one is the supported DLL itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rebase {
    /// Address in 7.4.778, length, address here.
    spans: Vec<(u32, u32, u32)>,
}

impl Rebase {
    /// The address here of the supported DLL's `address`, or an error when it is not located.
    pub fn at(&self, address: u32) -> Result<u32, String> {
        if self.spans.is_empty() {
            return Ok(address);
        }
        // The shortest span that holds it: a code range's end is a span of its own.
        self.spans
            .iter()
            .filter(|(start, len, _)| *start <= address && address - *start <= *len)
            .min_by_key(|(_, len, _)| *len)
            .map(|(start, _, here)| here + (address - start))
            .ok_or_else(|| format!("NGP address {address:#x} is not located in the DLL"))
    }

    /// The spans: address in 7.4.778, length, address here.
    pub fn spans(&self) -> &[(u32, u32, u32)] {
        &self.spans
    }
}

/// The image's pointer range: addresses from its base to the end of its last section.
fn pointer_range(image: &pe::Image) -> (u32, u32) {
    let end = image.sections.iter().map(|s| s.address + s.bytes.len() as u32).max().unwrap_or(image.base);
    (image.base, end)
}

/// Which bytes of `bytes` (at address `ip`) are not addresses: x86 code, whose absolute addresses (displacements and
/// immediates in the pointer range) and branch displacements are not.
fn fixed_code(bytes: &[u8], ip: u32, (lo, hi): (u32, u32)) -> Vec<bool> {
    let mut fixed = vec![true; bytes.len()];
    let in_range = |v: u32| (lo..hi).contains(&v);
    let mut decoder = Decoder::with_ip(32, bytes, u64::from(ip), DecoderOptions::NONE);
    let mut i = Instruction::default();
    while decoder.can_decode() {
        let at = decoder.position();
        decoder.decode_out(&mut i);
        if i.is_invalid() {
            continue;
        }
        let offsets = decoder.get_constant_offsets(&i);
        let mut clear = |offset: usize, size: usize| {
            (at + offset..(at + offset + size).min(bytes.len())).for_each(|k| fixed[k] = false)
        };
        if offsets.has_displacement() && in_range(i.memory_displacement32()) {
            clear(offsets.displacement_offset(), offsets.displacement_size());
        }
        if offsets.has_immediate() {
            let branch = matches!(i.op0_kind(), OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64);
            if branch
                || (i.op0_kind() == OpKind::Immediate32 && in_range(i.immediate32()))
                || (i.op_count() > 1 && i.op1_kind() == OpKind::Immediate32 && in_range(i.immediate32()))
            {
                clear(offsets.immediate_offset(), offsets.immediate_size());
            }
        }
    }
    fixed
}

/// Which bytes of `bytes` (at address `ip`, 4-aligned) are not addresses: data, whose aligned words in the pointer
/// range are.
fn fixed_data(bytes: &[u8], ip: u32, (lo, hi): (u32, u32)) -> Vec<bool> {
    let mut fixed = vec![true; bytes.len()];
    let skip = (4 - ip as usize % 4) % 4;
    for at in (skip..bytes.len().saturating_sub(3)).step_by(4) {
        let word = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if (lo..hi).contains(&word) {
            fixed[at..at + 4].fill(false);
        }
    }
    fixed
}

/// The positions in `haystack` where `pattern`'s fixed bytes (`fixed`) are; `align` is the step.
fn find(haystack: &[u8], pattern: &[u8], fixed: &[bool], align: usize) -> Vec<usize> {
    let Some(first) = fixed.iter().position(|&f| f) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for at in (0..haystack.len().saturating_sub(pattern.len() - 1)).step_by(align) {
        if haystack[at + first] == pattern[first]
            && pattern.iter().zip(fixed).zip(&haystack[at..]).all(|((p, &f), h)| !f || p == h)
        {
            found.push(at);
        }
    }
    found
}

fn section_of(image: &pe::Image, address: u32) -> Option<&pe::Section> {
    image.sections.iter().find(|s| s.address <= address && address < s.address + s.bytes.len() as u32)
}

/// Where the data `[start, end)` of `reference` is in `candidate`'s section of the same name: its bytes, with the
/// pointers not compared, and with context from around it when it is in several places.
fn locate_data(reference: &pe::Image, candidate: &pe::Image, read: &Read) -> Result<u32, String> {
    let section = section_of(reference, read.start).ok_or_else(|| format!("{}: outside the reference", read.what))?;
    let there = candidate
        .sections
        .iter()
        .find(|s| s.name == section.name)
        .ok_or_else(|| format!("{}: the candidate has no {}", read.what, section.name))?;
    let range = pointer_range(reference);
    let (from, to) = ((read.start - section.address) as usize, (read.end - section.address) as usize);
    let mut candidates = vec![0usize];
    candidates.extend(CONTEXT);
    for context in candidates {
        let (a, b) = (from.saturating_sub(context), (to + context).min(section.bytes.len()));
        let bytes = &section.bytes[a..b];
        let fixed = fixed_data(bytes, section.address + a as u32, range);
        let found = find(&there.bytes, bytes, &fixed, 4);
        match found[..] {
            [at] => return Ok(there.address + (at + (from - a)) as u32),
            [] => return Err(format!("{} ({:#x}): not found in the candidate", read.what, read.start)),
            _ => {}
        }
    }
    Err(format!("{} ({:#x}): found in several places in the candidate", read.what, read.start))
}

/// The anchor of code at `[start, end)` of the reference: its first instructions (`from_end` false) or its last, at
/// least `len` bytes, as bytes and which of them are fixed.
fn anchor(
    reference: &pe::Image,
    start: u32,
    end: u32,
    len: usize,
    from_end: bool,
) -> Result<(Vec<u8>, Vec<bool>), String> {
    let bytes = reference.read(start, (end - start) as usize).ok_or("a code range outside the reference")?;
    let fixed = fixed_code(bytes, start, pointer_range(reference));
    // Instruction starts of the range.
    let mut decoder = Decoder::with_ip(32, bytes, u64::from(start), DecoderOptions::NONE);
    let mut i = Instruction::default();
    let mut starts = Vec::new();
    while decoder.can_decode() {
        starts.push(decoder.position());
        decoder.decode_out(&mut i);
    }
    starts.push(bytes.len());
    let (a, b) = if from_end {
        let want = bytes.len().saturating_sub(len);
        (*starts.iter().rev().find(|&&s| s <= want).unwrap_or(&0), bytes.len())
    } else {
        (0, *starts.iter().find(|&&s| s >= len.min(bytes.len())).unwrap_or(&bytes.len()))
    };
    Ok((bytes[a..b].to_vec(), fixed[a..b].to_vec()))
}

/// Where the code `[start, end)` of the reference is in the candidate's `.text`: the places of its first and last
/// instructions, which must be one each; its start and its end there.
fn locate_code(reference: &pe::Image, candidate: &pe::Image, read: &Read) -> Result<(u32, u32), String> {
    let text = candidate.section(".text").ok_or("the candidate has no .text")?;
    let search = |from_end: bool| -> Result<u32, String> {
        let mut len = ANCHOR;
        loop {
            let (bytes, fixed) = anchor(reference, read.start, read.end, len, from_end)?;
            let found = find(&text.bytes, &bytes, &fixed, 1);
            match found[..] {
                [at] => return Ok(text.address + at as u32 + if from_end { bytes.len() as u32 } else { 0 }),
                [] => {
                    return Err(format!(
                        "{} ({:#x}): its {} not found in the candidate",
                        read.what,
                        read.start,
                        if from_end { "end" } else { "start" }
                    ))
                }
                _ if len < 400 => len *= 2,
                _ => {
                    return Err(format!(
                        "{} ({:#x}): its {} found in several places",
                        read.what,
                        read.start,
                        if from_end { "end" } else { "start" }
                    ))
                }
            }
        }
    };
    let start = search(false)?;
    if read.start == read.end {
        return Ok((start, start));
    }
    // An entry (a function) has only its start located: its end is a window, not an instruction.
    if read.kind == ReadKind::Entry {
        return Ok((start, start + (read.end - read.start)));
    }
    let end = search(true)?;
    if end <= start {
        return Err(format!("{} ({:#x}): its end is before its start in the candidate", read.what, read.start));
    }
    Ok((start, end))
}

/// Locates every range the decoders read ([`super::reads`]) in `candidate`.
pub fn locate(reference: &pe::Image, candidate: &pe::Image) -> Result<Rebase, String> {
    let mut spans = Vec::new();
    for read in reads() {
        match read.kind {
            ReadKind::Data => {
                let at = locate_data(reference, candidate, &read)?;
                spans.push((read.start, read.end - read.start, at));
            }
            ReadKind::Code | ReadKind::Entry => {
                let (start, end) = locate_code(reference, candidate, &read)?;
                // The range's two ends, as spans of their own.
                spans.push((read.start, 0, start));
                spans.push((read.end, 0, end));
            }
        }
    }
    Ok(Rebase { spans })
}

/// The code ranges the decoders read ([`ReadKind::Code`]) whose bytes differ in the candidate, addresses aside, or
/// whose length does: what they are, and where they are in the reference.
pub fn changed_code(reference: &pe::Image, candidate: &pe::Image, rebase: &Rebase) -> Vec<(&'static str, u32, u32)> {
    let masked = |image: &pe::Image, start: u32, end: u32| {
        let bytes = image.read(start, (end - start) as usize)?;
        let fixed = fixed_code(bytes, start, pointer_range(image));
        Some(bytes.iter().zip(fixed).map(|(&b, f)| if f { b } else { 0 }).collect::<Vec<u8>>())
    };
    reads()
        .into_iter()
        .filter(|r| r.kind == ReadKind::Code)
        .filter(|r| match (rebase.at(r.start), rebase.at(r.end)) {
            (Ok(a), Ok(b)) if b - a == r.end - r.start => masked(reference, r.start, r.end) != masked(candidate, a, b),
            _ => true,
        })
        .map(|r| (r.what, r.start, r.end))
        .collect()
}

/// How a section of the reference is found in the candidate's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionDiff {
    pub name: String,
    /// The reference's 32-byte blocks that are not all zero, and those found in the candidate.
    pub blocks: usize,
    pub matched: usize,
    /// The reference's addresses `[start, end)` of the blocks not found, merged.
    pub changed: Vec<(u32, u32)>,
}

/// The section as a stream that equals where the code or data does: the bytes that hold addresses zeroed.
fn normalized(section: &pe::Section, range: (u32, u32), code: bool) -> Vec<u8> {
    let fixed = if code {
        fixed_code(&section.bytes, section.address, range)
    } else {
        fixed_data(&section.bytes, section.address, range)
    };
    section.bytes.iter().zip(fixed).map(|(&b, f)| if f { b } else { 0 }).collect()
}

fn hash(block: &[u8]) -> u64 {
    block.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

/// Compares the reference's `name` section with the candidate's, blockwise and wherever the blocks are.
pub fn diff_section(reference: &pe::Image, candidate: &pe::Image, name: &str) -> Option<SectionDiff> {
    let (a, b) = (reference.section(name)?, candidate.section(name)?);
    let code = name == ".text";
    let (na, nb) = (normalized(a, pointer_range(reference), code), normalized(b, pointer_range(candidate), code));
    let step = if code { 1 } else { 4 };
    let theirs: HashSet<u64> = (0..nb.len().saturating_sub(BLOCK - 1))
        .step_by(step)
        .filter(|&at| nb[at..at + BLOCK].iter().any(|&x| x != 0))
        .map(|at| hash(&nb[at..at + BLOCK]))
        .collect();
    let (mut blocks, mut matched) = (0, 0);
    let mut changed: Vec<(u32, u32)> = Vec::new();
    for at in (0..na.len().saturating_sub(BLOCK - 1)).step_by(BLOCK) {
        let block = &na[at..at + BLOCK];
        if block.iter().all(|&x| x == 0) {
            continue;
        }
        blocks += 1;
        if theirs.contains(&hash(block)) {
            matched += 1;
            continue;
        }
        let (start, end) = (a.address + at as u32, a.address + (at + BLOCK) as u32);
        match changed.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => changed.push((start, end)),
        }
    }
    Some(SectionDiff { name: name.to_owned(), blocks, matched, changed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers_in_data_are_not_compared() {
        let range = (0x1000_0000, 0x1200_0000);
        let mut pattern = vec![1u8, 2, 3, 4, 0, 0, 0, 0, 9, 9, 9, 9];
        pattern[4..8].copy_from_slice(&0x1010_0000u32.to_le_bytes());
        let fixed = fixed_data(&pattern, 0x1000_1000, range);
        assert_eq!(fixed, [true, true, true, true, false, false, false, false, true, true, true, true]);
        // The same data with the pointer moved, behind other bytes.
        let mut other = vec![7u8; 8];
        other.extend_from_slice(&pattern);
        other[12..16].copy_from_slice(&0x1011_4000u32.to_le_bytes());
        other.extend_from_slice(&[5; 8]);
        assert_eq!(find(&other, &pattern, &fixed, 4), [8]);
    }

    #[test]
    fn code_addresses_are_not_compared() {
        let range = (0x1000_0000, 0x1200_0000);
        // mov eax, [0x10100000]; call +0x40; mov ecx, 5
        let code = [0xa1, 0x00, 0x00, 0x10, 0x10, 0xe8, 0x40, 0, 0, 0, 0xb9, 5, 0, 0, 0];
        let fixed = fixed_code(&code, 0x1000_1000, range);
        assert_eq!(fixed.iter().filter(|&&f| !f).count(), 8);
        assert!(fixed[10..].iter().all(|&f| f), "an immediate outside the pointer range is compared");
    }

    #[test]
    fn a_rebase_maps_spans_and_refuses_the_rest() {
        let rebase = Rebase { spans: vec![(0x100, 0x40, 0x500), (0x200, 0, 0x900)] };
        assert_eq!(rebase.at(0x110), Ok(0x510));
        assert_eq!(rebase.at(0x200), Ok(0x900));
        assert!(rebase.at(0x300).is_err());
        assert_eq!(Rebase::default().at(0x300), Ok(0x300));
    }
}
