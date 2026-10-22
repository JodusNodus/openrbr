//! Every retail `.lsp` key resolves to a loader key table.
//!
//! Runs only when `RBR_DATA` points at the extracted install tree, like the
//! other retail tests. For each parsed file the test walks the block tree and
//! checks every bare key against the [`lsp_keys::Class`] tables that block
//! feeds (see `notes/formats.md` "Physics scripts" and
//! `data/physics-lsp-fields.md`). Unknown keys fail the test, so a
//! failure means either a missing table entry or a wrong block mapping.
//!
//! Deliberately skipped: `Misc/state.lsp`'s `PhysicsState` root (a save
//! snapshot: its `vecWorldPosition`/`PhysicsState` strings do not exist in
//! the executable, so the NTweak loader never parses it) and bare
//! `NULL_PPTOKEN` atoms (unexpanded preprocessor tokens the loader skips).
//!
//! The second test below covers the NGP mod's `.lsp` files with the same
//! [`Walker`], resolving keys through NGP's key tables
//! ([`lsp_keys_ngp`], `notes/ngp.md` "Load path") and allowing
//! the DLL's material records (see `notes/ngp.md` "NGP .lsp keys").

use rbr_formats::{lsp, lsp_keys, lsp_keys_ngp, rbz};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use lsp::Node;
use lsp_keys::Class;

fn is_number(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

/// Toggle (`+FLAG`) or signed number (`+0.04`)?
fn is_toggle(s: &str) -> bool {
    (s.starts_with('+') || s.starts_with('-')) && !is_number(s)
}

/// Block name of a list: first element when it is an atom or a quoted string.
fn block_head(list: &[Node]) -> Option<&str> {
    match list.first()? {
        Node::Atom(s) | Node::Quoted(s) => Some(s.as_str()),
        Node::List(_) => None,
    }
}

/// File shapes, classified by top-level block names (robust against the two
/// misnamed `gravel.lsp`/`tarmac.lsp` setup files).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Def,
    Setup,
    Options,
    TyreTypes,
    Surfaces,
    FlatPhys,
    Prime,
    StateSave,
}

fn classify(top: &[String]) -> Shape {
    if top.contains(&"CarOptions".to_owned()) {
        Shape::Options
    } else if top.contains(&"TyreType00".to_owned()) {
        Shape::TyreTypes
    } else if top.contains(&"MaterialManager".to_owned()) {
        Shape::Surfaces
    } else if top.contains(&"DefManager".to_owned()) {
        if top.iter().any(|b| b == "NULL_PPTOKEN") {
            Shape::Prime
        } else {
            Shape::Def
        }
    } else if top.contains(&"Car".to_owned()) && top.contains(&"TyreLF".to_owned()) {
        Shape::Setup
    } else if top.is_empty() {
        Shape::FlatPhys
    } else {
        Shape::StateSave
    }
}

/// Classes a top-level block feeds, by file shape.
fn top_classes(shape: Shape, block: &str) -> Vec<Class> {
    use Class as C;
    match shape {
        Shape::Options => match block {
            "CarOptions" => vec![C::CarSetupOptions],
            "DriveOptions" => vec![C::DriveSetupOptions],
            "ControlOptions" => vec![C::VcuSetupOptions],
            "EngineOptions" => vec![C::EngineSetupOptions],
            b if b.starts_with("SpringDamperOptions") => vec![C::SpringSetupOptions],
            b if b.starts_with("TyreOptions") => vec![C::TyreSetupOptions],
            b if b.starts_with("WheelOptions") => vec![C::WheelSetupOptions],
            _ => vec![],
        },
        Shape::Setup => match block {
            "Car" => vec![C::CarSetup],
            "Drive" => vec![C::DriveSetup],
            "Engine" => vec![C::EngineSetup],
            "VehicleControlUnit" => vec![C::VcuSetup],
            b if b.starts_with("Wheel") => vec![C::WheelSetup],
            b if b.starts_with("SpringDamper") => vec![C::SpringSetup],
            b if b.starts_with("Tyre") => vec![C::TyreSetup],
            _ => vec![],
        },
        Shape::TyreTypes => match block {
            b if b.starts_with("TyreType") => vec![C::TyreTypes],
            _ => vec![],
        },
        Shape::Def => match block {
            // Car blocks mix chassis keys: the def, the chassis solid and
            // the body state (see notes/formats.md "Physics scripts").
            "Car" => vec![C::CarDef, C::SolidDef, C::BodyState],
            "CarBody" => vec![C::CarBodyDef],
            "HingedBody" => vec![C::HingedDef],
            "MountedBody" => vec![C::MountedDef],
            "Ground" => vec![C::GroundDef],
            "Solid" => vec![C::SolidDef, C::BodyState],
            "Wheel" => vec![C::WheelDef],
            _ => vec![],
        },
        Shape::Surfaces => match block {
            "CollisionSolver" => vec![C::CollisionSolverDef],
            "DefManager" | "MaterialManager" => vec![],
            _ => entity_classes(block),
        },
        Shape::FlatPhys => vec![],
        Shape::Prime | Shape::StateSave => vec![],
    }
}

