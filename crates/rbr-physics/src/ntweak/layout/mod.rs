//! The NTweak walk and tweak constructors on game memory (`super` describes the walk).

use rbr_formats::lsp::Node;
use rbr_formats::lsp_keys::{Class, KeyInfo};
use rbr_formats::lsp_keys_ngp::ngp_key_info;

use super::{atof, atoi, get_quoted, is_float, is_integer, set_bit, Unsupported};

use crate::math::vec::clear_vec4;
use crate::mem::Mem;
use crate::physics_core::layout::{construct as physics, flags, material};

mod parse;

pub use parse::{parse, stack_alloc, tokenize, Frame, ParseHost, EMPTY_TOKENS, TOKEN_HEAP};

/// The members' parse functions (vtable slot 2), by which the walk tells them apart.
pub const REAL_PARSE: u32 = 0x52f770;
pub const UNSIGNED_PARSE: u32 = 0x52f840;
pub const INTEGER_PARSE: u32 = 0x52fab0;
pub const VECTOR_PARSE: u32 = 0x52fb20;
pub const VECTOR4_PARSE: u32 = 0x52fbe0;
pub const BIT_FIELD_PARSE: u32 = 0x52f930;
/// `NTweak::CBase::Parse` itself: a member object with keys of its own.
pub const BASE_PARSE: u32 = 0x52f0c0;
/// Under NGP, the vtable of `CDrive::CDef +0x58c` (`BrakeWearLimit_NGP`): NGP's copy of `CUnsigned`'s (0x740534) with
/// its own reader (`0x1003c7f0`), which `0x1003c6c0` makes and puts on the unsigned (NGP's call in `E39_Ctor870`,
/// 0x50291b). The word is the copy's address in NGP's file; NGP's memory is not mapped, so the walk tells the reader by
/// the vtable itself.
pub const NGP_TABLE_VTABLE: u32 = 0x1017_6518;
/// The 30 floats NGP's reader takes after the unsigned, into the words from the member's +0x10 (1.02's table at
/// `CDrive::CDef +0x59c`, which no 1.02 key reads): three rows of ten.
const NGP_TABLE: u32 = 0x10;
const NGP_TABLE_LEN: usize = 30;
/// The objects' slot 5 (`Unk14`), which takes the walk's inner groups: `CBase`'s skips each item; the entity
/// definitions' (`E43PairWalk::MF8B0`) links the group's name; the definition manager's (`CDefManager::Vf14`) makes or
/// finds the definition the item names and walks the item on it.
const UNK14_SKIP: u32 = 0x52fe80;
const UNK14_LINK: u32 = 0x5198b0;
const UNK14_DEFINE: u32 = 0x501ef0;
/// The token-heap block each item of an inner group is copied into (`GetQuoted(tmp, 0x20)`).
const ITEM_BLOCK: u32 = 0x20;
/// An entity definition's links: their count, then the registry indices.
const LINK_COUNT: u32 = 0x18;
const LINKS: u32 = 0x1c;

/// A tweak value's payload, after its `CBase` header (vtable, name, dirty flag); a vector's is 16-byte aligned. A
/// bit-field's mask is its value.
const VALUE: u32 = 0x0c;
const VECTOR_VALUE: u32 = 0x10;

/// The bit-field classes by vtable: their names (`GetBitName`'s table) and `GetBitCount`.
const BIT_FIELDS: [(u32, u32, u32); 4] = [
    (physics::FLAGS_VTABLE, flags::CDEF_NAMES, flags::CDEF_BITS),
    (physics::DBG_FLAGS_VTABLE, flags::DBG_NAMES, flags::DBG_BITS),
    (physics::LOG_FLAGS_VTABLE, flags::LOG_NAMES, flags::LOG_BITS),
    (material::FLAGS_VTABLE, flags::MATERIAL_NAMES, flags::MATERIAL_BITS),
];

