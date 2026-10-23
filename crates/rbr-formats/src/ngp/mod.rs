//! Immutable physics data read from the player's NGP 7.4.778 DLL. No plugin code is loaded or executed.
//! Constant-setting instruction sequences are decoded by [`initializers`]; everything else is PE data.

pub mod compat;
mod initializers;
pub mod locate;

use crate::data::Data;
use crate::lsp_keys::{Class, KEYS};
use crate::pe;
use locate::Rebase;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, String>;
pub type Words = Vec<(u32, u32)>;

/// The coefficients of NGP's height-dependent atmosphere, with the DLL's original precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereConstants {
    pub ground_temperature: f32,
    pub temperature_lapse: f32,
    pub pressure_lapse: f32,
    pub reference_temperature: f32,
    pub barometric_exponent: f64,
    pub ground_pressure: f32,
    pub air_gas_constant: f32,
    pub pressure_factor_115: f32,
    pub pressure_factor_0987: f32,
}

/// One preset's writes into the wheel definition and the wheel object.
#[derive(Debug, Clone, PartialEq)]
pub struct Tyre {
    pub definition: Words,
    pub wheel: Words,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub class: Class,
    pub name: String,
    pub offset: u32,
}

/// Decoded, immutable input data. Addresses and layouts here describe the supported DLL, never live state.
#[derive(Debug, PartialEq)]
pub struct Tables {
    pub stage: [Words; 2],
    pub tyres: [Tyre; 7],
    pub keys: Vec<Key>,
    pub removed: Vec<(Class, String)>,
    pub engine_defaults: Vec<(u32, f32)>,
    pub probe_defaults: [f32; 3],
    pub temperatures: [[f32; 8]; 2],
    pub tyre_ambient: [f32; 7],
    pub temperature_offsets: [f32; 7],
    pub atmosphere: AtmosphereConstants,
    pub road_surfaces: Vec<u32>,
    /// The DLL's read-only section, mapped by the boot for bindings that read constants from memory.
    pub rdata: pe::Section,
}

/// The DLLs the readers are verified for: each one's SHA-256 and its version. A DLL is added once
/// [`compat::compare`] says what we read from it is the same as 7.4.778's and its drives pass (`notes/ngp.md`,
/// "Adding an NGP version"); the decoders below read the addresses of 7.4.778.
pub const SUPPORTED: &[(&str, &str)] =
    &[("95b6dbf5e8957357d857a8e919cff177a6bfb832f5324da90467d43732edaaa4", "7.4.778")];
static TABLES: OnceLock<Tables> = OnceLock::new();

/// The version of the supported DLL `bytes` is, if it is one.
pub fn version(bytes: &[u8]) -> Option<&'static str> {
    let hash = format!("{:x}", Sha256::digest(bytes));
    SUPPORTED.iter().find(|(h, _)| *h == hash).map(|(_, v)| *v)
}

/// Parse only the verified binary. This check also protects the version-specific initializer boundaries.
pub fn validate(bytes: &[u8]) -> Result<()> {
    if version(bytes).is_none() {
        let versions: Vec<&str> = SUPPORTED.iter().map(|(_, v)| *v).collect();
        let found =
            compat::version_string(bytes, "FileVersion").map_or(String::new(), |v| format!(" (this one is {v})"));
        return Err(format!(
            "unsupported NGP.dll{found}: rerbr needs the original NGP {}, which the launcher downloads from gvrc.de",
            versions.join(" or ")
        ));
    }
    Ok(())
}

/// Decode the supported plugin's immutable inputs.
pub fn parse(bytes: &[u8]) -> Result<Tables> {
    validate(bytes)?;
    decode(bytes)
}

/// The tables as the decoders read them from the supported DLL.
fn decode(bytes: &[u8]) -> Result<Tables> {
    decode_with(bytes, &Rebase::default())
}

