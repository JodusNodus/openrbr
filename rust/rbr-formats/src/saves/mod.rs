//! The game's own save files in `SavedGames/` (`CPC_SaveFileManager`, 0x452760-0x453fd0): the profile
//! (`pf<name>.rbr`, [`profile`]), with the championship season inside it ([`season`]), the records table
//! (`pfresult.rst`, [`results`]) and the controller action map (`pf<name>.acm`, [`action_map`]). Layouts and
//! addresses: `notes/formats/saves.md`.
//!
//! The game writes every value as a little-endian 32-bit word, tags as four bytes (the multi-character constant's
//! bytes, so `'OPTI'` reads `ITPO` on disk), and fills a fixed-size buffer whose first word is a checksum of the rest.
//! Kind 1 files (the profile) are also encrypted ([`encrypt`]).

pub mod action_map;
pub mod after_season;
pub mod profile;
pub mod results;
pub mod season;
pub mod setup_slots;

use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

/// The file kinds of `CPC_SaveFileManager::Vf14` and `E20MakeName` (0x49e230). Setups (3) and ghosts (5) are other
/// tasks; replays (2) are not written here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `pf<name>.rbr`, encrypted.
    Profile,
    /// `pf<name>.rst`; the game uses it for one file, `pfresult.rst`.
    Results,
    /// `pf<name>.acm`, the controller action map ([`action_map`]).
    ActionMap,
}

impl Kind {
    /// `E20MakeName`'s name for `base`.
    pub fn file_name(self, base: &str) -> String {
        match self {
            Kind::Profile => format!("pf{base}.rbr"),
            Kind::Results => format!("pf{base}.rst"),
            Kind::ActionMap => format!("pf{base}.acm"),
        }
    }
}

/// The directory every kind here lives in, relative to the game's directory.
pub const DIR: &str = "SavedGames";

/// A game directory's `SavedGames`, with the game's file names. This is the launcher's way in: the selected profile's
/// settings are [`profile::ProfileFile::menu`] and [`profile::ProfileFile::controls`].
#[derive(Clone, Debug)]
pub struct SaveDir {
    pub path: PathBuf,
}

impl SaveDir {
    /// `<game>/SavedGames`.
    pub fn new(game: &Path) -> Self {
        Self { path: game.join(DIR) }
    }