/// The calls the walk makes outside the object.
pub trait ApplyHost {
    /// `E11_StackAlloc(len, g_tokenHeap, 0x10)`: `len` bytes for the object's name.
    fn alloc_name(&mut self, mem: &mut Mem, len: u32) -> u32;
    /// The object's slot 17 (`Unk44`), after its items.
    fn after_parse(&mut self, mem: &mut Mem, this: u32);
    /// `CDictBase::Find` on the definition registry (0x7c8040): the index of the definition named `name`.
    fn def_index(&mut self, mem: &Mem, name: &str) -> Option<u32>;
    /// The definition manager's slot 5 (`CDefManager::Vf14`, 0x501ef0) on one item of a group the walk hands it:
    /// `factory`'s definition named `name` (copied to the token-heap block `block`), found in the registry or made,
    /// then the item's `group` walked on it.
    fn define(
        &mut self,
        mem: &mut Mem,
        factory: &str,
        block: u32,
        name: &str,
        group: &[Node],
    ) -> Result<(), Unsupported>;
}

fn atom(item: Option<&Node>) -> Option<&str> {
    match item {
        Some(Node::Atom(t)) => Some(t),
        _ => None,
    }
}

/// One effect of the walk, in the original's order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action<'a> {
    /// A copy of the name on the token heap; the object's `+0x04` points at it.
    Name(u32, String),
    /// A member's value word (address, value).
    Store(u32, u32),
    /// A token-heap block for an inner group's item, holding what `GetQuoted` takes from the item.
    Item(String),
    /// The registry index of the name, appended to the object's links when the registry has the name.
    Link(u32, String),
    /// The definition manager's slot 5 on an item: the factory, the definition's name and the item's group. The name's
    /// block is the last item's.
    Define(&'a str, String, &'a [Node]),
    /// The object's slot 17.
    After(u32),
}

/// The floats from item `i` on, up to `n`, as words from `at`: what a `CReal`, `CVector` or `CVector4` reader takes.
/// Returns the item after the last one taken.
fn floats(items: &[Node], i: usize, n: usize, at: u32, out: &mut Vec<Action>) -> usize {
    let taken = (0..n)
        .map_while(|k| atom(items.get(i + k)).filter(|v| is_float(v)))
        .enumerate()
        .map(|(k, v)| Action::Store(at + 4 * k as u32, atof(v).to_bits()));
    let before = out.len();
    out.extend(taken);
    i + out.len() - before
}

/// NGP's reader on the unsigned at `member` (`0x1003c7f0`): `CUnsigned::Parse`, whose result it ignores, then up to
/// [`NGP_TABLE_LEN`] floats into the table, stopping at the first item that is not one. Returns the item after the last
/// one taken.
fn ngp_table(items: &[Node], i: usize, member: u32, out: &mut Vec<Action>) -> usize {
    let i = match atom(items.get(i)).filter(|v| is_integer(v)) {
        Some(v) => {
            out.push(Action::Store(member + VALUE, atoi(v)));
            i + 1
        }
        None => i,
    };
    floats(items, i, NGP_TABLE_LEN, member + NGP_TABLE, out)
}