/// The tables as the decoders read them from any DLL whose addresses are `rebase`'s ([`compat`]).
fn decode_with(bytes: &[u8], rebase: &Rebase) -> Result<Tables> {
    let image = pe::parse(bytes).map_err(|e| format!("NGP.dll: {e}"))?;
    let f = |at: u32| {
        let at = rebase.at(at)?;
        image.u32(at).map(f32::from_bits).ok_or_else(|| format!("NGP.dll: missing {at:#x}"))
    };
    let row = |addresses: [u32; 8]| -> Result<[f32; 8]> {
        let mut out = [0.0; 8];
        for (value, at) in out.iter_mut().zip(addresses) {
            *value = f(at)?;
        }
        Ok(out)
    };
    let temperatures = [
        row([0x10148ac8, 0x10148f00, 0x10148f08, 0x10148f10, 0x10148f1c, 0x10148f24, 0x10148f2c, 0x10148f34])?,
        row([0x10148efc, 0x10148f04, 0x10148f0c, 0x10148f14, 0x10148f20, 0x10148f28, 0x10148f30, 0x10148f38])?,
    ];
    let engine_defaults =
        (0x10171aa4..=0x10171ad4).step_by(4).chain([0x10171a9c]).map(|at| Ok((at, f(at)?))).collect::<Result<_>>()?;
    let (keys, removed) = key_tables(&image, rebase)?;
    Ok(Tables {
        stage: initializers::stage(&image, rebase)?,
        tyres: initializers::tyres(&image, rebase)?,
        keys,
        removed,
        engine_defaults,
        probe_defaults: [f(0x10171300)?, f(0x10171304)?, f(0x10171308)?],
        temperatures,
        tyre_ambient: [
            f(0x10148f14)?,
            f(0x10148f0c)?,
            f(0x10148f04)?,
            f(0x10148f18)?,
            f(0x10148f10)?,
            f(0x10148f08)?,
            f(0x10148f10)?,
        ],
        temperature_offsets: [0.0, f(0x10148f50)?, f(0x10148f54)?, 0.0, f(0x10148f50)?, f(0x10148f54)?, f(0x10148f50)?],
        atmosphere: AtmosphereConstants {
            ground_temperature: f(0x10148ac8)?,
            temperature_lapse: f(0x10148a48)?,
            pressure_lapse: f(0x10148a4c)?,
            reference_temperature: f(0x10148ac4)?,
            barometric_exponent: f64::from_le_bytes(
                image.read(rebase.at(0x10148aa8)?, 8).ok_or("NGP exponent")?.try_into().unwrap(),
            ),
            ground_pressure: f(0x10148ad0)?,
            air_gas_constant: f(0x10148ac0)?,
            pressure_factor_115: f(0x10148a74)?,
            pressure_factor_0987: f(0x10148a68)?,
        },
        road_surfaces: image
            .read(rebase.at(0x10171280)?, 93)
            .ok_or("NGP road surfaces")?
            .iter()
            .map(|&b| u32::from(b))
            .collect(),
        rdata: image.section(".rdata").ok_or("NGP .rdata")?.clone(),
    })
}

/// Explicit override, then the installed plugin. Separate archive setups can use `RBR_NGP_DLL`.
pub fn path(data: &Data) -> Option<PathBuf> {
    std::env::var_os("RBR_NGP_DLL")
        .map(PathBuf::from)
        .or_else(|| data.resolve("Plugins/NGP.dll"))
        .or_else(|| std::env::var_os("RBR_NGP").map(|p| PathBuf::from(p).join("7.4.778/Plugins/NGP.dll")))
}

/// Load once per process; the supported binary and its immutable tables are identical for every car and stage.
pub fn load(data: &Data) -> Result<&'static Tables> {
    if let Some(tables) = TABLES.get() {
        return Ok(tables);
    }
    let path = path(data).ok_or("NGP.dll not found: install NGP 7.4.778 or set RBR_NGP_DLL to its DLL")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let tables = parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = TABLES.set(tables);
    Ok(TABLES.get().expect("NGP tables installed"))
}

/// The installed schema, or an explicitly configured test dataset's. Retail-only parser tests need no DLL.
pub fn current() -> Option<&'static Tables> {
    TABLES.get().or_else(|| {
        let data = Data::from_env().unwrap_or_else(|| Data::new("", None));
        path(&data)?;
        Some(load(&data).expect("configured NGP.dll"))
    })
}

fn string(image: &pe::Image, address: u32) -> Result<String> {
    let section = image.section(".rdata").ok_or("NGP .rdata")?;
    let offset = address.checked_sub(section.address).ok_or("NGP string pointer")? as usize;
    let bytes = section.bytes.get(offset..).ok_or("NGP string outside .rdata")?;
    let end = bytes.iter().position(|&b| b == 0).ok_or("NGP unterminated string")?;
    std::str::from_utf8(&bytes[..end]).map(str::to_owned).map_err(|e| e.to_string())
}

type KeyTables = (Vec<Key>, Vec<(Class, String)>);

/// The key tables: class, address and entry count (16 bytes an entry).
const KEY_TABLES: [(Class, u32, u32); 8] = [
    (Class::CarDef, 0x10171318, 20),
    (Class::DriveDef, 0x10171480, 69),
    (Class::DriveSetup, 0x101718d0, 28),
    (Class::EngineDef, 0x10171ae8, 133),
    (Class::MaterialManager, 0x10172350, 149),
    (Class::TyreDef, 0x10172cb0, 16),
    (Class::VcuDef, 0x10172db8, 26),
    (Class::WheelDef, 0x10172f68, 33),
];

/// The function the rename calls call (`CBase`'s key rename), found by its callers.
const RENAME: u32 = 0x10060db0;

/// What kind of bytes a [`Read`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadKind {
    /// Tables: found by their bytes, the pointers in them aside.
    Data,
    /// Code decoded for the constants it sets: found by its first and last instructions, the addresses in them aside.
    Code,
    /// A function whose start is found, and whose body is read up to its return.
    Entry,
}

