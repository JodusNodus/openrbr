//! Reader for `.dls` AnimData files.
//!
//! `E12_AnimSet::Load020` (`FUN_00479020`) checks the two-word
//! `MINAATAD` signature, reads version/minor words, then follows ten
//! file-relative table offsets. The tables below keep unknown records as
//! their original u32 words while decoding the widths and nesting the loader
//! actually reads. In particular, table 6 is an array of Vec4 sphere records;
//! `E12_Spheres::Add` (`FUN_004772a0`) copies xyz and squares w as its radius.
//!
//! The offsets deliberately remain present alongside decoded tables: many
//! animation arrays are referenced from records, and some of their field
//! meanings are still unknown.

use std::fmt::{self, Display, Formatter};

/// File signature, ASCII `MINAATAD`.
pub const MAGIC: &[u8; 8] = b"MINAATAD";
/// Version expected by the retail 1.02 loader.
pub const RETAIL_VERSION: u32 = 18;
/// Header length: signature, two version words, and ten offsets.
pub const HEADER_LEN: usize = 56;

/// One fixed-width raw array referenced by an animation record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlsRawArray {
    pub count: u32,
    pub offset: u32,
    pub record_words: u8,
    /// Exact little-endian source words, kept without guessed meanings.
    pub words: Vec<u32>,
}

/// One nested group in an animation record's variable table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlsNestedGroup {
    pub header: [u32; 3],
    pub pairs: Vec<[u32; 2]>,
}

/// The 7-word stream header and arrays consumed by `CReplayManager::ReadStream`
/// (`0x498840`). Non-version-6 streams are left at the header, matching the
/// game's early return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlsReplayStream {
    pub offset: u32,
    pub header: [u32; 7],
    pub ten_word_records: Vec<[u32; 10]>,
    pub six_word_records: Vec<[u32; 6]>,
    pub pairs: Vec<[u32; 2]>,
}

/// One entry in the first (animation) table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlsAnimationEntry {
    pub pair: [u32; 2],
    /// The loader follows the first word of the table pair.
    pub record_offset: u32,
    /// The loader copies 17 words from this referenced record. Word 0 is
    /// retained as an unresolved reference into table 7; words 1..7 are
    /// counts, words 9..15 are corresponding offsets, and word 16 is the
    /// replay-stream offset. All remain raw here as well as in decoded views.
    pub fields: [u32; 17],
    pub arrays: Vec<DlsRawArray>,
    /// The loader walks a variable-width field at words 4/12. These records
    /// follow a three-word header whose third word is a pair count.
    pub nested_groups: Vec<DlsNestedGroup>,
    pub replay: DlsReplayStream,
}

/// One entry from table 1: three words followed by `child_count` 11-word
/// records, as read by `FUN_00479020`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlsTrackGroup {
    pub header: [u32; 3],
    pub children: Vec<[u32; 11]>,
}

/// One sphere used by the animation/spline trigger system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DlsSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

/// One table-2 trigger record (14 words), as `E12_AnimSet::Load020`
/// (`FUN_00479020`) reads it. Words 2/3 are matched against the table-1
/// spline group/id pairs (no match resolves to index 0; retail `(0, 0)`
/// rows are that default). Words 4..6 are stored as position floats;
/// words 7..9 are multiplied by pi/180 (degrees to radians). Word 11 is
/// only tested against zero (nonzero sets flag bit 1 of the record);
/// words 10, 12, 13 are stored raw. The records are qsorted after load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DlsTrigger {
    pub ids: [u32; 2],
    pub spline_group: u32,
    pub spline_id: u32,
    pub position: [f32; 3],
    /// Rotation in degrees on disk; the loader stores radians.
    pub rotation_deg: [f32; 3],
    /// Word 10, stored raw.
    pub word_10: u32,
    /// Word 11 as float bits. The loader only tests it against zero;
    /// retail holds floats (1.0 in 4,522 of 4,531 rows).
    pub word_11: f32,
    /// Words 12/13, stored raw.
    pub words_12_13: [u32; 2],
}