/// The NUL-terminated string at `at`.
fn c_string(mem: &Mem, at: u32) -> String {
    let mut out = Vec::new();
    while let Some(c) = Some(mem.u8(at + out.len() as u32)).filter(|&c| c != 0) {
        out.push(c);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The word at `at` as the walk has left it so far: its last store, else memory's.
fn current(mem: &Mem, out: &[Action], at: u32) -> u32 {
    out.iter()
        .rev()
        .find_map(|a| match a {
            Action::Store(to, value) if *to == at => Some(*value),
            _ => None,
        })
        .unwrap_or_else(|| mem.u32(at))
}

/// `CBitField::Parse` (0x52f930) on the bit-field at `member`: the items from `i` on that start with `+` or `-`, each
/// setting or clearing its bit in the mask. Returns the item after the last one taken.
fn bit_field(mem: &Mem, member: u32, items: &[Node], i: usize, out: &mut Vec<Action>) -> Result<usize, Unsupported> {
    let vtable = mem.u32(member);
    let Some(&(_, table, count)) = BIT_FIELDS.iter().find(|(v, _, _)| *v == vtable) else {
        return Err(Unsupported("a bit-field of a class not ported"));
    };
    let names: Vec<String> = (0..count).map(|k| c_string(mem, flags::bit_name(mem, table, k))).collect();
    let at = member + VALUE;
    let mut mask = current(mem, out, at);
    let mut next = i;
    while let Some(item) = atom(items.get(next)).filter(|t| t.starts_with(['+', '-'])) {
        mask = set_bit(mask, &names, item);
        next += 1;
    }
    if next > i {
        out.push(Action::Store(at, mask));
    }
    Ok(next)
}

/// The key record of `key` in `class` from NGP's tables (its renamed and appended keys, `notes/ngp.md` "Load path").
fn lookup(class: Class, key: &str) -> Option<KeyInfo> {
    ngp_key_info(class, key)
}

/// The member a key names in the object at `this`: at its offset, or in the material manager the material of its tag
/// (the manager's key words are `88 * tag + 3`; the key tables hold the tag).
fn member_at(this: u32, info: &KeyInfo) -> u32 {
    match info.class {
        Class::MaterialManager => material::entry(this, info.offset),
        _ => this + info.offset,
    }
}

/// One step of the walk at item `i`: the item to go on at, or `None` when the walk ends.
fn step<'a>(
    mem: &Mem,
    this: u32,
    class: Class,
    items: &'a [Node],
    i: usize,
    out: &mut Vec<Action<'a>>,
) -> Result<Option<usize>, Unsupported> {
    let t = match &items[i] {
        Node::Atom(t) => t,
        Node::Quoted(_) => return Ok(Some(i + 1)),
        Node::List(group) => {
            inner_group(mem, this, group, out)?;
            return Ok(Some(i + 1));
        }
    };
    let Some(info) = (!is_float(t)).then(|| lookup(class, t)).flatten() else {
        return Ok(Some(i + 1));
    };
    let i = i + 1;
    if i == items.len() {
        return Ok(None);
    }
    let member = member_at(this, &info);
    if mem.u32(member) == NGP_TABLE_VTABLE {
        return Ok(Some(ngp_table(items, i, member, out)));
    }
    Ok(Some(match mem.u32(mem.u32(member) + 8) {
        REAL_PARSE => floats(items, i, 1, member + VALUE, out),
        VECTOR_PARSE => floats(items, i, 3, member + VECTOR_VALUE, out),
        VECTOR4_PARSE => floats(items, i, 4, member + VECTOR_VALUE, out),
        UNSIGNED_PARSE | INTEGER_PARSE => match atom(items.get(i)).filter(|v| is_integer(v)) {
            Some(v) => {
                out.push(Action::Store(member + VALUE, atoi(v)));
                i + 1
            }
            None => i,
        },
        BIT_FIELD_PARSE => bit_field(mem, member, items, i, out)?,
        BASE_PARSE => match (&items[i], info.child) {
            (Node::List(group), Some(child)) => {
                walk(mem, member, child, group, out)?;
                i + 1
            }
            _ => return Err(Unsupported("a member object without a group of a known class")),
        },
        _ => return Err(Unsupported("a member whose reader is not ported")),
    }))
}

/// A group inside the group, handed to the object's slot 5 (`Unk14`) once per item after its leading quoted name, each
/// item's quoted name first copied to a token-heap block. The original takes a group token only when it holds
/// whitespace; a group of one item may have none, so the walk declines it.
fn inner_group<'a>(mem: &Mem, this: u32, group: &'a [Node], out: &mut Vec<Action<'a>>) -> Result<(), Unsupported> {
    let [Node::Quoted(name), items @ ..] = group else {
        return Err(Unsupported("an inner group without a name"));
    };
    if items.is_empty() {
        return Err(Unsupported("an inner group of one item"));
    }
    let unk14 = mem.u32(mem.u32(this) + 0x14);
    for item in items {
        let quoted = get_quoted(item, ITEM_BLOCK);
        out.push(Action::Item(quoted.clone()));
        match (unk14, item) {
            (UNK14_SKIP, _) => {}
            (UNK14_LINK, _) => out.push(Action::Link(this, name.clone())),
            (UNK14_DEFINE, Node::List(def)) => out.push(Action::Define(name, quoted, def)),
            (UNK14_DEFINE, _) => return Err(Unsupported("a definition that is not a group")),
            _ => return Err(Unsupported("an inner group of an object with its own Unk14")),
        }
    }
    Ok(())
}

