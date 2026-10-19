//! The car placements' handles, from the retail files and the exe's `.data`/`.rdata` (`lead/reports/car-handles.md`).
//!
//! The car load (0x46b160) takes each placement's handle from `CBinarySectionMap::Lookup(1, name)` (`"parent"`,
//! `e_wheel_<class>_lf`, `e_door_l`, ...). HedgeHog3D fills that map when it loads the car model: its
//! `CHH_NodeAnimationConnector::Visit` (HH 0x10034770 and the next ones) registers each built node's name with
//! LocoMotion's answer for it, `Vf28(Vf24(character, name))`. That is the node's last animation object
//! (`CLM_CharacterManager::FindNodeIndex` 0x10025ff0, `GetNodeUnkF8` 0x10026140): an index into
//! `CLM_ControllerManager`'s object list, which every `AddAnimationToCharacter` extends.
//!
//! - `CLM_Character::Unk1abb0` (LM 0x1001abb0) adds a clip's objects. It walks the character's node tree from its
//!   root on a stack: it pops the last node, adds its objects (a PRS node one, `VisitPRS` 0x10029230; a PS node two,
//!   `VisitPS` 0x100292c0; the root none) and pushes its children in order. The tree is the model's node table
//!   (`LoadNodes` 0x10014c20, each node under the earlier node whose id is its parent, `-1` under the root).
//! - Before the car the list holds the session's dummy object (`CLM_ControllerManager::Clear` 0x10039de0) and the
//!   car-flip spectators (`E03CarFlipInit` 0x445380, run before the car in every session). Those are five
//!   `load_car_flip<i>.lua` scripts from the rally's script folder, each one spectator model (26 PRS nodes in every
//!   retail one) times its clips: on the Hokkaido maps one clip each (`0x83` objects), elsewhere 2, 2, 2, 1, 1 (`0xd1`).
//!
//! Checked against every name the section map holds in 25 snapshots of the original (`handles_are_the_originals`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};

use rbr_formats::data::Data;
use rbr_formats::{rbz, sgs};
use rbr_physics::mem::Mem;

/// `g_e09CloudTable` (0x7c12e4): the tyre class (0 tarmac, 1 gravel, 2 snow) of each `StartGame` tyre.
const TYRE_CLASSES: u32 = 0x007C_12E4;
/// `E11_TyreLookup`'s tables (0x46f760): per tyre class four `(name, the class's name)` pairs, 0x20 bytes apart.
const WHEEL_NAMES: u32 = 0x007C_1388;
/// The maps `E03CarFlipInit` loads no flip spectators on (the `i32`s from 0x72ace0 to 0x72ad34).
const NO_FLIP_MAPS: (u32, u32) = (0x0072_ACE0, 0x0072_AD34);
/// `E01FrontendName` (0x43cd60): the script folder of each ten maps (`"arctic"` for 10-19). The game reads the table up
/// to index 0x2b, but its last rally entry is `rallyschool` (maps 70-79): after three empty ones come other `.data`
/// words (`LOAD_START_RACE` for maps 110-119, a null pointer for 160-169). Ours, as `rbr-render`'s characters: maps
/// past it (RSF stages) take no folder, the common scripts, where the game would read a string from those words.
const RALLY_FOLDERS: u32 = 0x007C_03DC;
const RALLY_FOLDER_LAST: u32 = 7;
/// `CLM_ControllerManager`'s dummy object, the first of every session.
const DUMMY_OBJECTS: u32 = 1;

