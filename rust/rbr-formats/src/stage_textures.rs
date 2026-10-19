//! Texture file names for stage geometry: from a stage `.ini` `[TextureInfo]`
//! entry to the `.dds` (or shadow stream) inside the `*_textures.rbz`
//! archives.
//!
//! The chain, from HedgeHog3D's `CHH_TrackLoader::LoadTextures`,
//! `LoadShadowTextures` and `LoadSpecularTextures` (`HH_TrackLoaderTextures.cpp`):
//!
//! 1. The `.ini` gives the name (`Texture<N>=CHAIR 01.tga`), `IsGroundTexture`
//!    and the counts. Chunk texture indices are these `N`.
//! 2. The loader builds `<track>_textures\<weather>\<stem>.dds`, or
//!    `<track>_textures\<weather>\<wear>\<stem>.dds` when the texture is a
//!    ground texture. `<stem>` is the name's last path component without its
//!    extension, lower-cased. `<weather>` is `dry`, `damp` or `wet`; `<wear>`
//!    is `new`, `normal` or `worn`.
//! 3. [`FilenameMap::resolve`] maps that request through `TextureFilenameMap.dat`
//!    (`ResolveFilename`). The map has no entry for most requests, which then
//!    stay as they are. Some entries point at a sibling stage's archive.
//!
//! Shadow textures (`ShadowTexture<N>`) use `<track>_textures\<name>` directly.
//! Specular textures try `<track>_textures\<weather>\<wear>\<stem>.dds` and fall
//! back to `<track>_textures\<weather>\<stem>.dds`.
//!
//! The weather and wear are chosen at run time. [`DEFAULT_WEATHER`] and
//! [`DEFAULT_WEAR`] are the values the retail archives are checked with.
//! Notes: `notes/formats/lbs.md`.

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

/// Weather folder used when none is chosen.
pub const DEFAULT_WEATHER: &str = "dry";
/// Wear folder used when none is chosen (wear index 0).
pub const DEFAULT_WEAR: &str = "new";

/// Parse failure with the byte offset it was found at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The count word or a NUL-terminated string runs past the end.
    Truncated { offset: usize },
    /// Bytes remain after the last pair.
    Trailing { offset: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset } => write!(f, "truncated at offset {offset}"),
            Error::Trailing { offset } => write!(f, "trailing bytes at offset {offset}"),
        }
    }
}

impl std::error::Error for Error {}

/// `TextureFilenameMap.dat`: requested archive path to the archive path to load.
/// Keys are lower-case, with `\` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameMap {
    entries: HashMap<String, String>,
}

impl FilenameMap {
    /// Parses `u32 count`, then `count` pairs of NUL-terminated strings
    /// (`requested`, `target`). The file must end after the last pair.
    pub fn parse(bytes: &[u8]) -> Result<FilenameMap, Error> {
        let count_bytes = bytes.get(..4).ok_or(Error::Truncated { offset: 0 })?;
        let count = u32::from_le_bytes([count_bytes[0], count_bytes[1], count_bytes[2], count_bytes[3]]);
        let mut pos = 4usize;
        let mut entries = HashMap::new();
        for _ in 0..count {
            let key = read_nul_string(bytes, &mut pos)?;
            let value = read_nul_string(bytes, &mut pos)?;
            entries.insert(key.to_ascii_lowercase(), value);
        }
        if pos != bytes.len() {
            return Err(Error::Trailing { offset: pos });
        }
        Ok(FilenameMap { entries })
    }