/// The walk over one group: the name, the items, then slot 17. A member object's group (a `CBase` member, such as a
/// wheel's `SpringDamper`) is walked in place, as its own `Parse` does it.
fn walk<'a>(
    mem: &Mem,
    this: u32,
    class: Class,
    items: &'a [Node],
    out: &mut Vec<Action<'a>>,
) -> Result<(), Unsupported> {
    let mut i = match items.first() {
        Some(Node::Quoted(name)) => {
            out.push(Action::Name(this, name.clone()));
            1
        }
        _ => 0,
    };
    while i < items.len() {
        match step(mem, this, class, items, i, out)? {
            Some(next) => i = next,
            None => break,
        }
    }
    out.push(Action::After(this));
    Ok(())
}

/// What a group does to its object and its member objects, worked out before anything is written. A definition the
/// definition manager makes is walked when the plan is applied, as it does not exist before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<'a> {
    actions: Vec<Action<'a>>,
}

/// The walk over the group, or `Unsupported` for a group it does not cover.
pub fn plan<'a>(mem: &Mem, this: u32, class: Class, items: &'a [Node]) -> Result<Plan<'a>, Unsupported> {
    let mut actions = Vec::new();
    walk(mem, this, class, items, &mut actions)?;
    Ok(Plan { actions })
}

/// `text` and its NUL at `at`.
fn write_string(mem: &mut Mem, at: u32, text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    mem.map(at, &bytes);
}

impl Plan<'_> {
    /// The names, the members' values, the definitions and the slot-17 calls, in the walk's order. Fails only when a
    /// definition's own walk does.
    pub fn apply(self, mem: &mut Mem, host: &mut impl ApplyHost) -> Result<(), Unsupported> {
        let mut block = 0;
        for action in self.actions {
            match action {
                Action::Name(this, name) => {
                    let at = host.alloc_name(mem, name.len() as u32 + 1);
                    write_string(mem, at, &name);
                    mem.set_u32(this + 4, at);
                }
                Action::Store(at, value) => mem.set_u32(at, value),
                Action::Item(text) => {
                    block = host.alloc_name(mem, ITEM_BLOCK);
                    write_string(mem, block, &text);
                }
                Action::Link(this, name) => {
                    if let Some(index) = host.def_index(mem, &name) {
                        let count = mem.u32(this + LINK_COUNT);
                        mem.set_u32(this + LINKS + 4 * count, index);
                        mem.set_u32(this + LINK_COUNT, count + 1);
                    }
                }
                Action::Define(factory, name, group) => host.define(mem, factory, block, &name, group)?,
                Action::After(this) => host.after_parse(mem, this),
            }
        }
        Ok(())
    }
}

/// [`plan`], then [`Plan::apply`].
pub fn apply(
    mem: &mut Mem,
    host: &mut impl ApplyHost,
    this: u32,
    class: Class,
    items: &[Node],
) -> Result<(), Unsupported> {
    plan(mem, this, class, items)?.apply(mem, host)
}

/// `NTweak::CVector`'s vtable (0x7405ec) at +0, and its name word (0x740638) at +4.
pub const VECTOR_VTABLE: u32 = 0x0074_05EC;
pub(crate) const VECTOR_NAME: u32 = 0x0074_0638;
/// `NTweak::CReal`'s vtable (0x73b644) at +0, and its name word (0x740528) at +4.
pub const REAL_VTABLE: u32 = 0x0073_B644;
pub const REAL_NAME: u32 = 0x0074_0528;
/// The dirty flag of a tweak header, at +8.
const DIRTY: u32 = 0x08;

/// `NTweak::CBase::SetDirty` (0x52f450): sets the dirty flag of the tweak at `at` to 1.
pub fn set_dirty(mem: &mut Mem, at: u32) {
    mem.set_u32(at + DIRTY, 1);
}

/// `NTweak::CBase`'s vtable (0x72e444), the base of every tweak header.
pub const BASE_VTABLE: u32 = 0x0072_E444;
/// `NTweak::CVector4`'s vtable (0x740644) and name word (0x740690).
const VECTOR4_VTABLE: u32 = 0x0074_0644;
const VECTOR4_NAME: u32 = 0x0074_0690;
/// `NTweak::CUnsigned`'s vtable (0x740534) and name word (0x740580).
pub const UNSIGNED_VTABLE: u32 = 0x0074_0534;
pub const UNSIGNED_NAME: u32 = 0x0074_0580;