/// Entity blocks under `DefManager` (quoted or bare). Car blocks mix
/// chassis keys: the def, the chassis solid and the body state.
fn entity_classes(block: &str) -> Vec<Class> {
    use Class as C;
    match block {
        "Car" => vec![C::CarDef, C::SolidDef, C::BodyState],
        "CarBody" => vec![C::CarBodyDef],
        "HingedBody" => vec![C::HingedDef],
        "MountedBody" => vec![C::MountedDef],
        "Ground" => vec![C::GroundDef],
        "Solid" => vec![C::SolidDef, C::BodyState],
        "Wheel" => vec![C::WheelDef],
        _ => vec![],
    }
}

/// Classes a nested block feeds: entity children under `DefManager`, car
/// children under `Car`, and surface names under `MaterialManager`.
fn child_classes(parent: &str, block: &str) -> Vec<Class> {
    use Class as C;
    match parent {
        "Car" => match block {
            "Drive" => vec![C::DriveDef],
            "ControlUnit" => vec![C::VcuDef],
            "LightSystem" => vec![C::CarLightDef],
            "Engine" => vec![C::EngineDef],
            _ => vec![],
        },
        "MaterialManager" => vec![C::Material],
        "Wheel" => match block {
            "SpringDamper" => vec![C::SpringDef],
            "Tyre" => vec![C::TyreDef],
            _ => vec![],
        },
        _ => vec![],
    }
}

struct Walker {
    shape: Shape,
    /// Class set of the enclosing block (empty = structural/root level).
    classes: Vec<Class>,
    failures: Vec<String>,
    flag_names: BTreeSet<String>,
    seen: BTreeSet<(Class, String)>,
    /// When set, `MaterialManager` child names must resolve in the 1.02
    /// table or be in this list. The NGP test passes the DLL's material names;
    /// retail files are not surface-checked.
    extra_surfaces: Option<Vec<String>>,
    /// Resolve keys with the NGP key tables (`lsp_keys_ngp`) instead of 1.02's.
    ngp: bool,
    /// Stock keys the files use that NGP renamed: its parser skips them (as 1.02's skips unknown keys).
    renamed: BTreeSet<String>,
}

impl Walker {
    fn check_key(&mut self, where_: &str, key: &str) {
        let mut ok = None;
        for class in &self.classes {
            let off =
                if self.ngp { lsp_keys_ngp::ngp_key_offset(*class, key) } else { lsp_keys::key_offset(*class, key) };
            if let Some(off) = off {
                self.seen.insert((*class, key.to_owned()));
                ok = Some((*class, off));
                break;
            }
        }
        if ok.is_none() {
            let renamed = self.ngp && self.classes.iter().any(|c| lsp_keys_ngp::is_removed(*c, key));
            if renamed {
                self.renamed.insert(key.to_owned());
            } else {
                self.failures.push(format!("{where_}: unknown key `{key}`"));
            }
        }
    }