/// An address range `[start, end)` of the supported DLL that the decoders read, and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    pub start: u32,
    pub end: u32,
    pub what: &'static str,
    pub kind: ReadKind,
}

/// The address ranges of the supported DLL the decoders read: data tables, the initializer code they decode, and the
/// functions they start from. (The rename calls are found by scanning all of `.text` for calls to [`RENAME`]; the
/// callers are not listed, `compat` compares what the scan finds.) Kept beside [`decode_with`] and [`initializers`]:
/// [`locate`] finds these in another DLL, and everything the decoders read must be in them (a [`Rebase`] fails an
/// address outside them).
pub fn reads() -> Vec<Read> {
    use ReadKind::*;
    let read = |start, end, what, kind| Read { start, end, what, kind };
    let mut reads = vec![
        read(0x10148a48, 0x10148a50, "atmosphere lapse rates", Data),
        read(0x10148a68, 0x10148a78, "atmosphere pressure factors", Data),
        read(0x10148aa8, 0x10148ab0, "atmosphere exponent", Data),
        read(0x10148ac0, 0x10148ad4, "atmosphere and ground temperature", Data),
        read(0x10148efc, 0x10148f58, "temperatures and tyre offsets", Data),
        read(0x10171280, 0x10171280 + 93, "road surfaces", Data),
        read(0x10171300, 0x1017130c, "probe defaults", Data),
        read(0x10171a9c, 0x10171ad8, "engine defaults", Data),
        read(0x1004e364, 0x1004e382, "stage defaults", Code),
        read(0x1004e3a7, 0x1004e3b1, "stage defaults", Code),
        read(0x1004e3c2, 0x1004e414, "stage initializer", Code),
        read(0x1004e41f, 0x100539dc, "stage initializer", Code),
        read(0x100539dd, 0x10058f9a, "stage initializer", Code),
        read(0x1003f635, 0x10041122, "tyre initializer", Code),
        read(0x100b79d0, 0x100b79d0 + 0x500, "tyre definition copy", Entry),
        read(0x100b7e70, 0x100b7e70 + 0x500, "tyre wheel copy", Entry),
        read(RENAME, RENAME + 0x20, "key rename", Entry),
    ];
    reads.extend(KEY_TABLES.iter().map(|&(_, base, count)| read(base, base + 16 * count, "key table", Data)));
    reads
}

fn key_tables(image: &pe::Image, rebase: &Rebase) -> Result<KeyTables> {
    use Class::*;
    let mut keys = Vec::new();
    for (class, base, count) in KEY_TABLES {
        let base = rebase.at(base)?;
        for i in 0..count {
            let at = base + 16 * i;
            let name = image.u32(at).ok_or("NGP key name")?;
            if name == 0 {
                continue;
            } // the stock prefix is copied from the exe at initialization
            let member = image.u32(at + 8).ok_or("NGP key member")?;
            let offset = if class == MaterialManager { (member - 3) / 88 } else { member * 4 };
            keys.push(Key { class, name: string(image, name)?, offset });
        }
    }
    let text = image.section(".text").ok_or("NGP .text")?;
    let mut removed = Vec::new();
    let rename = rebase.at(RENAME)?;
    // Rename calls push two .rdata string pointers. Decode only those operands, not the surrounding control flow.
    for (i, bytes) in text.bytes.windows(5).enumerate() {
        if bytes[0] != 0xe8 {
            continue;
        }
        let call = text.address + i as u32;
        let target = (call + 5).wrapping_add(u32::from_le_bytes(bytes[1..].try_into().unwrap()));
        if target != rename {
            continue;
        }
        let mut names = Vec::new();
        for j in i.saturating_sub(32)..i.saturating_sub(4) {
            if text.bytes[j] == 0x68 {
                let pointer = u32::from_le_bytes(text.bytes[j + 1..j + 5].try_into().unwrap());
                if let Ok(name) = string(image, pointer) {
                    names.push(name);
                }
            }
        }
        let [new, old] = names.get(names.len().saturating_sub(2)..).ok_or("NGP rename operands")? else {
            return Err(format!("NGP rename operands at {call:#x}"));
        };
        let candidates: Vec<_> = KEYS
            .iter()
            .filter(|k| {
                k.key == old
                    && matches!(
                        k.class,
                        CarDef
                            | DriveDef
                            | DriveSetup
                            | DriveSetupOptions
                            | EngineDef
                            | EngineSetup
                            | EngineSetupOptions
                            | MaterialManager
                            | PhysDef
                            | TyreDef
                            | VcuDef
                            | WheelDef
                    )
            })
            .collect();
        let [stock] = candidates.as_slice() else {
            return Err(format!("NGP ambiguous rename {old}"));
        };
        keys.push(Key { class: stock.class, name: new.clone(), offset: stock.offset });
        removed.push((stock.class, old.clone()));
    }
    if keys.len() != 254 || removed.len() != 41 {
        return Err("NGP key table shape changed".into());
    }
    Ok((keys, removed))
}