    /// Parses RallySimFans's `TextureFilenameMap<id>.ini`: one `requested<TAB>target` pair per line, `;` comments.
    /// The targets are archive paths as in `TextureFilenameMap.dat`, often another stage's (`track-13_o_textures\..`);
    /// a target without an archive (`damp\bka.dds`) is in the request's archive (ours, from the files: 1,555 of
    /// stage 129's targets). Lines without a tab are skipped.
    pub fn parse_rsf(text: &str) -> FilenameMap {
        let entries = text
            .lines()
            .filter(|line| !line.trim_start().starts_with(';'))
            .filter_map(|line| line.split_once('\t'))
            .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .filter(|(key, value)| !key.is_empty() && !value.is_empty())
            .map(|(key, value)| {
                // A target without an archive (`damp\bka.dds`) is in the request's own archive.
                let archive = |path: &str| path.split_once('\\').map(|(a, _)| a.to_owned());
                match (archive(&key), archive(&value)) {
                    (Some(own), Some(first)) if !first.to_ascii_lowercase().ends_with("_textures") => {
                        let value = format!("{own}\\{value}");
                        (key, value)
                    }
                    _ => (key, value),
                }
            })
            .collect();
        FilenameMap { entries }
    }

    /// The map with `over`'s pairs added, replacing this map's for the same request.
    pub fn overlaid(mut self, over: FilenameMap) -> FilenameMap {
        self.entries.extend(over.entries);
        self
    }

    /// The archive path for `requested`, or `requested` unchanged when the
    /// map has no entry (the game's identity fallback).
    pub fn resolve(&self, requested: &str) -> String {
        let key = requested.to_ascii_lowercase();
        match self.entries.get(&key) {
            Some(target) => target.clone(),
            None => key,
        }
    }

    /// Number of pairs.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no pairs.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn read_nul_string(bytes: &[u8], pos: &mut usize) -> Result<String, Error> {
    let rest = bytes.get(*pos..).ok_or(Error::Truncated { offset: *pos })?;
    let len = rest.iter().position(|&b| b == 0).ok_or(Error::Truncated { offset: *pos })?;
    let s = String::from_utf8_lossy(&rest[..len]).into_owned();
    *pos += len + 1;
    Ok(s)
}

/// Last path component of `name` without its extension (`CZipPath::m_base`).
pub fn stem(name: &str) -> &str {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    }
}

/// The requested path for a `[TextureInfo]` texture, lower-cased.
pub fn texture_request(track: &str, name: &str, ground: bool, weather: &str, wear: &str) -> String {
    let stem = stem(name);
    let path = if ground {
        format!("{track}_textures\\{weather}\\{wear}\\{stem}.dds")
    } else {
        format!("{track}_textures\\{weather}\\{stem}.dds")
    };
    path.to_ascii_lowercase()
}

/// The two requests for a specular texture, in the order the loader tries
/// them: the wear-folder form, then the plain weather form.
pub fn specular_requests(track: &str, name: &str, weather: &str, wear: &str) -> [String; 2] {
    let stem = stem(name);
    [
        format!("{track}_textures\\{weather}\\{wear}\\{stem}.dds").to_ascii_lowercase(),
        format!("{track}_textures\\{weather}\\{stem}.dds").to_ascii_lowercase(),
    ]
}

/// The request for a shadow texture stream (`ShadowTexture<N>=`).
pub fn shadow_request(track: &str, name: &str) -> String {
    format!("{track}_textures\\{name}").to_ascii_lowercase()
}

