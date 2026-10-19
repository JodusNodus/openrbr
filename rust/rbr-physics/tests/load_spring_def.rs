//! The left-front spring-damper definition built from scratch, as the load builds it, against the game's.
//!
//! `vectors/load-spring-def.txt` holds the definition (0x11c bytes) and its name from the snap-0 dump of a Corolla
//! gravel run (`harness/load_refs.py`). The game's load constructs the definition (0x52d750), applies the
//! `SpringDamper` group of `Physics/t_coroll/gravel.lsp`'s `CAR_WHEEL_LF` (`NTweak::CBase::Parse`), then the setup
//! layer copies nine values of `setups/d_gravel.lsp`'s `SpringDamperLF` into it (0x52dbd0). This test does the same
//! with the ports, reading the vtables from the retail exe. Runs only when `RBR_DATA` and `RBR_PATCH` are set.

use rbr_formats::data::Data;
use rbr_formats::lsp::{self, Node};
use rbr_formats::lsp_keys::Class;
use rbr_formats::{pe, rbz};
use rbr_physics::mem::Mem;
use rbr_physics::ntweak::{self, ApplyHost};
use rbr_physics::spring_damper::layout;
use rbr_physics::Variant;
use std::io::Read;

const VECTORS: &str = include_str!("vectors/load-spring-def.txt");

/// Where the test puts the setup object, the controller word that points at the definition, and the names.
const SETUP: u32 = 0x1000_0000;
const CONTROLLER: u32 = 0x1000_1000;
const NAMES: u32 = 0x1000_2000;
/// `NTweak::CReal`'s vtable: the setup's nine members.
const REAL_VTABLE: u32 = 0x73b644;

struct Names(u32);

impl ApplyHost for Names {
    fn alloc_name(&mut self, _: &mut Mem, len: u32) -> u32 {
        self.0 += len.next_multiple_of(0x10);
        self.0 - len.next_multiple_of(0x10)
    }

    /// The definition's slot 17 is 0x52f480, which returns; the setup's is not called on its values.
    fn after_parse(&mut self, _: &mut Mem, _: u32) {}

    /// The groups here link no definitions.
    fn def_index(&mut self, _: &Mem, _: &str) -> Option<u32> {
        None
    }

    /// No definition manager walks here.
    fn define(&mut self, _: &mut Mem, _: &str, _: u32, _: &str, _: &[Node]) -> Result<(), ntweak::Unsupported> {
        unreachable!("no definition manager here")
    }
}

/// The items of the group after the atom `key` inside the list that starts with the quoted `owner` (anywhere when
/// `owner` is `None`).
fn group<'a>(items: &'a [Node], owner: Option<&str>, key: &str) -> Option<&'a [Node]> {
    let here = match owner {
        None => true,
        Some(o) => matches!(items.first(), Some(Node::Quoted(q)) if q == o),
    };
    if here {
        let found = items.windows(2).find_map(|w| match w {
            [Node::Atom(a), Node::List(g)] if a == key => Some(g.as_slice()),
            _ => None,
        });
        if found.is_some() {
            return found;
        }
    }
    items.iter().find_map(|n| match n {
        Node::List(inner) => group(inner, owner, key),
        _ => None,
    })
}

fn lsp_file(data: &Data, name: &str) -> Vec<Node> {
    let path = data.resolve("Root/physics.rbz").expect("Root/physics.rbz");
    let mut archive = rbz::open(&path).unwrap();
    let mut text = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    lsp::parse(&text).unwrap()
}

fn reference() -> Vec<(u32, Vec<u8>)> {
    VECTORS
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let f: Vec<_> = l.split(' ').collect();
            let at = u32::from_str_radix(f[1].trim_start_matches("0x"), 16).unwrap();
            let bytes = (0..f[2].len()).step_by(2).map(|i| u8::from_str_radix(&f[2][i..i + 2], 16).unwrap()).collect();
            (at, bytes)
        })
        .collect()
}

#[test]
fn left_front_definition_matches_the_game() {
    let Some(data) = Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let exe = data.resolve("RichardBurnsRally_SSE.exe").expect("exe");
    let image = pe::parse(&std::fs::read(exe).unwrap()).unwrap();
    let mut mem = Mem::new();
    let rdata = image.section(".rdata").expect(".rdata");
    mem.map(rdata.address, &rdata.bytes);

    let [(def, expected), (_, name)] = &reference()[..] else {
        panic!("vectors: the definition, then its name");
    };
    mem.map(*def, &vec![0; expected.len()]);
    layout::construct_definition(&mut mem, *def);

    let mut names = Names(NAMES);
    let car = lsp_file(&data, "Physics/t_coroll/gravel.lsp");
    let spring = group(&car, Some("CAR_WHEEL_LF"), "SpringDamper").expect("CAR_WHEEL_LF SpringDamper");
    ntweak::apply(&mut mem, &mut names, Variant::Stock102, *def, Class::SpringDef, spring).unwrap();

    mem.map(SETUP, &[0; 0x9c]);
    for at in (0x0c..0x9c).step_by(0x10) {
        mem.set_u32(SETUP + at, REAL_VTABLE);
    }
    let setup = lsp_file(&data, "Physics/t_coroll/setups/d_gravel.lsp");
    let lf = group(&setup, None, "SpringDamperLF").expect("SpringDamperLF");
    ntweak::apply(&mut mem, &mut names, Variant::Stock102, SETUP, Class::SpringSetup, lf).unwrap();
    mem.set_u32(CONTROLLER + 4, *def);
    layout::apply_setup(&mut mem, CONTROLLER, SETUP);

    // The name pointer is the game's heap address; the name itself is compared.
    let ours = mem.read(*def, expected.len());
    assert_eq!(mem.read(mem.u32(*def + 4), name.len()), *name);
    let differing: Vec<_> = (0..expected.len())
        .step_by(4)
        .filter(|&i| i != 4 && ours[i..i + 4] != expected[i..i + 4])
        .map(|i| format!("+{i:#x}: ours {:02x?} the game's {:02x?}", &ours[i..i + 4], &expected[i..i + 4]))
        .collect();
    assert!(differing.is_empty(), "{}", differing.join("\n"));
}