    /// The profile names, sorted: every `pf<name>.rbr` (`CPC_SaveFileManager::Vf0c` lists `*.rbr`). An absent
    /// directory has none.
    pub fn profiles(&self) -> Result<Vec<String>, Error> {
        let entries = match fs::read_dir(&self.path) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if let Some(base) = name.strip_prefix("pf").and_then(|n| n.strip_suffix(".rbr")) {
                names.push(base.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn load_profile(&self, name: &str) -> Result<profile::ProfileFile, Error> {
        profile::ProfileFile::read(&fs::read(self.path.join(Kind::Profile.file_name(name)))?)
    }

    pub fn save_profile(&self, name: &str, file: &profile::ProfileFile) -> Result<(), Error> {
        self.store(&Kind::Profile.file_name(name), &file.write()?)
    }

    pub fn load_action_map(&self, name: &str) -> Result<action_map::ActionMap, Error> {
        action_map::ActionMap::read(&fs::read(self.path.join(Kind::ActionMap.file_name(name)))?)
    }

    pub fn load_results(&self) -> Result<results::Results, Error> {
        results::Results::read(&fs::read(self.path.join(Kind::Results.file_name("result")))?)
    }

    pub fn save_results(&self, results: &results::Results) -> Result<(), Error> {
        self.store(&Kind::Results.file_name("result"), &results.write()?)
    }

    /// Writes through a temporary file and a rename, so a failed write leaves the old file (ours; the game writes
    /// in place).
    fn store(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        fs::create_dir_all(&self.path)?;
        let tmp = self.path.join(format!("{name}.tmp"));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, self.path.join(name))?;
        Ok(())
    }
}

/// `CFileEncryptor`'s 500-byte key (0x7d12c8 in the 1.02 exe; the test `key_is_the_exes` checks it).
#[rustfmt::skip]
pub const KEY: [u8; 500] = [
    0xf0, 0xf0, 0x9d, 0x09, 0x0a, 0x66, 0xad, 0x6a, 0x85, 0x1d, 0xfd, 0x3f, 0x51, 0x23, 0xe7, 0xf3, 0xb1, 0x0e, 0x78, 0xec,
    0xd1, 0x50, 0x7b, 0x6b, 0x17, 0x3f, 0x61, 0xc5, 0x79, 0x0c, 0x57, 0x32, 0x1a, 0xf3, 0xb8, 0x6b, 0x68, 0xde, 0x2a, 0x5f,
    0x01, 0xba, 0x98, 0x3a, 0x99, 0xc0, 0x54, 0x02, 0x24, 0xf7, 0x9b, 0x09, 0x87, 0x23, 0xc4, 0x6f, 0x0e, 0x6c, 0x44, 0xfa,
    0xdb, 0xfb, 0xe8, 0x85, 0xab, 0xc2, 0x65, 0x3c, 0x0e, 0xc4, 0x93, 0xf6, 0x6d, 0x0b, 0x8a, 0xd6, 0x11, 0x8d, 0xe3, 0x8f,
    0x71, 0x52, 0x5d, 0x6e, 0xfc, 0xfd, 0x29, 0x82, 0xb0, 0x1d, 0x13, 0x11, 0xae, 0x5c, 0xd5, 0xa9, 0x1b, 0xf8, 0xce, 0xfc,
    0x79, 0x9c, 0x5a, 0xd6, 0xce, 0xfd, 0x0c, 0x64, 0xca, 0x60, 0x16, 0x12, 0x31, 0x5b, 0x08, 0x3a, 0xcf, 0x04, 0x3e, 0xea,
    0x23, 0xdc, 0x28, 0xfa, 0x20, 0xa5, 0xc0, 0xb8, 0x21, 0x73, 0x5e, 0x6c, 0x6a, 0x2b, 0x31, 0xe9, 0x6d, 0xbd, 0x9a, 0x73,
    0x11, 0x4c, 0xb1, 0x43, 0x3a, 0x8e, 0x28, 0xce, 0xdc, 0x9b, 0xd4, 0x31, 0xcf, 0x77, 0x1d, 0xe4, 0x9f, 0x8a, 0x8b, 0x0a,
    0xb2, 0x4e, 0xc0, 0x8d, 0xdd, 0x74, 0x0b, 0x56, 0xcf, 0xb7, 0xee, 0xd5, 0x74, 0xa7, 0xb5, 0x1b, 0xa1, 0xa9, 0x85, 0xcb,
    0x45, 0x68, 0xff, 0x1f, 0x59, 0xfb, 0xcd, 0x42, 0xda, 0xff, 0x59, 0x37, 0x05, 0xe7, 0xdc, 0x9e, 0x12, 0xbd, 0x1b, 0x87,
    0xbb, 0x97, 0x02, 0x9a, 0xc2, 0x04, 0x66, 0xd3, 0xbe, 0xa7, 0x2c, 0x11, 0x66, 0x4e, 0x10, 0xbd, 0xa8, 0xb3, 0x54, 0xc2,
    0xc0, 0x39, 0x8d, 0x17, 0x91, 0xda, 0xe0, 0x21, 0x86, 0x8a, 0xd3, 0x24, 0x37, 0x4a, 0x10, 0x13, 0x0a, 0x38, 0x45, 0xe2,
    0x26, 0xc6, 0x66, 0xc0, 0xde, 0x73, 0x9b, 0x53, 0xe2, 0x2d, 0x0a, 0x57, 0x7e, 0xac, 0xc9, 0xc4, 0x0c, 0x04, 0x33, 0xd5,
    0xfa, 0x9f, 0xe5, 0x15, 0x8a, 0xfd, 0x95, 0xcf, 0x9a, 0x57, 0x16, 0x02, 0xb2, 0x81, 0xbe, 0x39, 0x8c, 0x3a, 0x72, 0x6a,
    0x6f, 0x34, 0x8a, 0x2f, 0x84, 0x0e, 0xee, 0x96, 0x6d, 0x80, 0x83, 0xbc, 0x6a, 0x02, 0x45, 0x84, 0x3a, 0x1c, 0x49, 0xa0,
    0x01, 0xb7, 0xda, 0x2c, 0x76, 0x96, 0xff, 0x1d, 0x8e, 0x49, 0xa7, 0xca, 0xf5, 0xd6, 0xb0, 0xbd, 0x7f, 0x51, 0x21, 0x25,
    0xea, 0xac, 0xb7, 0x15, 0x16, 0xf6, 0x24, 0xd7, 0x0e, 0x54, 0x27, 0x96, 0x0d, 0xec, 0xd4, 0x96, 0xc9, 0x00, 0x33, 0x4d,
    0x43, 0x83, 0x8c, 0x7b, 0x59, 0x5e, 0x96, 0xaf, 0x5f, 0xac, 0xc3, 0x4a, 0xf9, 0x23, 0xfc, 0x62, 0x7b, 0xff, 0xf5, 0xb9,
    0x0c, 0x91, 0x6a, 0x01, 0xcd, 0xc9, 0x87, 0xbb, 0x43, 0xfc, 0xa4, 0xe7, 0x49, 0x0d, 0xb5, 0xc7, 0xc3, 0x5a, 0x95, 0xf7,
    0x52, 0x91, 0x78, 0x1d, 0x52, 0xc4, 0xbc, 0x63, 0x5a, 0xe4, 0x6a, 0x11, 0x7b, 0xff, 0x8d, 0x72, 0x8e, 0x64, 0xb5, 0x53,
    0xb8, 0x07, 0xdd, 0x4e, 0x7f, 0x4d, 0xf4, 0x35, 0x99, 0x96, 0x4a, 0xc6, 0xc6, 0xb7, 0x20, 0xf6, 0xeb, 0xa9, 0xa1, 0x18,
    0xaf, 0xa7, 0x77, 0x07, 0xe2, 0x0b, 0x49, 0xba, 0xe1, 0x12, 0x60, 0x55, 0x41, 0xdd, 0xa8, 0x21, 0x03, 0xe5, 0x5b, 0x8f,
    0x81, 0x1e, 0x8d, 0x8b, 0x6a, 0x11, 0xe0, 0x6f, 0xf9, 0x2f, 0x96, 0xc1, 0xba, 0x8e, 0x4d, 0x06, 0x06, 0x62, 0x9a, 0xe8,
    0x92, 0x66, 0xcc, 0xfb, 0x34, 0x7b, 0x11, 0x42, 0x34, 0xbc, 0x3d, 0xdc, 0x63, 0x3e, 0x7a, 0xf7, 0x2c, 0xd4, 0x19, 0x60,
    0xf5, 0xf3, 0xc5, 0xe1, 0xf9, 0x1d, 0x5f, 0xb4, 0xef, 0xef, 0xba, 0x4e, 0xb1, 0x35, 0x7b, 0xbd, 0x26, 0x1d, 0x61, 0xd0,
];

/// `CFileEncryptor::Encrypt` (0x55cc70): `b = !((b + 0xa0) ^ key[i % 500])`.
pub fn encrypt(buf: &mut [u8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b = !(b.wrapping_add(0xa0) ^ KEY[i % KEY.len()]);
    }
}

/// `CFileEncryptor::Decrypt` (0x55ccd0): `b = (!b ^ key[i % 500]) + 0x60`.
pub fn decrypt(buf: &mut [u8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b = (!*b ^ KEY[i % KEY.len()]).wrapping_add(0x60);
    }
}

/// `Crc32::Run` (0x490c00) over the game's table. The table builder (0x490c40) computes the CRC-32 entries and then
/// overwrites each with its index's bit reversal, so this is not the standard CRC-32.
pub fn checksum(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0; 256];
        let mut i = 0;
        while i < 256 {
            t[i] = (i as u32).reverse_bits();
            i += 1;
        }
        t
    };
    !data.iter().fold(!0u32, |crc, &b| crc >> 8 ^ TABLE[((b as u32 ^ crc) & 0xff) as usize])
}