    /// Walk the body items of one block.
    fn walk_body(&mut self, items: &[Node], where_: &str, parent: &str) {
        let mut i = 0;
        while i < items.len() {
            match &items[i] {
                Node::Atom(k) if k == "NULL_PPTOKEN" => {
                    i += 1;
                }
                Node::Atom(k) if is_toggle(k) => {
                    self.flag_names.insert(k[1..].to_owned());
                    i += 1;
                }
                Node::Atom(k) => {
                    if i + 1 < items.len() {
                        if let Node::List(body) = &items[i + 1] {
                            // `Block (body...)`: k is both a possible key of
                            // the enclosing class and the sub-block name.
                            // Under MaterialManager any other name starts a
                            // surface instance block.
                            // NGP-only: surface instance names under
                            // MaterialManager (all shipped surfaces use this
                            // bare-atom form).
                            if self.extra_surfaces.as_ref().is_some_and(|ss| {
                                parent == "MaterialManager"
                                    && lsp_keys::key_offset(Class::MaterialManager, k).is_none()
                                    && !ss.contains(k)
                            }) {
                                self.failures.push(format!("{where_}: unknown surface `{k}`"));
                            }
                            let is_surface = self.shape == Shape::Surfaces
                                && self.classes == vec![Class::PhysDef]
                                && !matches!(k.as_str(), "DefManager" | "CollisionSolver" | "MaterialManager");
                            if is_surface {
                                let allowed = self.extra_surfaces.as_ref().is_some_and(|ss| ss.contains(k));
                                if lsp_keys::key_offset(Class::MaterialManager, k).is_none() && !allowed {
                                    self.failures.push(format!("{where_}: unknown surface `{k}`"));
                                }
                            } else if !self.classes.is_empty() {
                                self.check_key(where_, k);
                            }
                            let sub = if is_surface {
                                vec![Class::Material]
                            } else {
                                let by_name = top_classes(self.shape, k);
                                if by_name.is_empty() {
                                    child_classes(parent, k)
                                } else {
                                    by_name
                                }
                            };
                            let inner = if block_head(body) == Some(k.as_str()) {
                                // `Drive ("Drive" keys...)`: head repeats.
                                &body[1..]
                            } else {
                                &body[..]
                            };
                            let saved_classes = std::mem::replace(&mut self.classes, sub);
                            let saved_parent = parent;
                            self.walk_body(inner, &format!("{where_}/{k}"), k);
                            self.classes = saved_classes;
                            let _ = saved_parent;
                            i += 2;
                            continue;
                        }
                    }
                    if is_number(k) {
                        i += 1; // stray value
                    } else if KNOWN_SKIPPED_KEYS.contains(&k.as_str()) {
                        i += 1; // documented loader-skipped key
                    } else {
                        self.check_key(where_, k);
                        i += 1;
                    }
                }
                Node::Quoted(_) => {
                    i += 1; // instance name
                }
                Node::List(inner) => {
                    // `("Block" ...)` / `("Block" ("inst" ...))`.
                    match block_head(inner) {
                        Some(name) => {
                            let (inst_body, rest) = match inner.get(1) {
                                Some(Node::List(b)) if block_head(b).is_some_and(|h| h != name) => {
                                    (b.as_slice(), &inner[2..])
                                }
                                _ => (&inner[1..], &[][..]),
                            };
                            let _ = rest;
                            let by_name = top_classes(self.shape, name);
                            let sub = if by_name.is_empty() { child_classes(parent, name) } else { by_name };
                            // Surface blocks resolve by instance name.
                            let sub = if parent == "MaterialManager" { vec![Class::Material] } else { sub };
                            if sub.is_empty() && self.shape == Shape::Surfaces {
                                // Surface body lists start with the quoted
                                // surface name; handled via parent above.
                            }
                            let saved = std::mem::replace(&mut self.classes, sub);
                            // Skip a leading quoted instance name.
                            let body = match inst_body.first() {
                                Some(Node::Quoted(_)) => &inst_body[1..],
                                _ => inst_body,
                            };
                            self.walk_body(body, &format!("{where_}/{name}"), name);
                            self.classes = saved;
                        }
                        None => self.walk_body(inner, where_, parent),
                    }
                    i += 1;
                }
            }
        }
    }
}