/// One table-3 camera record (6 words), as `FUN_00479020` reads it. Word 0
/// is resolved through the id lookup (`BSearch50`); word 1 goes through
/// the same lookup except the sentinel values `-16`/`-15`/`-14`/`-1`
/// (`0xfffffff0`, `0xfffffff1`, `0xfffffff2`, `0xffffffff`), which are kept
/// raw (fixed, car, chase and driver looks). Words 2/3 are stored raw;
/// word 4 bit 0 sets flag bit 0 and bit 1 sets flag bit 2; word 5 is
/// stored raw. The records are qsorted after load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DlsCamera {
    pub id: u32,
    pub look_at: u32,
    pub words_2_3: [u32; 2],
    pub mode: u32,
    pub extra: u32,
}

/// One table-5 record (7 words), as `FUN_00479020` reads it. Word 0 is
/// resolved through the id lookup (`BSearch50`; a miss fails the load).
/// Word 1 is a byte offset into the table-7 string block (the name).
/// Words 2/3 are stored raw; words 4..6 are stored as floats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DlsTable5Record {
    pub trigger: u32,
    pub name_offset: u32,
    pub aux: [u32; 2],
    pub values: [f32; 3],
}

/// One table-9 record (2 words): the named triggers. The loader stores the
/// pair swapped (word 1 at record `+0`, word 0 at `+4`); `FindByName`
/// (`0x4765c0`) compares the table-7 string at `+0` and returns `+4`, so
/// word 0 is the trigger's object id and word 1 the name's byte offset into
/// table 7 (`markers::named_triggers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DlsTable9Record {
    pub words: [u32; 2],
}

/// Parsed `.dls` AnimData file.
#[derive(Debug, Clone, PartialEq)]
pub struct DlsFile<'a> {
    pub version: u32,
    pub minor_version: u32,
    pub table_offsets: [u32; 10],
    pub animation_entries: Vec<DlsAnimationEntry>,
    pub track_groups: Vec<DlsTrackGroup>,
    /// Trigger records (table 2), decoded as the loader reads them.
    pub table_2: Vec<DlsTrigger>,
    /// Camera records (table 3), decoded as the loader reads them.
    pub table_3: Vec<DlsCamera>,
    /// Three-word emitter definitions passed to `FUN_005e96f0`.
    pub emitters: Vec<[u32; 3]>,
    /// Table-5 records, decoded as the loader reads them.
    pub table_5: Vec<DlsTable5Record>,
    /// Four floats per record; the loader copies xyz and squares w.
    pub spheres: Vec<DlsSphere>,
    /// Byte table copied by the loader for referenced animation names/data.
    pub string_bytes: Vec<u8>,
    /// Optional four-word world/default vector (`0xffffffff` means absent).
    pub world_vector: Option<[u32; 4]>,
    /// Table-9 word pairs in file order (the loader stores them swapped).
    pub table_9: Vec<DlsTable9Record>,
    /// Entire original file, retaining unclaimed gaps and opaque fields.
    pub raw: &'a [u8],
}

/// Parse failure with the offending byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated { offset: usize, needed: usize, len: usize },
    BadMagic,
    BadOffset { table: usize, value: u32, len: usize },
    Allocation { offset: usize, count: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated at offset {offset}: need {needed} bytes but file is {len} bytes")
            }
            Error::BadMagic => write!(f, "bad AnimData magic; expected MINAATAD"),
            Error::BadOffset { table, value, len } => {
                write!(f, "table {table} offset {value:#x} is outside the {len}-byte file")
            }
            Error::Allocation { offset, count } => {
                write!(f, "could not allocate {count} records at offset {offset}")
            }
        }
    }
}

impl std::error::Error for Error {}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn at(bytes: &'a [u8], offset: usize) -> Result<Self, Error> {
        if offset > bytes.len() {
            return Err(Error::Truncated { offset, needed: 0, len: bytes.len() });
        }
        Ok(Self { bytes, offset })
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(count).ok_or(Error::Truncated {
            offset: self.offset,
            needed: count,
            len: self.bytes.len(),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Truncated { offset: self.offset, needed: count, len: self.bytes.len() });
        }
        let out = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let mut b = [0u8; 4];
        b.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(b))
    }

    fn words<const N: usize>(&mut self) -> Result<[u32; N], Error> {
        let mut words = [0u32; N];
        for word in &mut words {
            *word = self.u32()?;
        }
        Ok(words)
    }
}

fn checked_range(bytes: &[u8], offset: usize, size: usize) -> Result<(), Error> {
    let end = offset.checked_add(size).ok_or(Error::Truncated { offset, needed: size, len: bytes.len() })?;
    if end > bytes.len() {
        return Err(Error::Truncated { offset, needed: size, len: bytes.len() });
    }
    Ok(())
}