/// Each node's first animation object counted from the character's first, in the order `Unk1abb0` adds them, and the
/// count. A name maps to its last node in the table (`AddNode`'s `m_nodeIndices[name] = index`); a PS node's name maps
/// to `None` (its handle is not traced). A node goes under the parent `LM_FindNodeByUnk48` (LM 0x1002a4a0) finds: the
/// first node with the parent's id in the same pop-last walk from the root, which matters when two nodes share an id
/// (community models: `evo_IX.sgc`'s `d_bonnet`).
fn anim_objects(table: &sgs::Hierarchy) -> Result<(u32, HashMap<&str, Option<u32>>), String> {
    // The root is node `None` (id -1); `children[0]` holds its children, `children[k + 1]` node k's.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); table.nodes.len() + 1];
    let find = |children: &[Vec<usize>], id: i32| -> Option<usize> {
        if id == -1 {
            return Some(0);
        }
        let mut stack = children[0].clone();
        while let Some(k) = stack.pop() {
            if table.nodes[k].id == id {
                return Some(k + 1);
            }
            stack.extend(&children[k + 1]);
        }
        None
    };
    for (k, node) in table.nodes.iter().enumerate() {
        let parent = find(&children, node.parent)
            .ok_or(format!("node {}: its parent {} is not an earlier node", node.name, node.parent))?;
        children[parent].push(k);
    }
    let mut first = vec![0; table.nodes.len()];
    let mut count = 0;
    let mut stack = children[0].clone();
    while let Some(k) = stack.pop() {
        let node = &table.nodes[k];
        first[k] = count;
        count += match node.node_type {
            0x2f => 1,
            0x30 => 2,
            t => return Err(format!("node {}: type {t:#x} is neither PRS nor PS", node.name)),
        };
        stack.extend(&children[k + 1]);
    }
    let names = table.nodes.iter().zip(first).map(|(n, k)| (n.name.as_str(), (n.node_type == 0x2f).then_some(k)));
    Ok((count, names.collect()))
}

/// The car model's nodes' animation objects, counted from the car's first.
pub struct CarNodes(HashMap<String, Option<u32>>);

impl CarNodes {
    pub fn new(model: &[u8]) -> Result<CarNodes, String> {
        let table = sgs::node_table(model).map_err(|e| format!("the car's node table: {e}"))?;
        let (_, names) = anim_objects(&table)?;
        Ok(CarNodes(names.into_iter().map(|(name, k)| (name.to_owned(), k)).collect()))
    }

    /// The handle the car load looks up for `name` with `tyre` (`"e_wheel_lf"` becomes the tyre class's wheel), the
    /// objects before the car's being `before`; 0 for a name the model lacks.
    pub fn handle(&self, mem: &Mem, name: &str, tyre: u32, before: u32) -> Result<u32, String> {
        if tyre >= 7 {
            return Err(format!("tyre {tyre}: not a retail tyre"));
        }
        let names = WHEEL_NAMES + 0x20 * mem.u32(TYRE_CLASSES + 4 * tyre);
        let name = (0..4)
            .find(|k| c_string(mem, mem.u32(names + 8 * k)) == name)
            .map_or(name.to_owned(), |k| c_string(mem, mem.u32(names + 8 * k + 4)));
        match self.0.get(&name) {
            Some(Some(k)) => Ok(before + k),
            Some(None) => Err(format!("{name}: a PS node")),
            // `CBinarySectionMap::Lookup` answers 0 for a name the model did not register: RSF's Delta HF 4WD,
            // Evo II, Clio III R3 and Celica ST185 have no `e_exhaust`.
            None => Ok(0),
        }
    }
}

/// The animation objects a session holds before the car's on `map`: the dummy and the car-flip spectators'.
pub fn objects_before_car(data: &Data, mem: &Mem, map: u32) -> Result<u32, String> {
    let (lo, hi) = NO_FLIP_MAPS;
    if (lo..hi).step_by(4).any(|at| mem.u32(at) == map) {
        return Ok(DUMMY_OBJECTS);
    }
    let folder = match map / 10 {
        k if k > RALLY_FOLDER_LAST => String::new(),
        k => c_string(mem, mem.u32(RALLY_FOLDERS + 4 * k)),
    };
    let mut scripts = Scripts::open(data)?;
    let mut objects = DUMMY_OBJECTS;
    for i in 1..=5 {
        // `E01ResolveA` (0x43cdd0): the rally's folder's script, else the common one.
        let file = format!("load_car_flip{i}.lua");
        let Some(text) =
            scripts.read(&format!("scripts/{folder}/{file}")).or_else(|| scripts.read(&format!("scripts/{file}")))
        else {
            continue;
        };
        objects += flip_objects(data, &text).map_err(|e| format!("{file} ({folder}): {e}"))?;
    }
    Ok(objects)
}