/// `NTweak::CBase::CBase(name)` (0x52f090) on the header at `at`: the base vtable, the name, and the dirty flag 0.
pub fn construct_base(mem: &mut Mem, at: u32, name: u32) {
    mem.set_u32(at, BASE_VTABLE);
    mem.set_u32(at + 4, name);
    mem.set_u32(at + DIRTY, 0);
}

/// `NTweak::CVector4::CVector4` (0x52fff0) on the tweak at `at`: the header, the vector at +0x10 zero except its fourth
/// lane (`1.0`, at +0x1c).
pub fn construct_vector4(mem: &mut Mem, at: u32) {
    mem.set_u32(at, VECTOR4_VTABLE);
    mem.set_u32(at + 4, VECTOR4_NAME);
    mem.set_u32(at + DIRTY, 0);
    mem.set_f32(at + 0x10, 0.0);
    mem.set_f32(at + 0x14, 0.0);
    mem.set_f32(at + 0x18, 0.0);
    mem.set_f32(at + 0x1C, 1.0);
}

/// `NTweak::CUnsigned::CUnsigned` (0x52fec0) on the tweak at `at`: the header, and the value 0 at +0xc.
pub fn construct_unsigned(mem: &mut Mem, at: u32) {
    mem.set_u32(at, UNSIGNED_VTABLE);
    mem.set_u32(at + 4, UNSIGNED_NAME);
    mem.set_u32(at + DIRTY, 0);
    mem.set_u32(at + VALUE, 0);
}

/// `NTweak::CInteger`'s vtable (0x740594) and name word (0x7405e0).
const INTEGER_VTABLE: u32 = 0x0074_0594;
const INTEGER_NAME: u32 = 0x0074_05E0;

/// `NTweak::CInteger::CInteger` (0x52ff20) on the tweak at `at`: the header, and the value 0 at +0xc.
pub fn construct_integer(mem: &mut Mem, at: u32) {
    mem.set_u32(at, INTEGER_VTABLE);
    mem.set_u32(at + 4, INTEGER_NAME);
    mem.set_u32(at + DIRTY, 0);
    mem.set_u32(at + VALUE, 0);
}

/// `NTweak::CReal::CReal` (0x52fea0) on the tweak at `at`: the header, and the value 0.0.
pub fn construct_real(mem: &mut Mem, at: u32) {
    mem.set_u32(at, REAL_VTABLE);
    mem.set_u32(at + 4, REAL_NAME);
    mem.set_u32(at + DIRTY, 0);
    mem.set_f32(at + VALUE, 0.0);
}

/// `NTweak::CVector::CVector` (0x52ff80) on the tweak at `at`: the header, and the zero vector at +0x10.
pub fn construct_vector(mem: &mut Mem, at: u32) {
    mem.set_u32(at, VECTOR_VTABLE);
    mem.set_u32(at + 4, VECTOR_NAME);
    mem.set_u32(at + DIRTY, 0);
    let zero = clear_vec4();
    mem.set_f32(at + 0x10, zero.x);
    mem.set_f32(at + 0x14, zero.y);
    mem.set_f32(at + 0x18, zero.z);
    mem.set_f32(at + 0x1C, zero.w);
}

/// `"(null)"`, the name `NTweak::CBase::GetNameOrNull` (0x52f0b0) returns for a null base (at 0x739520).
pub(crate) const NULL_NAME: u32 = 0x0073_9520;