/// Splits an archive path (`track-10_o_textures\dry\x.dds`) into the archive
/// file name (`track-10_o_textures.rbz`) and the entry path inside it
/// (`dry/x.dds`), both lower-case. `None` when the path has no folder part.
pub fn split_archive_path(path: &str) -> Option<(String, String)> {
    let (archive, entry) = path.split_once('\\')?;
    Some((format!("{archive}.rbz"), entry.replace('\\', "/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut b = (pairs.len() as u32).to_le_bytes().to_vec();
        for (k, v) in pairs {
            b.extend_from_slice(k.as_bytes());
            b.push(0);
            b.extend_from_slice(v.as_bytes());
            b.push(0);
        }
        b
    }

    #[test]
    fn parses_pairs_and_resolves_case_insensitively() {
        let bytes = map(&[(r"Track-1_O_textures\dry\new\Chair 01.dds", r"track-1_o_textures\dry\Chair 01.dds")]);
        let m = FilenameMap::parse(&bytes).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.resolve(r"track-1_o_textures\dry\new\chair 01.dds"), r"track-1_o_textures\dry\Chair 01.dds");
        assert_eq!(m.resolve(r"track-1_o_textures\dry\x.dds"), r"track-1_o_textures\dry\x.dds");
    }

    #[test]
    fn rsf_maps_overlay_the_retail_map() {
        let retail =
            FilenameMap::parse(&map(&[("t1_textures\\x.dds", "t2_textures\\x.dds"), ("c\\y.dds", "d\\y.dds")]))
                .unwrap();
        let rsf = FilenameMap::parse_rsf(
            "; 20240623 remap\r\n; TAB char is the separator\r\nTrack-101_o_textures\\dry\\A.dds\ttrack-13_o_textures\\dry\\a.dds\r\n\
             t1_textures\\x.dds\tt3_textures\\x.dds\r\nno tab here\r\ntrack-129_m_textures\\damp\\bkn1.dds\tdamp\\bka.dds\r\n",
        );
        assert_eq!(rsf.len(), 3);
        assert_eq!(rsf.resolve("track-129_m_textures\\damp\\bkn1.dds"), "track-129_m_textures\\damp\\bka.dds");
        let m = retail.overlaid(rsf);
        assert_eq!(m.resolve("track-101_o_textures\\dry\\a.dds"), "track-13_o_textures\\dry\\a.dds");
        assert_eq!(m.resolve("t1_textures\\x.dds"), "t3_textures\\x.dds");
        assert_eq!(m.resolve("c\\y.dds"), "d\\y.dds");
    }

    #[test]
    fn rejects_truncated_and_trailing_data() {
        assert!(matches!(FilenameMap::parse(&[1, 0]), Err(Error::Truncated { offset: 0 })));
        let mut bytes = map(&[("a", "b")]);
        bytes.push(0);
        assert!(matches!(FilenameMap::parse(&bytes), Err(Error::Trailing { .. })));
        let mut cut = map(&[("a", "b")]);
        cut.truncate(cut.len() - 1);
        assert!(matches!(FilenameMap::parse(&cut), Err(Error::Truncated { .. })));
    }

    #[test]
    fn huge_count_fails_without_allocating() {
        let bytes = u32::MAX.to_le_bytes();
        assert!(FilenameMap::parse(&bytes).is_err());
    }

    #[test]
    fn stems_drop_directories_and_last_extension() {
        assert_eq!(stem("CHAIR 01.tga"), "CHAIR 01");
        assert_eq!(stem(r"MAPS\ARCTIC\AR_SUPERBOWL_11.tga_0.tga"), "AR_SUPERBOWL_11.tga_0");
        assert_eq!(stem("Specular"), "Specular");
        assert_eq!(stem(".hidden"), ".hidden");
    }

    #[test]
    fn requests_follow_the_loader() {
        assert_eq!(
            texture_request("Track-10_O", "CHAIR 01.tga", false, "dry", "new"),
            r"track-10_o_textures\dry\chair 01.dds"
        );
        assert_eq!(
            texture_request("track-10_O", "ICE_PATCH.tga", true, "dry", "new"),
            r"track-10_o_textures\dry\new\ice_patch.dds"
        );
        assert_eq!(
            specular_requests("track-10_O", "Specular.tga", "dry", "new"),
            [
                r"track-10_o_textures\dry\new\specular.dds".to_string(),
                r"track-10_o_textures\dry\specular.dds".to_string()
            ]
        );
        assert_eq!(shadow_request("track-20_N", "shadow_0.dat"), r"track-20_n_textures\shadow_0.dat");
    }

    #[test]
    fn archive_split() {
        assert_eq!(
            split_archive_path(r"track-10_o_textures\dry\new\x.dds"),
            Some(("track-10_o_textures.rbz".to_string(), "dry/new/x.dds".to_string()))
        );
        assert_eq!(split_archive_path("plain"), None);
    }
}
