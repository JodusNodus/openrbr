//! Immutable physics data read from the player's NGP 7.4.778 DLL. No plugin code is loaded or executed.
//! Constant-setting instruction sequences are decoded by [`initializers`]; everything else is PE data.

mod initializers;

use crate::data::Data;
use crate::lsp_keys::{Class, KEYS};
use crate::pe;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, String>;
pub type Words = Vec<(u32, u32)>;

/// The coefficients of NGP's height-dependent atmosphere, with the DLL's original precision.
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug, Clone)]
pub struct Tyre {
    pub definition: Words,
    pub wheel: Words,
}

#[derive(Debug, Clone)]
pub struct Key {
    pub class: Class,
    pub name: String,
    pub offset: u32,
}

/// Decoded, immutable input data. Addresses and layouts here describe the supported DLL, never live state.
#[derive(Debug)]
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

const HASH: &str = "95b6dbf5e8957357d857a8e919cff177a6bfb832f5324da90467d43732edaaa4";
static TABLES: OnceLock<Tables> = OnceLock::new();

/// Parse only the verified binary. This check also protects the version-specific initializer boundaries.
pub fn validate(bytes: &[u8]) -> Result<()> {
    if format!("{:x}", Sha256::digest(bytes)) != HASH {
        return Err("unsupported NGP.dll: expected the original NGP 7.4.778 binary".into());
    }
    Ok(())
}

/// Decode the supported plugin's immutable inputs.
pub fn parse(bytes: &[u8]) -> Result<Tables> {
    validate(bytes)?;
    let image = pe::parse(bytes).map_err(|e| format!("NGP.dll: {e}"))?;
    let f = |at| image.u32(at).map(f32::from_bits).ok_or_else(|| format!("NGP.dll: missing {at:#x}"));
    let temperatures = [
        [0x10148ac8, 0x10148f00, 0x10148f08, 0x10148f10, 0x10148f1c, 0x10148f24, 0x10148f2c, 0x10148f34],
        [0x10148efc, 0x10148f04, 0x10148f0c, 0x10148f14, 0x10148f20, 0x10148f28, 0x10148f30, 0x10148f38],
    ]
    .map(|row| row.map(|at| f(at).expect("verified DLL temperature")));
    let engine_defaults =
        (0x10171aa4..=0x10171ad4).step_by(4).chain([0x10171a9c]).map(|at| Ok((at, f(at)?))).collect::<Result<_>>()?;
    let (keys, removed) = key_tables(&image)?;
    Ok(Tables {
        stage: initializers::stage(&image)?,
        tyres: initializers::tyres(&image)?,
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
                image.read(0x10148aa8, 8).ok_or("NGP exponent")?.try_into().unwrap(),
            ),
            ground_pressure: f(0x10148ad0)?,
            air_gas_constant: f(0x10148ac0)?,
            pressure_factor_115: f(0x10148a74)?,
            pressure_factor_0987: f(0x10148a68)?,
        },
        road_surfaces: image.read(0x10171280, 93).ok_or("NGP road surfaces")?.iter().map(|&b| u32::from(b)).collect(),
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

fn key_tables(image: &pe::Image) -> Result<KeyTables> {
    use Class::*;
    let mut keys = Vec::new();
    for (class, base, count) in [
        (CarDef, 0x10171318, 20),
        (DriveDef, 0x10171480, 69),
        (DriveSetup, 0x101718d0, 28),
        (EngineDef, 0x10171ae8, 133),
        (MaterialManager, 0x10172350, 149),
        (TyreDef, 0x10172cb0, 16),
        (VcuDef, 0x10172db8, 26),
        (WheelDef, 0x10172f68, 33),
    ] {
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
    // Rename calls push two .rdata string pointers. Decode only those operands, not the surrounding control flow.
    for (i, bytes) in text.bytes.windows(5).enumerate() {
        if bytes[0] != 0xe8 {
            continue;
        }
        let call = text.address + i as u32;
        let target = (call + 5).wrapping_add(u32::from_le_bytes(bytes[1..].try_into().unwrap()));
        if target != 0x10060db0 {
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