/// `NTweak::CBase::GetNameOrNull(base)` (0x52f0b0): the base's name pointer (the word at +4), or `"(null)"` for a null base.
pub fn name_or_null(mem: &Mem, base: u32) -> u32 {
    if base == 0 {
        NULL_NAME
    } else {
        mem.u32(base + 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::lsp;

    const THIS: u32 = 0x1000;
    const NAME: u32 = 0x8000;
    const REAL_VTABLE: u32 = 0x4000;
    const UNSIGNED_VTABLE: u32 = 0x4100;

    #[derive(Default)]
    struct Host {
        after: Vec<u32>,
        defined: Vec<(String, u32, String, usize)>,
    }

    impl ApplyHost for Host {
        fn alloc_name(&mut self, _: &mut Mem, _: u32) -> u32 {
            NAME
        }

        fn after_parse(&mut self, _: &mut Mem, this: u32) {
            self.after.push(this);
        }

        fn def_index(&mut self, _: &Mem, name: &str) -> Option<u32> {
            (name == "CAR_WHEEL_LF").then_some(0x21)
        }

        fn define(
            &mut self,
            _: &mut Mem,
            factory: &str,
            block: u32,
            name: &str,
            group: &[Node],
        ) -> Result<(), Unsupported> {
            self.defined.push((factory.to_string(), block, name.to_string(), group.len()));
            Ok(())
        }
    }

    /// A spring-damper definition: `DebugID` (unsigned) at 0x0c, then reals from 0x1c, 0x10 apart.
    fn spring_def() -> Mem {
        let mut mem = Mem::new();
        mem.map(THIS, &[0; 0x120]);
        mem.set_u32(REAL_VTABLE + 8, REAL_PARSE);
        mem.set_u32(UNSIGNED_VTABLE + 8, UNSIGNED_PARSE);
        mem.set_u32(THIS + 0x0c, UNSIGNED_VTABLE);
        for at in (0x1c..0x11c).step_by(0x10) {
            mem.set_u32(THIS + at, REAL_VTABLE);
        }
        mem
    }

    fn group(text: &str) -> Vec<Node> {
        match lsp::parse(text).unwrap().remove(0) {
            Node::List(items) => items,
            _ => unreachable!(),
        }
    }

    #[test]
    fn names_the_object_and_stores_the_keys() {
        let mut mem = spring_def();
        let mut host = Host::default();
        let items = group("(\"SpringDamper\" InnerBumpRubberPos 0.17 Unknown 3 DebugID +2 12 SpringLength)");
        apply(&mut mem, &mut host, THIS, Class::SpringDef, &items).unwrap();
        assert_eq!(mem.read(NAME, 13), b"SpringDamper\0");
        assert_eq!(mem.u32(THIS + 4), NAME);
        assert_eq!(mem.f32(THIS + 0x1c + VALUE), 0.17);
        assert_eq!(mem.u32(THIS + 0x0c + VALUE), 2);
        assert_eq!(host.after, [THIS]);
    }

    #[test]
    fn a_refused_value_is_left_for_the_walk() {
        let mut mem = spring_def();
        let mut host = Host::default();
        let items = group("(SpringLength DampingBump 5250)");
        apply(&mut mem, &mut host, THIS, Class::SpringDef, &items).unwrap();
        assert_eq!(mem.u32(THIS + 0x3c + VALUE), 0);
        assert_eq!(mem.f32(THIS + 0xac + VALUE), 5250.0);
    }

    #[test]
    fn a_one_item_inner_group_writes_nothing() {
        let mut mem = spring_def();
        let mut host = Host::default();
        let items = group("(\"SpringDamper\" SpringLength 0.31 (\"x\"))");
        assert!(apply(&mut mem, &mut host, THIS, Class::SpringDef, &items).is_err());
        assert_eq!(mem.u32(THIS + 0x3c + VALUE), 0);
        assert!(host.after.is_empty());
    }

    #[test]
    fn an_inner_group_links_its_name_once_per_item() {
        const DEF: u32 = 0x2000;
        const VTABLE: u32 = 0x4400;
        let mut mem = Mem::new();
        mem.map(DEF, &[0; 0x100]);
        mem.map(VTABLE, &[0; 0x20]);
        mem.set_u32(DEF, VTABLE);
        mem.set_u32(VTABLE + 0x14, UNK14_LINK);
        let mut host = Host::default();
        let items = group(
            "(\"CAR_ROOT\" (\"CAR_WHEEL_LF\" (\"CAR_WHEEL_LF\")) NULL_PPTOKEN (\"CAR_X\" (\"CAR_X\") (\"CAR_X\")))",
        );
        apply(&mut mem, &mut host, DEF, Class::CarDef, &items).unwrap();
        assert_eq!(mem.u32(DEF + LINK_COUNT), 1);
        assert_eq!(mem.u32(DEF + LINKS), 0x21);
        assert_eq!(host.after, [DEF]);
    }

    #[test]
    fn a_member_object_walks_its_group_in_place() {
        const WHEEL: u32 = 0x2000;
        const VECTOR_VTABLE: u32 = 0x4200;
        const BASE_VTABLE: u32 = 0x4300;
        let mut mem = Mem::new();
        mem.map(WHEEL, &[0; 0x600]);
        mem.set_u32(REAL_VTABLE + 8, REAL_PARSE);
        mem.set_u32(VECTOR_VTABLE + 8, VECTOR_PARSE);
        mem.set_u32(BASE_VTABLE + 8, BASE_PARSE);
        mem.set_u32(WHEEL + 0x250, VECTOR_VTABLE);
        mem.set_u32(WHEEL + 0x450, BASE_VTABLE);
        mem.set_u32(WHEEL + 0x450 + 0x1c, REAL_VTABLE);
        let mut host = Host::default();
        let items = group(
            "(\"CAR_WHEEL_LF\" vecJointMount_VF +0.36 -2.47 SpringDamper (\"SpringDamper\" InnerBumpRubberPos 0.17))",
        );
        apply(&mut mem, &mut host, WHEEL, Class::WheelDef, &items).unwrap();
        assert_eq!(mem.f32(WHEEL + 0x260), 0.36);
        assert_eq!(mem.f32(WHEEL + 0x264), -2.47);
        assert_eq!(mem.u32(WHEEL + 0x268), 0);
        assert_eq!(mem.f32(WHEEL + 0x450 + 0x28), 0.17);
        assert_eq!(mem.u32(WHEEL + 0x454), NAME);
        assert_eq!(host.after, [WHEEL + 0x450, WHEEL]);
    }

    #[test]
    fn a_bit_field_takes_the_signed_names_after_it() {
        const PHYS: u32 = 0x2000;
        const NAMES: u32 = 0x5000;
        let mut mem = Mem::new();
        mem.map(PHYS, &[0; 0x16160]);
        mem.map(flags::CDEF_NAMES, &[0; 0x50]);
        mem.map(NAMES, b"FACE_COLL\0EDGE_COLL\0\0");
        for k in 0..flags::CDEF_BITS {
            mem.set_u32(flags::CDEF_NAMES + 4 * k, NAMES + [0, 10, 20][k.min(2) as usize]);
        }
        mem.set_u32(PHYS + 0x160c0, physics::FLAGS_VTABLE);
        mem.map(physics::FLAGS_VTABLE, &[0; 0x20]);
        mem.set_u32(physics::FLAGS_VTABLE + 8, BIT_FIELD_PARSE);
        mem.set_u32(PHYS + 0x160cc, 1);
        let mut host = Host::default();
        let items = group("(Flags -face_coll +EDGE_COLL +SETI MaxTimeStep)");
        apply(&mut mem, &mut host, PHYS, Class::PhysDef, &items).unwrap();
        assert_eq!(mem.u32(PHYS + 0x160cc), 2);
    }

    #[test]
    fn the_definition_manager_hands_each_definition_to_the_host() {
        const MANAGER: u32 = 0x2000;
        const VTABLE: u32 = 0x4400;
        let mut mem = Mem::new();
        mem.map(MANAGER, &[0; 0x10]);
        mem.map(VTABLE, &[0; 0x20]);
        mem.set_u32(MANAGER, VTABLE);
        mem.set_u32(VTABLE + 0x14, UNK14_DEFINE);
        let mut host = Host::default();
        let items = group("((\"Solid\" (\"GROUND\")) NULL_PPTOKEN (\"Ground\" (\"GROUND\" Width 3)))");
        apply(&mut mem, &mut host, MANAGER, Class::DefManager, &items).unwrap();
        let defined: Vec<_> = host.defined.iter().map(|(f, _, n, len)| (f.as_str(), n.as_str(), *len)).collect();
        assert_eq!(defined, [("Solid", "GROUND", 1), ("Ground", "GROUND", 3)]);
        assert_eq!(mem.read(NAME, 7), b"GROUND\0");
    }
}