/// A flip script's objects: its clips (`AddAnimationToCharacter` calls) times its spectator model's objects. The
/// model is one of its `LoadCharacter` calls by `LuaRandom`, so they must all count the same.
fn flip_objects(data: &Data, script: &str) -> Result<u32, String> {
    let code = script.lines().map(|line| line.split("--").next().unwrap_or_default());
    let (mut models, mut clips) = (Vec::new(), 0);
    for line in code {
        if line.contains("app:LoadCharacter(") {
            let model = line.split('"').nth(1).ok_or(format!("no model in {line:?}"))?;
            let bytes = std::fs::read(data.resolve(model).ok_or(format!("{model}: not found"))?)
                .map_err(|e| format!("{model}: {e}"))?;
            let table = sgs::node_table(&bytes).map_err(|e| format!("{model}: {e}"))?;
            models.push(anim_objects(&table)?.0);
        }
        clips += line.matches("app:AddAnimationToCharacter(").count() as u32;
    }
    match models.split_first() {
        Some((&first, rest)) if rest.iter().all(|&n| n == first) => Ok(clips * first),
        Some(_) => Err("its spectator models count different objects".into()),
        None => Err("no spectator model".into()),
    }
}

/// `Scripts.rbz`: the patch's (the game's root) if it has one, else the install's.
struct Scripts {
    zip: rbz::ZipArchive<BufReader<File>>,
    names: HashMap<String, String>,
}

impl Scripts {
    fn open(data: &Data) -> Result<Scripts, String> {
        let path =
            data.resolve("Scripts.rbz").or_else(|| data.resolve("Root/Scripts.rbz")).ok_or("Scripts.rbz: not found")?;
        let zip = rbz::open(&path).map_err(|e| format!("Scripts.rbz: {e}"))?;
        let names = zip.file_names().map(|n| n.map(|n| (n.to_ascii_lowercase(), n.into_owned())));
        let names = names.collect::<Result<_, _>>().map_err(|e| format!("Scripts.rbz: {e}"))?;
        Ok(Scripts { zip, names })
    }

    /// The text of `path`, its case ignored.
    fn read(&mut self, path: &str) -> Option<String> {
        let name = self.names.get(&path.to_ascii_lowercase())?;
        let mut text = String::new();
        self.zip.by_name(name).ok()?.read_to_string(&mut text).ok()?;
        Some(text)
    }
}