/// Bits the loader accepts that are absent from the exe bit-name tables
/// (`CBitField::Parse` skips unknown names silently; see
/// `data/physics-lsp-fields.md`).
const TOLERATED_BITS: &[&str] = &[
    "BLOCK_MAP_CULLING",
    "BLOCK_MAP_CULLING_FIX",
    "RENDER_TRINODE_COLLISION",
    "INVALID_MATERIAL_ALERT",
    // Present in localphysics.lsp DbgFlags; absent from the 27-name table
    // at 0x7cbcb0, so the loader skips it like any unknown bit.
    "RENDER_MATERIAL_ID",
];

/// Retail keys with no key-table entry anywhere in the executable: the
/// loader's silent-skip path drops them (`CBase::Parse` miss branch).
const KNOWN_SKIPPED_KEYS: &[&str] = &[
    // `Wheel` blocks in car overlays. The `Radius` string has no reference
    // in any table or code; the wheel radius comes from the `Tyre` child
    // (`CarcassRadius`/`RimRadius`).
    "Radius",
];

/// Every bit name the loader accepts, from the exe's tables (generated
/// `lsp_tables`, not transcribed): `CPhysics::CDef` Flags/DbgFlags/LogFlags
/// plus `CMaterial` Flags.
fn known_bits() -> BTreeSet<String> {
    use rbr_formats::lsp_tables;
    [
        lsp_tables::BITFIELD_FLAGS,
        lsp_tables::BITFIELD_DBGFLAGS,
        lsp_tables::BITFIELD_LOGFLAGS,
        lsp_tables::BITFIELD_MATERIAL,
    ]
    .iter()
    .flat_map(|t| t.iter())
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn all_retail_lsp_keys_resolve() {
    let Ok(data_dir) = std::env::var("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let mut failures = Vec::new();
    let mut flag_names: BTreeSet<String> = BTreeSet::new();
    let mut seen: BTreeSet<(Class, String)> = BTreeSet::new();
    let mut files = 0;

    for archive_name in ["Root/physics.rbz", "Root/misc.rbz"] {
        let path = Path::new(&data_dir).join(archive_name);
        let mut archive = rbz::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).expect("entry");
            let entry_name = entry.name().expect("ascii name").into_owned();
            if !entry_name.ends_with(".lsp") {
                continue;
            }
            let name = format!("{}!{entry_name}", path.display());
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let text = std::str::from_utf8(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let doc = lsp::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            files += 1;
            let Node::List(root) = &doc[0] else {
                panic!("{name}: expected one top-level list");
            };
            // Retail files wrap once more: `(("Root" ...))`.
            let root = match &root[..] {
                [Node::List(inner)] => inner,
                _ => panic!("{name}: expected single root list"),
            };
            // Top-level block names (bare atom followed by a list).
            let mut top = Vec::new();
            let mut j = 0;
            while j < root.len() {
                if let Node::Atom(k) = &root[j] {
                    if j + 1 < root.len() && matches!(root[j + 1], Node::List(_)) {
                        top.push(k.clone());
                    }
                }
                j += 1;
            }
            let shape = classify(&top);
            if entry_name.ends_with("Misc/state.lsp") {
                // Save snapshot, not loader input (see module docs).
                continue;
            }
            let body_start = match root.first() {
                Some(Node::Quoted(_)) => 1,
                _ => 0,
            };
            // Top-level blocks dispatch into the setup containers
            // (`E16_LoadTyres` / `FUN_0048d940` Parse on
            // `g_e14SetupOptions` / `g_e14Setup`), so their names count as
            // key uses there. Other shapes' tops are registry-dispatched
            // (Def) or inert (Prime/StateSave).
            let container = match shape {
                Shape::Setup => Some(Class::CompleteCarSetup),
                Shape::Options | Shape::TyreTypes => Some(Class::CompleteCarSetupOptions),
                _ => None,
            };
            if let Some(container) = container {
                let mut record = |block: &str| {
                    if lsp_keys::key_offset(container, block).is_some() {
                        seen.insert((container, block.to_owned()));
                    }
                };
                for block in &top {
                    record(block);
                }
                for item in &root[body_start..] {
                    if let Node::List(inner) = item {
                        if let Some(name) = block_head(inner) {
                            record(name);
                        }
                    }
                }
            }
            // Root-level keys feed the physics def (physics.lsp holds
            // `MaxTimeStep`/`vecGravity`/flags next to its blocks).
            let classes =
                if shape == Shape::FlatPhys || shape == Shape::Surfaces { vec![Class::PhysDef] } else { vec![] };
            let mut walker = Walker {
                shape,
                classes,
                failures: Vec::new(),
                flag_names: BTreeSet::new(),
                seen: BTreeSet::new(),
                extra_surfaces: None,
                ngp: false,
                renamed: BTreeSet::new(),
            };
            walker.walk_body(&root[body_start..], &name, "");
            // Surface names under MaterialManager must be known surfaces.
            failures.extend(walker.failures);
            flag_names.extend(walker.flag_names);
            seen.extend(walker.seen);
        }
    }
    assert!(files > 80, "expected most retail .lsp files, got {files}");

    let known = known_bits();
    let mut bad_bits: Vec<&String> =
        flag_names.iter().filter(|b| !known.contains(*b) && !TOLERATED_BITS.contains(&b.as_str())).collect();
    bad_bits.sort();
    for b in &bad_bits {
        failures.push(format!("unknown flag bit `{b}`"));
    }

    if !failures.is_empty() {
        failures.sort();
        panic!("{} unknown keys/bits:\n{}", failures.len(), failures.join("\n"));
    }

    // Coverage: every table key ideally appears in retail (informational).
    // Unused keys keep `used_in_retail == false` (checked below), so the
    // load-path apply only trusts keys retail exercises.
    let mut missing = BTreeSet::new();
    for info in lsp_keys::KEYS {
        if !seen.contains(&(info.class, info.key.to_owned())) {
            missing.insert(format!("{:?}::{}", info.class, info.key));
        }
    }
    eprintln!("{} table keys unused in retail", missing.len());
    let recorded: BTreeSet<String> =
        include_str!("../../../scripts/lsp-unused.txt").lines().map(|s| s.to_owned()).collect();
    assert_eq!(missing, recorded, "scripts/lsp-unused.txt drifted from retail use");
    let mut unknown_used = Vec::new();
    let mut flag_drift = Vec::new();
    for info in lsp_keys::KEYS {
        let id = format!("{:?}::{}", info.class, info.key);
        if info.reader == lsp_keys::ReaderKind::Unknown && !missing.contains(&id) {
            unknown_used.push(id.clone());
        }
        if info.used_in_retail == missing.contains(&id) {
            flag_drift.push(id);
        }
    }
    assert!(unknown_used.is_empty(), "keys without a proven reader are used in retail: {unknown_used:?}");
    assert!(flag_drift.is_empty(), "used_in_retail drift: {flag_drift:?}");
}

/// NGP `.lsp` keys resolve through NGP's key tables (`lsp_keys_ngp`).
///
/// Runs only when `RBR_NGP` points at the NGP data tree (`original/ngp`:
/// `7.4.778/physics.rbz` plus `cars-rsf/`), like the `RBR_DATA` retail test
/// above. Parses NGP's `Physics/physics.lsp` from the zip plus every `.lsp`
/// under `cars-rsf/` and walks each file with the same [`Walker`], this
/// time resolving keys with [`lsp_keys_ngp::ngp_key_offset`] and allowing
/// the DLL's material records.
#[test]
fn all_ngp_lsp_keys_resolve_or_are_ngp() {
    let Ok(ngp_dir) = std::env::var("RBR_NGP") else {
        eprintln!("skipping: RBR_NGP not set");
        return;
    };
    let ngp = Path::new(&ngp_dir);
    let mut files: Vec<(String, String)> = Vec::new();
    let zip_path = ngp.join("7.4.778/physics.rbz");
    let mut archive = rbz::open(&zip_path).unwrap_or_else(|e| panic!("{}: {e}", zip_path.display()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        let entry_name = entry.name().expect("ascii name").into_owned();
        if entry_name != "Physics/physics.lsp" {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read physics.lsp from zip");
        files.push((format!("physics.rbz!{entry_name}"), String::from_utf8(bytes).expect("physics.lsp is ASCII")));
    }
    let cars_dir = ngp.join("cars-rsf");
    let mut car_dirs: Vec<PathBuf> = std::fs::read_dir(&cars_dir)
        .unwrap_or_else(|e| panic!("{}: {e}", cars_dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    car_dirs.sort();
    let cars = car_dirs.len();
    for car in &car_dirs {
        let mut stack = vec![car.clone()];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("read_dir") {
                    stack.push(entry.expect("dir entry").path());
                }
                continue;
            }
            if path.extension().is_some_and(|e| e == "lsp") {
                files.push((
                    path.strip_prefix(ngp).expect("car file under RBR_NGP").display().to_string(),
                    std::fs::read_to_string(&path).expect("read .lsp"),
                ));
            }
        }
    }
    files.sort();
    assert_eq!(cars, 42, "expected 42 NGP car dirs");
    // 42 cars x 10 .lsp + Physics/physics.lsp from the zip.
    assert_eq!(files.len(), 421, "expected all NGP .lsp files");

    let mut failures = Vec::new();
    let mut flag_names: BTreeSet<String> = BTreeSet::new();
    let mut renamed: BTreeSet<String> = BTreeSet::new();
    for (name, text) in &files {
        let doc = lsp::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(doc.len(), 1, "{name}: expected one top-level list");
        let Node::List(root) = &doc[0] else {
            panic!("{name}: expected one top-level list");
        };
        let root = match &root[..] {
            [Node::List(inner)] => inner,
            _ => panic!("{name}: expected single root list"),
        };
        let mut top = Vec::new();
        let mut j = 0;
        while j < root.len() {
            if let Node::Atom(k) = &root[j] {
                if j + 1 < root.len() && matches!(root[j + 1], Node::List(_)) {
                    top.push(k.clone());
                }
            }
            j += 1;
        }
        let shape = classify(&top);
        let body_start = match root.first() {
            Some(Node::Quoted(_)) => 1,
            _ => 0,
        };
        let classes = if shape == Shape::FlatPhys || shape == Shape::Surfaces { vec![Class::PhysDef] } else { vec![] };
        let mut walker = Walker {
            shape,
            classes,
            failures: Vec::new(),
            flag_names: BTreeSet::new(),
            seen: BTreeSet::new(),
            extra_surfaces: Some(
                rbr_formats::ngp::current()
                    .expect("NGP tables")
                    .keys
                    .iter()
                    .filter(|k| k.class == Class::MaterialManager)
                    .map(|k| k.name.clone())
                    .collect(),
            ),
            ngp: true,
            renamed: BTreeSet::new(),
        };
        walker.walk_body(&root[body_start..], name, "");
        failures.extend(walker.failures);
        renamed.extend(walker.renamed);
        flag_names.extend(walker.flag_names);
    }

    // NGP's files still use two stock keys NGP renamed (`SubTics` -> `reserved_NGP`, `vecDriveAxleJointPos_VF` ->
    // `vecPosOffset_NGP`): under NGP they don't parse, and their members keep the defaults.
    assert_eq!(renamed.iter().map(String::as_str).collect::<Vec<_>>(), ["SubTics", "vecDriveAxleJointPos_VF"]);
    let known = known_bits();
    let mut bad_bits: Vec<&String> =
        flag_names.iter().filter(|b| !known.contains(*b) && !TOLERATED_BITS.contains(&b.as_str())).collect();
    bad_bits.sort();
    for b in &bad_bits {
        failures.push(format!("unknown flag bit `{b}`"));
    }

    if !failures.is_empty() {
        failures.sort();
        panic!("{} unknown keys/bits/surfaces:\n{}", failures.len(), failures.join("\n"));
    }
}