fn checked_product(count: usize, stride: usize, offset: usize, len: usize) -> Result<usize, Error> {
    count.checked_mul(stride).ok_or(Error::Truncated { offset, needed: usize::MAX, len })
}

fn table_offset(offsets: &[u32; 10], index: usize, bytes: &[u8]) -> Result<usize, Error> {
    let value = offsets[index];
    if value == u32::MAX || value as usize >= bytes.len() {
        return Err(Error::BadOffset { table: index, value, len: bytes.len() });
    }
    Ok(value as usize)
}

fn parse_rows<const N: usize>(bytes: &[u8], offset: usize, count: usize) -> Result<Vec<[u32; N]>, Error> {
    let stride = N.checked_mul(4).ok_or(Error::Truncated { offset, needed: usize::MAX, len: bytes.len() })?;
    let total = checked_product(count, stride, offset, bytes.len())?;
    checked_range(bytes, offset, total)?;
    let mut cursor = Cursor::at(bytes, offset)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| Error::Allocation { offset, count })?;
    for _ in 0..count {
        rows.push(cursor.words::<N>()?);
    }
    Ok(rows)
}

fn parse_table<const N: usize>(bytes: &[u8], offset: usize) -> Result<Vec<[u32; N]>, Error> {
    let mut cursor = Cursor::at(bytes, offset)?;
    let count = cursor.u32()? as usize;
    parse_rows(bytes, cursor.offset, count)
}

fn parse_raw_array(bytes: &[u8], count: u32, offset: u32, record_words: u8) -> Result<DlsRawArray, Error> {
    if count == 0 {
        return Ok(DlsRawArray { count, offset, record_words, words: Vec::new() });
    }
    let count_usize = count as usize;
    let stride = record_words as usize * 4;
    let offset_usize = offset as usize;
    let total = checked_product(count_usize, stride, offset_usize, bytes.len())?;
    checked_range(bytes, offset_usize, total)?;
    let word_count = count_usize.checked_mul(record_words as usize).ok_or(Error::Truncated {
        offset: offset_usize,
        needed: usize::MAX,
        len: bytes.len(),
    })?;
    let mut cursor = Cursor::at(bytes, offset_usize)?;
    let mut words = Vec::new();
    words.try_reserve_exact(word_count).map_err(|_| Error::Allocation { offset: offset_usize, count: word_count })?;
    for _ in 0..word_count {
        words.push(cursor.u32()?);
    }
    Ok(DlsRawArray { count, offset, record_words, words })
}

fn parse_nested_groups(bytes: &[u8], count: u32, offset: u32) -> Result<Vec<DlsNestedGroup>, Error> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let minimum_bytes = checked_product(count as usize, 12, offset as usize, bytes.len())?;
    checked_range(bytes, offset as usize, minimum_bytes)?;
    let mut cursor = Cursor::at(bytes, offset as usize)?;
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(count as usize)
        .map_err(|_| Error::Allocation { offset: offset as usize, count: count as usize })?;
    for _ in 0..count {
        let header = cursor.words::<3>()?;
        let pair_count = header[2] as usize;
        let bytes_needed = checked_product(pair_count, 8, cursor.offset, bytes.len())?;
        checked_range(bytes, cursor.offset, bytes_needed)?;
        let mut pairs = Vec::new();
        pairs
            .try_reserve_exact(pair_count)
            .map_err(|_| Error::Allocation { offset: cursor.offset, count: pair_count })?;
        for _ in 0..pair_count {
            pairs.push(cursor.words::<2>()?);
        }
        groups.push(DlsNestedGroup { header, pairs });
    }
    Ok(groups)
}

fn parse_replay_stream(bytes: &[u8], offset: u32) -> Result<DlsReplayStream, Error> {
    let mut cursor = Cursor::at(bytes, offset as usize)?;
    let header = cursor.words::<7>()?;
    let mut ten_word_records = Vec::new();
    let mut six_word_records = Vec::new();
    let mut pairs = Vec::new();
    if header[0] == 6 {
        let n10 = header[1] as usize;
        let n6 = header[2] as usize;
        let n2 = header[3] as usize;
        let bytes10 = checked_product(n10, 40, cursor.offset, bytes.len())?;
        checked_range(bytes, cursor.offset, bytes10)?;
        ten_word_records = parse_rows::<10>(bytes, cursor.offset, n10)?;
        cursor.offset += bytes10;

        let bytes6 = checked_product(n6, 24, cursor.offset, bytes.len())?;
        checked_range(bytes, cursor.offset, bytes6)?;
        six_word_records = parse_rows::<6>(bytes, cursor.offset, n6)?;
        cursor.offset += bytes6;

        let bytes2 = checked_product(n2, 8, cursor.offset, bytes.len())?;
        checked_range(bytes, cursor.offset, bytes2)?;
        pairs = parse_rows::<2>(bytes, cursor.offset, n2)?;
    }
    Ok(DlsReplayStream { offset, header, ten_word_records, six_word_records, pairs })
}