/// Fills `out` to `size` with zeros and puts the checksum of everything after the first word in the first word,
/// as the writers end (`E20WriteProfile` 0x49ec70, `CMenuSaving::WriteSave`).
fn seal(mut out: Vec<u8>, size: usize) -> Result<Vec<u8>, Error> {
    if out.len() > size {
        return Err(Error::TooLarge { len: out.len(), size });
    }
    out.resize(size, 0);
    let crc = checksum(&out[4..]);
    out[..4].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// Checks the size and the first word's checksum, as the readers start.
fn unseal(bytes: &[u8], size: usize) -> Result<Reader<'_>, Error> {
    if bytes.len() != size {
        return Err(Error::Size { len: bytes.len(), size });
    }
    let mut r = Reader { bytes, at: 0 };
    let stored = r.u32()?;
    let crc = checksum(&bytes[4..]);
    if stored != crc {
        return Err(Error::Checksum { stored, computed: crc });
    }
    Ok(r)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Size { len: usize, size: usize },
    Checksum { stored: u32, computed: u32 },
    Version { found: u32, want: u32 },
    Tag { at: usize, found: [u8; 4], want: [u8; 4] },
    Eof { at: usize },
    TooLarge { len: usize, size: usize },
    Io { kind: io::ErrorKind, message: String },
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io { kind: e.kind(), message: e.to_string() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Size { len, size } => write!(f, "file is {len} bytes, expected {size}"),
            Error::Checksum { stored, computed } => write!(f, "checksum {stored:#010x}, computed {computed:#010x}"),
            Error::Version { found, want } => write!(f, "version {found}, expected {want}"),
            Error::Tag { at, found, want } => write!(
                f,
                "tag {:?} at {at:#x}, expected {:?}",
                String::from_utf8_lossy(found),
                String::from_utf8_lossy(want)
            ),
            Error::Eof { at } => write!(f, "data ends at {at:#x}"),
            Error::TooLarge { len, size } => write!(f, "{len} bytes do not fit the {size}-byte file"),
            Error::Io { message, .. } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

/// A cursor over the game's stream of words.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(crate) fn at(&self) -> usize {
        self.at
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.bytes.get(self.at..self.at + n).ok_or(Error::Eof { at: self.at })?;
        self.at += n;
        Ok(s)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub(crate) fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub(crate) fn words<const N: usize>(&mut self) -> Result<[u32; N], Error> {
        let mut w = [0; N];
        for v in &mut w {
            *v = self.u32()?;
        }
        Ok(w)
    }

    pub(crate) fn tag(&mut self, want: [u8; 4]) -> Result<(), Error> {
        let at = self.at;
        let found = self.array()?;
        if found != want {
            return Err(Error::Tag { at, found, want });
        }
        Ok(())
    }

    pub(crate) fn rest(&mut self) -> &'a [u8] {
        let s = &self.bytes[self.at..];
        self.at = self.bytes.len();
        s
    }
}