fn c_string(mem: &Mem, at: u32) -> String {
    (at..).map(|a| mem.u8(a)).take_while(|&b| b != 0).map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use rbr_formats::{cars, pe};

    use super::*;
    use crate::boot::CAR_PARTS;

    /// The retail data with the 1.02 patch (`RBR_DATA`, `RBR_PATCH`), and the exe's `.rdata` and `.data`.
    fn retail() -> Option<(Data, Mem)> {
        let data = Data::from_env().filter(|d| d.patch().is_some())?;
        let exe = std::fs::read(data.resolve("RichardBurnsRally_SSE.exe")?).unwrap();
        let exe = pe::parse(&exe).unwrap();
        let mut mem = Mem::new();
        for name in [".rdata", ".data"] {
            let section = exe.section(name).unwrap();
            mem.map(section.address, &section.bytes);
        }
        Some((data, mem))
    }

    /// Each retail car's nodes (`Cars/Cars.ini`'s models).
    fn car_nodes(data: &Data) -> Vec<CarNodes> {
        let ini = std::fs::read_to_string(data.resolve("Cars/Cars.ini").unwrap()).unwrap();
        let mut cars = cars::parse(&ini).unwrap();
        cars.sort_by_key(|c| c.index);
        let models = cars.iter().map(|c| std::fs::read(data.resolve(&c.file_name).unwrap()).unwrap());
        models.map(|m| CarNodes::new(&m).unwrap()).collect()
    }

    fn handles(mem: &Mem, nodes: &CarNodes, tyre: u32, before: u32) -> Vec<u32> {
        let names = CAR_PARTS.iter().filter_map(|p| p.node);
        names.map(|name| nodes.handle(mem, name, tyre, before).unwrap()).collect()
    }

    /// The section map's values for [`CAR_PARTS`]' names but the car body's, read from snapshots of the original
    /// (`g_e14Sig63c`, 0x89363c, at step 1000 or later): drive, map, car, tyre, handles. Every car, tyre class and rally
    /// folder; v3-snow-accent-15 is a car and class no other drive has (`cachy:rbr-harness-runs`).
    const ORIGINAL: [(&str, u32, u32, u32, [u32; 12]); 25] = [
        (
            "v3-gravel-lancer-23",
            23,
            3,
            4,
            [0xd5, 0x12d, 0x191, 0x12b, 0x12f, 0x161, 0x159, 0x175, 0x150, 0x16d, 0x172, 0x155],
        ),
        ("tanner-corolla", 54, 6, 4, [0x87, 0xd7, 0x146, 0xd9, 0xdb, 0x116, 0x10c, 0x12e, 0xfc, 0x126, 0x129, 0x100]),
        (
            "v2-gravel-206-64",
            64,
            4,
            4,
            [0xd5, 0x1b0, 0x14b, 0x14d, 0x143, 0x178, 0x16f, 0x190, 0x16d, 0x186, 0x18b, 0x169],
        ),
        (
            "v2-gravel-accent-54",
            54,
            1,
            4,
            [0x88, 0xd6, 0xd4, 0x14d, 0xd2, 0x11e, 0x104, 0x133, 0x102, 0x129, 0x12e, 0x11a],
        ),
        (
            "v2-gravel-corolla-21",
            21,
            6,
            4,
            [0xd5, 0x125, 0x194, 0x127, 0x129, 0x164, 0x15a, 0x17c, 0x14a, 0x174, 0x177, 0x14e],
        ),
        (
            "v2-gravel-impreza00-63",
            63,
            7,
            4,
            [0xd5, 0x127, 0x129, 0x125, 0x1a8, 0x15c, 0x154, 0x16e, 0x152, 0x166, 0x169, 0x146],
        ),
        (
            "v2-gravel-impreza03-24",
            24,
            5,
            4,
            [0xd5, 0x179, 0x144, 0x146, 0x142, 0x197, 0x18f, 0x1ad, 0x13b, 0x1a3, 0x1a7, 0x185],
        ),
        (
            "v2-gravel-impreza03-33",
            33,
            5,
            4,
            [0xd5, 0x179, 0x144, 0x146, 0x142, 0x197, 0x18f, 0x1ad, 0x13b, 0x1a3, 0x1a7, 0x185],
        ),
        (
            "v2-gravel-lancer-52",
            52,
            3,
            4,
            [0x87, 0xdf, 0x143, 0xdd, 0xe1, 0x113, 0x10b, 0x127, 0x102, 0x11f, 0x124, 0x107],
        ),
        (
            "v2-gravel-xsara-22",
            22,
            0,
            4,
            [0xd5, 0x1a3, 0x12c, 0x130, 0x12e, 0x174, 0x16a, 0x187, 0x154, 0x17e, 0x182, 0x15e],
        ),
        (
            "v2-gravel-zr-25",
            25,
            2,
            4,
            [0xd5, 0x11e, 0x1b8, 0x11c, 0x120, 0x17f, 0x176, 0x197, 0x174, 0x18f, 0x192, 0xe2],
        ),
        (
            "v2-snow-corolla-14",
            14,
            6,
            6,
            [0xd5, 0x123, 0x121, 0x196, 0x11f, 0x164, 0x15a, 0x17c, 0x14a, 0x174, 0x177, 0x14e],
        ),
        (
            "v2-snow-lancer-13",
            13,
            3,
            6,
            [0xd5, 0x133, 0x129, 0x193, 0x123, 0x161, 0x159, 0x175, 0x150, 0x16d, 0x172, 0x155],
        ),
        (
            "v2-snow-xsara-10",
            10,
            0,
            6,
            [0xd5, 0x1a1, 0x126, 0x12a, 0x128, 0x174, 0x16a, 0x187, 0x154, 0x17e, 0x182, 0x15e],
        ),
        (
            "v2-tarmac-206-46",
            46,
            4,
            0,
            [0xd5, 0x1ae, 0x141, 0x151, 0x149, 0x178, 0x16f, 0x190, 0x16d, 0x186, 0x18b, 0x169],
        ),
        (
            "v2-tarmac-accent-44",
            44,
            1,
            0,
            [0xd6, 0x132, 0x12c, 0x130, 0x12e, 0x16c, 0x152, 0x181, 0x150, 0x177, 0x17c, 0x168],
        ),
        (
            "v2-tarmac-impreza00-45",
            45,
            7,
            0,
            [0xd5, 0x12f, 0x12b, 0x131, 0x12d, 0x15c, 0x154, 0x16e, 0x152, 0x166, 0x169, 0x146],
        ),
        (
            "v2-tarmac-zr-42",
            42,
            2,
            0,
            [0xd5, 0x12e, 0x128, 0x12c, 0x12a, 0x17f, 0x176, 0x197, 0x174, 0x18f, 0x192, 0xe2],
        ),
        (
            "v1-gravel-impreza03",
            62,
            5,
            4,
            [0xd5, 0x179, 0x144, 0x146, 0x142, 0x197, 0x18f, 0x1ad, 0x13b, 0x1a3, 0x1a7, 0x185],
        ),
        (
            "v1-snow-xsara",
            12,
            0,
            6,
            [0xd5, 0x1a1, 0x126, 0x12a, 0x128, 0x174, 0x16a, 0x187, 0x154, 0x17e, 0x182, 0x15e],
        ),
        ("v1-tarmac-zr", 43, 2, 0, [0xd5, 0x12e, 0x128, 0x12c, 0x12a, 0x17f, 0x176, 0x197, 0x174, 0x18f, 0x192, 0xe2]),
        ("arctic-snow", 13, 6, 6, [0xd5, 0x123, 0x121, 0x196, 0x11f, 0x164, 0x15a, 0x17c, 0x14a, 0x174, 0x177, 0x14e]),
        (
            "tarmac-lancer",
            41,
            3,
            0,
            [0xd5, 0x135, 0x127, 0x131, 0x125, 0x161, 0x159, 0x175, 0x150, 0x16d, 0x172, 0x155],
        ),
        ("tanner-damage", 54, 6, 4, [0x87, 0xd7, 0x146, 0xd9, 0xdb, 0x116, 0x10c, 0x12e, 0xfc, 0x126, 0x129, 0x100]),
        (
            "v3-snow-accent-15",
            15,
            1,
            6,
            [0xd6, 0x126, 0x199, 0x128, 0x12a, 0x16c, 0x152, 0x181, 0x150, 0x177, 0x17c, 0x168],
        ),
    ];

    #[test]
    fn handles_are_the_originals() {
        let Some((data, mem)) = retail() else {
            eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
            return;
        };
        let cars = car_nodes(&data);
        for (drive, map, car, tyre, want) in ORIGINAL {
            let before = objects_before_car(&data, &mem, map).unwrap();
            assert_eq!(handles(&mem, &cars[car as usize], tyre, before), want, "{drive}");
        }
    }

    /// Every retail car on every stage's map with every tyre derives 12 distinct handles.
    #[test]
    fn every_car_map_and_tyre_derives() {
        let Some((data, mem)) = retail() else {
            eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
            return;
        };
        let cars = car_nodes(&data);
        let maps_dir = data.resolve("Maps").unwrap();
        let mut maps: Vec<u32> = std::fs::read_dir(maps_dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap().to_ascii_lowercase();
                name.strip_prefix("track-")?.strip_suffix(".lbs")?.split_once('_')?.0.parse().ok()
            })
            .collect();
        maps.sort_unstable();
        maps.dedup();
        assert_eq!(maps.len(), 58, "the retail stages' maps");
        let mut bases = std::collections::BTreeMap::new();
        for &map in &maps {
            let before = objects_before_car(&data, &mem, map).unwrap();
            *bases.entry(before).or_insert(0) += 1;
            for nodes in &cars {
                for tyre in 0..7 {
                    let mut h = handles(&mem, nodes, tyre, before);
                    h.sort_unstable();
                    h.dedup();
                    assert_eq!(h.len(), 12, "map {map} tyre {tyre}");
                }
            }
        }
        // The skipped maps (4, 68, 69, 72-89), Hokkaido and the rally school (root scripts), the other rallies.
        assert_eq!(bases, [(1, 21), (0x83, 7), (0xd1, 30)].into());
    }
}