fn parse_animation_entries(bytes: &[u8], offset: usize) -> Result<Vec<DlsAnimationEntry>, Error> {
    let mut cursor = Cursor::at(bytes, offset)?;
    let count = cursor.u32()? as usize;
    let pair_bytes = checked_product(count, 8, cursor.offset, bytes.len())?;
    checked_range(bytes, cursor.offset, pair_bytes)?;
    let mut pairs = Vec::new();
    pairs.try_reserve_exact(count).map_err(|_| Error::Allocation { offset: cursor.offset, count })?;
    for _ in 0..count {
        pairs.push(cursor.words::<2>()?);
    }

    let mut entries = Vec::new();
    entries.try_reserve_exact(count).map_err(|_| Error::Allocation { offset, count })?;
    for pair in pairs {
        let record_offset = pair[0];
        let mut record = Cursor::at(bytes, record_offset as usize)?;
        let fields = record.words::<17>()?;

        // FUN_00479020 uses words 1..7 as counts and words 9..15 as
        // file-relative offsets in each 17-word record. Field meanings are
        // still unknown, but sizes and strides are visible in the loops.
        let mut arrays = Vec::new();
        for (count_index, offset_index, words) in
            [(1usize, 9usize, 2u8), (2, 10, 3), (3, 11, 3), (5, 13, 4), (6, 14, 3), (7, 15, 5)]
        {
            arrays.push(parse_raw_array(bytes, fields[count_index], fields[offset_index], words)?);
        }
        let nested_groups = parse_nested_groups(bytes, fields[4], fields[12])?;
        entries.push(DlsAnimationEntry {
            pair,
            record_offset,
            fields,
            arrays,
            nested_groups,
            replay: parse_replay_stream(bytes, fields[16])?,
        });
    }
    Ok(entries)
}