/// The writing side of [`Reader`].
#[derive(Default)]
pub(crate) struct Writer {
    pub(crate) out: Vec<u8>,
}

impl Writer {
    pub(crate) fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub(crate) fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub(crate) fn words(&mut self, w: &[u32]) {
        w.iter().for_each(|&v| self.u32(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_round_trips() {
        let plain: Vec<u8> = (0..1200u32).map(|i| (i * 7) as u8).collect();
        let mut b = plain.clone();
        encrypt(&mut b);
        assert_ne!(b, plain);
        decrypt(&mut b);
        assert_eq!(b, plain);
    }

    #[test]
    fn checksum_uses_the_games_table() {
        // The table holds bit-reversed indices: [1] gives 0x80000000 where the standard CRC-32 gives 0xa505df1b.
        assert_eq!(checksum(&[]), 0);
        assert_eq!(checksum(&[1]), 0x8000_0000);
    }

    #[test]
    fn save_dir_lists_and_stores() {
        let game = std::env::temp_dir().join(format!("rbr-saves-{}", std::process::id()));
        let dir = SaveDir::new(&game);
        assert_eq!(dir.profiles().unwrap(), Vec::<String>::new());
        let results = results::Results { records: vec![] };
        dir.save_results(&results).unwrap();
        assert_eq!(dir.load_results().unwrap(), results);
        fs::write(dir.path.join("pfann.rbr"), b"").unwrap();
        fs::write(dir.path.join("other.rbr"), b"").unwrap();
        assert_eq!(dir.profiles().unwrap(), vec!["ann".to_owned()]);
        assert!(matches!(dir.load_profile("ann"), Err(Error::Size { len: 0, .. })));
        assert!(matches!(dir.load_profile("bob"), Err(Error::Io { kind: io::ErrorKind::NotFound, .. })));
        fs::remove_dir_all(&game).unwrap();
    }

    #[test]
    fn key_is_the_exes() {
        let Some(patch) = std::env::var_os("RBR_PATCH") else {
            eprintln!("skipping: RBR_PATCH not set");
            return;
        };
        let exe = std::fs::read(std::path::Path::new(&patch).join("RichardBurnsRally_SSE.exe")).unwrap();
        let image = crate::pe::parse(&exe).unwrap();
        assert_eq!(image.read(0x7d12c8, 500).unwrap(), &KEY[..]);
    }
}