/// Parses a whole `.dls` file. Unknown record fields and file gaps are kept.
pub fn parse(bytes: &[u8]) -> Result<DlsFile<'_>, Error> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::Truncated { offset: 0, needed: HEADER_LEN, len: bytes.len() });
    }
    if &bytes[..8] != MAGIC {
        return Err(Error::BadMagic);
    }
    let mut header = Cursor::at(bytes, 8)?;
    let version = header.u32()?;
    let minor_version = header.u32()?;
    let table_offsets = header.words::<10>()?;

    let animation_entries = parse_animation_entries(bytes, table_offset(&table_offsets, 0, bytes)?)?;

    let mut track_cursor = Cursor::at(bytes, table_offset(&table_offsets, 1, bytes)?)?;
    let track_count = track_cursor.u32()? as usize;
    let minimum_track_bytes = checked_product(track_count, 12, track_cursor.offset, bytes.len())?;
    checked_range(bytes, track_cursor.offset, minimum_track_bytes)?;
    let mut track_groups = Vec::new();
    track_groups
        .try_reserve_exact(track_count)
        .map_err(|_| Error::Allocation { offset: track_cursor.offset, count: track_count })?;
    for _ in 0..track_count {
        let group_header = track_cursor.words::<3>()?;
        let child_count = group_header[2] as usize;
        let needed = checked_product(child_count, 44, track_cursor.offset, bytes.len())?;
        checked_range(bytes, track_cursor.offset, needed)?;
        let mut children = Vec::new();
        children
            .try_reserve_exact(child_count)
            .map_err(|_| Error::Allocation { offset: track_cursor.offset, count: child_count })?;
        for _ in 0..child_count {
            children.push(track_cursor.words::<11>()?);
        }
        track_groups.push(DlsTrackGroup { header: group_header, children });
    }

    let table_2 = parse_table::<14>(bytes, table_offset(&table_offsets, 2, bytes)?)?
        .into_iter()
        .map(|w| DlsTrigger {
            ids: [w[0], w[1]],
            spline_group: w[2],
            spline_id: w[3],
            position: [f32::from_bits(w[4]), f32::from_bits(w[5]), f32::from_bits(w[6])],
            rotation_deg: [f32::from_bits(w[7]), f32::from_bits(w[8]), f32::from_bits(w[9])],
            word_10: w[10],
            word_11: f32::from_bits(w[11]),
            words_12_13: [w[12], w[13]],
        })
        .collect();
    let table_3 = parse_table::<6>(bytes, table_offset(&table_offsets, 3, bytes)?)?
        .into_iter()
        .map(|w| DlsCamera { id: w[0], look_at: w[1], words_2_3: [w[2], w[3]], mode: w[4], extra: w[5] })
        .collect();
    let emitters = parse_table::<3>(bytes, table_offset(&table_offsets, 4, bytes)?)?;
    let table_5 = parse_table::<7>(bytes, table_offset(&table_offsets, 5, bytes)?)?
        .into_iter()
        .map(|w| DlsTable5Record {
            trigger: w[0],
            name_offset: w[1],
            aux: [w[2], w[3]],
            values: [f32::from_bits(w[4]), f32::from_bits(w[5]), f32::from_bits(w[6])],
        })
        .collect();

    let mut sphere_cursor = Cursor::at(bytes, table_offset(&table_offsets, 6, bytes)?)?;
    let sphere_count = sphere_cursor.u32()? as usize;
    let sphere_bytes = checked_product(sphere_count, 16, sphere_cursor.offset, bytes.len())?;
    checked_range(bytes, sphere_cursor.offset, sphere_bytes)?;
    let mut spheres = Vec::new();
    spheres
        .try_reserve_exact(sphere_count)
        .map_err(|_| Error::Allocation { offset: sphere_cursor.offset, count: sphere_count })?;
    for _ in 0..sphere_count {
        let words = sphere_cursor.words::<4>()?;
        spheres.push(DlsSphere {
            center: [f32::from_bits(words[0]), f32::from_bits(words[1]), f32::from_bits(words[2])],
            radius: f32::from_bits(words[3]),
        });
    }

    let mut string_cursor = Cursor::at(bytes, table_offset(&table_offsets, 7, bytes)?)?;
    let string_len = string_cursor.u32()? as usize;
    let string_bytes = string_cursor.take(string_len)?.to_vec();

    let world_vector = if table_offsets[8] == u32::MAX {
        None
    } else {
        let offset = table_offset(&table_offsets, 8, bytes)?;
        let mut cursor = Cursor::at(bytes, offset)?;
        Some(cursor.words::<4>()?)
    };

    let table_9 = parse_table::<2>(bytes, table_offset(&table_offsets, 9, bytes)?)?
        .into_iter()
        .map(|words| DlsTable9Record { words })
        .collect();

    Ok(DlsFile {
        version,
        minor_version,
        table_offsets,
        animation_entries,
        track_groups,
        table_2,
        table_3,
        emitters,
        table_5,
        spheres,
        string_bytes,
        world_vector,
        table_9,
        raw: bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn append_words(bytes: &mut Vec<u8>, words: &[u32]) -> u32 {
        let offset = bytes.len() as u32;
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        offset
    }

    fn animation_file() -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[..8].copy_from_slice(MAGIC);
        patch_u32(&mut bytes, 8, RETAIL_VERSION);

        let table0 = bytes.len();
        patch_u32(&mut bytes, 16, table0 as u32);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let record_ref = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // companion word

        let table1 = bytes.len();
        patch_u32(&mut bytes, 20, table1 as u32);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[1u32, 2, 1].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>());
        for word in 0..11u32 {
            bytes.extend_from_slice(&(10 + word).to_le_bytes());
        }

        for at in [24, 28, 32, 36, 40] {
            let offset = bytes.len() as u32;
            patch_u32(&mut bytes, at, offset);
            bytes.extend_from_slice(&0u32.to_le_bytes());
        }
        let table7 = bytes.len();
        patch_u32(&mut bytes, 44, table7 as u32);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        patch_u32(&mut bytes, 48, u32::MAX);
        let table9 = bytes.len();
        patch_u32(&mut bytes, 52, table9 as u32);
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let record_offset = bytes.len() as u32;
        patch_u32(&mut bytes, record_ref, record_offset);
        bytes.extend_from_slice(&[0u8; 17 * 4]);

        let array0 = append_words(&mut bytes, &[101, 102]);
        let array1 = append_words(&mut bytes, &[201, 202, 203]);
        let array2 = append_words(&mut bytes, &[301, 302, 303]);
        let nested = append_words(&mut bytes, &[401, 402, 1, 403, 404]);
        let array4 = append_words(&mut bytes, &[501, 502, 503, 504]);
        let array5 = append_words(&mut bytes, &[601, 602, 603]);
        let array6 = append_words(&mut bytes, &[701, 702, 703, 704, 705]);
        let replay = append_words(&mut bytes, &[6, 1, 1, 1, 0, 0, 0]);
        append_words(&mut bytes, &(801..=810).collect::<Vec<_>>());
        append_words(&mut bytes, &(901..=906).collect::<Vec<_>>());
        append_words(&mut bytes, &[1001, 1002]);

        let fields = [0, 1, 1, 1, 1, 1, 1, 1, 0, array0, array1, array2, nested, array4, array5, array6, replay];
        for (index, value) in fields.into_iter().enumerate() {
            patch_u32(&mut bytes, record_offset as usize + index * 4, value);
        }
        bytes
    }

    fn empty_file() -> Vec<u8> {
        let offsets = [56u32, 60, 64, 68, 72, 76, 80, 84, u32::MAX, 88];
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&RETAIL_VERSION.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        for offset in offsets {
            b.extend_from_slice(&offset.to_le_bytes());
        }
        for _ in 0..8 {
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // table 9 count
        b
    }

    #[test]
    fn parses_empty_tables_and_optional_world_vector() {
        let bytes = empty_file();
        let file = parse(&bytes).unwrap();
        assert_eq!(file.version, RETAIL_VERSION);
        assert_eq!(file.minor_version, 0);
        assert!(file.animation_entries.is_empty());
        assert!(file.track_groups.is_empty());
        assert!(file.table_2.is_empty());
        assert!(file.emitters.is_empty());
        assert!(file.spheres.is_empty());
        assert!(file.string_bytes.is_empty());
        assert_eq!(file.world_vector, None);
        assert!(file.table_9.is_empty());
    }

    #[test]
    fn parses_sphere_table() {
        let mut bytes = empty_file();
        let offset = bytes.len() as u32;
        bytes[40..44].copy_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let file = parse(&bytes).unwrap();
        assert_eq!(file.spheres.len(), 1);
        assert_eq!(file.spheres[0].center, [1.0, 2.0, 3.0]);
        assert_eq!(file.spheres[0].radius, 4.0);
    }

    #[test]
    fn parses_animation_arrays_nested_groups_and_replay_stream() {
        let bytes = animation_file();
        let file = parse(&bytes).unwrap();
        assert_eq!(file.animation_entries.len(), 1);
        let entry = &file.animation_entries[0];
        assert_eq!(entry.record_offset, 156);
        assert_eq!(entry.arrays.len(), 6);
        assert_eq!(entry.arrays[0].words, [101, 102]);
        assert_eq!(entry.arrays[1].words, [201, 202, 203]);
        assert_eq!(entry.nested_groups.len(), 1);
        assert_eq!(entry.nested_groups[0].header, [401, 402, 1]);
        assert_eq!(entry.nested_groups[0].pairs, [[403, 404]]);
        assert_eq!(entry.replay.header[0], 6);
        assert_eq!(entry.replay.ten_word_records[0], [801, 802, 803, 804, 805, 806, 807, 808, 809, 810]);
        assert_eq!(entry.replay.six_word_records[0], [901, 902, 903, 904, 905, 906]);
        assert_eq!(entry.replay.pairs, [[1001, 1002]]);
        assert_eq!(file.track_groups[0].children[0], [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]);
    }

    #[test]
    fn bad_magic() {
        let mut bytes = empty_file();
        bytes[0] = b'X';
        assert!(matches!(parse(&bytes), Err(Error::BadMagic)));
    }

    #[test]
    fn truncated_header() {
        assert!(matches!(parse(&[]), Err(Error::Truncated { .. })));
        assert!(matches!(parse(&[0; 55]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn invalid_table_offset() {
        let mut bytes = empty_file();
        bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse(&bytes), Err(Error::BadOffset { table: 0, .. })));
    }

    #[test]
    fn hostile_table_count_is_bounded() {
        let mut bytes = empty_file();
        bytes[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse(&bytes), Err(Error::Truncated { .. })));
    }
}
